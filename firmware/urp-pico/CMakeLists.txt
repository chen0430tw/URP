cmake_minimum_required(VERSION 3.13)

# pico_sdk_import.cmake 必须在 project() 之前
include(pico_sdk_import.cmake)

project(urp_pico C CXX ASM)
set(CMAKE_C_STANDARD 11)
set(CMAKE_CXX_STANDARD 17)

pico_sdk_init()

add_executable(urp_pico
    src/main.c
    src/usb_descriptors.c
)

# tusb_config.h 需要在 include path 里让 TinyUSB 能找到
target_include_directories(urp_pico PRIVATE
    ${CMAKE_CURRENT_LIST_DIR}/src
)

target_link_libraries(urp_pico PRIVATE
    pico_stdlib
    pico_unique_id
    tinyusb_device
    tinyusb_board
)

# 解除 RP2040-E5 USB 枚举勘误（占用 GPIO15，仅在遇到 Windows 枚举失败时启用）
# target_compile_definitions(urp_pico PRIVATE PICO_RP2040_USB_DEVICE_ENUMERATION_FIX=1)

# 生成 .uf2 / .hex / .bin / .map
pico_add_extra_outputs(urp_pico)

# 不使用 pico stdio USB，我们自己管理 CDC
pico_enable_stdio_usb(urp_pico 0)
pico_enable_stdio_uart(urp_pico 0)
