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
}

/// Walk parent directories looking for a rosup.toml file.
fn find_root(start: &Path) -> Option<PathBuf> {
    let mut current = if start.is_absolute() {
        start.to_owned()
    } else {
        std::env::current_dir().ok()?.join(start)
    };

    loop {
        if current.join("rosup.toml").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
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
    use crate::test_helpers::copy_fixture;
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
}
