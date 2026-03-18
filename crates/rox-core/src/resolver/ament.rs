use std::path::Path;

/// Check whether `pkg_name` is installed in any prefix on `AMENT_PREFIX_PATH`.
pub fn is_installed(pkg_name: &str) -> bool {
    let prefix_path = std::env::var("AMENT_PREFIX_PATH").unwrap_or_default();
    is_installed_in(&prefix_path, pkg_name)
}

/// Inner function that takes the prefix path string for testability.
pub fn is_installed_in(prefix_path: &str, pkg_name: &str) -> bool {
    if prefix_path.is_empty() {
        return false;
    }
    prefix_path.split(':').any(|prefix| {
        Path::new(prefix)
            .join("share/ament_index/resource_index/packages")
            .join(pkg_name)
            .exists()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_ament_prefix(pkg_name: &str) -> TempDir {
        let tmp = TempDir::new().unwrap();
        let pkg_dir = tmp.path().join("share/ament_index/resource_index/packages");
        fs::create_dir_all(&pkg_dir).unwrap();
        fs::write(pkg_dir.join(pkg_name), "").unwrap();
        tmp
    }

    #[test]
    fn detects_installed_package() {
        let prefix = make_ament_prefix("rclcpp");
        assert!(is_installed_in(
            &prefix.path().display().to_string(),
            "rclcpp"
        ));
    }

    #[test]
    fn returns_false_for_missing_package() {
        let prefix = make_ament_prefix("rclcpp");
        assert!(!is_installed_in(
            &prefix.path().display().to_string(),
            "sensor_msgs"
        ));
    }

    #[test]
    fn checks_multiple_prefixes() {
        let p1 = make_ament_prefix("rclcpp");
        let p2 = make_ament_prefix("sensor_msgs");
        let prefix_path = format!("{}:{}", p1.path().display(), p2.path().display());
        assert!(is_installed_in(&prefix_path, "rclcpp"));
        assert!(is_installed_in(&prefix_path, "sensor_msgs"));
        assert!(!is_installed_in(&prefix_path, "nav2_core"));
    }

    #[test]
    fn returns_false_for_empty_prefix_path() {
        assert!(!is_installed_in("", "rclcpp"));
    }

    #[test]
    fn detects_real_humble_packages() {
        // Verify against the actual humble install if present.
        let humble = "/opt/ros/humble";
        if !std::path::Path::new(humble).exists() {
            return;
        }
        assert!(is_installed_in(humble, "rclcpp"));
        assert!(is_installed_in(humble, "ament_cmake"));
        assert!(!is_installed_in(humble, "nonexistent_package_xyz"));
    }
}
