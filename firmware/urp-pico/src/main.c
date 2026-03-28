/*
 * URP Pico Compute Node
 * =====================
 * Implements the URP USB wire protocol over USB CDC ACM.
 *
 * Wire format (matches usb_executor.rs exactly):
 *
 *   Frame:   SYNC(1=0xA5) | LEN(2 LE) | PAYLOAD | CRC8(1)
 *   Request: OPCODE(1) | N_IN(1) | [IN_LEN(2 LE) | IN_BYTES] ...
 *   Response:STATUS(1) | [OUT_LEN(2 LE) | OUT_BYTES]
 *
 *   PayloadCodec I64: 0x01 | int64_t (8B LE)
 *   PayloadCodec F64: 0x04 | double  (8B LE, IEEE 754)
 *   PayloadCodec Str: 0x02 | uint32_t len (4B LE) | UTF-8 bytes
 *
 *   CRC: MSB-first, poly=0x31, init=0x00 (matches Rust crc8())
 */

#include <stdio.h>
#include <string.h>
#include <math.h>
#include <stdint.h>
#include <stdbool.h>

#include "pico/stdlib.h"
#include "pico/unique_id.h"
#include "hardware/gpio.h"
#include "tusb.h"

// ─────────────────────────────────────────────────────────────────────────────
// Protocol constants — keep in sync with usb_executor.rs
// ─────────────────────────────────────────────────────────────────────────────

#define FRAME_SYNC          0xA5u
#define STATUS_OK           0x00u
#define STATUS_UNSUPPORTED  0x01u
#define STATUS_ERROR        0xFFu

#define TYPE_I64            0x01u
#define TYPE_STR            0x02u
#define TYPE_F64            0x04u

// Opcode IDs — one-to-one with UsbOpcodeId enum
#define OP_CONST_I64    0x01u
#define OP_CONST_F64    0x02u
#define OP_UADD         0x10u
#define OP_USUB         0x11u
#define OP_UMUL         0x12u
#define OP_UDIV         0x13u
#define OP_UREM         0x14u
#define OP_UCMPEQ       0x15u
#define OP_UCMPLT       0x16u
#define OP_UCMPLE       0x17u
#define OP_UAND         0x18u
#define OP_UOR          0x19u
#define OP_UXOR         0x1Au
#define OP_UNOT         0x1Bu
#define OP_USHL         0x1Cu
#define OP_USHR         0x1Du
#define OP_USHRA        0x1Eu
#define OP_FADD         0x20u
#define OP_FSUB         0x21u
#define OP_FMUL         0x22u
#define OP_FDIV         0x23u
#define OP_FPOW         0x24u
#define OP_FSQRT        0x25u
#define OP_FABS         0x26u
#define OP_FNEG         0x27u
#define OP_FFLOOR       0x28u
#define OP_FCEIL        0x29u
#define OP_FROUND       0x2Au
#define OP_FCMPEQ       0x30u
#define OP_FCMPLT       0x31u
#define OP_FCMPLE       0x32u
#define OP_USELECT      0x40u
#define OP_UMIN         0x41u
#define OP_UMAX         0x42u
#define OP_UABS         0x43u
#define OP_UASSERT      0x44u
#define OP_F64TOI64     0x50u
#define OP_I64TOF64     0x51u
#define OP_HELLO        0xF0u

// ─────────────────────────────────────────────────────────────────────────────
// CRC-8 — MSB-first, poly=0x31, init=0x00
// Must match Rust crc8() in usb_executor.rs exactly.
// ─────────────────────────────────────────────────────────────────────────────

static uint8_t crc8(const uint8_t *data, uint32_t len) {
    uint8_t crc = 0x00u;
    for (uint32_t i = 0; i < len; i++) {
        crc ^= data[i];
        for (int b = 0; b < 8; b++) {
            crc = (crc & 0x80u) ? (uint8_t)((crc << 1) ^ 0x31u)
                                : (uint8_t)(crc << 1);
        }
    }
    return crc;
}

// ─────────────────────────────────────────────────────────────────────────────
// Val — lightweight tagged union (I64 / F64 / NONE)
// ─────────────────────────────────────────────────────────────────────────────

typedef enum { VAL_NONE, VAL_I64, VAL_F64 } ValType;
typedef struct {
    ValType type;
    union { int64_t i; double f; } v;
} Val;

static inline Val val_i64(int64_t i) { return (Val){VAL_I64, {.i = i}}; }
static inline Val val_f64(double  f) { return (Val){VAL_F64, {.f = f}}; }
static inline Val val_none(void)     { return (Val){VAL_NONE, {.i = 0}}; }

// Decode a single PayloadCodec value from buf[0..len-1].
// Returns val_none() on unrecognised type or short buffer.
static Val decode_val(const uint8_t *buf, uint32_t len) {
    if (len < 9) return val_none();
    if (buf[0] == TYPE_I64) {
        int64_t v; memcpy(&v, buf + 1, 8); return val_i64(v);
    }
    if (buf[0] == TYPE_F64) {
        double  v; memcpy(&v, buf + 1, 8); return val_f64(v);
    }
    return val_none();
}

// Encode a Val into buf (always 9 bytes for I64/F64).
// Returns bytes written (9), or 0 for VAL_NONE.
static uint32_t encode_val(const Val *v, uint8_t *buf) {
    if (v->type == VAL_I64) {
        buf[0] = TYPE_I64; memcpy(buf + 1, &v->v.i, 8); return 9;
    }
    if (v->type == VAL_F64) {
        buf[0] = TYPE_F64; memcpy(buf + 1, &v->v.f, 8); return 9;
    }
    return 0;
}

// ─────────────────────────────────────────────────────────────────────────────
// Frame RX state machine
// ─────────────────────────────────────────────────────────────────────────────

// Max payload = 2 inputs × (2-byte len prefix + 9-byte value) + opcode(1) + n_in(1)
// = 2*11 + 2 = 24; round up generously for USelect (3 inputs) and HELLO.
#define MAX_PAYLOAD 64u

typedef enum {
    ST_SYNC,
    ST_LEN0,
    ST_LEN1,
    ST_PAYLOAD,
    ST_CRC,
} RxState;

static struct {
    RxState  state;
    uint8_t  buf[MAX_PAYLOAD];
    uint32_t expect;   // total payload bytes expected
    uint32_t pos;      // bytes received so far
} rx_sm;

// Feed one byte; returns true + sets *out_payload / *out_len when a frame completes.
static bool rx_feed(uint8_t byte,
                    const uint8_t **out_payload, uint32_t *out_len)
{
    switch (rx_sm.state) {
    case ST_SYNC:
        if (byte == FRAME_SYNC) rx_sm.state = ST_LEN0;
        break;
    case ST_LEN0:
        rx_sm.expect = byte;
        rx_sm.state  = ST_LEN1;
        break;
    case ST_LEN1:
        rx_sm.expect |= ((uint32_t)byte << 8);
        if (rx_sm.expect == 0 || rx_sm.expect > MAX_PAYLOAD) {
            rx_sm.state = ST_SYNC;  // bad length — resync
        } else {
            rx_sm.pos   = 0;
            rx_sm.state = ST_PAYLOAD;
        }
        break;
    case ST_PAYLOAD:
        rx_sm.buf[rx_sm.pos++] = byte;
        if (rx_sm.pos >= rx_sm.expect) rx_sm.state = ST_CRC;
        break;
    case ST_CRC: {
        rx_sm.state = ST_SYNC;
        if (crc8(rx_sm.buf, rx_sm.expect) == byte) {
            *out_payload = rx_sm.buf;
            *out_len     = rx_sm.expect;
            return true;
        }
        // CRC mismatch — drop frame, keep LED blinking as error indicator
        gpio_xor_mask(1u << PICO_DEFAULT_LED_PIN);
        break;
    }
    }
    return false;
}

// ─────────────────────────────────────────────────────────────────────────────
// Parse inputs from request payload
// payload: OPCODE(1) N_IN(1) [IN_LEN(2LE) IN_BYTES] ...
// Fills vals[0..n_in-1], caps at 3 for USelect.
// Returns true on success.
// ─────────────────────────────────────────────────────────────────────────────

#define MAX_INPUTS 3u

static bool parse_inputs(const uint8_t *payload, uint32_t len,
                          Val vals[MAX_INPUTS], uint32_t *n_in)
{
    if (len < 2) return false;
    uint32_t n = payload[1];
    if (n > MAX_INPUTS) n = MAX_INPUTS;
    *n_in = n;

    uint32_t pos = 2;
    for (uint32_t i = 0; i < n; i++) {
        if (pos + 2 > len) return false;
        uint32_t ilen = (uint32_t)payload[pos] | ((uint32_t)payload[pos+1] << 8);
        pos += 2;
        if (pos + ilen > len) return false;
        vals[i] = decode_val(payload + pos, ilen);
        pos += ilen;
    }
    return true;
}

// ─────────────────────────────────────────────────────────────────────────────
// Execute an opcode
// ─────────────────────────────────────────────────────────────────────────────

static Val exec_op(uint8_t op, const Val *a, const Val *b, const Val *c) {
    switch (op) {
    // Integer arithmetic
    case OP_UADD:    return val_i64(a->v.i + b->v.i);
    case OP_USUB:    return val_i64(a->v.i - b->v.i);
    case OP_UMUL:    return val_i64(a->v.i * b->v.i);
    case OP_UDIV:    return b->v.i ? val_i64(a->v.i / b->v.i) : val_none();
    case OP_UREM:    return b->v.i ? val_i64(a->v.i % b->v.i) : val_none();
    case OP_UCMPEQ:  return val_i64(a->v.i == b->v.i ? 1 : 0);
    case OP_UCMPLT:  return val_i64(a->v.i <  b->v.i ? 1 : 0);
    case OP_UCMPLE:  return val_i64(a->v.i <= b->v.i ? 1 : 0);
    // Logic / shift
    case OP_UAND:    return val_i64(a->v.i & b->v.i);
    case OP_UOR:     return val_i64(a->v.i | b->v.i);
    case OP_UXOR:    return val_i64(a->v.i ^ b->v.i);
    case OP_UNOT:    return val_i64(~a->v.i);
    case OP_USHL:    return val_i64(a->v.i << (b->v.i & 63));
    case OP_USHR:    return val_i64((int64_t)((uint64_t)a->v.i >> (uint32_t)(b->v.i & 63)));
    case OP_USHRA:   return val_i64(a->v.i >> (b->v.i & 63));
    // Aggregation
    case OP_UMIN:    return val_i64(a->v.i < b->v.i ? a->v.i : b->v.i);
    case OP_UMAX:    return val_i64(a->v.i > b->v.i ? a->v.i : b->v.i);
    case OP_UABS:    return val_i64(a->v.i < 0 ? -a->v.i : a->v.i);
    case OP_UASSERT: return a->v.i ? *a : val_none();
    case OP_USELECT: return a->v.i ? *b : *c;  // cond=a, true=b, false=c
    // Float arithmetic
    case OP_FADD:    return val_f64(a->v.f + b->v.f);
    case OP_FSUB:    return val_f64(a->v.f - b->v.f);
    case OP_FMUL:    return val_f64(a->v.f * b->v.f);
    case OP_FDIV:    return val_f64(a->v.f / b->v.f);   // NaN/Inf on /0, no trap
    case OP_FPOW:    return val_f64(pow(a->v.f, b->v.f));
    // Float unary
    case OP_FSQRT:   return val_f64(sqrt(a->v.f));
    case OP_FABS:    return val_f64(fabs(a->v.f));
    case OP_FNEG:    return val_f64(-a->v.f);
    case OP_FFLOOR:  return val_f64(floor(a->v.f));
    case OP_FCEIL:   return val_f64(ceil(a->v.f));
    case OP_FROUND:  return val_f64(round(a->v.f));
    // Float comparison
    case OP_FCMPEQ:  return val_i64(a->v.f == b->v.f ? 1 : 0);
    case OP_FCMPLT:  return val_i64(a->v.f <  b->v.f ? 1 : 0);
    case OP_FCMPLE:  return val_i64(a->v.f <= b->v.f ? 1 : 0);
    // Type conversion
    case OP_F64TOI64: return val_i64((int64_t)a->v.f);
    case OP_I64TOF64: return val_f64((double)a->v.i);
    // Pass-through constants (host supplies the value as the first input)
    case OP_CONST_I64: return *a;
    case OP_CONST_F64: return *a;
    default: return val_none();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Build response frame into out[].
// Returns total frame length.
// ─────────────────────────────────────────────────────────────────────────────

// Worst case: STATUS(1) + OUT_LEN(2) + TYPE(1) + VALUE(8) = 12 bytes payload
// + SYNC(1) + LEN(2) + CRC(1) = 16 bytes total
#define RESP_BUF_SIZE 16u

static uint32_t build_resp(uint8_t *out, uint8_t status, const Val *result) {
    uint8_t payload[12];
    uint32_t plen = 0;

    payload[plen++] = status;

    if (status == STATUS_OK && result && result->type != VAL_NONE) {
        uint8_t enc[9];
        uint32_t enc_len = encode_val(result, enc);
        payload[plen++] = (uint8_t)(enc_len & 0xFF);
        payload[plen++] = (uint8_t)((enc_len >> 8) & 0xFF);
        memcpy(payload + plen, enc, enc_len);
        plen += enc_len;
    }

    uint8_t crc = crc8(payload, plen);
    out[0] = FRAME_SYNC;
    out[1] = (uint8_t)(plen & 0xFF);
    out[2] = (uint8_t)((plen >> 8) & 0xFF);
    memcpy(out + 3, payload, plen);
    out[3 + plen] = crc;
    return 4 + plen;
}

// ─────────────────────────────────────────────────────────────────────────────
// Build HELLO response frame.
// Response payload: STATUS(1) + OUT_LEN(2) + TYPE_STR(1) + STR_LEN(4) + UTF-8
// ─────────────────────────────────────────────────────────────────────────────

#define HELLO_BUF_SIZE 192u

static uint32_t build_hello(uint8_t *out, uint32_t out_size) {
    char serial[2 * PICO_UNIQUE_BOARD_ID_SIZE_BYTES + 1] = {0};
    pico_get_unique_board_id_string(serial, sizeof(serial));

    // info string: key=value\n pairs
    char info[128];
    int info_len = snprintf(info, sizeof(info),
        "name=urp-pico-%s\n"
        "caps=0x10-0x51\n"
        "throughput=133000\n",
        serial);
    if (info_len <= 0 || (uint32_t)info_len >= sizeof(info)) info_len = 0;

    // Str encoding: TYPE_STR(1) + len(4 LE) + bytes
    uint32_t str_enc_len = 1 + 4 + (uint32_t)info_len;

    // Full payload: STATUS(1) + OUT_LEN(2) + str_enc_len
    uint32_t payload_len = 1 + 2 + str_enc_len;

    if (4 + payload_len + 1 > out_size) return 0;  // won't fit

    uint8_t *p = out;
    // Frame header
    *p++ = FRAME_SYNC;
    *p++ = (uint8_t)(payload_len & 0xFF);
    *p++ = (uint8_t)((payload_len >> 8) & 0xFF);

    uint8_t *payload_start = p;

    // Payload
    *p++ = STATUS_OK;
    *p++ = (uint8_t)(str_enc_len & 0xFF);
    *p++ = (uint8_t)((str_enc_len >> 8) & 0xFF);

    // Str value
    *p++ = TYPE_STR;
    *p++ = (uint8_t)(info_len & 0xFF);
    *p++ = (uint8_t)((info_len >> 8) & 0xFF);
    *p++ = 0;
    *p++ = 0;
    memcpy(p, info, (uint32_t)info_len);
    p += info_len;

    *p = crc8(payload_start, payload_len);
    return (uint32_t)(p + 1 - out);
}

// ─────────────────────────────────────────────────────────────────────────────
// CDC write helper — handles TX FIFO full + mandatory flush
// ─────────────────────────────────────────────────────────────────────────────

static void cdc_write_all(const uint8_t *data, uint32_t len) {
    uint32_t sent = 0;
    while (sent < len) {
        uint32_t avail = tud_cdc_write_available();
        if (avail == 0) {
            tud_cdc_write_flush();
            tud_task();
            continue;
        }
        uint32_t chunk = len - sent;
        if (chunk > avail) chunk = avail;
        tud_cdc_write(data + sent, chunk);
        sent += chunk;
    }
    tud_cdc_write_flush();  // mandatory: data stays in FIFO without this
}

// ─────────────────────────────────────────────────────────────────────────────
// Process one complete frame
// ─────────────────────────────────────────────────────────────────────────────

static void handle_frame(const uint8_t *payload, uint32_t len) {
    if (len < 1) return;
    uint8_t op = payload[0];

    // ── HELLO ─────────────────────────────────────────────────────────────────
    if (op == OP_HELLO) {
        uint8_t buf[HELLO_BUF_SIZE];
        uint32_t n = build_hello(buf, sizeof(buf));
        if (n > 0) cdc_write_all(buf, n);
        return;
    }

    // ── Regular opcode ────────────────────────────────────────────────────────
    Val vals[MAX_INPUTS] = {val_none(), val_none(), val_none()};
    uint32_t n_in = 0;

    uint8_t resp[RESP_BUF_SIZE];
    uint32_t resp_len;

    if (!parse_inputs(payload, len, vals, &n_in)) {
        resp_len = build_resp(resp, STATUS_ERROR, NULL);
        cdc_write_all(resp, resp_len);
        return;
    }

    Val result = exec_op(op, &vals[0], &vals[1], &vals[2]);
    uint8_t status = (result.type != VAL_NONE) ? STATUS_OK : STATUS_UNSUPPORTED;
    resp_len = build_resp(resp, status, &result);
    cdc_write_all(resp, resp_len);
}

// ─────────────────────────────────────────────────────────────────────────────
// TinyUSB callbacks (required symbols)
// ─────────────────────────────────────────────────────────────────────────────

void tud_mount_cb(void)                               { gpio_put(PICO_DEFAULT_LED_PIN, 1); }
void tud_umount_cb(void)                              { gpio_put(PICO_DEFAULT_LED_PIN, 0); }
void tud_suspend_cb(bool remote_wakeup_en)            { (void)remote_wakeup_en; }
void tud_resume_cb(void)                              {}
void tud_cdc_line_state_cb(uint8_t itf, bool dtr, bool rts) { (void)itf; (void)dtr; (void)rts; }

// ─────────────────────────────────────────────────────────────────────────────
// main
// ─────────────────────────────────────────────────────────────────────────────

int main(void) {
    board_init();   // clock to 125 MHz, SysTick — must come before tusb_init()

    gpio_init(PICO_DEFAULT_LED_PIN);
    gpio_set_dir(PICO_DEFAULT_LED_PIN, GPIO_OUT);

    tusb_init();

    memset(&rx_sm, 0, sizeof(rx_sm));
    rx_sm.state = ST_SYNC;

    while (true) {
        // tud_task() processes the USB event queue; call as often as possible.
        // Never use sleep_ms() here — USB enumeration will time out.
        tud_task();

        if (!tud_cdc_connected()) continue;

        // Drain RX FIFO byte by byte into the state machine
        while (tud_cdc_available()) {
            uint8_t byte;
            if (tud_cdc_read(&byte, 1) != 1) break;

            const uint8_t *payload;
            uint32_t plen;
            if (rx_feed(byte, &payload, &plen)) {
                handle_frame(payload, plen);
            }
        }
    }
}
