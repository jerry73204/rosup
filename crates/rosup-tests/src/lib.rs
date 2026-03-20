//! Shared test infrastructure for rosup integration tests.

pub mod project;
pub mod shims;

use std::path::Path;
use tempfile::TempDir;

pub use project::{PackageProject, WorkspaceProject, install_humble_cache};
pub use shims::shim_dir;

// ── ShimInvocation ────────────────────────────────────────────────────────────

/// A recorded invocation from a shim binary.
#[derive(Debug, serde::Deserialize)]
pub struct ShimInvocation {
    pub tool: String,
    pub args: Vec<String>,
}

/// Read all shim invocations from `log_dir`, sorted by filename (time order).
pub fn shim_log(log_dir: &Path) -> Vec<ShimInvocation> {
    let mut entries: Vec<_> = std::fs::read_dir(log_dir)
        .expect("read_dir shim log")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    entries.sort_by_key(|e| e.file_name());

    entries
        .iter()
        .filter_map(|e| {
            let content = std::fs::read_to_string(e.path()).ok()?;
            serde_json::from_str(&content).ok()
        })
        .collect()
}

/// Return invocations for a specific tool.
pub fn invocations_for<'a>(log: &'a [ShimInvocation], tool: &str) -> Vec<&'a ShimInvocation> {
    log.iter().filter(|i| i.tool == tool).collect()
}

/// Assert that an invocation's args contain all of `expected` as a subsequence.
pub fn assert_args_contain(inv: &ShimInvocation, expected: &[&str]) {
    let args: Vec<&str> = inv.args.iter().map(String::as_str).collect();
    for &exp in expected {
        assert!(args.contains(&exp), "expected arg {:?} in {:?}", exp, args);
    }
}

// ── TestEnv ───────────────────────────────────────────────────────────────────

/// Bundles a project dir, a fake HOME, and a shim log dir for one test.
pub struct TestEnv {
    pub home: TempDir,
    pub log: TempDir,
}

impl TestEnv {
    pub fn new() -> Self {
        Self {
            home: TempDir::new().expect("home tempdir"),
            log: TempDir::new().expect("log tempdir"),
        }
    }

    /// Populate fake HOME with the humble rosdistro cache.
    pub fn with_cache(self) -> Self {
        install_humble_cache(self.home.path());
        self
    }

    /// Build a `rosup` Command pointing at `project_dir` with shims on PATH
    /// and HOME/SHIM_LOG_DIR configured.
    pub fn cmd(&self, project_dir: &Path) -> assert_cmd::Command {
        let shims = shim_dir();
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{original_path}", shims.display());

        let mut cmd = assert_cmd::Command::cargo_bin("rosup").expect("rosup binary");
        cmd.current_dir(project_dir)
            .env("PATH", &new_path)
            .env("HOME", self.home.path())
            .env("SHIM_LOG_DIR", self.log.path())
            .env_remove("AMENT_PREFIX_PATH")
            .env_remove("ROS_DISTRO");
        cmd
    }

    pub fn log(&self) -> Vec<ShimInvocation> {
        shim_log(self.log.path())
    }

    pub fn colcon_calls(&self) -> Vec<ShimInvocation> {
        self.log()
            .into_iter()
            .filter(|i| i.tool == "colcon")
            .collect()
    }

    pub fn git_calls(&self) -> Vec<ShimInvocation> {
        self.log().into_iter().filter(|i| i.tool == "git").collect()
    }

    pub fn rosdep_calls(&self) -> Vec<ShimInvocation> {
        self.log()
            .into_iter()
            .filter(|i| i.tool == "rosdep")
            .collect()
    }

    pub fn ros2_calls(&self) -> Vec<ShimInvocation> {
        self.log()
            .into_iter()
            .filter(|i| i.tool == "ros2")
            .collect()
    }
}

impl Default for TestEnv {
    fn default() -> Self {
        Self::new()
    }
}
