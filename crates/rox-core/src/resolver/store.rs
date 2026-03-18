use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("cannot determine home directory")]
    NoHome,
    #[error("failed to create {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Global store at `~/.rox/`.
///
/// Shared across all projects. Contains only data that is safe to share:
/// - `cache/` — rosdistro YAML files (download cache)
/// - `src/`   — bare git clones (object store cache)
#[derive(Debug, Clone)]
pub struct GlobalStore {
    pub root: PathBuf,
    pub cache: PathBuf,
    pub src: PathBuf,
}

impl GlobalStore {
    pub fn open() -> Result<Self, StoreError> {
        let home = dirs::home_dir().ok_or(StoreError::NoHome)?;
        let root = home.join(".rox");
        Ok(Self {
            cache: root.join("cache"),
            src: root.join("src"),
            root,
        })
    }

    pub fn ensure(&self) -> Result<(), StoreError> {
        for dir in [&self.root, &self.cache, &self.src] {
            std::fs::create_dir_all(dir).map_err(|e| StoreError::Io {
                path: dir.clone(),
                source: e,
            })?;
        }
        Ok(())
    }

    /// Path to the bare clone for a given repository name.
    pub fn bare_clone(&self, repo_name: &str) -> PathBuf {
        self.src.join(format!("{repo_name}.git"))
    }
}

/// Per-project store at `<project_root>/.rox/`.
///
/// Analogous to Cargo's `target/` — owned by one project, git-ignored.
/// - `src/`     — git worktrees backed by `~/.rox/src/*.git`
/// - `build/`   — colcon build artifacts for the dep layer
/// - `install/` — colcon install prefix for the dep layer
#[derive(Debug, Clone)]
pub struct ProjectStore {
    pub root: PathBuf,
    pub src: PathBuf,
    pub build: PathBuf,
    pub install: PathBuf,
}

impl ProjectStore {
    pub fn open(project_root: &Path) -> Self {
        let root = project_root.join(".rox");
        Self {
            src: root.join("src"),
            build: root.join("build"),
            install: root.join("install"),
            root,
        }
    }

    pub fn ensure(&self) -> Result<(), StoreError> {
        for dir in [&self.root, &self.src, &self.build, &self.install] {
            std::fs::create_dir_all(dir).map_err(|e| StoreError::Io {
                path: dir.clone(),
                source: e,
            })?;
        }
        Ok(())
    }

    /// Path to the worktree for a given repository name.
    pub fn worktree(&self, repo_name: &str) -> PathBuf {
        self.src.join(repo_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn global_store_paths() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join(".rox");
        let store = GlobalStore {
            cache: root.join("cache"),
            src: root.join("src"),
            root: root.clone(),
        };
        assert_eq!(store.bare_clone("rclcpp"), root.join("src/rclcpp.git"));
        assert_eq!(
            store.bare_clone("common_interfaces"),
            root.join("src/common_interfaces.git")
        );
    }

    #[test]
    fn global_store_ensure_creates_dirs() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join(".rox");
        let store = GlobalStore {
            cache: root.join("cache"),
            src: root.join("src"),
            root,
        };
        store.ensure().unwrap();
        assert!(store.cache.is_dir());
        assert!(store.src.is_dir());
    }

    #[test]
    fn project_store_paths() {
        let tmp = TempDir::new().unwrap();
        let store = ProjectStore::open(tmp.path());
        assert_eq!(store.root, tmp.path().join(".rox"));
        assert_eq!(store.src, tmp.path().join(".rox/src"));
        assert_eq!(store.build, tmp.path().join(".rox/build"));
        assert_eq!(store.install, tmp.path().join(".rox/install"));
        assert_eq!(store.worktree("rclcpp"), tmp.path().join(".rox/src/rclcpp"));
    }

    #[test]
    fn project_store_ensure_creates_dirs() {
        let tmp = TempDir::new().unwrap();
        let store = ProjectStore::open(tmp.path());
        store.ensure().unwrap();
        assert!(store.src.is_dir());
        assert!(store.build.is_dir());
        assert!(store.install.is_dir());
    }
}
