//! Git sparse-checkout primitives for on-demand package fetching.
//!
//! Multi-package ROS repos (e.g. `common_interfaces` with 10+ packages) are
//! checked out sparsely so only the needed subdirectories are materialised on
//! disk.
//!
//! Uses the low-level sparse-checkout mechanism (`core.sparseCheckout` config +
//! `info/sparse-checkout` file + `git read-tree`) rather than `git
//! sparse-checkout` porcelain, because the porcelain doesn't work with
//! worktrees from bare repos on git < 2.38.

use std::path::Path;
use std::process::Command;

use thiserror::Error;
use tracing::debug;

#[derive(Debug, Error)]
pub enum SparseError {
    #[error("git error running `{cmd}`: {detail}")]
    Git { cmd: String, detail: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ── primitives ───────────────────────────────────────────────────────────────

/// Create a bare partial clone (blob-less). Objects are fetched on demand
/// when a worktree checks out files.
///
/// If the bare repo already exists this is a no-op — call `git fetch` separately.
pub fn partial_bare_clone(url: &str, bare_path: &Path) -> Result<(), SparseError> {
    if bare_path.exists() {
        return Ok(());
    }
    let parent = bare_path
        .parent()
        .ok_or_else(|| SparseError::Io(std::io::Error::other("bare_path has no parent")))?;
    let bare_str = bare_path.display().to_string();
    git(
        parent,
        &[
            "clone",
            "--bare",
            "--filter=blob:none",
            "--quiet",
            url,
            &bare_str,
        ],
    )
}

/// Create a bare shallow clone (`--depth=1`). Fetches only the latest commit
/// and no blobs until checkout. Suitable for CI where history is not needed.
pub fn shallow_bare_clone(url: &str, bare_path: &Path) -> Result<(), SparseError> {
    if bare_path.exists() {
        return Ok(());
    }
    let parent = bare_path
        .parent()
        .ok_or_else(|| SparseError::Io(std::io::Error::other("bare_path has no parent")))?;
    let bare_str = bare_path.display().to_string();
    git(
        parent,
        &[
            "clone",
            "--bare",
            "--filter=blob:none",
            "--depth=1",
            "--quiet",
            url,
            &bare_str,
        ],
    )
}

/// Add a worktree with `--no-checkout` so no files are materialised until
/// sparse-checkout paths are configured.
pub fn sparse_worktree_add(
    bare_path: &Path,
    worktree_path: &Path,
    target: &str,
) -> Result<(), SparseError> {
    let wt_str = worktree_path.display().to_string();
    git(
        bare_path,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            "--no-checkout",
            &wt_str,
            target,
        ],
    )
}

/// Enable sparse-checkout on a worktree and check out only the given paths.
///
/// Uses the low-level mechanism: sets `core.sparseCheckout = true` in the
/// worktree's git config, writes directory patterns to the sparse-checkout
/// file, then runs `git read-tree -mu HEAD` to materialise matching files.
pub fn sparse_init_and_checkout(worktree_path: &Path, paths: &[&str]) -> Result<(), SparseError> {
    // Enable sparse checkout.
    git(worktree_path, &["config", "core.sparseCheckout", "true"])?;

    // Write the sparse-checkout patterns file.
    let git_dir = resolve_git_dir(worktree_path)?;
    let info_dir = git_dir.join("info");
    std::fs::create_dir_all(&info_dir)?;
    let patterns = build_patterns(paths);
    std::fs::write(info_dir.join("sparse-checkout"), patterns)?;

    // Materialise the matching files.
    git(worktree_path, &["read-tree", "-mu", "HEAD"])
}

/// Add more directory paths to an existing sparse-checkout. Additive — does
/// not remove existing paths.
pub fn sparse_add_paths(worktree_path: &Path, paths: &[&str]) -> Result<(), SparseError> {
    if paths.is_empty() {
        return Ok(());
    }

    let git_dir = resolve_git_dir(worktree_path)?;
    let sc_file = git_dir.join("info/sparse-checkout");

    // Read existing patterns, merge with new ones.
    let mut existing: Vec<String> = if sc_file.exists() {
        std::fs::read_to_string(&sc_file)?
            .lines()
            .map(|l| l.to_owned())
            .collect()
    } else {
        Vec::new()
    };

    for p in paths {
        let dir_pattern = format!("{p}/");
        if !existing.contains(&dir_pattern) {
            existing.push(dir_pattern);
        }
    }

    std::fs::write(&sc_file, existing.join("\n") + "\n")?;
    git(worktree_path, &["read-tree", "-mu", "HEAD"])
}

/// List the currently configured sparse-checkout directory paths.
pub fn sparse_list(worktree_path: &Path) -> Result<Vec<String>, SparseError> {
    let git_dir = resolve_git_dir(worktree_path)?;
    let sc_file = git_dir.join("info/sparse-checkout");

    if !sc_file.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&sc_file)?;
    Ok(content
        .lines()
        .map(|l| l.trim_end_matches('/').to_owned())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Check whether sparse-checkout is enabled on a worktree.
pub fn is_sparse(worktree_path: &Path) -> bool {
    git_output(worktree_path, &["config", "--get", "core.sparseCheckout"])
        .map(|v| v.trim() == "true")
        .unwrap_or(false)
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Resolve the git directory for a worktree. For worktrees created from bare
/// repos, the `.git` file points to the worktree-specific directory inside
/// the bare repo (e.g. `<bare>/worktrees/<name>/`).
fn resolve_git_dir(worktree_path: &Path) -> Result<std::path::PathBuf, SparseError> {
    let out = git_output(worktree_path, &["rev-parse", "--git-dir"])?;
    Ok(std::path::PathBuf::from(out.trim()))
}

/// Build sparse-checkout patterns from directory names.
fn build_patterns(paths: &[&str]) -> String {
    let mut patterns = String::new();
    for p in paths {
        patterns.push_str(p);
        if !p.ends_with('/') {
            patterns.push('/');
        }
        patterns.push('\n');
    }
    patterns
}

fn git(cwd: &Path, args: &[&str]) -> Result<(), SparseError> {
    debug!("git {}", args.join(" "));
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(SparseError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SparseError::Git {
            cmd: format!("git {}", args.join(" ")),
            detail: stderr.trim().to_owned(),
        });
    }
    Ok(())
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String, SparseError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(SparseError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(SparseError::Git {
            cmd: format!("git {}", args.join(" ")),
            detail: stderr.trim().to_owned(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Set up a real local git repo to test against (no network).
    fn make_local_repo() -> (TempDir, String) {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("origin");
        std::fs::create_dir_all(&repo).unwrap();

        let run = |cwd: &Path, args: &[&str]| {
            Command::new("git")
                .args(args)
                .current_dir(cwd)
                .env("GIT_AUTHOR_NAME", "test")
                .env("GIT_AUTHOR_EMAIL", "t@t")
                .env("GIT_COMMITTER_NAME", "test")
                .env("GIT_COMMITTER_EMAIL", "t@t")
                .output()
                .unwrap()
        };

        run(&repo, &["init", "-b", "main"]);

        // Create two "packages" as subdirectories.
        let pkg_a = repo.join("pkg_a");
        let pkg_b = repo.join("pkg_b");
        std::fs::create_dir_all(&pkg_a).unwrap();
        std::fs::create_dir_all(&pkg_b).unwrap();
        std::fs::write(pkg_a.join("package.xml"), "<pkg>a</pkg>").unwrap();
        std::fs::write(pkg_b.join("package.xml"), "<pkg>b</pkg>").unwrap();
        std::fs::write(repo.join("README.md"), "hello").unwrap();

        run(&repo, &["add", "."]);
        run(
            &repo,
            &["-c", "commit.gpgsign=false", "commit", "-m", "init"],
        );

        let url = format!("file://{}", repo.display());
        (tmp, url)
    }

    #[test]
    fn partial_clone_creates_bare_repo() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        partial_bare_clone(&url, &bare).unwrap();
        assert!(bare.exists());
        assert!(bare.join("HEAD").exists());
    }

    #[test]
    fn partial_clone_is_idempotent() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        partial_bare_clone(&url, &bare).unwrap();
        partial_bare_clone(&url, &bare).unwrap();
        assert!(bare.exists());
    }

    #[test]
    fn shallow_clone_creates_bare_repo() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        shallow_bare_clone(&url, &bare).unwrap();
        assert!(bare.join("HEAD").exists());
    }

    #[test]
    fn sparse_worktree_no_checkout() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        partial_bare_clone(&url, &bare).unwrap();

        let wt = tmp.path().join("wt");
        sparse_worktree_add(&bare, &wt, "main").unwrap();

        // Worktree exists but no files checked out.
        assert!(wt.exists());
        assert!(!wt.join("README.md").exists());
        assert!(!wt.join("pkg_a").exists());
    }

    #[test]
    fn sparse_init_checks_out_only_requested_dirs() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        partial_bare_clone(&url, &bare).unwrap();

        let wt = tmp.path().join("wt");
        sparse_worktree_add(&bare, &wt, "main").unwrap();
        sparse_init_and_checkout(&wt, &["pkg_a"]).unwrap();

        assert!(wt.join("pkg_a/package.xml").exists());
        assert!(!wt.join("pkg_b/package.xml").exists());
    }

    #[test]
    fn sparse_add_is_additive() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        partial_bare_clone(&url, &bare).unwrap();

        let wt = tmp.path().join("wt");
        sparse_worktree_add(&bare, &wt, "main").unwrap();
        sparse_init_and_checkout(&wt, &["pkg_a"]).unwrap();
        sparse_add_paths(&wt, &["pkg_b"]).unwrap();

        assert!(wt.join("pkg_a/package.xml").exists());
        assert!(wt.join("pkg_b/package.xml").exists());
    }

    #[test]
    fn sparse_list_returns_checked_out_dirs() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        partial_bare_clone(&url, &bare).unwrap();

        let wt = tmp.path().join("wt");
        sparse_worktree_add(&bare, &wt, "main").unwrap();
        sparse_init_and_checkout(&wt, &["pkg_a", "pkg_b"]).unwrap();

        let paths = sparse_list(&wt).unwrap();
        assert!(paths.contains(&"pkg_a".to_owned()));
        assert!(paths.contains(&"pkg_b".to_owned()));
    }

    #[test]
    fn sparse_add_empty_paths_is_noop() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        partial_bare_clone(&url, &bare).unwrap();

        let wt = tmp.path().join("wt");
        sparse_worktree_add(&bare, &wt, "main").unwrap();
        sparse_init_and_checkout(&wt, &["pkg_a"]).unwrap();
        // Should not error.
        sparse_add_paths(&wt, &[]).unwrap();
        // pkg_a still there.
        assert!(wt.join("pkg_a/package.xml").exists());
    }

    #[test]
    fn is_sparse_detects_config() {
        let (tmp, url) = make_local_repo();
        let bare = tmp.path().join("test.git");
        partial_bare_clone(&url, &bare).unwrap();

        let wt = tmp.path().join("wt");
        sparse_worktree_add(&bare, &wt, "main").unwrap();
        assert!(!is_sparse(&wt));

        sparse_init_and_checkout(&wt, &["pkg_a"]).unwrap();
        assert!(is_sparse(&wt));
    }
}
