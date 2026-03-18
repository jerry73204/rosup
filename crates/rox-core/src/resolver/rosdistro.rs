use std::{
    collections::HashMap,
    io::Read,
    path::Path,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use thiserror::Error;

/// Age after which the cache is considered stale and should be refreshed.
const CACHE_MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60); // 24 h

const CACHE_URL_TEMPLATE: &str = "https://repo.ros2.org/rosdistro_cache/{distro}-cache.yaml.gz";

#[derive(Debug, Error)]
pub enum RosdistroError {
    #[error("network error fetching rosdistro cache: {0}")]
    Network(#[from] reqwest::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to decompress cache: {0}")]
    Decompress(std::io::Error),
    #[error("failed to parse cache YAML: {0}")]
    Parse(#[from] serde_yaml::Error),
}

/// Source repository information for a ROS package.
#[derive(Debug, Clone)]
pub struct PackageSource {
    /// Repository name in rosdistro.
    pub repo: String,
    /// Git clone URL.
    pub url: String,
    /// Branch or tag.
    pub branch: String,
}

/// In-memory rosdistro distribution cache.
///
/// Maps package names to their source repository information.
#[derive(Debug)]
pub struct DistroCache {
    /// package_name → PackageSource
    index: HashMap<String, PackageSource>,
}

impl DistroCache {
    /// Load from a pre-parsed index (used in tests).
    pub fn from_index(index: HashMap<String, PackageSource>) -> Self {
        Self { index }
    }

    /// Load the cache from disk (or download it if missing/stale).
    pub fn load(
        cache_dir: &Path,
        distro: &str,
        force_refresh: bool,
    ) -> Result<Self, RosdistroError> {
        let cache_path = cache_dir.join(format!("{distro}-cache.yaml"));
        let ts_path = cache_dir.join(format!("{distro}-cache.timestamp"));

        let needs_refresh = force_refresh || !cache_path.exists() || is_stale(&ts_path);

        if needs_refresh {
            tracing::info!("fetching rosdistro cache for {distro}");
            fetch_and_store(distro, &cache_path, &ts_path)?;
        }

        let content = std::fs::read_to_string(&cache_path)?;
        let raw: RawDistroCache = serde_yaml::from_str(&content)?;
        Ok(Self {
            index: build_index(raw),
        })
    }

    /// Look up the source information for a package name.
    pub fn lookup(&self, pkg_name: &str) -> Option<&PackageSource> {
        self.index.get(pkg_name)
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

// ── cache fetch ───────────────────────────────────────────────────────────────

fn fetch_and_store(distro: &str, cache_path: &Path, ts_path: &Path) -> Result<(), RosdistroError> {
    let url = CACHE_URL_TEMPLATE.replace("{distro}", distro);
    let bytes = reqwest::blocking::get(&url)?.bytes()?;

    let mut decoder = GzDecoder::new(bytes.as_ref());
    let mut yaml = String::new();
    decoder
        .read_to_string(&mut yaml)
        .map_err(RosdistroError::Decompress)?;

    std::fs::write(cache_path, &yaml)?;

    // Write current timestamp (seconds since epoch).
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    std::fs::write(ts_path, ts.to_string())?;

    tracing::info!("rosdistro cache saved to {}", cache_path.display());
    Ok(())
}

fn is_stale(ts_path: &Path) -> bool {
    let Ok(content) = std::fs::read_to_string(ts_path) else {
        return true;
    };
    let Ok(ts) = content.trim().parse::<u64>() else {
        return true;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    now.saturating_sub(ts) > CACHE_MAX_AGE.as_secs()
}

// ── YAML serde types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct RawDistroCache {
    distribution_file: Vec<RawDistroFile>,
}

#[derive(Debug, Deserialize)]
struct RawDistroFile {
    repositories: HashMap<String, RawRepo>,
}

#[derive(Debug, Deserialize)]
struct RawRepo {
    source: Option<RawSource>,
    release: Option<RawRelease>,
}

#[derive(Debug, Deserialize)]
struct RawSource {
    #[serde(rename = "type")]
    kind: Option<String>,
    url: String,
    version: String,
}

#[derive(Debug, Deserialize)]
struct RawRelease {
    packages: Option<Vec<String>>,
}

// ── index building ────────────────────────────────────────────────────────────

fn build_index(raw: RawDistroCache) -> HashMap<String, PackageSource> {
    let mut index = HashMap::new();
    let Some(distro_file) = raw.distribution_file.into_iter().next() else {
        return index;
    };

    for (repo_name, repo) in distro_file.repositories {
        let Some(src) = repo.source else { continue };
        // Only git repos are supported for source pull.
        if src.kind.as_deref() != Some("git") && src.kind.is_some() {
            continue;
        }

        // Enumerate packages from release.packages; fall back to repo_name.
        let package_names: Vec<String> = repo
            .release
            .and_then(|r| r.packages)
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| vec![repo_name.clone()]);

        for pkg in package_names {
            let entry = PackageSource {
                repo: repo_name.clone(),
                url: src.url.clone(),
                branch: src.version.clone(),
            };
            index.insert(pkg, entry);
        }
    }

    index
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::fixture_path;

    fn load_fixture() -> DistroCache {
        let content =
            std::fs::read_to_string(fixture_path("rosdistro/humble-cache-sample.yaml")).unwrap();
        let raw: RawDistroCache = serde_yaml::from_str(&content).unwrap();
        DistroCache {
            index: build_index(raw),
        }
    }

    #[test]
    fn indexes_single_package_repo() {
        let cache = load_fixture();
        let src = cache.lookup("solo_pkg").unwrap();
        assert_eq!(src.url, "https://github.com/example/solo_pkg.git");
        assert_eq!(src.branch, "main");
        assert_eq!(src.repo, "solo_pkg");
    }

    #[test]
    fn indexes_multi_package_repo() {
        let cache = load_fixture();
        for pkg in &["sensor_msgs", "std_msgs", "geometry_msgs"] {
            let src = cache.lookup(pkg).unwrap();
            assert_eq!(src.url, "https://github.com/ros2/common_interfaces.git");
            assert_eq!(src.repo, "common_interfaces");
        }
    }

    #[test]
    fn rclcpp_repo_includes_sub_packages() {
        let cache = load_fixture();
        let rclcpp = cache.lookup("rclcpp").unwrap();
        assert!(rclcpp.url.contains("rclcpp"));
        let action = cache.lookup("rclcpp_action").unwrap();
        assert_eq!(action.url, rclcpp.url);
    }

    #[test]
    fn unknown_package_returns_none() {
        let cache = load_fixture();
        assert!(cache.lookup("totally_unknown_pkg").is_none());
    }

    #[test]
    fn skips_non_git_repos() {
        let cache = load_fixture();
        // The fixture includes a non-git entry that should be skipped.
        assert!(cache.lookup("svn_pkg").is_none());
    }
}
