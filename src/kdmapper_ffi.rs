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
    DriverEntryFailed,
    Unknown(String),
}

impl std::fmt::Display for KDMapperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KDMapperError::DriverLoadFailed => write!(f, "Failed to load driver"),
            KDMapperError::DriverAlreadyRunning => write!(f, "Driver already running"),
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
#[derive(Debug, Clone)]
pub struct DriverMappingConfig {
    /// Path to the target driver (.sys file)
    pub driver_path: String,

    /// Path to the Intel vulnerable driver (default: "iqvw64e.sys")
    pub intel_driver_path: String,

    /// Custom initialization shellcode
    pub init_shellcode: Option<Vec<u8>>,

    /// Erase PE headers after loading
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
                return Err(KDMapperError::DriverLoadFailed);
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
                return Err(KDMapperError::DriverEntryFailed);
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
