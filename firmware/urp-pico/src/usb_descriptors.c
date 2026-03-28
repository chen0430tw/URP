#include "tusb.h"
#include "usb_descriptors.h"
#include "pico/unique_id.h"
#include <string.h>

// ── Device descriptor ─────────────────────────────────────────────────────────
// CDC 复合设备：bDeviceClass = MISC + IAD，让 Windows 正确识别
static const tusb_desc_device_t desc_device = {
    .bLength            = sizeof(tusb_desc_device_t),
    .bDescriptorType    = TUSB_DESC_DEVICE,
    .bcdUSB             = 0x0200,
    .bDeviceClass       = TUSB_CLASS_MISC,
    .bDeviceSubClass    = MISC_SUBCLASS_COMMON,
    .bDeviceProtocol    = MISC_PROTOCOL_IAD,
    .bMaxPacketSize0    = CFG_TUD_ENDPOINT0_SIZE,
    .idVendor           = 0x2E8A,   // Raspberry Pi
    .idProduct          = 0x000A,   // URP firmware (matches UsbDiscovery filter)
    .bcdDevice          = 0x0100,
    .iManufacturer      = 0x01,
    .iProduct           = 0x02,
    .iSerialNumber      = 0x03,
    .bNumConfigurations = 0x01
};

uint8_t const *tud_descriptor_device_cb(void) {
    return (uint8_t const *)&desc_device;
}

// ── Configuration descriptor ──────────────────────────────────────────────────
#define CONFIG_TOTAL_LEN  (TUD_CONFIG_DESC_LEN + TUD_CDC_DESC_LEN)

static const uint8_t desc_fs_configuration[] = {
    TUD_CONFIG_DESCRIPTOR(
        1,               // bConfigurationValue
        ITF_NUM_TOTAL,   // bNumInterfaces
        0,               // iConfiguration
        CONFIG_TOTAL_LEN,
        0x00,            // bus-powered
        100              // 200 mA max
    ),
    TUD_CDC_DESCRIPTOR(
        ITF_NUM_CDC_CTRL,
        4,               // iInterface string index ("URP Serial")
        EPNUM_CDC_NOTIF,
        8,               // notification EP MPS
        EPNUM_CDC_OUT,
        EPNUM_CDC_IN,
        64               // data EP MPS (Full-Speed max)
    )
};

uint8_t const *tud_descriptor_configuration_cb(uint8_t index) {
    (void)index;
    return desc_fs_configuration;
}

// ── String descriptors ────────────────────────────────────────────────────────
// Index 0: language, 1: manufacturer, 2: product, 3: serial, 4: CDC iface
static char serial_str[2 * PICO_UNIQUE_BOARD_ID_SIZE_BYTES + 1];

static const char *string_desc_arr[] = {
    (const char[]){0x09, 0x04},   // 0: Language = 0x0409 (English)
    "Anthropic URP",              // 1: Manufacturer
    "URP Pico Compute Node",      // 2: Product
    serial_str,                   // 3: Serial (filled at runtime)
    "URP Serial",                 // 4: CDC interface name
};
#define STRING_DESC_COUNT  (sizeof(string_desc_arr) / sizeof(string_desc_arr[0]))

static uint16_t desc_str_buf[32];

uint16_t const *tud_descriptor_string_cb(uint8_t index, uint16_t langid) {
    (void)langid;

    // Fill serial number once from flash UID
    if (index == 3 && serial_str[0] == '\0') {
        pico_get_unique_board_id_string(serial_str, sizeof(serial_str));
    }

    uint8_t chr_count;
    if (index == 0) {
        memcpy(&desc_str_buf[1], string_desc_arr[0], 2);
        chr_count = 1;
    } else {
        if (index >= STRING_DESC_COUNT) return NULL;
        const char *str = string_desc_arr[index];
        chr_count = (uint8_t)strlen(str);
        if (chr_count > 31) chr_count = 31;
        for (uint8_t i = 0; i < chr_count; i++)
            desc_str_buf[1 + i] = str[i];  // ASCII → UTF-16LE
    }

    desc_str_buf[0] = (uint16_t)((TUSB_DESC_STRING << 8) | (2 * chr_count + 2));
    return desc_str_buf;
}
