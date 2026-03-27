//! KDMapper C++ Wrapper for Rust FFI
//!
//! This file provides C-compatible wrapper functions for KDMapper functionality.
//! These functions are designed to be called from Rust via FFI.

#pragma once

#include <Windows.h>
#include <cstdint>

#ifdef __cplusplus
extern "C" {
#endif

// ============================================================================
// Opaque Handle Types
// ============================================================================

/// Opaque handle for the KDMapper device (internally a HANDLE)
typedef struct KDMapperDevice_t {
    void* _unused;
} KDMapperDevice;

// ============================================================================
// Intel Driver Management
// ============================================================================

/// Check if the Intel driver is already running
/// @return true if driver is running, false otherwise
bool kdmapper_is_running(void);

/// Load the Intel vulnerable driver (iqvw64e.sys)
/// @param driver_path Path to the Intel driver (can be NULL for default)
/// @return Handle to the device, or NULL on failure
KDMapperDevice* kdmapper_load_intel_driver(const char* driver_path);

/// Unload the Intel driver
/// @param handle Device handle from kdmapper_load_intel_driver
void kdmapper_unload_intel_driver(KDMapperDevice* handle);

// ============================================================================
// Memory Operations
// ============================================================================

/// Read kernel memory
/// @param handle Device handle
/// @param address Kernel virtual address to read from
/// @param buffer Output buffer
/// @param size Number of bytes to read
/// @return true on success, false on failure
bool kdmapper_read_memory(
    KDMapperDevice* handle,
    uint64_t address,
    uint8_t* buffer,
    uint64_t size
);

/// Write kernel memory
/// @param handle Device handle
/// @param address Kernel virtual address to write to
/// @param buffer Input buffer
/// @param size Number of bytes to write
/// @return true on success, false on failure
bool kdmapper_write_memory(
    KDMapperDevice* handle,
    uint64_t address,
    const uint8_t* buffer,
    uint64_t size
);

/// Set kernel memory to a specific value
/// @param handle Device handle
/// @param address Kernel virtual address
/// @param value Value to set (32-bit)
/// @param size Number of bytes to set
/// @return true on success, false on failure
bool kdmapper_set_memory(
    KDMapperDevice* handle,
    uint64_t address,
    uint32_t value,
    uint64_t size
);

// ============================================================================
// Pool Allocation
// ============================================================================

/// Allocate kernel memory pool
/// @param handle Device handle
/// @param pool_type Pool type (0=NonPagedPool, 2=PagedPool, etc.)
/// @param size Size in bytes
/// @return Kernel virtual address, or 0 on failure
uint64_t kdmapper_allocate_pool(
    KDMapperDevice* handle,
    uint32_t pool_type,
    uint64_t size
);

/// Free kernel memory pool
/// @param handle Device handle
/// @param address Kernel virtual address to free
/// @return true on success, false on failure
bool kdmapper_free_pool(
    KDMapperDevice* handle,
    uint64_t address
);

// ============================================================================
// Driver Mapping
// ============================================================================

/// Map a driver into kernel memory
/// @param handle Device handle
/// @param driver_path Path to the target driver (.sys file)
/// @param out_base_address Output: base address of loaded driver
/// @param out_image_size Output: size of driver image
/// @param out_entry_point Output: entry point address
/// @param out_status Output: DriverEntry return status (NTSTATUS)
/// @return true on success, false on failure
bool kdmapper_map_driver(
    KDMapperDevice* handle,
    const char* driver_path,
    uint64_t* out_base_address,
    uint64_t* out_image_size,
    uint64_t* out_entry_point,
    uint32_t* out_status
);

// ============================================================================
// Shellcode Execution
// ============================================================================

/// Execute shellcode in kernel context
/// @param handle Device handle
/// @param shellcode Shellcode bytes
/// @param shellcode_size Size of shellcode in bytes
/// @param timeout_ms Timeout in milliseconds
/// @param out_result Output: return value from shellcode
/// @return true on success, false on failure
bool kdmapper_execute_shellcode(
    KDMapperDevice* handle,
    const uint8_t* shellcode,
    uint32_t shellcode_size,
    uint32_t timeout_ms,
    uint64_t* out_result
);

// ============================================================================
// Module Information
// ============================================================================

/// Get kernel module base address
/// @param handle Device handle
/// @param module_name Name of the module (e.g., "ntoskrnl.exe")
/// @return Base address, or 0 if not found
uint64_t kdmapper_get_module_base(
    KDMapperDevice* handle,
    const char* module_name
);

/// Get kernel module export address
/// @param handle Device handle
/// @param module_base Base address of the module
/// @param function_name Name of the exported function
/// @return Function address, or 0 if not found
uint64_t kdmapper_get_module_export(
    KDMapperDevice* handle,
    uint64_t module_base,
    const char* function_name
);

// ============================================================================
// Anti-Forensics
// ============================================================================

/// Clear the MmUnloadedDrivers list (hide traces of loaded drivers)
/// @param handle Device handle
/// @return true on success, false on failure
bool kdmapper_clear_unloaded_drivers(KDMapperDevice* handle);

// ============================================================================
// Utility Functions
// ============================================================================

/// Get the last error message (call after a failure)
/// @return Error message string (valid until next call)
const char* kdmapper_get_last_error(void);

#ifdef __cplusplus
}
#endif
