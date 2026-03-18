//! Build, test, and clean orchestration.
//!
//! Wraps `colcon` with the correct environment and arguments derived from
//! `rosup.toml` configuration. All colcon output streams directly to the
//! terminal; exit codes are propagated.

use std::path::{Path, PathBuf};
use std::process::Command;

use thiserror::Error;
use tracing::{debug, info, warn};

use crate::config::{BuildConfig, BuildOverride, TestConfig};
use crate::resolver::store::ProjectStore;

// ── errors ────────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum BuildError {
    #[error("colcon exited with status {0}")]
    Colcon(i32),
    #[error("failed to run colcon: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to remove {path}: {source}")]
    Remove {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

// ── environment helpers ───────────────────────────────────────────────────────

/// Returns the current `AMENT_PREFIX_PATH` if set and non-empty.
pub fn ros_env() -> Option<String> {
    std::env::var("AMENT_PREFIX_PATH")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Warn if the base ROS environment does not appear to be sourced.
///
/// Returns `true` if `AMENT_PREFIX_PATH` is set, `false` (with a warning)
/// if it is not.
pub fn check_ros_env(distro: Option<&str>) -> bool {
    if ros_env().is_some() {
        return true;
    }
    let hint = distro
        .map(|d| format!("/opt/ros/{d}/setup.bash"))
        .unwrap_or_else(|| "/opt/ros/<distro>/setup.bash".to_owned());
    warn!(
        "AMENT_PREFIX_PATH is not set — base ROS environment may not be sourced.\n\
         Run: source {hint}"
    );
    false
}

/// Build the `AMENT_PREFIX_PATH` for a colcon subprocess that needs to see the
/// dep layer installed into `<project>/.rosup/install/`.
///
/// Prepends the dep layer prefix only if `.rosup/install/` exists.
pub fn ament_prefix_with_deps(project_store: &ProjectStore) -> String {
    let existing = std::env::var("AMENT_PREFIX_PATH").unwrap_or_default();
    let dep_install = &project_store.install;
    if dep_install.exists() {
        let prefix = dep_install.display().to_string();
        if existing.is_empty() {
            prefix
        } else {
            format!("{prefix}:{existing}")
        }
    } else {
        existing
    }
}

// ── dep layer build ───────────────────────────────────────────────────────────

/// Build all source-pulled worktrees in `<project>/.rosup/src/` into
/// `<project>/.rosup/install/`.
///
/// Skipped when `.rosup/src/` is empty or the install dir is already populated,
/// unless `rebuild` is `true`.
pub fn build_dep_layer(
    project_root: &Path,
    project_store: &ProjectStore,
    rebuild: bool,
) -> Result<(), BuildError> {
    let src = &project_store.src;

    if !src.exists() {
        debug!("no .rosup/src/ — dep layer build skipped");
        return Ok(());
    }

    let has_worktrees = std::fs::read_dir(src)
        .map(|mut d| d.next().is_some())
        .unwrap_or(false);
    if !has_worktrees {
        debug!(".rosup/src/ is empty — dep layer build skipped");
        return Ok(());
    }

    if !rebuild && project_store.install.exists() {
        let has_content = std::fs::read_dir(&project_store.install)
            .map(|mut d| d.next().is_some())
            .unwrap_or(false);
        if has_content {
            debug!("dep layer already built — skipping (use --rebuild-deps to force)");
            return Ok(());
        }
    }

    info!("building dep layer");
    run_colcon(
        Command::new("colcon")
            .arg("build")
            .arg("--base-paths")
            .arg(src)
            .arg("--build-base")
            .arg(&project_store.build)
            .arg("--install-base")
            .arg(&project_store.install)
            .current_dir(project_root),
    )?;
    Ok(())
}

// ── workspace build ───────────────────────────────────────────────────────────

/// Options for `rox build`.
pub struct BuildOptions<'a> {
    pub packages: &'a [String],
    /// `--packages-up-to` instead of `--packages-select`.
    pub include_deps: bool,
    pub cmake_args: &'a [String],
    pub release: bool,
    pub debug: bool,
}

/// Build the user's workspace packages via colcon.
pub fn build_workspace(
    project_root: &Path,
    project_store: &ProjectStore,
    build_cfg: &BuildConfig,
    opts: &BuildOptions<'_>,
) -> Result<(), BuildError> {
    // Determine packages that have per-package overrides.
    let override_pkgs: Vec<&str> = build_cfg
        .overrides
        .keys()
        .filter(|k| opts.packages.is_empty() || opts.packages.iter().any(|p| p == *k))
        .map(String::as_str)
        .collect();

    // Build all packages without overrides in one colcon invocation.
    let base_packages: Vec<&String> = opts
        .packages
        .iter()
        .filter(|p| !build_cfg.overrides.contains_key(p.as_str()))
        .collect();

    // If packages were specified and all of them have overrides, skip the base call.
    let skip_base = !opts.packages.is_empty() && base_packages.is_empty();
    if !skip_base {
        let mut cmd = base_build_cmd(project_root, project_store, build_cfg, opts);
        if !opts.packages.is_empty() {
            if opts.include_deps {
                cmd.arg("--packages-up-to");
            } else {
                cmd.arg("--packages-select");
            }
            cmd.args(&base_packages);
        }
        run_colcon(&mut cmd)?;
    }

    // Build each package that has overrides in a separate colcon call.
    for pkg in override_pkgs {
        let pkg_override = &build_cfg.overrides[pkg];
        run_colcon(&mut override_build_cmd(
            project_root,
            project_store,
            build_cfg,
            pkg,
            pkg_override,
            opts,
        ))?;
    }

    Ok(())
}

fn base_build_cmd(
    project_root: &Path,
    project_store: &ProjectStore,
    build_cfg: &BuildConfig,
    opts: &BuildOptions<'_>,
) -> Command {
    let mut cmd = Command::new("colcon");
    cmd.arg("build").current_dir(project_root);
    cmd.env("AMENT_PREFIX_PATH", ament_prefix_with_deps(project_store));

    if build_cfg.symlink_install {
        cmd.arg("--symlink-install");
    }
    if let Some(jobs) = build_cfg.parallel_jobs {
        cmd.arg("--parallel-workers").arg(jobs.to_string());
    }

    let cmake_args = merged_cmake_args(
        &build_cfg.cmake_args,
        opts.cmake_args,
        opts.release,
        opts.debug,
    );
    if !cmake_args.is_empty() {
        cmd.arg("--cmake-args").args(cmake_args);
    }

    cmd
}

fn override_build_cmd(
    project_root: &Path,
    project_store: &ProjectStore,
    build_cfg: &BuildConfig,
    pkg: &str,
    pkg_override: &BuildOverride,
    opts: &BuildOptions<'_>,
) -> Command {
    let mut cmd = Command::new("colcon");
    cmd.arg("build")
        .arg("--packages-select")
        .arg(pkg)
        .current_dir(project_root);
    cmd.env("AMENT_PREFIX_PATH", ament_prefix_with_deps(project_store));

    if build_cfg.symlink_install {
        cmd.arg("--symlink-install");
    }
    let jobs = pkg_override.parallel_jobs.or(build_cfg.parallel_jobs);
    if let Some(j) = jobs {
        cmd.arg("--parallel-workers").arg(j.to_string());
    }

    let cmake_args = merged_cmake_args(
        &pkg_override.cmake_args,
        opts.cmake_args,
        opts.release,
        opts.debug,
    );
    if !cmake_args.is_empty() {
        cmd.arg("--cmake-args").args(cmake_args);
    }

    cmd
}

/// Merge cmake-args from rosup.toml with CLI overrides and build-type shorthands.
fn merged_cmake_args(base: &[String], extra: &[String], release: bool, debug: bool) -> Vec<String> {
    let mut args: Vec<String> = base.to_vec();
    args.extend_from_slice(extra);
    if release {
        args.push("-DCMAKE_BUILD_TYPE=Release".to_owned());
    } else if debug {
        args.push("-DCMAKE_BUILD_TYPE=Debug".to_owned());
    }
    args
}

// ── test ──────────────────────────────────────────────────────────────────────

/// Options for `rox test`.
pub struct TestOptions<'a> {
    pub packages: &'a [String],
    pub retest_until_pass: Option<usize>,
}

/// Run `colcon test` then print a result summary.
///
/// Retries the full test run up to `retest_until_pass` additional times on
/// failure.
pub fn test_workspace(
    project_root: &Path,
    project_store: &ProjectStore,
    test_cfg: &TestConfig,
    opts: &TestOptions<'_>,
) -> Result<(), BuildError> {
    let max_attempts = 1 + opts.retest_until_pass.unwrap_or(0);
    let mut last_err: Option<BuildError> = None;

    for attempt in 1..=max_attempts {
        if attempt > 1 {
            info!("retrying failed tests (attempt {attempt}/{max_attempts})");
        }
        match run_colcon(&mut build_test_cmd(
            project_root,
            project_store,
            test_cfg,
            opts,
        )) {
            Ok(()) => {
                last_err = None;
                break;
            }
            Err(e) => last_err = Some(e),
        }
    }

    // Print summary regardless of pass/fail.
    let _ = Command::new("colcon")
        .arg("test-result")
        .arg("--verbose")
        .current_dir(project_root)
        .status();

    match last_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

fn build_test_cmd(
    project_root: &Path,
    project_store: &ProjectStore,
    test_cfg: &TestConfig,
    opts: &TestOptions<'_>,
) -> Command {
    let mut cmd = Command::new("colcon");
    cmd.arg("test").current_dir(project_root);
    cmd.env("AMENT_PREFIX_PATH", ament_prefix_with_deps(project_store));

    if let Some(jobs) = test_cfg.parallel_jobs {
        cmd.arg("--parallel-workers").arg(jobs.to_string());
    }
    if !opts.packages.is_empty() {
        cmd.arg("--packages-select").args(opts.packages);
    }
    cmd
}

// ── clean ─────────────────────────────────────────────────────────────────────

/// Options for `rox clean`.
pub struct CleanOptions<'a> {
    /// Also remove `install/` (user workspace).
    pub all: bool,
    /// Remove `.rosup/build/` and `.rosup/install/` (dep layer artifacts).
    pub deps: bool,
    /// Also remove `.rosup/src/` worktrees (implies `deps`).
    pub src: bool,
    /// Clean only the named packages from `build/` and `install/`.
    pub packages: &'a [String],
}

/// Remove build artifacts according to the requested scope.
pub fn clean(
    project_root: &Path,
    project_store: &ProjectStore,
    opts: &CleanOptions<'_>,
) -> Result<(), BuildError> {
    if opts.packages.is_empty() {
        remove_if_exists(&project_root.join("build"))?;
        remove_if_exists(&project_root.join("log"))?;
        if opts.all {
            remove_if_exists(&project_root.join("install"))?;
        }
        if opts.deps || opts.src {
            remove_if_exists(&project_store.build)?;
            remove_if_exists(&project_store.install)?;
        }
        if opts.src {
            remove_if_exists(&project_store.src)?;
        }
    } else {
        for pkg in opts.packages {
            for dir in ["build", "install"] {
                remove_if_exists(&project_root.join(dir).join(pkg))?;
            }
        }
    }
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn run_colcon(cmd: &mut Command) -> Result<(), BuildError> {
    // Inherit stdio so colcon output streams directly to the terminal.
    let status = cmd.status()?;
    if !status.success() {
        return Err(BuildError::Colcon(status.code().unwrap_or(1)));
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> Result<(), BuildError> {
    if !path.exists() {
        return Ok(());
    }
    info!("removing {}", path.display());
    std::fs::remove_dir_all(path).map_err(|e| BuildError::Remove {
        path: path.to_owned(),
        source: e,
    })
}
