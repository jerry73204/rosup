pub mod builder;
pub mod config;
pub mod init;
pub mod manifest;
pub mod new;
pub mod overlay;
pub mod package_xml;
pub mod project;
pub mod resolver;
pub mod runner;

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

    /// Recursively copy a fixture directory tree into a destination.
    ///
    /// `rel` is relative to `tests/fixtures/` (e.g. `"workspaces/sort_order"`).
    /// The *contents* of the fixture directory are copied into `dest_dir`.
    pub fn copy_fixture_dir(rel: &str, dest_dir: &Path) {
        let src = fixture_path(rel);
        assert!(
            src.is_dir(),
            "fixture dir {rel} not found at {}",
            src.display()
        );
        copy_dir_recursive(&src, dest_dir);
    }

    fn copy_dir_recursive(src: &Path, dest: &Path) {
        for entry in std::fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let ty = entry.file_type().unwrap();
            let target = dest.join(entry.file_name());
            if ty.is_dir() {
                std::fs::create_dir_all(&target).unwrap();
                copy_dir_recursive(&entry.path(), &target);
            } else {
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
}
