use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::config::{RoxConfig, WorkspaceConfig};
use crate::package_xml::{self, PackageManifest};

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("no rox.toml found in {0} or any parent directory — run `rox init`")]
    NotFound(PathBuf),
    #[error("failed to read rox.toml at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse rox.toml at {path}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml::de::Error,
    },
    #[error(transparent)]
    Config(#[from] eyre::Error),
    #[error(transparent)]
    PackageXml(#[from] package_xml::PackageXmlError),
    #[error("glob error: {0}")]
    Glob(#[from] glob::PatternError),
}

/// A fully loaded rox project rooted at a specific directory.
#[derive(Debug)]
pub struct Project {
    /// Directory containing rox.toml.
    pub root: PathBuf,
    pub config: RoxConfig,
}

impl Project {
    /// Load a project by walking up from `start` until a rox.toml is found.
    pub fn load_from(start: &Path) -> Result<Self, ProjectError> {
        let root = find_root(start).ok_or_else(|| ProjectError::NotFound(start.to_owned()))?;
        Self::load_at(&root)
    }

    /// Load a project from an explicit root directory.
    pub fn load_at(root: &Path) -> Result<Self, ProjectError> {
        let toml_path = root.join("rox.toml");
        let content = std::fs::read_to_string(&toml_path).map_err(|e| ProjectError::Io {
            path: toml_path.clone(),
            source: e,
        })?;
        let config: RoxConfig =
            toml::from_str(&content).map_err(|e| ProjectError::Parse {
                path: toml_path,
                source: e,
            })?;
        Ok(Self {
            root: root.to_owned(),
            config,
        })
    }

    /// Discover member packages for a workspace project.
    pub fn members(&self) -> Result<Vec<PackageManifest>, ProjectError> {
        let ws = match &self.config.workspace {
            Some(ws) => ws,
            None => return Ok(Vec::new()),
        };
        discover_members(&self.root, ws)
    }
}

/// Walk parent directories looking for a rox.toml file.
fn find_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_absolute() {
        start.to_owned()
    } else {
        std::env::current_dir().ok()?.join(start)
    };

    loop {
        if current.join("rox.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Expand glob patterns in workspace members and discover package.xml files.
fn discover_members(
    root: &Path,
    ws: &WorkspaceConfig,
) -> Result<Vec<PackageManifest>, ProjectError> {
    let excludes: Vec<glob::Pattern> = ws
        .exclude
        .iter()
        .map(|p| glob::Pattern::new(p))
        .collect::<Result<_, _>>()?;

    let mut manifests = Vec::new();

    for pattern in &ws.members {
        let abs_pattern = root.join(pattern).display().to_string();
        for entry in glob::glob(&abs_pattern).map_err(glob::PatternError::from)? {
            let entry = match entry {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Check against exclude patterns (relative to root).
            let rel = entry.strip_prefix(root).unwrap_or(&entry);
            let rel_str = rel.display().to_string();
            if excludes.iter().any(|ex| ex.matches(&rel_str)) {
                continue;
            }

            let pkg_xml = entry.join("package.xml");
            if pkg_xml.exists() {
                tracing::debug!("discovered member: {}", entry.display());
                manifests.push(package_xml::parse_file(&pkg_xml)?);
            }
        }
    }

    Ok(manifests)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    fn minimal_package_xml(name: &str) -> String {
        format!(
            r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.1.0</version>
  <description>test</description>
  <maintainer email="x@x.com">X</maintainer>
  <license>MIT</license>
  <buildtool_depend>ament_cmake</buildtool_depend>
</package>"#
        )
    }

    #[test]
    fn find_root_walks_up() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "rox.toml", "[package]\nname = \"x\"\n");
        let nested = tmp.path().join("a/b/c");
        fs::create_dir_all(&nested).unwrap();
        assert_eq!(find_root(&nested).unwrap(), tmp.path());
    }

    #[test]
    fn find_root_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        assert!(find_root(tmp.path()).is_none());
    }

    #[test]
    fn load_package_project() {
        let tmp = TempDir::new().unwrap();
        write(tmp.path(), "rox.toml", "[package]\nname = \"my_pkg\"\n");
        let project = Project::load_at(tmp.path()).unwrap();
        assert!(project.config.package.is_some());
    }

    #[test]
    fn discover_workspace_members() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let pkg_a = src.join("pkg_a");
        let pkg_b = src.join("pkg_b");
        let pkg_ex = src.join("experimental_pkg");
        fs::create_dir_all(&pkg_a).unwrap();
        fs::create_dir_all(&pkg_b).unwrap();
        fs::create_dir_all(&pkg_ex).unwrap();

        write(&pkg_a, "package.xml", &minimal_package_xml("pkg_a"));
        write(&pkg_b, "package.xml", &minimal_package_xml("pkg_b"));
        write(&pkg_ex, "package.xml", &minimal_package_xml("experimental_pkg"));

        write(
            tmp.path(),
            "rox.toml",
            r#"[workspace]
members = ["src/*"]
exclude = ["src/experimental_*"]
"#,
        );

        let project = Project::load_at(tmp.path()).unwrap();
        let mut members = project.members().unwrap();
        members.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name, "pkg_a");
        assert_eq!(members[1].name, "pkg_b");
    }
}
