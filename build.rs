//! Build script for URP runtime
//!
//! This build script handles conditional compilation of the KDMapper C++ wrapper.

use std::env;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Check if kdmapper-native feature is enabled
    let kdmapper_native = env::var("CARGO_FEATURE_KDMAPPER_NATIVE").is_ok();

    if kdmapper_native {
        println!("cargo:warning=KDMapper native feature enabled - using dynamic DLL loading");
        println!("cargo:warning=Ensure kdmapper_cpp.dll is in the search path");
        // No static linking needed - we use libloading for dynamic loading
    }
}
