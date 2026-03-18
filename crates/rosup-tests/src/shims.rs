//! Helpers for locating shim binaries.

use std::path::PathBuf;

/// Directory containing the shim binaries (target/debug/ or target/release/).
///
/// Relies on the shims crate being built as part of the same workspace,
/// which nextest ensures before running any tests.
pub fn shim_dir() -> PathBuf {
    assert_cmd::cargo::cargo_bin("colcon")
        .parent()
        .expect("colcon binary has no parent dir")
        .to_owned()
}
