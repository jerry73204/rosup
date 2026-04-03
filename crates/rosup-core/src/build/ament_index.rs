//! Ament index marker generation.
//!
//! For `ament_python` packages (where pip doesn't generate ament markers),
//! this writes the ament index package marker so `ament_index_python` can
//! discover the package. For `ament_cmake` packages, cmake --install
//! generates this automatically.

use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum AmentIndexError {
    #[error("failed to write {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

fn write_file(path: &Path, content: &str) -> Result<(), AmentIndexError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AmentIndexError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    std::fs::write(path, content).map_err(|e| AmentIndexError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

/// Write the ament index marker for a package.
///
/// Creates `<install>/share/ament_index/resource_index/packages/<name>`
/// (empty file). This is how `ament_index_python` discovers installed
/// packages.
pub fn write_package_marker(install_base: &Path, pkg_name: &str) -> Result<(), AmentIndexError> {
    let marker = install_base
        .join("share/ament_index/resource_index/packages")
        .join(pkg_name);
    write_file(&marker, "")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn writes_package_marker() {
        let tmp = TempDir::new().unwrap();
        write_package_marker(tmp.path(), "my_pkg").unwrap();
        let marker = tmp
            .path()
            .join("share/ament_index/resource_index/packages/my_pkg");
        assert!(marker.exists());
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "");
    }
}
