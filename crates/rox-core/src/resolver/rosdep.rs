use std::process::Command;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RosdepError {
    #[error("rosdep not found — install it with: pip install rosdep")]
    NotFound,
    #[error("rosdep database not initialised — run: rosdep update")]
    NotInitialised,
    #[error("rosdep error: {0}")]
    Other(String),
    #[error("io error running rosdep: {0}")]
    Io(#[from] std::io::Error),
}

/// Result of resolving one rosdep key.
#[derive(Debug, Clone)]
pub struct RosdepResolution {
    /// Platform installer (e.g. "apt").
    pub installer: String,
    /// Platform packages (e.g. ["ros-humble-rclcpp"]).
    pub packages: Vec<String>,
}

/// Check whether rosdep can resolve `dep` for the given distro.
pub fn resolve(dep: &str, distro: &str) -> Result<RosdepResolution, RosdepError> {
    let out = rosdep_cmd()
        .args(["resolve", dep, "--rosdistro", distro])
        .env("ROS_PYTHON_VERSION", "3")
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RosdepError::NotFound
            } else {
                RosdepError::Io(e)
            }
        })?;

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);

    if !out.status.success() {
        if stderr.contains("No definition") || stderr.contains("no rosdep rule") {
            return Err(RosdepError::Other(format!("no rosdep rule for '{dep}'")));
        }
        if stderr.contains("rosdep update") || stderr.contains("not been initialized") {
            return Err(RosdepError::NotInitialised);
        }
        return Err(RosdepError::Other(stderr.trim().to_owned()));
    }

    parse_resolve_output(&stdout)
}

/// Install rosdep keys via the system package manager.
///
/// Runs: `rosdep install --default-yes --rosdistro <distro> --from-keys <keys...>`
pub fn install(keys: &[&str], distro: &str, dry_run: bool) -> Result<(), RosdepError> {
    let mut cmd = rosdep_cmd();
    cmd.args(["install", "--rosdistro", distro, "--from-keys"])
        .args(keys)
        .env("ROS_PYTHON_VERSION", "3");

    if dry_run {
        cmd.arg("--simulate");
    } else {
        cmd.arg("--default-yes");
    }

    let status = cmd.status().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            RosdepError::NotFound
        } else {
            RosdepError::Io(e)
        }
    })?;

    if !status.success() {
        return Err(RosdepError::Other("rosdep install failed".to_owned()));
    }
    Ok(())
}

fn rosdep_cmd() -> Command {
    Command::new("rosdep")
}

/// Parse `rosdep resolve` stdout: `#installer\npkg1\npkg2\n...`
fn parse_resolve_output(stdout: &str) -> Result<RosdepResolution, RosdepError> {
    let mut lines = stdout.lines();
    let installer_line = lines
        .next()
        .ok_or_else(|| RosdepError::Other("empty rosdep output".to_owned()))?;

    if !installer_line.starts_with('#') {
        return Err(RosdepError::Other(format!(
            "unexpected rosdep output: {installer_line}"
        )));
    }

    let installer = installer_line.trim_start_matches('#').trim().to_owned();
    let packages: Vec<String> = lines.filter(|l| !l.is_empty()).map(str::to_owned).collect();

    Ok(RosdepResolution {
        installer,
        packages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_apt_output() {
        let out = "#apt\nros-humble-rclcpp\n";
        let r = parse_resolve_output(out).unwrap();
        assert_eq!(r.installer, "apt");
        assert_eq!(r.packages, vec!["ros-humble-rclcpp"]);
    }

    #[test]
    fn parse_multi_package_output() {
        let out = "#apt\nros-humble-rclcpp\nros-humble-rclcpp-action\n";
        let r = parse_resolve_output(out).unwrap();
        assert_eq!(r.packages.len(), 2);
    }

    #[test]
    #[ignore = "requires live rosdep with humble database"]
    fn resolves_rclcpp_live() {
        let r = resolve("rclcpp", "humble").unwrap();
        assert_eq!(r.installer, "apt");
        assert!(r.packages.iter().any(|p| p.contains("rclcpp")));
    }
}
