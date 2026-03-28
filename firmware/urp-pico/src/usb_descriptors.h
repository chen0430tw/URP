#pragma once
#include "tusb.h"

// Interface numbers
enum {
    ITF_NUM_CDC_CTRL = 0,   // CDC Control
    ITF_NUM_CDC_DATA,       // CDC Data
    ITF_NUM_TOTAL
};

// Endpoint addresses
// IN = device→host  (0x8x)
// OUT = host→device (0x0x)
#define EPNUM_CDC_NOTIF   0x81   // IN  EP1  — CDC notifications
#define EPNUM_CDC_OUT     0x02   // OUT EP2  — data from host
#define EPNUM_CDC_IN      0x82   // IN  EP2  — data to host
