use std::path::PathBuf;
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

/// Manages the `~/.rox/` directory layout.
#[derive(Debug, Clone)]
pub struct RoxDir {
    pub root: PathBuf,
    pub cache: PathBuf,
    pub src: PathBuf,
    pub build: PathBuf,
    pub install: PathBuf,
}

impl RoxDir {
    pub fn open() -> Result<Self, StoreError> {
        let home = dirs::home_dir().ok_or(StoreError::NoHome)?;
        let root = home.join(".rox");
        Ok(Self {
            cache: root.join("cache"),
            src: root.join("src"),
            build: root.join("build"),
            install: root.join("install"),
            root,
        })
    }

    /// Create all directories if they don't exist.
    pub fn ensure(&self) -> Result<(), StoreError> {
        for dir in [
            &self.root,
            &self.cache,
            &self.src,
            &self.build,
            &self.install,
        ] {
            std::fs::create_dir_all(dir).map_err(|e| StoreError::Io {
                path: dir.clone(),
                source: e,
            })?;
        }
        Ok(())
    }
}
