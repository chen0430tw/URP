//! KDMapper FFI Layer
//!
//! This module provides safe Rust bindings to KDMapper functionality.

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

extern "C" {
    /// Check if the Intel driver is already running
    fn kdmapper_is_running() -> bool;

    /// Load the Intel vulnerable driver
    /// Returns handle on success, null on failure
    fn kdmapper_load_intel_driver(driver_path: *const c_char) -> *mut FFIHandle;

    /// Unload the Intel driver
    fn kdmapper_unload_intel_driver(handle: *mut FFIHandle);

    /// Read kernel memory
    fn kdmapper_read_memory(
        handle: *mut FFIHandle,
        address: u64,
        buffer: *mut u8,
        size: u64,
    ) -> bool;

    /// Write kernel memory
    fn kdmapper_write_memory(
        handle: *mut FFIHandle,
        address: u64,
        buffer: *const u8,
        size: u64,
    ) -> bool;

    /// Allocate kernel memory pool
    fn kdmapper_allocate_pool(
        handle: *mut FFIHandle,
        pool_type: u32,
        size: u64,
    ) -> u64;

    /// Free kernel memory pool
    fn kdmapper_free_pool(
        handle: *mut FFIHandle,
        address: u64,
    ) -> bool;

    /// Map a driver into kernel memory
    fn kdmapper_map_driver(
        handle: *mut FFIHandle,
        driver_path: *const c_char,
        out_base_address: *mut u64,
        out_image_size: *mut u64,
        out_entry_point: *mut u64,
        out_status: *mut u32,
    ) -> bool;

    /// Execute shellcode in kernel context
    fn kdmapper_execute_shellcode(
        handle: *mut FFIHandle,
        shellcode: *const u8,
        shellcode_size: u32,
        timeout_ms: u32,
        out_result: *mut u64,
    ) -> bool;

    /// Get kernel module base address
    fn kdmapper_get_module_base(
        handle: *mut FFIHandle,
        module_name: *const c_char,
    ) -> u64;

    /// Get kernel module export address
    fn kdmapper_get_module_export(
        handle: *mut FFIHandle,
        module_base: u64,
        function_name: *const c_char,
    ) -> u64;

    /// Set memory to a specific value
    fn kdmapper_set_memory(
        handle: *mut FFIHandle,
        address: u64,
        value: u32,
        size: u64,
    ) -> bool;

    /// Clear the MmUnloadedDrivers list (hide traces)
    fn kdmapper_clear_unloaded_drivers(handle: *mut FFIHandle) -> bool;
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
