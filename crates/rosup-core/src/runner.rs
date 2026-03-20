//! ROS node and launch file execution.
//!
//! Constructs the correct subprocess environment by sourcing `local_setup.sh`
//! from each install layer, then delegates to `ros2 run` or `ros2 launch`.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

use thiserror::Error;
use tracing::warn;

use crate::overlay;
use crate::resolver::store::ProjectStore;

#[derive(Debug, Error)]
pub enum RunError {
    #[error("workspace not built — run `rosup build` first")]
    NotBuilt,
    #[error("ros2 {subcmd} exited with status {code}")]
    SubprocessFailed { subcmd: String, code: i32 },
    #[error("failed to run ros2: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Shell(String),
}

/// Build the subprocess environment for running ROS 2 nodes and launch files.
///
/// Sources `local_setup.sh` from each install layer in underlay→overlay order:
/// 1. `<project>/.rosup/install/` (dep layer — only if it exists)
/// 2. `<project>/install/` (user workspace — must exist)
///
/// The current process environment (which already includes base ROS overlays
/// sourced at startup) is inherited as the base.
pub fn build_run_env(
    project_root: &Path,
    project_store: &ProjectStore,
) -> Result<HashMap<String, String>, RunError> {
    let user_install = project_root.join("install");
    if !user_install.exists() {
        return Err(RunError::NotBuilt);
    }

    // Collect layers that have a local_setup.sh to source.
    // Layers without the script are skipped — the install directory may exist
    // before colcon has populated it with setup scripts.
    let mut layers: Vec<&Path> = Vec::new();

    // Dep layer first (underlay).
    if project_store.install.join("local_setup.sh").exists() {
        layers.push(&project_store.install);
    }

    // User workspace on top (overlay).
    if user_install.join("local_setup.sh").exists() {
        layers.push(&user_install);
    }

    if layers.is_empty() {
        return Ok(HashMap::new());
    }

    // Source local_setup.sh from each layer in order. local_setup.sh applies
    // only that layer's packages without re-chaining, avoiding double-applied
    // prefixes.
    //
    // Paths are passed as positional parameters ($1, $2, ...) via Command::arg
    // so they never go through the shell parser.
    let script = (1..=layers.len())
        .map(|i| format!(". \"${i}/local_setup.sh\""))
        .collect::<Vec<_>>()
        .join(" && ");

    let mut cmd = Command::new("sh");
    cmd.arg("-c").arg(format!("{script} && env -0")).arg("--");
    for layer in &layers {
        cmd.arg(layer);
    }
    let output = cmd.output()?;

    if !output.status.success() {
        return Err(RunError::Shell(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }

    let mut env = HashMap::new();
    let stdout = String::from_utf8_lossy(&output.stdout);
    for entry in stdout.split('\0').filter(|s| !s.is_empty()) {
        if let Some((key, val)) = entry.split_once('=') {
            env.insert(key.to_owned(), val.to_owned());
        }
    }
    Ok(env)
}

/// Run a ROS 2 node via `ros2 run`.
///
/// Inherits stdio so node output streams directly to the terminal.
/// The child receives SIGINT/SIGTERM from the terminal (same process group).
pub fn run(
    project_root: &Path,
    project_store: &ProjectStore,
    overlay_env: &HashMap<String, String>,
    package: &str,
    executable: &str,
    args: &[String],
) -> Result<(), RunError> {
    check_ros_sourced(overlay_env);
    let run_env = build_run_env(project_root, project_store)?;

    let mut cmd = Command::new("ros2");
    cmd.arg("run").arg(package).arg(executable);
    if !args.is_empty() {
        cmd.arg("--");
        cmd.args(args);
    }
    apply_env(&mut cmd, overlay_env, &run_env);

    let status = cmd.status()?;
    if !status.success() {
        return Err(RunError::SubprocessFailed {
            subcmd: "run".to_owned(),
            code: status.code().unwrap_or(1),
        });
    }
    Ok(())
}

/// Launch a ROS 2 launch file via `ros2 launch`.
///
/// Inherits stdio so launch output streams directly to the terminal.
/// The child receives SIGINT/SIGTERM from the terminal (same process group).
pub fn launch(
    project_root: &Path,
    project_store: &ProjectStore,
    overlay_env: &HashMap<String, String>,
    package: &str,
    launch_file: &str,
    args: &[String],
) -> Result<(), RunError> {
    check_ros_sourced(overlay_env);
    let run_env = build_run_env(project_root, project_store)?;

    let mut cmd = Command::new("ros2");
    cmd.arg("launch").arg(package).arg(launch_file);
    if !args.is_empty() {
        cmd.arg("--");
        cmd.args(args);
    }
    apply_env(&mut cmd, overlay_env, &run_env);

    let status = cmd.status()?;
    if !status.success() {
        return Err(RunError::SubprocessFailed {
            subcmd: "launch".to_owned(),
            code: status.code().unwrap_or(1),
        });
    }
    Ok(())
}

/// Apply the overlay and run environments to a subprocess command.
///
/// The overlay env provides the base ROS environment. The run env layers
/// dep-install and user-workspace on top via `local_setup.sh`.
fn apply_env(
    cmd: &mut Command,
    overlay_env: &HashMap<String, String>,
    run_env: &HashMap<String, String>,
) {
    // Start with overlay env (base ROS).
    cmd.envs(overlay_env);
    // Layer the run env on top (dep + user workspace).
    cmd.envs(run_env);
}

/// Warn if the base ROS environment does not appear to be sourced.
fn check_ros_sourced(overlay_env: &HashMap<String, String>) {
    let prefix = overlay::ament_prefix_path(overlay_env);
    if prefix.is_empty() {
        warn!(
            "AMENT_PREFIX_PATH is not set — ROS environment may not be sourced.\n\
             Ensure overlays are configured in rosup.toml or source your ROS setup.bash."
        );
    }
}
