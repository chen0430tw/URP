#ifndef TUSB_CONFIG_H_
#define TUSB_CONFIG_H_

#ifdef __cplusplus
extern "C" {
#endif

// ── Board / port ──────────────────────────────────────────────────────────────
#ifndef BOARD_TUD_RHPORT
#define BOARD_TUD_RHPORT      0
#endif
#ifndef BOARD_TUD_MAX_SPEED
#define BOARD_TUD_MAX_SPEED   OPT_MODE_DEFAULT_SPEED
#endif

// ── Common ────────────────────────────────────────────────────────────────────
#ifndef CFG_TUSB_MCU
#error CFG_TUSB_MCU must be defined by the build system
#endif
#ifndef CFG_TUSB_OS
#define CFG_TUSB_OS           OPT_OS_NONE
#endif
#define CFG_TUSB_DEBUG        0
#define CFG_TUD_ENABLED       1
#define CFG_TUD_MAX_SPEED     BOARD_TUD_MAX_SPEED

#ifndef CFG_TUSB_MEM_SECTION
#define CFG_TUSB_MEM_SECTION
#endif
#ifndef CFG_TUSB_MEM_ALIGN
#define CFG_TUSB_MEM_ALIGN    __attribute__((aligned(4)))
#endif

// ── Device ────────────────────────────────────────────────────────────────────
#ifndef CFG_TUD_ENDPOINT0_SIZE
#define CFG_TUD_ENDPOINT0_SIZE  64
#endif

#define CFG_TUD_CDC       1
#define CFG_TUD_MSC       0
#define CFG_TUD_HID       0
#define CFG_TUD_MIDI      0
#define CFG_TUD_VENDOR    0

// RX/TX FIFO 各 256 字节；Full-Speed 每包最大 64 字节
#define CFG_TUD_CDC_RX_BUFSIZE   256
#define CFG_TUD_CDC_TX_BUFSIZE   256
#define CFG_TUD_CDC_RX_EPSIZE    64
#define CFG_TUD_CDC_TX_EPSIZE    64

#ifdef __cplusplus
}
#endif
#endif // TUSB_CONFIG_H_
