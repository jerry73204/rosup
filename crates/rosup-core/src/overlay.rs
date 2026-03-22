//! Ament overlay environment activation.
//!
//! Sources `setup.sh` from each configured overlay prefix in underlay→overlay
//! order and captures the resulting environment. The captured env is then
//! applied to all rosup subprocess invocations (colcon, rosdep, git).

use std::collections::HashMap;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OverlayError {
    #[error("overlay prefix `{0}` has no setup.sh")]
    NoSetupScript(PathBuf),
    #[error("failed to run shell to source overlays: {0}")]
    Io(#[from] std::io::Error),
    #[error("shell exited with error while sourcing overlays:\n{0}")]
    Shell(String),
}

/// Build the subprocess environment by sourcing each overlay's `setup.sh` in
/// underlay-first order (index 0 = lowest priority, last = highest).
///
/// Returns the full captured environment that should be applied to subprocesses.
/// Returns an empty map when `overlays` is empty (caller inherits process env).
pub fn build_env(overlays: &[PathBuf]) -> Result<HashMap<String, String>, OverlayError> {
    if overlays.is_empty() {
        return Ok(HashMap::new());
    }

    for prefix in overlays {
        if !prefix.join("setup.bash").exists() {
            return Err(OverlayError::NoSetupScript(prefix.clone()));
        }
    }

    // Source each overlay in order (underlay first), then dump the env null-terminated.
    //
    // Paths are passed as positional parameters ($1, $2, ...) via Command::arg so
    // they never go through the shell parser — no quoting or injection risk.
    let script = (1..=overlays.len())
        .map(|i| format!("source \"${i}/setup.bash\""))
        .collect::<Vec<_>>()
        .join(" && ");

    let mut cmd = std::process::Command::new("bash");
    cmd.arg("-c").arg(format!("{script} && env -0")).arg("--"); // $0 placeholder
    for prefix in overlays {
        cmd.arg(prefix); // becomes $1, $2, ...
    }
    let output = cmd.output()?;

    if !output.status.success() {
        return Err(OverlayError::Shell(
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

/// Extract `AMENT_PREFIX_PATH` from the overlay env map, falling back to the
/// current process env when no overlays are configured.
pub fn ament_prefix_path(overlay_env: &HashMap<String, String>) -> String {
    if overlay_env.is_empty() {
        std::env::var("AMENT_PREFIX_PATH").unwrap_or_default()
    } else {
        overlay_env
            .get("AMENT_PREFIX_PATH")
            .cloned()
            .unwrap_or_default()
    }
}
