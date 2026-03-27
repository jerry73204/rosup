use std::collections::HashMap;
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
/// Writes a minimal `package.xml` to a temp directory and runs
/// `rosdep install --from-paths <tmpdir> --ignore-src --rosdistro <distro>`.
///
/// - `dry_run`: simulate only (`--simulate`).
/// - `yes`: assume yes to all prompts (`--default-yes`). When false, rosdep
///   prompts interactively for confirmation (may require sudo).
pub fn install(keys: &[&str], distro: &str, dry_run: bool, yes: bool) -> Result<(), RosdepError> {
    // Write a minimal package.xml so rosdep can discover the keys via --from-paths.
    let tmp = tempfile::tempdir().map_err(RosdepError::Io)?;
    let dep_lines: String = keys
        .iter()
        .map(|k| format!("  <exec_depend>{k}</exec_depend>\n"))
        .collect();
    let pkg_xml = format!(
        "<?xml version=\"1.0\"?>\n\
         <package format=\"3\">\n\
           <name>rosup_install_helper</name>\n\
           <version>0.0.0</version>\n\
           <description>Temporary package for rosup dependency installation</description>\n\
           <maintainer email=\"rosup@users.noreply.github.com\">rosup</maintainer>\n\
           <license>Apache-2.0</license>\n\
         {dep_lines}</package>\n"
    );
    std::fs::write(tmp.path().join("package.xml"), pkg_xml).map_err(RosdepError::Io)?;

    let mut cmd = rosdep_cmd();
    cmd.args([
        "install",
        "--from-paths",
        tmp.path().to_str().unwrap_or("."),
        "--ignore-src",
        "--rosdistro",
        distro,
    ])
    .env("ROS_PYTHON_VERSION", "3");

    if dry_run {
        cmd.arg("--simulate");
    } else if yes {
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

/// Resolve multiple rosdep keys and group the results by installer.
///
/// Calls `rosdep resolve` for each key individually, collects the results,
/// and returns them grouped by installer (e.g. "apt" → [...], "pip" → [...]).
///
/// Keys that fail to resolve are collected in the returned `failures` vec
/// rather than aborting — the caller decides whether failures are fatal.
pub fn resolve_all(keys: &[&str], distro: &str) -> Result<ResolveAllResult, RosdepError> {
    let mut by_installer: HashMap<String, Vec<InstalledPackage>> = HashMap::new();
    let mut failures: Vec<(String, String)> = Vec::new();

    for &key in keys {
        match resolve(key, distro) {
            Ok(r) => {
                let entry = by_installer.entry(r.installer).or_default();
                for pkg in &r.packages {
                    entry.push(InstalledPackage {
                        rosdep_key: key.to_owned(),
                        system_package: pkg.clone(),
                    });
                }
            }
            Err(RosdepError::Other(msg)) => {
                failures.push((key.to_owned(), msg));
            }
            Err(e) => return Err(e),
        }
    }

    Ok(ResolveAllResult {
        by_installer,
        failures,
    })
}

/// Result of batch-resolving rosdep keys.
#[derive(Debug)]
pub struct ResolveAllResult {
    /// System packages grouped by installer (e.g. "apt" → [...]).
    pub by_installer: HashMap<String, Vec<InstalledPackage>>,
    /// Keys that failed to resolve: (rosdep_key, error_message).
    pub failures: Vec<(String, String)>,
}

/// A system package with its originating rosdep key for traceability.
#[derive(Debug, Clone)]
pub struct InstalledPackage {
    /// The rosdep key that resolved to this package (e.g. "rclcpp").
    pub rosdep_key: String,
    /// The system package name (e.g. "ros-humble-rclcpp").
    pub system_package: String,
}

/// Install system packages directly via the platform installer, bypassing
/// `rosdep install`.
///
/// Takes the output of `resolve_all()` and calls the installer directly
/// (e.g. `apt-get install`, `pip install`). This gives rosup full control
/// over the install process — better error messages, batching, and dry-run
/// output that shows exactly what will be installed.
pub fn install_direct(
    resolved: &ResolveAllResult,
    dry_run: bool,
    yes: bool,
) -> Result<(), RosdepError> {
    for (installer, packages) in &resolved.by_installer {
        let system_pkgs: Vec<&str> = packages.iter().map(|p| p.system_package.as_str()).collect();

        if system_pkgs.is_empty() {
            continue;
        }

        match installer.as_str() {
            "apt" => install_apt(&system_pkgs, dry_run, yes)?,
            "pip" => install_pip(&system_pkgs, dry_run, yes)?,
            other => {
                // Fall back to rosdep install for unknown installers.
                tracing::warn!("unknown installer '{other}', falling back to rosdep install");
                let keys: Vec<&str> = packages.iter().map(|p| p.rosdep_key.as_str()).collect();
                install(&keys, "", dry_run, yes)?;
            }
        }
    }
    Ok(())
}

fn install_apt(packages: &[&str], dry_run: bool, yes: bool) -> Result<(), RosdepError> {
    if dry_run {
        let joined = packages.join(" ");
        tracing::info!("would run: sudo apt-get install {joined}");
        return Ok(());
    }

    let mut cmd = Command::new("sudo");
    cmd.arg("apt-get").arg("install");
    if yes {
        cmd.arg("-y");
    }
    cmd.args(packages);

    tracing::info!("installing {} apt package(s)", packages.len());
    let status = cmd.status().map_err(RosdepError::Io)?;
    if !status.success() {
        return Err(RosdepError::Other(format!(
            "apt-get install failed for: {}",
            packages.join(", ")
        )));
    }
    Ok(())
}

fn install_pip(packages: &[&str], dry_run: bool, yes: bool) -> Result<(), RosdepError> {
    if dry_run {
        let joined = packages.join(" ");
        tracing::info!("would run: pip install {joined}");
        return Ok(());
    }

    let mut cmd = Command::new("pip");
    cmd.arg("install");
    if yes {
        // pip has no --yes flag, but it's non-interactive by default.
        // Nothing to do.
    }
    cmd.args(packages);

    tracing::info!("installing {} pip package(s)", packages.len());
    let status = cmd.status().map_err(RosdepError::Io)?;
    if !status.success() {
        return Err(RosdepError::Other(format!(
            "pip install failed for: {}",
            packages.join(", ")
        )));
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

    #[test]
    fn resolve_all_result_groups_by_installer() {
        let mut by_installer = HashMap::new();
        by_installer.insert(
            "apt".to_owned(),
            vec![
                InstalledPackage {
                    rosdep_key: "rclcpp".to_owned(),
                    system_package: "ros-humble-rclcpp".to_owned(),
                },
                InstalledPackage {
                    rosdep_key: "std_msgs".to_owned(),
                    system_package: "ros-humble-std-msgs".to_owned(),
                },
            ],
        );
        by_installer.insert(
            "pip".to_owned(),
            vec![InstalledPackage {
                rosdep_key: "empy".to_owned(),
                system_package: "empy".to_owned(),
            }],
        );

        let result = ResolveAllResult {
            by_installer,
            failures: vec![("unknown_pkg".to_owned(), "no rule".to_owned())],
        };

        assert_eq!(result.by_installer.len(), 2);
        assert_eq!(result.by_installer["apt"].len(), 2);
        assert_eq!(result.by_installer["pip"].len(), 1);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].0, "unknown_pkg");
    }

    #[test]
    fn installed_package_tracks_rosdep_key() {
        let pkg = InstalledPackage {
            rosdep_key: "rclcpp".to_owned(),
            system_package: "ros-humble-rclcpp".to_owned(),
        };
        assert_eq!(pkg.rosdep_key, "rclcpp");
        assert_eq!(pkg.system_package, "ros-humble-rclcpp");
    }
}
