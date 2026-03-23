//! Ament index and colcon metadata generation.
//!
//! After a package is installed, these files must exist for other packages
//! and `ros2 run`/`ros2 launch` to find it.

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

/// Write the colcon runtime dependencies file.
///
/// Creates `<install>/share/colcon-core/packages/<name>` containing
/// colon-separated runtime dependency names. Used by colcon's prefix
/// scripts to source deps in topological order.
pub fn write_runtime_deps(
    install_base: &Path,
    pkg_name: &str,
    runtime_deps: &[String],
) -> Result<(), AmentIndexError> {
    let path = install_base
        .join("share/colcon-core/packages")
        .join(pkg_name);
    let content = runtime_deps.join(":");
    write_file(&path, &content)
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

    #[test]
    fn writes_runtime_deps() {
        let tmp = TempDir::new().unwrap();
        let deps = vec!["rclcpp".to_owned(), "std_msgs".to_owned()];
        write_runtime_deps(tmp.path(), "my_pkg", &deps).unwrap();
        let path = tmp.path().join("share/colcon-core/packages/my_pkg");
        assert!(path.exists());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "rclcpp:std_msgs");
    }

    #[test]
    fn writes_empty_runtime_deps() {
        let tmp = TempDir::new().unwrap();
        write_runtime_deps(tmp.path(), "standalone_pkg", &[]).unwrap();
        let path = tmp.path().join("share/colcon-core/packages/standalone_pkg");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }
}
