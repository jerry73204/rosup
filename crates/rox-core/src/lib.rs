pub mod config;
pub mod init;
pub mod manifest;
pub mod package_xml;
pub mod project;
pub mod resolver;

#[cfg(test)]
pub(crate) mod test_helpers {
    use std::path::{Path, PathBuf};

    /// Absolute path to a fixture file under `tests/fixtures/`.
    pub fn fixture_path(rel: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(rel)
    }

    /// Read a fixture file to a String.
    pub fn fixture(rel: &str) -> String {
        std::fs::read_to_string(fixture_path(rel)).unwrap_or_else(|e| panic!("fixture {rel}: {e}"))
    }

    /// Copy a fixture file into a tempdir and return the destination path.
    pub fn copy_fixture(rel: &str, dest_dir: &Path, dest_name: &str) -> PathBuf {
        let dest = dest_dir.join(dest_name);
        std::fs::write(&dest, fixture(rel)).unwrap_or_else(|e| panic!("copy_fixture {rel}: {e}"));
        dest
    }
}
