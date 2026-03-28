//! USB protocol cross-validation tests
//!
//! Simulates what the Pico firmware does (in Rust) and verifies that the
//! host-side codec in usb_executor.rs produces frames the firmware can parse,
//! and that firmware responses decode correctly on the host side.
//!
//! These tests run entirely in software — no hardware required.

use urx_runtime_v08::{
    crc8, decode_response, encode_request, encode_response,
    PayloadValue, UsbOpcodeId, DeviceInfo,
    FRAME_SYNC, STATUS_OK, STATUS_UNSUPPORTED, STATUS_ERROR,
};

// ─────────────────────────────────────────────────────────────────────────────
// Firmware simulator (mirrors main.c logic in Rust)
// ─────────────────────────────────────────────────────────────────────────────

/// Simulate the Pico firmware processing a request frame.
/// Returns the response frame bytes, exactly as the firmware would send.
fn firmware_process(request: &[u8]) -> Vec<u8> {
    // Parse frame
    if request.len() < 4 || request[0] != FRAME_SYNC {
        return fw_error_frame();
    }
    let payload_len = u16::from_le_bytes([request[1], request[2]]) as usize;
    if request.len() < 3 + payload_len + 1 {
        return fw_error_frame();
    }
    let payload = &request[3..3 + payload_len];
    let recv_crc = request[3 + payload_len];
    if crc8(payload) != recv_crc {
        return fw_error_frame();
    }
    if payload.is_empty() {
        return fw_error_frame();
    }

    let op = payload[0];

    // HELLO
    if op == 0xF0 {
        let info = "name=urp-pico-AABBCCDD\ncaps=0x10-0x51\nthroughput=133000\n";
        return encode_response(STATUS_OK, Some(&PayloadValue::Str(info.into())));
    }

    // Parse inputs
    let n_in = if payload.len() > 1 { payload[1] as usize } else { return fw_error_frame() };
    let mut vals: Vec<PayloadValue> = Vec::new();
    let mut pos = 2usize;
    for _ in 0..n_in {
        if pos + 2 > payload.len() { return fw_error_frame(); }
        let in_len = u16::from_le_bytes([payload[pos], payload[pos+1]]) as usize;
        pos += 2;
        if pos + in_len > payload.len() { return fw_error_frame(); }
        use urx_runtime_v08::packet::PayloadCodec;
        vals.push(PayloadCodec::decode(&payload[pos..pos+in_len]));
        pos += in_len;
    }

    // Execute
    let result = fw_exec(op, &vals);
    match result {
        Some(v) => encode_response(STATUS_OK, Some(&v)),
        None    => encode_response(STATUS_UNSUPPORTED, None),
    }
}

fn fw_error_frame() -> Vec<u8> {
    encode_response(STATUS_ERROR, None)
}

fn fw_exec(op: u8, vals: &[PayloadValue]) -> Option<PayloadValue> {
    let i = |idx: usize| match vals.get(idx) {
        Some(PayloadValue::I64(v)) => Some(*v),
        Some(PayloadValue::F64(v)) => Some(*v as i64),
        _ => None,
    };
    let f = |idx: usize| match vals.get(idx) {
        Some(PayloadValue::F64(v)) => Some(*v),
        Some(PayloadValue::I64(v)) => Some(*v as f64),
        _ => None,
    };

    Some(match op {
        0x10 => PayloadValue::I64(i(0)? + i(1)?),   // UAdd
        0x11 => PayloadValue::I64(i(0)? - i(1)?),   // USub
        0x12 => PayloadValue::I64(i(0)?.wrapping_mul(i(1)?)), // UMul
        0x13 => { let b = i(1)?; if b == 0 { return None; } PayloadValue::I64(i(0)? / b) }
        0x14 => { let b = i(1)?; if b == 0 { return None; } PayloadValue::I64(i(0)? % b) }
        0x15 => PayloadValue::I64((i(0)? == i(1)?) as i64),   // UCmpEq
        0x16 => PayloadValue::I64((i(0)? <  i(1)?) as i64),   // UCmpLt
        0x17 => PayloadValue::I64((i(0)? <= i(1)?) as i64),   // UCmpLe
        0x18 => PayloadValue::I64(i(0)? & i(1)?),   // UAnd
        0x19 => PayloadValue::I64(i(0)? | i(1)?),   // UOr
        0x1A => PayloadValue::I64(i(0)? ^ i(1)?),   // UXor
        0x1B => PayloadValue::I64(!i(0)?),           // UNot
        0x1C => PayloadValue::I64(i(0)?.wrapping_shl((i(1)? & 63) as u32)), // UShl
        0x1D => PayloadValue::I64(((i(0)? as u64).wrapping_shr((i(1)? & 63) as u32)) as i64), // UShr
        0x1E => PayloadValue::I64(i(0)?.wrapping_shr((i(1)? & 63) as u32)), // UShra
        0x41 => PayloadValue::I64(i(0)?.min(i(1)?)), // UMin
        0x42 => PayloadValue::I64(i(0)?.max(i(1)?)), // UMax
        0x43 => PayloadValue::I64(i(0)?.wrapping_abs()), // UAbs
        0x20 => PayloadValue::F64(f(0)? + f(1)?),
        0x21 => PayloadValue::F64(f(0)? - f(1)?),
        0x22 => PayloadValue::F64(f(0)? * f(1)?),
        0x23 => PayloadValue::F64(f(0)? / f(1)?),
        0x24 => PayloadValue::F64(f(0)?.powf(f(1)?)),
        0x25 => PayloadValue::F64(f(0)?.sqrt()),
        0x26 => PayloadValue::F64(f(0)?.abs()),
        0x27 => PayloadValue::F64(-f(0)?),
        0x28 => PayloadValue::F64(f(0)?.floor()),
        0x29 => PayloadValue::F64(f(0)?.ceil()),
        0x2A => PayloadValue::F64(f(0)?.round()),
        0x30 => PayloadValue::I64((f(0)? == f(1)?) as i64),
        0x31 => PayloadValue::I64((f(0)? <  f(1)?) as i64),
        0x32 => PayloadValue::I64((f(0)? <= f(1)?) as i64),
        0x50 => PayloadValue::I64(f(0)? as i64),
        0x51 => PayloadValue::F64(i(0)? as f64),
        _    => return None,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: send a request through the firmware simulator and get the result
// ─────────────────────────────────────────────────────────────────────────────

fn roundtrip(op: UsbOpcodeId, inputs: &[PayloadValue]) -> (u8, Option<PayloadValue>) {
    let refs: Vec<&PayloadValue> = inputs.iter().collect();
    let request = encode_request(op, &refs);
    let response = firmware_process(&request);
    decode_response(&response).expect("decode_response failed")
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: integer operations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fw_uadd() {
    let (s, v) = roundtrip(UsbOpcodeId::UAdd, &[PayloadValue::I64(17), PayloadValue::I64(25)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(42)));
}

#[test]
fn fw_usub() {
    let (s, v) = roundtrip(UsbOpcodeId::USub, &[PayloadValue::I64(100), PayloadValue::I64(58)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(42)));
}

#[test]
fn fw_umul() {
    let (s, v) = roundtrip(UsbOpcodeId::UMul, &[PayloadValue::I64(6), PayloadValue::I64(7)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(42)));
}

#[test]
fn fw_udiv() {
    let (s, v) = roundtrip(UsbOpcodeId::UDiv, &[PayloadValue::I64(84), PayloadValue::I64(2)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(42)));
}

#[test]
fn fw_udiv_by_zero_returns_unsupported() {
    let (s, _) = roundtrip(UsbOpcodeId::UDiv, &[PayloadValue::I64(1), PayloadValue::I64(0)]);
    assert_eq!(s, STATUS_UNSUPPORTED);
}

#[test]
fn fw_urem() {
    let (s, v) = roundtrip(UsbOpcodeId::URem, &[PayloadValue::I64(100), PayloadValue::I64(58)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(42)));
}

#[test]
fn fw_ucmpeq_true() {
    let (s, v) = roundtrip(UsbOpcodeId::UCmpEq, &[PayloadValue::I64(7), PayloadValue::I64(7)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(1)));
}

#[test]
fn fw_ucmpeq_false() {
    let (s, v) = roundtrip(UsbOpcodeId::UCmpEq, &[PayloadValue::I64(7), PayloadValue::I64(8)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(0)));
}

#[test]
fn fw_ucmplt() {
    let (s, v) = roundtrip(UsbOpcodeId::UCmpLt, &[PayloadValue::I64(3), PayloadValue::I64(9)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(1)));
}

#[test]
fn fw_uand() {
    let (s, v) = roundtrip(UsbOpcodeId::UAnd, &[PayloadValue::I64(0xFF), PayloadValue::I64(0x0F)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(0x0F)));
}

#[test]
fn fw_uor() {
    let (s, v) = roundtrip(UsbOpcodeId::UOr, &[PayloadValue::I64(0xF0), PayloadValue::I64(0x0F)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(0xFF)));
}

#[test]
fn fw_uxor() {
    let (s, v) = roundtrip(UsbOpcodeId::UXor, &[PayloadValue::I64(0xFF), PayloadValue::I64(0x0F)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(0xF0)));
}

#[test]
fn fw_unot() {
    let (s, v) = roundtrip(UsbOpcodeId::UNot, &[PayloadValue::I64(0)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(-1)));
}

#[test]
fn fw_ushl() {
    let (s, v) = roundtrip(UsbOpcodeId::UShl, &[PayloadValue::I64(1), PayloadValue::I64(10)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(1024)));
}

#[test]
fn fw_ushr() {
    let (s, v) = roundtrip(UsbOpcodeId::UShr, &[PayloadValue::I64(1024), PayloadValue::I64(10)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(1)));
}

#[test]
fn fw_umin_umax() {
    let (_, v) = roundtrip(UsbOpcodeId::UMin, &[PayloadValue::I64(3), PayloadValue::I64(7)]);
    assert_eq!(v, Some(PayloadValue::I64(3)));
    let (_, v) = roundtrip(UsbOpcodeId::UMax, &[PayloadValue::I64(3), PayloadValue::I64(7)]);
    assert_eq!(v, Some(PayloadValue::I64(7)));
}

#[test]
fn fw_uabs() {
    let (s, v) = roundtrip(UsbOpcodeId::UAbs, &[PayloadValue::I64(-42)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(42)));
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: float operations
// ─────────────────────────────────────────────────────────────────────────────

fn approx(v: Option<PayloadValue>, expected: f64) -> bool {
    match v {
        Some(PayloadValue::F64(x)) => (x - expected).abs() < 1e-9,
        Some(PayloadValue::I64(x)) => (x as f64 - expected).abs() < 1e-9,
        _ => false,
    }
}

#[test]
fn fw_fadd() {
    let (s, v) = roundtrip(UsbOpcodeId::FAdd, &[PayloadValue::F64(1.5), PayloadValue::F64(2.5)]);
    assert_eq!(s, STATUS_OK);
    assert!(approx(v, 4.0));
}

#[test]
fn fw_fmul() {
    let (s, v) = roundtrip(UsbOpcodeId::FMul, &[PayloadValue::F64(3.0), PayloadValue::F64(14.0)]);
    assert_eq!(s, STATUS_OK);
    assert!(approx(v, 42.0));
}

#[test]
fn fw_fsqrt() {
    let (s, v) = roundtrip(UsbOpcodeId::FSqrt, &[PayloadValue::F64(1764.0)]);
    assert_eq!(s, STATUS_OK);
    assert!(approx(v, 42.0));
}

#[test]
fn fw_fpow() {
    let (s, v) = roundtrip(UsbOpcodeId::FPow, &[PayloadValue::F64(2.0), PayloadValue::F64(10.0)]);
    assert_eq!(s, STATUS_OK);
    assert!(approx(v, 1024.0));
}

#[test]
fn fw_ffloor_fceil_fround() {
    let (_, v) = roundtrip(UsbOpcodeId::FFloor, &[PayloadValue::F64(2.9)]);
    assert!(approx(v, 2.0));
    let (_, v) = roundtrip(UsbOpcodeId::FCeil,  &[PayloadValue::F64(2.1)]);
    assert!(approx(v, 3.0));
    let (_, v) = roundtrip(UsbOpcodeId::FRound, &[PayloadValue::F64(2.5)]);
    assert!(approx(v, 3.0));
}

#[test]
fn fw_fcmpeq_lt_le() {
    let (_, v) = roundtrip(UsbOpcodeId::FCmpEq, &[PayloadValue::F64(3.0), PayloadValue::F64(3.0)]);
    assert_eq!(v, Some(PayloadValue::I64(1)));
    let (_, v) = roundtrip(UsbOpcodeId::FCmpLt, &[PayloadValue::F64(2.0), PayloadValue::F64(3.0)]);
    assert_eq!(v, Some(PayloadValue::I64(1)));
    let (_, v) = roundtrip(UsbOpcodeId::FCmpLe, &[PayloadValue::F64(3.0), PayloadValue::F64(3.0)]);
    assert_eq!(v, Some(PayloadValue::I64(1)));
}

#[test]
fn fw_f64toi64() {
    let (s, v) = roundtrip(UsbOpcodeId::F64ToI64, &[PayloadValue::F64(3.9)]);
    assert_eq!(s, STATUS_OK);
    assert_eq!(v, Some(PayloadValue::I64(3)));  // truncate toward zero
}

#[test]
fn fw_i64tof64() {
    let (s, v) = roundtrip(UsbOpcodeId::I64ToF64, &[PayloadValue::I64(42)]);
    assert_eq!(s, STATUS_OK);
    assert!(approx(v, 42.0));
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: HELLO handshake
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fw_hello_returns_device_info() {
    let (s, v) = roundtrip(UsbOpcodeId::Hello, &[]);
    assert_eq!(s, STATUS_OK);
    match v {
        Some(PayloadValue::Str(info)) => {
            let d = DeviceInfo::parse(&info, "/dev/ttyACM0");
            assert!(d.name.starts_with("urp-pico-"), "name={}", d.name);
            assert_eq!(d.caps, "0x10-0x51");
            assert_eq!(d.throughput, 133_000);
            assert!(d.compute_capacity() > 10.0);
        }
        other => panic!("expected Str, got {:?}", other),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests: error handling
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fw_unknown_opcode_returns_unsupported() {
    // UConcat (0x03) has no USB ID — Unknown maps to 0x00
    let frame = encode_request(UsbOpcodeId::Unknown, &[&PayloadValue::I64(1)]);
    let response = firmware_process(&frame);
    let (s, _) = decode_response(&response).unwrap();
    assert_eq!(s, STATUS_UNSUPPORTED);
}

#[test]
fn fw_bad_crc_gets_error() {
    let mut frame = encode_request(UsbOpcodeId::UAdd, &[&PayloadValue::I64(1), &PayloadValue::I64(2)]);
    *frame.last_mut().unwrap() ^= 0xFF;  // corrupt CRC
    let response = firmware_process(&frame);
    // Bad CRC → firmware returns error frame
    let (s, _) = decode_response(&response).unwrap();
    assert_eq!(s, STATUS_ERROR);
}

#[test]
fn fw_bad_sync_gets_error() {
    let mut frame = encode_request(UsbOpcodeId::UAdd, &[&PayloadValue::I64(1), &PayloadValue::I64(2)]);
    frame[0] = 0x00;  // corrupt sync byte
    let response = firmware_process(&frame);
    let (s, _) = decode_response(&response).unwrap();
    assert_eq!(s, STATUS_ERROR);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: L2 norm pipeline (matches bench_test.rs graph)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn fw_l2_norm_pipeline() {
    // sqrt(3^2 + 4^2) = 5  — manually walk the graph through firmware
    let a = PayloadValue::F64(3.0);
    let b = PayloadValue::F64(4.0);

    // aa = a * a = 9
    let (_, aa) = roundtrip(UsbOpcodeId::FMul, &[a.clone(), a.clone()]);
    // bb = b * b = 16
    let (_, bb) = roundtrip(UsbOpcodeId::FMul, &[b.clone(), b.clone()]);
    // sum = aa + bb = 25
    let (_, sum) = roundtrip(UsbOpcodeId::FAdd, &[aa.unwrap(), bb.unwrap()]);
    // h = sqrt(25) = 5
    let (s, h) = roundtrip(UsbOpcodeId::FSqrt, &[sum.unwrap()]);

    assert_eq!(s, STATUS_OK);
    assert!(approx(h, 5.0), "expected 5.0");
}
