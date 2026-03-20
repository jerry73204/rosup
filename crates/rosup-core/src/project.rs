use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::config::{RosupConfig, WorkspaceConfig};
use crate::init::{self, InitError};
use crate::package_xml::{self, PackageManifest};

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("no rosup.toml found in {0} or any parent directory — run `rosup init`")]
    NotFound(PathBuf),
    #[error("failed to read rosup.toml at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse rosup.toml at {path}: {source}")]
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
    #[error(transparent)]
    Init(#[from] InitError),
}

/// A fully loaded rosup project rooted at a specific directory.
#[derive(Debug)]
pub struct Project {
    /// Directory containing rosup.toml.
    pub root: PathBuf,
    pub config: RosupConfig,
}

impl Project {
    /// Load a project by walking up from `start` until a rosup.toml is found.
    pub fn load_from(start: &Path) -> Result<Self, ProjectError> {
        let root = find_root(start).ok_or_else(|| ProjectError::NotFound(start.to_owned()))?;
        Self::load_at(&root)
    }

    /// Load a project from an explicit manifest path, or fall back to the
    /// upward directory search from `$CWD`.
    pub fn load(manifest_path: Option<&Path>) -> Result<Self, ProjectError> {
        match manifest_path {
            Some(path) => {
                let root = path
                    .parent()
                    .ok_or_else(|| ProjectError::NotFound(path.to_owned()))?;
                Self::load_at(root)
            }
            None => {
                let cwd = std::env::current_dir().map_err(|e| ProjectError::Io {
                    path: PathBuf::from("."),
                    source: e,
                })?;
                Self::load_from(&cwd)
            }
        }
    }

    /// Load a project from an explicit root directory.
    pub fn load_at(root: &Path) -> Result<Self, ProjectError> {
        let toml_path = root.join("rosup.toml");
        let content = std::fs::read_to_string(&toml_path).map_err(|e| ProjectError::Io {
            path: toml_path.clone(),
            source: e,
        })?;
        let config: RosupConfig = toml::from_str(&content).map_err(|e| ProjectError::Parse {
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

    /// Collect all unique external dependency names.
    ///
    /// In workspace mode, deps whose names match workspace member package
    /// names are excluded — colcon builds those from source.
    /// In single-package mode, all deps are returned.
    ///
    /// Returns `(dep_names, member_names)`.
    pub fn external_deps(
        &self,
    ) -> Result<(Vec<String>, std::collections::HashSet<String>), ProjectError> {
        let manifests = if self.config.workspace.is_some() {
            self.members()?
        } else {
            let pkg_xml = self.root.join("package.xml");
            if pkg_xml.exists() {
                vec![package_xml::parse_file(&pkg_xml)?]
            } else {
                vec![]
            }
        };

        let member_names: std::collections::HashSet<String> =
            manifests.iter().map(|m| m.name.clone()).collect();

        let mut dep_names: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for pkg in &manifests {
            for dep in pkg.deps.all() {
                if member_names.contains(dep) {
                    continue;
                }
                if seen.insert(dep.to_owned()) {
                    dep_names.push(dep.to_owned());
                }
            }
        }
        Ok((dep_names, member_names))
    }

    /// Collect all dependency names (including workspace members).
    ///
    /// Unlike `external_deps`, this does NOT filter out workspace members.
    /// Used by `rosup tree` to show the complete dependency picture.
    pub fn all_dep_names(&self) -> Result<Vec<String>, ProjectError> {
        let manifests = if self.config.workspace.is_some() {
            self.members()?
        } else {
            let pkg_xml = self.root.join("package.xml");
            if pkg_xml.exists() {
                vec![package_xml::parse_file(&pkg_xml)?]
            } else {
                vec![]
            }
        };

        let mut dep_names: Vec<String> = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for pkg in &manifests {
            for dep in pkg.deps.all() {
                if seen.insert(dep.to_owned()) {
                    dep_names.push(dep.to_owned());
                }
            }
        }
        Ok(dep_names)
    }
}

/// Walk parent directories looking for a rosup.toml file.
///
/// Stops at:
/// - The filesystem root (no more parents).
/// - The user's home directory (safety bound — avoids picking up stale
///   manifests from unrelated parent projects).
/// - A filesystem boundary (different device ID on Unix).
fn find_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_absolute() {
        start.to_owned()
    } else {
        std::env::current_dir().ok()?.join(start)
    };

    let home = dirs::home_dir();
    let start_dev = device_id(&current);

    loop {
        if current.join("rosup.toml").exists() {
            return Some(current);
        }

        // Stop at home directory — don't search above it.
        if let Some(ref h) = home
            && current == *h
        {
            return None;
        }

        let parent = current.parent()?.to_owned();

        // Stop at filesystem boundary (different mount point).
        if let Some(start_dev) = start_dev
            && device_id(&parent) != Some(start_dev)
        {
            return None;
        }

        if !current.pop() {
            return None;
        }
    }
}

/// Get the device ID for a path (Unix only). Returns `None` on non-Unix
/// platforms or on error.
#[cfg(unix)]
fn device_id(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|m| m.dev())
}

#[cfg(not(unix))]
fn device_id(_path: &Path) -> Option<u64> {
    None
}

/// Discover member packages for a workspace.
///
/// - `members = None`: Colcon auto-discovery via `colcon_scan`.
/// - `members = Some(patterns)`: expand each glob; warn when a pattern matches
///   nothing; apply `exclude` patterns.
fn discover_members(
    root: &Path,
    ws: &WorkspaceConfig,
) -> Result<Vec<PackageManifest>, ProjectError> {
    let excludes: Vec<glob::Pattern> = ws
        .exclude
        .iter()
        .map(|p| glob::Pattern::new(p))
        .collect::<Result<_, _>>()?;

    let rel_paths: Vec<String> = match &ws.members {
        // Auto-discovery: delegate to Colcon rules.
        None => init::colcon_scan(root, &excludes)?,

        // Explicit glob patterns.
        Some(patterns) => {
            let mut paths = Vec::new();
            for pattern in patterns {
                let abs_pattern = root.join(pattern).display().to_string();
                let mut matched = 0usize;
                for entry in glob::glob(&abs_pattern)? {
                    let entry = match entry {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    // Only directories with a package.xml directly inside.
                    if !entry.is_dir() || !entry.join("package.xml").exists() {
                        continue;
                    }
                    let rel = entry.strip_prefix(root).unwrap_or(&entry);
                    let rel_str = rel.display().to_string();
                    if excludes.iter().any(|ex| ex.matches(&rel_str)) {
                        continue;
                    }
                    paths.push(rel_str);
                    matched += 1;
                }
                if matched == 0 {
                    tracing::warn!("workspace member pattern `{pattern}` matched no packages");
                }
            }
            paths
        }
    };

    rel_paths
        .iter()
        .map(|rel| {
            let pkg_xml = root.join(rel).join("package.xml");
            tracing::debug!("discovered member: {rel}");
            Ok(package_xml::parse_file(&pkg_xml)?)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{copy_fixture, copy_fixture_dir};
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn find_root_walks_up() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("rosup.toml"), "[package]\nname = \"x\"\n").unwrap();
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
        fs::write(
            tmp.path().join("rosup.toml"),
            "[package]\nname = \"my_pkg\"\n",
        )
        .unwrap();
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

        copy_fixture("package_xml/pkg_a.xml", &pkg_a, "package.xml");
        copy_fixture("package_xml/pkg_b.xml", &pkg_b, "package.xml");
        copy_fixture("package_xml/experimental_pkg.xml", &pkg_ex, "package.xml");

        fs::write(
            tmp.path().join("rosup.toml"),
            "[workspace]\nmembers = [\"src/*\"]\nexclude = [\"src/experimental_*\"]\n",
        )
        .unwrap();

        let project = Project::load_at(tmp.path()).unwrap();
        let mut members = project.members().unwrap();
        members.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name, "pkg_a");
        assert_eq!(members[1].name, "pkg_b");
    }

    #[test]
    fn auto_discovery_finds_packages() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("pkg_a")).unwrap();
        fs::create_dir_all(src.join("pkg_b")).unwrap();
        copy_fixture("package_xml/pkg_a.xml", &src.join("pkg_a"), "package.xml");
        copy_fixture("package_xml/pkg_b.xml", &src.join("pkg_b"), "package.xml");

        fs::write(tmp.path().join("rosup.toml"), "[workspace]\n").unwrap();

        let project = Project::load_at(tmp.path()).unwrap();
        let mut members = project.members().unwrap();
        members.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(members.len(), 2);
        assert_eq!(members[0].name, "pkg_a");
        assert_eq!(members[1].name, "pkg_b");
    }

    #[test]
    fn auto_discovery_respects_exclude() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(src.join("pkg_a")).unwrap();
        fs::create_dir_all(src.join("experimental_pkg")).unwrap();
        copy_fixture("package_xml/pkg_a.xml", &src.join("pkg_a"), "package.xml");
        copy_fixture(
            "package_xml/experimental_pkg.xml",
            &src.join("experimental_pkg"),
            "package.xml",
        );

        fs::write(
            tmp.path().join("rosup.toml"),
            "[workspace]\nexclude = [\"src/experimental_*\"]\n",
        )
        .unwrap();

        let project = Project::load_at(tmp.path()).unwrap();
        let members = project.members().unwrap();

        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "pkg_a");
    }

    #[test]
    fn glob_star_is_single_level() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        // pkg nested two levels deep — should NOT be found with `src/*`.
        let nested = src.join("sub").join("nested_pkg");
        fs::create_dir_all(src.join("pkg_a")).unwrap();
        fs::create_dir_all(&nested).unwrap();
        copy_fixture("package_xml/pkg_a.xml", &src.join("pkg_a"), "package.xml");
        copy_fixture("package_xml/nested_pkg.xml", &nested, "package.xml");

        fs::write(
            tmp.path().join("rosup.toml"),
            "[workspace]\nmembers = [\"src/*\"]\n",
        )
        .unwrap();

        let project = Project::load_at(tmp.path()).unwrap();
        let members = project.members().unwrap();

        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "pkg_a");
    }

    #[test]
    fn glob_double_star_is_recursive() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let nested = src.join("sub").join("nested_pkg");
        fs::create_dir_all(src.join("pkg_a")).unwrap();
        fs::create_dir_all(&nested).unwrap();
        copy_fixture("package_xml/pkg_a.xml", &src.join("pkg_a"), "package.xml");
        copy_fixture("package_xml/nested_pkg.xml", &nested, "package.xml");

        fs::write(
            tmp.path().join("rosup.toml"),
            "[workspace]\nmembers = [\"src/**\"]\n",
        )
        .unwrap();

        let project = Project::load_at(tmp.path()).unwrap();
        let mut members = project.members().unwrap();
        members.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(members.len(), 2);
        let names: Vec<&str> = members.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"pkg_a"));
        assert!(names.contains(&"nested_pkg"));
    }

    #[test]
    fn deep_exclude_filters_nested_test_packages_auto_discovery() {
        let tmp = TempDir::new().unwrap();
        copy_fixture_dir("workspaces/deep_exclude", tmp.path());

        fs::write(
            tmp.path().join("rosup.toml"),
            "[workspace]\nexclude = [\"**/tests/**\"]\n",
        )
        .unwrap();

        let project = Project::load_at(tmp.path()).unwrap();
        let members = project.members().unwrap();

        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "pkg_a");
    }

    #[test]
    fn deep_exclude_filters_nested_test_packages_glob_mode() {
        let tmp = TempDir::new().unwrap();
        copy_fixture_dir("workspaces/deep_exclude", tmp.path());

        fs::write(
            tmp.path().join("rosup.toml"),
            "[workspace]\nmembers = [\"src/**\"]\nexclude = [\"**/tests/**\"]\n",
        )
        .unwrap();

        let project = Project::load_at(tmp.path()).unwrap();
        let members = project.members().unwrap();

        assert_eq!(members.len(), 1);
        assert_eq!(members[0].name, "pkg_a");
    }
}
