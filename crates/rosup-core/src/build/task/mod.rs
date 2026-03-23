//! Build task trait and built-in implementations.
//!
//! Every package type (ament_cmake, ament_python, cmake, etc.) implements
//! the `BuildTask` trait. Built-in tasks are compiled into rosup; external
//! tasks are loaded via subprocess or PyO3.

pub mod ament_cmake;

use std::collections::HashMap;
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TaskError {
    #[error("command `{cmd}` failed with exit code {code}")]
    CommandFailed { cmd: String, code: i32 },
    #[error("failed to run `{cmd}`: {source}")]
    Io {
        cmd: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    Other(String),
}

/// Context passed to a build task.
#[derive(Debug)]
pub struct BuildContext {
    /// Package name.
    pub pkg_name: String,
    /// Absolute path to the package source directory.
    pub source_path: PathBuf,
    /// Per-package build directory (e.g. `build/<name>`).
    pub build_base: PathBuf,
    /// Per-package install directory (e.g. `install/<name>`).
    pub install_base: PathBuf,
    /// Use symlinks instead of copies where possible.
    pub symlink_install: bool,
    /// Additional CMake arguments.
    pub cmake_args: Vec<String>,
    /// Maximum parallel jobs (for `cmake --build --parallel`).
    pub parallel_jobs: Option<usize>,
    /// Install paths of resolved dependencies, keyed by dep name.
    /// Used to build CMAKE_PREFIX_PATH.
    pub dep_install_paths: Vec<PathBuf>,
    /// Environment variables to pass to subprocesses.
    pub env: HashMap<String, String>,
    /// Python version for PYTHONPATH hooks (e.g. "3.10"). None for non-Python packages.
    pub python_version: Option<String>,
    /// Runtime dependency names (for colcon-core/packages file).
    pub runtime_deps: Vec<String>,
}

/// Trait for build tasks. Each package type implements this.
pub trait BuildTask: Send + Sync {
    /// Build and install the package.
    fn build(&self, ctx: &BuildContext) -> Result<(), TaskError>;

    /// Run the package's tests.
    fn test(&self, ctx: &BuildContext) -> Result<(), TaskError>;
}

/// Run a command, inheriting stdio, and return an error on non-zero exit.
pub fn run_command(
    program: &str,
    args: &[&str],
    cwd: &std::path::Path,
    env: &HashMap<String, String>,
) -> Result<(), TaskError> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args).current_dir(cwd);
    if !env.is_empty() {
        cmd.envs(env);
    }
    let status = cmd.status().map_err(|e| TaskError::Io {
        cmd: format!("{program} {}", args.join(" ")),
        source: e,
    })?;
    if !status.success() {
        return Err(TaskError::CommandFailed {
            cmd: format!("{program} {}", args.join(" ")),
            code: status.code().unwrap_or(1),
        });
    }
    Ok(())
}
