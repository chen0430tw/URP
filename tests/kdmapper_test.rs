//! KDMapper integration tests
//!
//! These tests verify the KDMapper FFI bindings work correctly.
//! NOTE: Most tests require administrator privileges to function.
//! Run with: cargo test --features kdmapper

#![cfg(feature = "kdmapper")]

use urx_runtime_v08::kdmapper_ffi::*;

#[test]
fn test_error_display() {
    // Test error messages display correctly
    assert_eq!(
        format!("{}", KDMapperError::DriverLoadFailed),
        "Failed to load driver"
    );
    assert_eq!(
        format!("{}", KDMapperError::InvalidAddress),
        "Invalid address"
    );
    assert_eq!(
        format!("{}", KDMapperError::PermissionDenied),
        "Permission denied"
    );
}

#[test]
fn test_pool_type_values() {
    // Test pool type enum values match Windows definitions
    assert_eq!(PoolType::NonPagedPool as u32, 0);
    assert_eq!(PoolType::NonPagedPoolExecute as u32, 1);
    assert_eq!(PoolType::PagedPool as u32, 2);
    assert_eq!(PoolType::NonPagedPoolMustSucceed as u32, 3);
    assert_eq!(PoolType::NonPagedPoolNx as u32, 13);
}

#[test]
fn test_config_default() {
    // Test default configuration
    let config = DriverMappingConfig::default();
    assert_eq!(config.intel_driver_path, "iqvw64e.sys");
    assert!(config.erase_headers);
    assert_eq!(config.timeout_ms, 5000);
}

#[test]
fn test_executor_creation() {
    // Test executor can be created
    let executor = KDMapperExecutor::new();
    assert!(!executor.is_initialized());
    assert_eq!(executor.loaded_drivers().len(), 0);
}

#[test]
fn test_result_structures() {
    // Test result structures can be created
    let result = DriverMappingResult {
        base_address: 0x1000,
        image_size: 0x2000,
        entry_point: 0x1100,
        entry_status: 0,
        success: true,
    };
    assert_eq!(result.base_address, 0x1000);
    assert!(result.success);

    let mem_result = MemoryOperationResult {
        bytes_processed: 64,
        success: true,
    };
    assert_eq!(mem_result.bytes_processed, 64);
}

#[test]
fn test_module_info() {
    // Test module info structure
    let info = KernelModuleInfo {
        name: "ntoskrnl.exe".to_string(),
        base_address: 0xFFFF800000000000,
        size: 0x100000,
    };
    assert_eq!(info.name, "ntoskrnl.exe");
    assert_eq!(info.base_address, 0xFFFF800000000000);
}

// NOTE: The following tests require administrator privileges
// They are marked as #[ignore] by default

#[test]
#[ignore]
fn test_is_running_requires_admin() {
    // This test checks if the vulnerable driver is already running
    // Requires admin to actually communicate with kernel
    let executor = KDMapperExecutor::new();
    // Just verify the structure compiles correctly
    assert!(!executor.is_initialized());
}

#[test]
#[ignore]
fn test_initialize_without_driver() {
    // Test initialization fails gracefully when driver file doesn't exist
    let mut executor = KDMapperExecutor::new();
    let result = executor.initialize(Some("nonexistent.sys"));
    assert!(result.is_err());
}
