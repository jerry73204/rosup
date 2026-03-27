//! Parser for vcstool `.repos` files.
//!
//! The `.repos` format is YAML with the structure:
//!
//! ```yaml
//! repositories:
//!   path/to/repo:
//!     type: git
//!     url: https://github.com/org/repo.git
//!     version: 1.5.0     # tag, branch, or commit SHA
//! ```
//!
//! Each repo may contain multiple ROS packages. We need to map
//! package names to repo entries at resolution time.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReposFileError {
    #[error("failed to read .repos file {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse .repos file {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },
}

/// A single repository entry from a .repos file.
#[derive(Debug, Clone)]
pub struct RepoEntry {
    /// Relative path where the repo would be cloned (e.g. "core/autoware_core").
    pub path: String,
    /// Git clone URL.
    pub url: String,
    /// Version: tag, branch, or commit SHA.
    pub version: String,
}

/// Parsed contents of a .repos file.
#[derive(Debug)]
pub struct ReposFile {
    /// Source name (from rosup.toml `[[resolve.sources]]`).
    pub name: String,
    /// All repository entries.
    pub repos: Vec<RepoEntry>,
}

// ── YAML deserialization ─────────────────────────────────────────────────────

#[derive(Deserialize)]
struct RawReposFile {
    repositories: HashMap<String, RawRepoEntry>,
}

#[derive(Deserialize)]
struct RawRepoEntry {
    #[serde(rename = "type")]
    _type: Option<String>,
    url: String,
    version: String,
}

impl ReposFile {
    /// Parse a .repos file from a path.
    pub fn parse(path: &Path, name: &str) -> Result<Self, ReposFileError> {
        let content = std::fs::read_to_string(path).map_err(|e| ReposFileError::Io {
            path: path.display().to_string(),
            source: e,
        })?;
        Self::parse_str(&content, name, path)
    }

    /// Parse a .repos file from a string.
    pub fn parse_str(yaml: &str, name: &str, path: &Path) -> Result<Self, ReposFileError> {
        let raw: RawReposFile = serde_yaml::from_str(yaml).map_err(|e| ReposFileError::Parse {
            path: path.display().to_string(),
            source: e,
        })?;

        let mut repos: Vec<RepoEntry> = raw
            .repositories
            .into_iter()
            .map(|(path, entry)| RepoEntry {
                path,
                url: entry.url,
                version: entry.version,
            })
            .collect();
        repos.sort_by(|a, b| a.path.cmp(&b.path));

        Ok(Self {
            name: name.to_owned(),
            repos,
        })
    }

    /// Look up a repo entry by package name.
    ///
    /// Since a repo may contain multiple packages, we need to check if
    /// the package name matches any known package in each repo. This
    /// requires a pre-built package → repo index.
    pub fn find_by_package<'a>(
        index: &'a HashMap<String, &'a RepoEntry>,
        pkg_name: &str,
    ) -> Option<&'a RepoEntry> {
        index.get(pkg_name).copied()
    }
}

/// Build a package name → RepoEntry index from a .repos file.
///
/// Uses three strategies to map package names to repo entries:
///
/// 1. **Git scan** — clone the repo (bare, shallow) and scan for
///    `package.xml` files to extract package names. Most accurate,
///    works for all repos including multi-package and private repos.
///
/// 2. **rosdistro URL match** — if a rosdistro package's source URL
///    matches a repo entry's URL, that repo provides that package.
///    No clone needed.
///
/// 3. **Path-based fallback** — the last segment of the repo path is
///    treated as a package name. No clone needed. Works for
///    single-package repos.
///
/// `git_cache_dir` is the bare clone cache (e.g. `~/.rosup/src/`).
/// If `None`, only strategies 2 and 3 are used (no cloning).
pub fn build_package_index<'a>(
    repos_file: &'a ReposFile,
    rosdistro_packages: &HashMap<String, String>,
    git_cache_dir: Option<&Path>,
) -> HashMap<String, &'a RepoEntry> {
    let mut index = HashMap::new();

    // Strategy 1: git scan (clone + ls-tree)
    if let Some(cache_dir) = git_cache_dir {
        for entry in &repos_file.repos {
            let repo_name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
            let packages = scan_repo_packages(cache_dir, repo_name, &entry.url, &entry.version);
            for pkg_name in packages {
                index.entry(pkg_name).or_insert(entry);
            }
        }
    }

    // Strategy 2: rosdistro URL match (fills gaps for packages not found by scan)
    for (pkg_name, source_url) in rosdistro_packages {
        if index.contains_key(pkg_name) {
            continue;
        }
        let normalized = source_url.trim_end_matches(".git");
        for entry in &repos_file.repos {
            let entry_normalized = entry.url.trim_end_matches(".git");
            if normalized == entry_normalized {
                index.insert(pkg_name.clone(), entry);
                break;
            }
        }
    }

    // Strategy 3: last path segment (fills remaining gaps)
    for entry in &repos_file.repos {
        let last_segment = entry.path.rsplit('/').next().unwrap_or(&entry.path);
        if !index.contains_key(last_segment) {
            index.insert(last_segment.to_owned(), entry);
        }
    }

    index
}

/// Clone a repo (bare, shallow) into the cache and scan for package names.
///
/// Uses `git ls-tree` to list all `package.xml` files, then `git show`
/// to read each one and extract `<name>`. No worktree needed.
fn scan_repo_packages(cache_dir: &Path, repo_name: &str, url: &str, version: &str) -> Vec<String> {
    let bare_path = cache_dir.join(format!("{repo_name}.git"));

    // Ensure bare clone exists
    if !bare_path.exists() {
        let status = std::process::Command::new("git")
            .args(["clone", "--bare", "--filter=blob:none", "--quiet", url])
            .arg(&bare_path)
            .status();
        if status.map(|s| !s.success()).unwrap_or(true) {
            tracing::debug!("failed to clone {url} for package scan");
            return Vec::new();
        }
    } else {
        // Fetch latest
        let _ = std::process::Command::new("git")
            .args(["fetch", "--quiet", "origin"])
            .current_dir(&bare_path)
            .status();
    }

    // Resolve the version to a ref git understands
    let ref_name = resolve_git_ref(&bare_path, version);

    // List all package.xml files
    let output = std::process::Command::new("git")
        .args(["ls-tree", "-r", "--name-only", &ref_name])
        .current_dir(&bare_path)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => {
            tracing::debug!("git ls-tree failed for {repo_name}");
            return Vec::new();
        }
    };

    let file_list = String::from_utf8_lossy(&output.stdout);
    let pkg_xml_paths: Vec<&str> = file_list
        .lines()
        .filter(|l| l.ends_with("/package.xml") || *l == "package.xml")
        .collect();

    // Extract <name> from each package.xml via git show
    let mut packages = Vec::new();
    for path in pkg_xml_paths {
        let blob_ref = format!("{ref_name}:{path}");
        let show = std::process::Command::new("git")
            .args(["show", &blob_ref])
            .current_dir(&bare_path)
            .output();
        if let Ok(o) = show
            && o.status.success()
        {
            let content = String::from_utf8_lossy(&o.stdout);
            if let Some(name) = extract_package_name(&content) {
                packages.push(name);
            }
        }
    }

    tracing::debug!("scanned {repo_name}: found {} packages", packages.len());
    packages
}

/// Resolve a version string (tag, branch, or SHA) to a git ref.
fn resolve_git_ref(bare_path: &Path, version: &str) -> String {
    // Try as-is first (works for branches, tags, and full SHAs)
    let check = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &format!("{version}^{{commit}}")])
        .current_dir(bare_path)
        .output();
    if let Ok(o) = check
        && o.status.success()
    {
        return version.to_owned();
    }
    // Try origin/<version> for branches
    let remote_ref = format!("origin/{version}");
    let check = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &format!("{remote_ref}^{{commit}}")])
        .current_dir(bare_path)
        .output();
    if let Ok(o) = check
        && o.status.success()
    {
        return remote_ref;
    }
    // Fallback to HEAD
    "HEAD".to_owned()
}

/// Extract `<name>...</name>` from a package.xml string.
fn extract_package_name(xml: &str) -> Option<String> {
    // Simple regex-free extraction
    let start = xml.find("<name>")?;
    let after = &xml[start + 6..];
    let end = after.find("</name>")?;
    let name = after[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    const SAMPLE_REPOS: &str = r#"
repositories:
  core/autoware_msgs:
    type: git
    url: https://github.com/autowarefoundation/autoware_msgs.git
    version: 1.11.0
  core/autoware_core:
    type: git
    url: https://github.com/autowarefoundation/autoware_core.git
    version: 1.5.0
  universe/autoware_universe:
    type: git
    url: https://github.com/autowarefoundation/autoware_universe.git
    version: 0.48.0
  universe/external/muSSP:
    type: git
    url: https://github.com/tier4/muSSP.git
    version: c79e98fd5e658f4f90c06d93472faa977bc873b9
"#;

    #[test]
    fn parse_repos_file() {
        let file = ReposFile::parse_str(SAMPLE_REPOS, "autoware", &PathBuf::from("autoware.repos"))
            .unwrap();
        assert_eq!(file.name, "autoware");
        assert_eq!(file.repos.len(), 4);
    }

    #[test]
    fn repos_sorted_by_path() {
        let file =
            ReposFile::parse_str(SAMPLE_REPOS, "test", &PathBuf::from("test.repos")).unwrap();
        let paths: Vec<&str> = file.repos.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "core/autoware_core",
                "core/autoware_msgs",
                "universe/autoware_universe",
                "universe/external/muSSP",
            ]
        );
    }

    #[test]
    fn version_can_be_tag_branch_or_sha() {
        let file =
            ReposFile::parse_str(SAMPLE_REPOS, "test", &PathBuf::from("test.repos")).unwrap();
        let versions: Vec<&str> = file.repos.iter().map(|r| r.version.as_str()).collect();
        assert!(versions.contains(&"1.5.0")); // tag
        assert!(versions.contains(&"0.48.0")); // tag
        assert!(versions.contains(&"c79e98fd5e658f4f90c06d93472faa977bc873b9")); // SHA
    }

    #[test]
    fn build_index_by_url_match() {
        let file =
            ReposFile::parse_str(SAMPLE_REPOS, "test", &PathBuf::from("test.repos")).unwrap();

        // Simulate rosdistro: autoware_core comes from the autoware_core repo
        let mut rosdistro = HashMap::new();
        rosdistro.insert(
            "autoware_core".to_owned(),
            "https://github.com/autowarefoundation/autoware_core.git".to_owned(),
        );

        let index = build_package_index(&file, &rosdistro, None);
        let entry = index.get("autoware_core").unwrap();
        assert_eq!(entry.version, "1.5.0");
        assert_eq!(entry.path, "core/autoware_core");
    }

    #[test]
    fn build_index_by_path_heuristic() {
        let file =
            ReposFile::parse_str(SAMPLE_REPOS, "test", &PathBuf::from("test.repos")).unwrap();

        // No rosdistro match — falls back to path heuristic
        let rosdistro = HashMap::new();
        let index = build_package_index(&file, &rosdistro, None);

        // "autoware_core" matches last segment of "core/autoware_core"
        assert!(index.contains_key("autoware_core"));
        // "muSSP" matches last segment of "universe/external/muSSP"
        assert!(index.contains_key("muSSP"));
    }

    #[test]
    fn url_normalization_strips_git_suffix() {
        let file =
            ReposFile::parse_str(SAMPLE_REPOS, "test", &PathBuf::from("test.repos")).unwrap();

        // URL without .git suffix should still match
        let mut rosdistro = HashMap::new();
        rosdistro.insert(
            "autoware_core".to_owned(),
            "https://github.com/autowarefoundation/autoware_core".to_owned(), // no .git
        );

        let index = build_package_index(&file, &rosdistro, None);
        assert!(index.contains_key("autoware_core"));
    }
}
