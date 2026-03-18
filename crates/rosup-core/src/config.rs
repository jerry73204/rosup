use serde::Deserialize;
use std::collections::HashMap;

/// Top-level rosup.toml structure. Either `package` or `workspace` must be set,
/// but not both.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RosupConfig {
    pub package: Option<PackageConfig>,
    pub workspace: Option<WorkspaceConfig>,
    #[serde(default)]
    pub resolve: ResolveConfig,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub test: TestConfig,
}

impl RosupConfig {
    pub fn mode(&self) -> eyre::Result<ProjectMode> {
        match (&self.package, &self.workspace) {
            (Some(_), None) => Ok(ProjectMode::Package),
            (None, Some(_)) => Ok(ProjectMode::Workspace),
            (Some(_), Some(_)) => {
                eyre::bail!("`[package]` and `[workspace]` cannot both be set in rosup.toml")
            }
            (None, None) => {
                eyre::bail!("rosup.toml must contain either `[package]` or `[workspace]`")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectMode {
    Package,
    Workspace,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackageConfig {
    /// Must match `<name>` in package.xml.
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    /// Glob patterns or paths to member package directories.
    pub members: Vec<String>,
    /// Glob patterns to exclude from member discovery.
    #[serde(default)]
    pub exclude: Vec<String>,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ResolveConfig {
    /// ROS distribution (e.g. "jazzy"). Auto-detected from environment if absent.
    pub ros_distro: Option<String>,
    /// Dependency resolution preference.
    #[serde(default)]
    pub source_preference: SourcePreference,
    /// Per-dependency source overrides.
    #[serde(default)]
    pub overrides: HashMap<String, DepOverride>,
}

#[derive(Debug, Default, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourcePreference {
    /// Prefer rosdep binary packages; fall back to source.
    #[default]
    Auto,
    /// Always use rosdep binary packages.
    Binary,
    /// Always pull source.
    Source,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct DepOverride {
    pub git: String,
    pub branch: Option<String>,
    pub rev: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct BuildConfig {
    #[serde(default)]
    pub cmake_args: Vec<String>,
    pub parallel_jobs: Option<usize>,
    pub symlink_install: bool,
    /// Per-package build overrides (workspace mode only).
    #[serde(default)]
    pub overrides: HashMap<String, BuildOverride>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            cmake_args: Vec::new(),
            parallel_jobs: None,
            symlink_install: true,
            overrides: HashMap::new(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildOverride {
    #[serde(default)]
    pub cmake_args: Vec<String>,
    pub parallel_jobs: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestConfig {
    pub parallel_jobs: Option<usize>,
    pub retest_until_pass: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> RosupConfig {
        toml::from_str(s).expect("parse failed")
    }

    #[test]
    fn package_mode() {
        let cfg = parse(
            r#"[package]
name = "my_pkg"
"#,
        );
        assert_eq!(cfg.mode().unwrap(), ProjectMode::Package);
        assert_eq!(cfg.package.unwrap().name, "my_pkg");
    }

    #[test]
    fn workspace_mode() {
        let cfg = parse(
            r#"[workspace]
members = ["src/*"]
"#,
        );
        assert_eq!(cfg.mode().unwrap(), ProjectMode::Workspace);
        assert_eq!(cfg.workspace.unwrap().members, vec!["src/*"]);
    }

    #[test]
    fn both_sections_rejected() {
        let cfg = parse(
            r#"[package]
name = "x"
[workspace]
members = []
"#,
        );
        assert!(cfg.mode().is_err());
    }

    #[test]
    fn neither_section_rejected() {
        let cfg = parse("");
        assert!(cfg.mode().is_err());
    }

    #[test]
    fn resolve_override() {
        let cfg = parse(
            r#"[package]
name = "my_pkg"

[resolve.overrides.nav2_core]
git = "https://github.com/fork/nav2.git"
branch = "my-fix"
"#,
        );
        let ov = &cfg.resolve.overrides["nav2_core"];
        assert_eq!(ov.git, "https://github.com/fork/nav2.git");
        assert_eq!(ov.branch.as_deref(), Some("my-fix"));
    }

    #[test]
    fn symlink_install_defaults_true() {
        let cfg = parse("[package]\nname = \"x\"\n");
        assert!(cfg.build.symlink_install);
    }
}
