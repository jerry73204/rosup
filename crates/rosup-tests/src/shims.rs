//! Helpers for locating shim binaries.

use std::path::PathBuf;

/// Directory containing the shim binaries (`target/debug/`).
///
/// Nextest only sets `CARGO_BIN_EXE_*` for packages it compiles as test
/// targets. The `shims` crate has no tests, so we derive the path from
/// `CARGO_MANIFEST_DIR` instead.
pub fn shim_dir() -> PathBuf {
    // CARGO_MANIFEST_DIR is crates/rosup-tests; go up two levels to the
    // workspace root, then into target/debug/.
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .ancestors()
        .nth(2)
        .expect("workspace root not found");
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.join("target"));
    target.join("debug")
}
