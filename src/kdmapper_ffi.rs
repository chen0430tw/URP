//! KDMapper FFI Layer
//!
//! This module provides safe Rust bindings to KDMapper functionality.
//!
//! # Features
//! - `kdmapper` - Enable KDMapper integration (requires C++ library)
//!
//! # Note
//! When the `kdmapper` feature is enabled but the C++ library is not built,
//! the FFI calls will fail with linkage errors. For testing purposes without
//! the C++ library, use the mock functions or disable the feature.

use std::ffi::{c_char, CString};
use std::path::Path;
use std::ptr::NonNull;

/// KDMapper error types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KDMapperError {
    DriverLoadFailed,
    DriverAlreadyRunning,
    BlocklistEnabled,
    InvalidDriverPath,
    MemoryAllocationFailed,
    MemoryReadFailed,
    MemoryWriteFailed,
    ShellcodeExecutionFailed,
    InvalidAddress,
    PermissionDenied,
    Timeout,
    InvalidPEImage,
    RelocationFailed,
    ImportResolutionFailed,
    /// DriverEntry returned a non-zero NTSTATUS.
    /// The inner value is the raw NTSTATUS code (e.g. 0xC0000034 = STATUS_OBJECT_NAME_NOT_FOUND).
    DriverEntryFailed,
    DriverEntryNtStatus(u32),
    Unknown(String),
}

impl std::fmt::Display for KDMapperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KDMapperError::DriverLoadFailed => write!(f, "Failed to load driver"),
            KDMapperError::DriverAlreadyRunning => write!(f, "Driver already running"),
            KDMapperError::BlocklistEnabled => write!(f, "Windows Vulnerable Driver Blocklist is enabled; set VulnerableDriverBlocklistEnable=0 in HKLM\\SYSTEM\\CurrentControlSet\\Control\\CI\\Config then fully power off and restart"),
            KDMapperError::InvalidDriverPath => write!(f, "Invalid driver path"),
            KDMapperError::MemoryAllocationFailed => write!(f, "Memory allocation failed"),
            KDMapperError::MemoryReadFailed => write!(f, "Memory read failed"),
            KDMapperError::MemoryWriteFailed => write!(f, "Memory write failed"),
            KDMapperError::ShellcodeExecutionFailed => write!(f, "Shellcode execution failed"),
            KDMapperError::InvalidAddress => write!(f, "Invalid address"),
            KDMapperError::PermissionDenied => write!(f, "Permission denied"),
            KDMapperError::Timeout => write!(f, "Operation timeout"),
            KDMapperError::InvalidPEImage => write!(f, "Invalid PE image"),
            KDMapperError::RelocationFailed => write!(f, "Relocation failed"),
            KDMapperError::ImportResolutionFailed => write!(f, "Import resolution failed"),
            KDMapperError::DriverEntryFailed => write!(f, "Driver entry point failed"),
            KDMapperError::DriverEntryNtStatus(s) => write!(f, "Driver entry point failed with NTSTATUS 0x{:08X}", s),
            KDMapperError::Unknown(msg) => write!(f, "Unknown error: {}", msg),
        }
    }
}

impl std::error::Error for KDMapperError {}

/// KDMapper result type
pub type Result<T> = std::result::Result<T, KDMapperError>;

/// Pool type for kernel memory allocation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum PoolType {
    NonPagedPool = 0,
    NonPagedPoolExecute = 1,
    PagedPool = 2,
    NonPagedPoolMustSucceed = 3,
    NonPagedPoolNx = 13,
}

/// Driver mapping configuration
///
/// # Driverless Driver Requirements
///
/// Drivers loaded via kdmapper run outside the normal Windows driver model.
/// They are NOT registered in `PsLoadedModulesList`, so PatchGuard actively
/// monitors them. Violating any of the rules below causes BSOD (usually 0x109
/// `CRITICAL_STRUCTURE_CORRUPTION` or 0x139 `KERNEL_SECURITY_CHECK_FAILURE`):
///
/// | Rule | Reason |
/// |------|--------|
/// | `DriverEntry` must return immediately | PatchGuard scans during prolonged execution |
/// | No `DriverObject` / `RegistryPath` access | Both are `NULL` when loaded via kdmapper |
/// | No kernel callback registration | Callbacks (PsSetCreateProcessNotifyRoutine, ObRegisterCallbacks …) point outside `PsLoadedModulesList` → BSOD 0x109 |
/// | No SEH (`__try`/`__except`) | Mapped image lacks an exception directory |
/// | No `DriverUnload` | Not supported; cleanup must be manual |
/// | Long-running work → system thread | Use `PsCreateSystemThread`; never block in `DriverEntry` |
#[derive(Debug, Clone)]
pub struct DriverMappingConfig {
    /// Path to the target driver (.sys file).
    /// The driver must follow the driverless conventions described above.
    pub driver_path: String,

    /// Path to the Intel vulnerable driver (default: "iqvw64e.sys").
    /// On Windows 11 24H2+ this driver may be blocked by the Vulnerable Driver
    /// Blocklist. Check `KDMapperExecutor::initialize` for the pre-flight error.
    pub intel_driver_path: String,

    /// Custom initialization shellcode
    pub init_shellcode: Option<Vec<u8>>,

    /// Erase PE headers after loading to reduce forensic visibility
    pub erase_headers: bool,

    /// Timeout in milliseconds
    pub timeout_ms: u32,
}

impl Default for DriverMappingConfig {
    fn default() -> Self {
        Self {
            driver_path: String::new(),
            intel_driver_path: "iqvw64e.sys".to_string(),
            init_shellcode: None,
            erase_headers: true,
            timeout_ms: 5000,
        }
    }
}

/// Driver mapping result
#[derive(Debug, Clone)]
pub struct DriverMappingResult {
    /// Base address of the loaded driver in kernel memory
    pub base_address: u64,

    /// Size of the driver image
    pub image_size: u64,

    /// Entry point address
    pub entry_point: u64,

    /// DriverEntry return status (NTSTATUS)
    pub entry_status: u32,

    /// Success flag
    pub success: bool,
}

/// Memory operation result
#[derive(Debug, Clone)]
pub struct MemoryOperationResult {
    /// Number of bytes processed
    pub bytes_processed: usize,
    /// Success flag
    pub success: bool,
}

/// Kernel module information
#[derive(Debug, Clone)]
pub struct KernelModuleInfo {
    /// Module name
    pub name: String,

    /// Base address
    pub base_address: u64,

    /// Module size
    pub size: u64,
}

/// FFI declarations for KDMapper C++ interface
#[repr(C)]
pub struct FFIHandle {
    _private: [u8; 0],
}

/// Opaque handle to the KDMapper device
pub type KDMapperHandle = NonNull<FFIHandle>;

/// KDMapper executor - main interface for kernel operations
pub struct KDMapperExecutor {
    handle: Option<KDMapperHandle>,
    loaded_drivers: Vec<String>,
    intel_driver_loaded: bool,
}

unsafe impl Send for KDMapperExecutor {}
unsafe impl Sync for KDMapperExecutor {}

// ============================================================================
// FFI Declarations (linked to kdmapper_cpp library)
// ============================================================================

// When kdmapper-native feature is enabled, use dynamic loading via libloading
#[cfg(feature = "kdmapper-native")]
mod dynamic_ffi {
    use super::FFIHandle;
    use libloading::{Library, Symbol};
    use std::ffi::c_char;
    use std::path::Path;

    // Function pointer types
    type FnIsRunning = unsafe extern "C" fn() -> bool;
    type FnLoadIntelDriver = unsafe extern "C" fn(*const c_char) -> *mut FFIHandle;
    type FnUnloadIntelDriver = unsafe extern "C" fn(*mut FFIHandle);
    type FnReadMemory = unsafe extern "C" fn(*mut FFIHandle, u64, *mut u8, u64) -> bool;
    type FnWriteMemory = unsafe extern "C" fn(*mut FFIHandle, u64, *const u8, u64) -> bool;
    type FnSetMemory = unsafe extern "C" fn(*mut FFIHandle, u64, u32, u64) -> bool;
    type FnAllocatePool = unsafe extern "C" fn(*mut FFIHandle, u32, u64) -> u64;
    type FnFreePool = unsafe extern "C" fn(*mut FFIHandle, u64) -> bool;
    type FnMapDriver = unsafe extern "C" fn(*mut FFIHandle, *const c_char, *mut u64, *mut u64, *mut u64, *mut u32) -> bool;
    type FnExecuteShellcode = unsafe extern "C" fn(*mut FFIHandle, *const u8, u32, u32, *mut u64) -> bool;
    type FnGetModuleBase = unsafe extern "C" fn(*mut FFIHandle, *const c_char) -> u64;
    type FnGetModuleExport = unsafe extern "C" fn(*mut FFIHandle, u64, *const c_char) -> u64;
    type FnClearUnloadedDrivers = unsafe extern "C" fn(*mut FFIHandle) -> bool;
    type FnCheckBlocklist = unsafe extern "C" fn() -> bool;
    type FnGetLastError  = unsafe extern "C" fn() -> *const std::ffi::c_char;

    pub struct KDMapperLib {
        _library: Library,
    }

    impl KDMapperLib {
        pub fn load() -> Result<Self, String> {
            // Try multiple possible locations for the DLL
            let dll_paths = vec![
                "kdmapper_cpp.dll",
                "../kdmapper_cpp/kdmapper_cpp.dll",
                "./kdmapper_cpp.dll",
            ];

            let mut last_error = String::new();

            for dll_path in dll_paths {
                match unsafe { Library::new(dll_path) } {
                    Ok(lib) => {
                        return Ok(KDMapperLib { _library: lib });
                    }
                    Err(e) => {
                        last_error = format!("{}: {}", dll_path, e);
                    }
                }
            }

            Err(format!("Failed to load kdmapper_cpp.dll. Tried: {}", last_error))
        }

        pub unsafe fn is_running(&self) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnIsRunning> = lib.get(b"kdmapper_is_running")
                .map_err(|e| format!("Failed to get kdmapper_is_running: {}", e))?;
            Ok(func())
        }

        pub unsafe fn load_intel_driver(&self, driver_path: *const c_char) -> Result<*mut FFIHandle, String> {
            let lib = &self._library;
            let func: Symbol<FnLoadIntelDriver> = lib.get(b"kdmapper_load_intel_driver")
                .map_err(|e| format!("Failed to get kdmapper_load_intel_driver: {}", e))?;
            Ok(func(driver_path))
        }

        pub unsafe fn unload_intel_driver(&self, handle: *mut FFIHandle) -> Result<(), String> {
            let lib = &self._library;
            let func: Symbol<FnUnloadIntelDriver> = lib.get(b"kdmapper_unload_intel_driver")
                .map_err(|e| format!("Failed to get kdmapper_unload_intel_driver: {}", e))?;
            func(handle);
            Ok(())
        }

        pub unsafe fn read_memory(&self, handle: *mut FFIHandle, address: u64, buffer: *mut u8, size: u64) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnReadMemory> = lib.get(b"kdmapper_read_memory")
                .map_err(|e| format!("Failed to get kdmapper_read_memory: {}", e))?;
            Ok(func(handle, address, buffer, size))
        }

        pub unsafe fn write_memory(&self, handle: *mut FFIHandle, address: u64, buffer: *const u8, size: u64) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnWriteMemory> = lib.get(b"kdmapper_write_memory")
                .map_err(|e| format!("Failed to get kdmapper_write_memory: {}", e))?;
            Ok(func(handle, address, buffer, size))
        }

        pub unsafe fn set_memory(&self, handle: *mut FFIHandle, address: u64, value: u32, size: u64) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnSetMemory> = lib.get(b"kdmapper_set_memory")
                .map_err(|e| format!("Failed to get kdmapper_set_memory: {}", e))?;
            Ok(func(handle, address, value, size))
        }

        pub unsafe fn allocate_pool(&self, handle: *mut FFIHandle, pool_type: u32, size: u64) -> Result<u64, String> {
            let lib = &self._library;
            let func: Symbol<FnAllocatePool> = lib.get(b"kdmapper_allocate_pool")
                .map_err(|e| format!("Failed to get kdmapper_allocate_pool: {}", e))?;
            Ok(func(handle, pool_type, size))
        }

        pub unsafe fn free_pool(&self, handle: *mut FFIHandle, address: u64) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnFreePool> = lib.get(b"kdmapper_free_pool")
                .map_err(|e| format!("Failed to get kdmapper_free_pool: {}", e))?;
            Ok(func(handle, address))
        }

        pub unsafe fn map_driver(&self, handle: *mut FFIHandle, driver_path: *const c_char,
            out_base_address: *mut u64, out_image_size: *mut u64, out_entry_point: *mut u64, out_status: *mut u32
        ) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnMapDriver> = lib.get(b"kdmapper_map_driver")
                .map_err(|e| format!("Failed to get kdmapper_map_driver: {}", e))?;
            Ok(func(handle, driver_path, out_base_address, out_image_size, out_entry_point, out_status))
        }

        pub unsafe fn execute_shellcode(&self, handle: *mut FFIHandle, shellcode: *const u8, shellcode_size: u32, timeout_ms: u32, out_result: *mut u64) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnExecuteShellcode> = lib.get(b"kdmapper_execute_shellcode")
                .map_err(|e| format!("Failed to get kdmapper_execute_shellcode: {}", e))?;
            Ok(func(handle, shellcode, shellcode_size, timeout_ms, out_result))
        }

        pub unsafe fn get_module_base(&self, handle: *mut FFIHandle, module_name: *const c_char) -> Result<u64, String> {
            let lib = &self._library;
            let func: Symbol<FnGetModuleBase> = lib.get(b"kdmapper_get_module_base")
                .map_err(|e| format!("Failed to get kdmapper_get_module_base: {}", e))?;
            Ok(func(handle, module_name))
        }

        pub unsafe fn get_module_export(&self, handle: *mut FFIHandle, module_base: u64, function_name: *const c_char) -> Result<u64, String> {
            let lib = &self._library;
            let func: Symbol<FnGetModuleExport> = lib.get(b"kdmapper_get_module_export")
                .map_err(|e| format!("Failed to get kdmapper_get_module_export: {}", e))?;
            Ok(func(handle, module_base, function_name))
        }

        pub unsafe fn clear_unloaded_drivers(&self, handle: *mut FFIHandle) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnClearUnloadedDrivers> = lib.get(b"kdmapper_clear_unloaded_drivers")
                .map_err(|e| format!("Failed to get kdmapper_clear_unloaded_drivers: {}", e))?;
            Ok(func(handle))
        }

        pub unsafe fn check_blocklist(&self) -> Result<bool, String> {
            let lib = &self._library;
            let func: Symbol<FnCheckBlocklist> = lib.get(b"kdmapper_check_blocklist")
                .map_err(|e| format!("Failed to get kdmapper_check_blocklist: {}", e))?;
            Ok(func())
        }

        pub unsafe fn get_last_error(&self) -> String {
            let lib = &self._library;
            match lib.get::<FnGetLastError>(b"kdmapper_get_last_error") {
                Ok(func) => {
                    let ptr = func();
                    if ptr.is_null() {
                        return String::new();
                    }
                    std::ffi::CStr::from_ptr(ptr)
                        .to_string_lossy()
                        .into_owned()
                }
                Err(_) => String::new(),
            }
        }
    }

    // Global library instance (lazy loaded)
    use std::sync::Mutex;
    static LIBRARY: Mutex<Option<KDMapperLib>> = Mutex::new(None);

    pub fn get_library() -> Result<&'static Mutex<Option<KDMapperLib>>, String> {
        let mut lib = LIBRARY.lock().unwrap();
        if lib.is_none() {
            *lib = Some(KDMapperLib::load()?);
        }
        Ok(&LIBRARY)
    }

    // Wrapper functions that use the dynamic library
    pub unsafe fn kdmapper_is_running() -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.is_running().unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub unsafe fn kdmapper_load_intel_driver(driver_path: *const c_char) -> *mut FFIHandle {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.load_intel_driver(driver_path).unwrap_or(std::ptr::null_mut())
                } else {
                    std::ptr::null_mut()
                }
            }
            Err(_) => std::ptr::null_mut(),
        }
    }

    pub unsafe fn kdmapper_unload_intel_driver(handle: *mut FFIHandle) {
        if let Ok(lib) = get_library() {
            let lib_guard = lib.lock().unwrap();
            if let Some(ref l) = *lib_guard {
                let _ = l.unload_intel_driver(handle);
            }
        }
    }

    pub unsafe fn kdmapper_read_memory(handle: *mut FFIHandle, address: u64, buffer: *mut u8, size: u64) -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.read_memory(handle, address, buffer, size).unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub unsafe fn kdmapper_write_memory(handle: *mut FFIHandle, address: u64, buffer: *const u8, size: u64) -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.write_memory(handle, address, buffer, size).unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub unsafe fn kdmapper_set_memory(handle: *mut FFIHandle, address: u64, value: u32, size: u64) -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.set_memory(handle, address, value, size).unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub unsafe fn kdmapper_allocate_pool(handle: *mut FFIHandle, pool_type: u32, size: u64) -> u64 {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.allocate_pool(handle, pool_type, size).unwrap_or(0)
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    }

    pub unsafe fn kdmapper_free_pool(handle: *mut FFIHandle, address: u64) -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.free_pool(handle, address).unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub unsafe fn kdmapper_map_driver(handle: *mut FFIHandle, driver_path: *const c_char,
        out_base_address: *mut u64, out_image_size: *mut u64, out_entry_point: *mut u64, out_status: *mut u32
    ) -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.map_driver(handle, driver_path, out_base_address, out_image_size, out_entry_point, out_status).unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub unsafe fn kdmapper_execute_shellcode(handle: *mut FFIHandle, shellcode: *const u8, shellcode_size: u32, timeout_ms: u32, out_result: *mut u64) -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.execute_shellcode(handle, shellcode, shellcode_size, timeout_ms, out_result).unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub unsafe fn kdmapper_get_module_base(handle: *mut FFIHandle, module_name: *const c_char) -> u64 {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.get_module_base(handle, module_name).unwrap_or(0)
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    }

    pub unsafe fn kdmapper_get_module_export(handle: *mut FFIHandle, module_base: u64, function_name: *const c_char) -> u64 {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.get_module_export(handle, module_base, function_name).unwrap_or(0)
                } else {
                    0
                }
            }
            Err(_) => 0,
        }
    }

    pub unsafe fn kdmapper_clear_unloaded_drivers(handle: *mut FFIHandle) -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.clear_unloaded_drivers(handle).unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    pub unsafe fn kdmapper_check_blocklist() -> bool {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    l.check_blocklist().unwrap_or(false)
                } else {
                    false
                }
            }
            Err(_) => false,
        }
    }

    /// Returns the last error string from the C++ layer, or empty string.
    pub fn kdmapper_cpp_last_error() -> String {
        match get_library() {
            Ok(lib) => {
                let lib_guard = lib.lock().unwrap();
                if let Some(ref l) = *lib_guard {
                    unsafe { l.get_last_error() }
                } else {
                    String::new()
                }
            }
            Err(_) => String::new(),
        }
    }
}

// Re-export dynamic_ffi functions for kdmapper-native mode
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_is_running;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_load_intel_driver;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_unload_intel_driver;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_read_memory;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_write_memory;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_set_memory;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_allocate_pool;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_free_pool;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_map_driver;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_execute_shellcode;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_get_module_base;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_get_module_export;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_clear_unloaded_drivers;
#[cfg(feature = "kdmapper-native")]
pub use dynamic_ffi::kdmapper_check_blocklist;

// Mock implementations for when kdmapper-native is NOT enabled
// These are Rust functions that replace the C++ library for testing
#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_is_running() -> bool {
    false
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_load_intel_driver(_driver_path: *const c_char) -> *mut FFIHandle {
    std::ptr::null_mut()
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_unload_intel_driver(_handle: *mut FFIHandle) {}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_read_memory(
    _handle: *mut FFIHandle,
    _address: u64,
    _buffer: *mut u8,
    _size: u64,
) -> bool {
    false
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_write_memory(
    _handle: *mut FFIHandle,
    _address: u64,
    _buffer: *const u8,
    _size: u64,
) -> bool {
    false
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_allocate_pool(
    _handle: *mut FFIHandle,
    _pool_type: u32,
    _size: u64,
) -> u64 {
    0
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_free_pool(
    _handle: *mut FFIHandle,
    _address: u64,
) -> bool {
    false
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_map_driver(
    _handle: *mut FFIHandle,
    _driver_path: *const c_char,
    _out_base_address: *mut u64,
    _out_image_size: *mut u64,
    _out_entry_point: *mut u64,
    _out_status: *mut u32,
) -> bool {
    false
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_execute_shellcode(
    _handle: *mut FFIHandle,
    _shellcode: *const u8,
    _shellcode_size: u32,
    _timeout_ms: u32,
    _out_result: *mut u64,
) -> bool {
    false
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_get_module_base(
    _handle: *mut FFIHandle,
    _module_name: *const c_char,
) -> u64 {
    0
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_get_module_export(
    _handle: *mut FFIHandle,
    _module_base: u64,
    _function_name: *const c_char,
) -> u64 {
    0
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_set_memory(
    _handle: *mut FFIHandle,
    _address: u64,
    _value: u32,
    _size: u64,
) -> bool {
    false
}

#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_clear_unloaded_drivers(_handle: *mut FFIHandle) -> bool {
    false
}

/// Mock-mode blocklist check: reads the registry directly in Rust so the
/// check works even without kdmapper_cpp.dll loaded.
#[cfg(not(feature = "kdmapper-native"))]
#[no_mangle]
pub extern "C" fn kdmapper_check_blocklist() -> bool {
    check_blocklist_registry()
}

/// In mock mode there is no C++ layer, so this always returns empty.
#[cfg(not(feature = "kdmapper-native"))]
fn kdmapper_cpp_last_error() -> String {
    String::new()
}

// ============================================================================
// Rust-native environment checks (no DLL required)
// ============================================================================

/// Read VulnerableDriverBlocklistEnable from the registry using raw Win32 FFI.
/// Returns true if the blocklist is enabled (iqvw64e.sys will be rejected).
#[cfg(target_os = "windows")]
fn check_blocklist_registry() -> bool {
    #[link(name = "Advapi32")]
    extern "system" {
        fn RegOpenKeyExA(hkey: isize, subkey: *const u8, options: u32, desired: u32, result: *mut isize) -> i32;
        fn RegQueryValueExA(hkey: isize, value: *const u8, reserved: *mut u32, reg_type: *mut u32, data: *mut u8, data_len: *mut u32) -> i32;
        fn RegCloseKey(hkey: isize) -> i32;
    }
    const HKLM: isize = -2147483646_i64 as isize; // 0x80000002
    const KEY_READ: u32 = 0x20019;

    let subkey  = b"SYSTEM\\CurrentControlSet\\Control\\CI\\Config\0";
    let valname = b"VulnerableDriverBlocklistEnable\0";

    unsafe {
        let mut hkey: isize = 0;
        if RegOpenKeyExA(HKLM, subkey.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return false; // key absent → blocklist not configured
        }
        let mut value: u32 = 0;
        let mut size: u32  = std::mem::size_of::<u32>() as u32;
        let mut kind: u32  = 0;
        let ret = RegQueryValueExA(
            hkey,
            valname.as_ptr(),
            std::ptr::null_mut(),
            &mut kind,
            &mut value as *mut u32 as *mut u8,
            &mut size,
        );
        RegCloseKey(hkey);
        ret == 0 && value != 0
    }
}

#[cfg(not(target_os = "windows"))]
fn check_blocklist_registry() -> bool {
    false // non-Windows: no blocklist
}

// ============================================================================
// KDMapperExecutor Implementation
// ============================================================================

impl KDMapperExecutor {
    /// Create a new KDMapper executor
    pub fn new() -> Self {
        Self {
            handle: None,
            loaded_drivers: Vec::new(),
            intel_driver_loaded: false,
        }
    }

    /// Initialize the KDMapper executor and load the Intel driver
    pub fn initialize(&mut self, intel_driver_path: Option<&str>) -> Result<()> {
        if self.intel_driver_loaded {
            return Ok(());
        }

        // Pre-flight: Windows Vulnerable Driver Blocklist check.
        // On Win11 24H2+ this blocks iqvw64e.sys with STATUS_IMAGE_CERT_REVOKED.
        // We check in Rust so we get a clear error even without the DLL loaded.
        if check_blocklist_registry() {
            return Err(KDMapperError::BlocklistEnabled);
        }

        // Check if already running
        unsafe {
            if kdmapper_is_running() {
                return Err(KDMapperError::DriverAlreadyRunning);
            }
        }

        let driver_path = CString::new(
            intel_driver_path.unwrap_or("iqvw64e.sys")
        ).map_err(|_| KDMapperError::InvalidDriverPath)?;

        unsafe {
            let handle = kdmapper_load_intel_driver(driver_path.as_ptr());

            if handle.is_null() {
                let cpp_err = kdmapper_cpp_last_error();
                if cpp_err.is_empty() {
                    return Err(KDMapperError::DriverLoadFailed);
                }
                return Err(KDMapperError::Unknown(
                    format!("Intel driver load failed: {}", cpp_err)
                ));
            }

            self.handle = NonNull::new(handle);
            self.intel_driver_loaded = true;
        }

        Ok(())
    }

    /// Ensure the Intel driver is loaded
    fn ensure_initialized(&self) -> Result<()> {
        if !self.intel_driver_loaded || self.handle.is_none() {
            Err(KDMapperError::DriverLoadFailed)
        } else {
            Ok(())
        }
    }

    /// Map a driver into kernel memory
    pub fn map_driver(&mut self, config: DriverMappingConfig) -> Result<DriverMappingResult> {
        self.ensure_initialized()?;

        let driver_path = CString::new(config.driver_path.as_str())
            .map_err(|_| KDMapperError::InvalidDriverPath)?;

        if !Path::new(&config.driver_path).exists() {
            return Err(KDMapperError::InvalidDriverPath);
        }

        let mut result = DriverMappingResult {
            base_address: 0,
            image_size: 0,
            entry_point: 0,
            entry_status: 0,
            success: false,
        };

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let success = kdmapper_map_driver(
                handle,
                driver_path.as_ptr(),
                &mut result.base_address,
                &mut result.image_size,
                &mut result.entry_point,
                &mut result.entry_status,
            );

            result.success = success && result.base_address != 0;

            if success {
                self.loaded_drivers.push(config.driver_path.clone());
            } else {
                // If DriverEntry returned a non-zero NTSTATUS, surface it directly.
                if result.entry_status != 0 {
                    return Err(KDMapperError::DriverEntryNtStatus(result.entry_status));
                }
                // Otherwise attach the C++ error string for diagnosis.
                let cpp_err = kdmapper_cpp_last_error();
                if cpp_err.is_empty() {
                    return Err(KDMapperError::DriverEntryFailed);
                }
                return Err(KDMapperError::Unknown(
                    format!("Driver mapping failed: {}", cpp_err)
                ));
            }
        }

        Ok(result)
    }

    /// Read kernel memory
    pub fn read_kernel_memory(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.ensure_initialized()?;

        if address == 0 || size == 0 {
            return Err(KDMapperError::InvalidAddress);
        }

        let mut buffer = vec![0u8; size];

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let success = kdmapper_read_memory(handle, address, buffer.as_mut_ptr(), size as u64);

            if !success {
                return Err(KDMapperError::MemoryReadFailed);
            }
        }

        Ok(buffer)
    }

    /// Write kernel memory
    pub fn write_kernel_memory(&self, address: u64, data: &[u8]) -> Result<MemoryOperationResult> {
        self.ensure_initialized()?;

        if address == 0 || data.is_empty() {
            return Err(KDMapperError::InvalidAddress);
        }

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let success = kdmapper_write_memory(handle, address, data.as_ptr(), data.len() as u64);

            Ok(MemoryOperationResult {
                bytes_processed: if success { data.len() } else { 0 },
                success,
            })
        }
    }

    /// Set kernel memory to a specific value
    pub fn set_kernel_memory(&self, address: u64, value: u32, size: u64) -> Result<()> {
        self.ensure_initialized()?;

        if address == 0 || size == 0 {
            return Err(KDMapperError::InvalidAddress);
        }

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let success = kdmapper_set_memory(handle, address, value, size);

            if !success {
                return Err(KDMapperError::MemoryWriteFailed);
            }
        }

        Ok(())
    }

    /// Allocate kernel memory pool
    pub fn allocate_kernel_pool(&self, pool_type: PoolType, size: u64) -> Result<u64> {
        self.ensure_initialized()?;

        if size == 0 {
            return Err(KDMapperError::MemoryAllocationFailed);
        }

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let address = kdmapper_allocate_pool(handle, pool_type as u32, size);

            if address == 0 {
                Err(KDMapperError::MemoryAllocationFailed)
            } else {
                Ok(address)
            }
        }
    }

    /// Free kernel memory pool
    pub fn free_kernel_pool(&self, address: u64) -> Result<()> {
        self.ensure_initialized()?;

        if address == 0 {
            return Ok(());
        }

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let success = kdmapper_free_pool(handle, address);

            if !success {
                Err(KDMapperError::MemoryWriteFailed)
            } else {
                Ok(())
            }
        }
    }

    /// Execute shellcode in kernel context
    pub fn execute_shellcode(&self, shellcode: &[u8], timeout_ms: u32) -> Result<u64> {
        self.ensure_initialized()?;

        if shellcode.is_empty() {
            return Err(KDMapperError::ShellcodeExecutionFailed);
        }

        let mut result = 0u64;

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let success = kdmapper_execute_shellcode(
                handle,
                shellcode.as_ptr(),
                shellcode.len() as u32,
                timeout_ms,
                &mut result,
            );

            if !success {
                return Err(KDMapperError::ShellcodeExecutionFailed);
            }
        }

        Ok(result)
    }

    /// Get kernel module base address
    pub fn get_module_base(&self, module_name: &str) -> Result<u64> {
        self.ensure_initialized()?;

        let name = CString::new(module_name)
            .map_err(|_| KDMapperError::InvalidDriverPath)?;

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let address = kdmapper_get_module_base(handle, name.as_ptr());

            if address == 0 {
                Err(KDMapperError::InvalidAddress)
            } else {
                Ok(address)
            }
        }
    }

    /// Get kernel module export address
    pub fn get_module_export(&self, module_base: u64, function_name: &str) -> Result<u64> {
        self.ensure_initialized()?;

        if module_base == 0 {
            return Err(KDMapperError::InvalidAddress);
        }

        let name = CString::new(function_name)
            .map_err(|_| KDMapperError::InvalidDriverPath)?;

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            let address = kdmapper_get_module_export(handle, module_base, name.as_ptr());

            if address == 0 {
                Err(KDMapperError::InvalidAddress)
            } else {
                Ok(address)
            }
        }
    }

    /// Clear the MmUnloadedDrivers list (hide traces of loaded drivers)
    pub fn clear_unloaded_drivers(&self) -> Result<()> {
        self.ensure_initialized()?;

        unsafe {
            let handle = self.handle.unwrap().as_ptr();
            kdmapper_clear_unloaded_drivers(handle);
        }

        Ok(())
    }

    /// Check if the executor is initialized
    pub fn is_initialized(&self) -> bool {
        self.intel_driver_loaded && self.handle.is_some()
    }

    /// Get list of loaded drivers
    pub fn loaded_drivers(&self) -> &[String] {
        &self.loaded_drivers
    }
}

impl Default for KDMapperExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for KDMapperExecutor {
    fn drop(&mut self) {
        if let Some(handle) = self.handle {
            unsafe {
                kdmapper_unload_intel_driver(handle.as_ptr());
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        assert_eq!(format!("{}", KDMapperError::DriverLoadFailed), "Failed to load driver");
        assert_eq!(format!("{}", KDMapperError::InvalidAddress), "Invalid address");
        assert!(format!("{}", KDMapperError::BlocklistEnabled).contains("VulnerableDriverBlocklistEnable"));
        assert_eq!(
            format!("{}", KDMapperError::DriverEntryNtStatus(0xC0000034)),
            "Driver entry point failed with NTSTATUS 0xC0000034"
        );
        assert_eq!(
            format!("{}", KDMapperError::Unknown("test".into())),
            "Unknown error: test"
        );
    }

    #[test]
    fn test_check_blocklist_mock() {
        // In mock mode (no kdmapper-native feature) kdmapper_check_blocklist()
        // reads the registry; on a test machine the value is typically absent or 0.
        // We just assert it doesn't panic and returns a bool.
        let _result: bool = unsafe { kdmapper_check_blocklist() };
    }

    #[test]
    fn test_cpp_last_error_mock() {
        // In mock mode kdmapper_cpp_last_error() always returns empty string.
        assert_eq!(kdmapper_cpp_last_error(), "");
    }

    #[test]
    fn test_config_default() {
        let config = DriverMappingConfig::default();
        assert_eq!(config.intel_driver_path, "iqvw64e.sys");
        assert!(config.erase_headers);
        assert_eq!(config.timeout_ms, 5000);
    }

    #[test]
    fn test_executor_creation() {
        let executor = KDMapperExecutor::new();
        assert!(!executor.is_initialized());
        assert_eq!(executor.loaded_drivers().len(), 0);
    }

    #[test]
    fn test_pool_type_values() {
        assert_eq!(PoolType::NonPagedPool as u32, 0);
        assert_eq!(PoolType::PagedPool as u32, 2);
        assert_eq!(PoolType::NonPagedPoolNx as u32, 13);
    }
}
