use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PackageXmlError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse package.xml at {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: quick_xml::DeError,
    },
}

/// Parsed representation of a ROS package.xml (format 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    /// Value of <export><build_type>, e.g. "ament_cmake", "ament_python".
    pub build_type: Option<String>,
    /// All dependency declarations, grouped by type.
    pub deps: Dependencies,
}

/// All dependency types from package.xml format 3.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dependencies {
    /// `<depend>` — build + build_export + exec
    pub depend: Vec<String>,
    pub build_depend: Vec<String>,
    pub build_export_depend: Vec<String>,
    pub buildtool_depend: Vec<String>,
    pub exec_depend: Vec<String>,
    pub test_depend: Vec<String>,
    pub doc_depend: Vec<String>,
}

impl Dependencies {
    /// All unique dependency names across all types.
    pub fn all(&self) -> Vec<&str> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for name in self
            .depend
            .iter()
            .chain(&self.build_depend)
            .chain(&self.build_export_depend)
            .chain(&self.buildtool_depend)
            .chain(&self.exec_depend)
            .chain(&self.test_depend)
            .chain(&self.doc_depend)
        {
            if seen.insert(name.as_str()) {
                result.push(name.as_str());
            }
        }
        result
    }
}

// ── XML deserialization helpers ──────────────────────────────────────────────

/// Intermediate structure that maps directly onto the package.xml XML schema.
#[derive(Debug, serde::Deserialize)]
#[serde(rename = "package")]
struct RawPackage {
    name: String,
    version: String,
    description: String,
    #[serde(rename = "depend", default)]
    depend: Vec<String>,
    #[serde(rename = "build_depend", default)]
    build_depend: Vec<String>,
    #[serde(rename = "build_export_depend", default)]
    build_export_depend: Vec<String>,
    #[serde(rename = "buildtool_depend", default)]
    buildtool_depend: Vec<String>,
    #[serde(rename = "exec_depend", default)]
    exec_depend: Vec<String>,
    #[serde(rename = "test_depend", default)]
    test_depend: Vec<String>,
    #[serde(rename = "doc_depend", default)]
    doc_depend: Vec<String>,
    #[serde(rename = "export", default)]
    export: Option<RawExport>,
}

#[derive(Debug, serde::Deserialize, Default)]
struct RawExport {
    build_type: Option<String>,
}

pub fn parse_file(path: &Path) -> Result<PackageManifest, PackageXmlError> {
    let content = std::fs::read_to_string(path).map_err(|e| PackageXmlError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    parse_str(&content, path)
}

pub fn parse_str(xml: &str, path: &Path) -> Result<PackageManifest, PackageXmlError> {
    let raw: RawPackage = quick_xml::de::from_str(xml).map_err(|e| PackageXmlError::Parse {
        path: path.display().to_string(),
        source: e,
    })?;

    Ok(PackageManifest {
        name: raw.name,
        version: raw.version,
        description: raw.description,
        build_type: raw.export.and_then(|e| e.build_type),
        deps: Dependencies {
            depend: raw.depend,
            build_depend: raw.build_depend,
            build_export_depend: raw.build_export_depend,
            buildtool_depend: raw.buildtool_depend,
            exec_depend: raw.exec_depend,
            test_depend: raw.test_depend,
            doc_depend: raw.doc_depend,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{fixture, fixture_path};
    use std::path::PathBuf;

    fn dummy_path() -> PathBuf {
        PathBuf::from("package.xml")
    }

    #[test]
    fn parses_basic_manifest() {
        let manifest = parse_str(&fixture("package_xml/basic.xml"), &dummy_path()).unwrap();
        assert_eq!(manifest.name, "my_robot_nav");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.deps.depend, vec!["rclcpp", "sensor_msgs"]);
        assert_eq!(manifest.deps.buildtool_depend, vec!["ament_cmake"]);
        assert_eq!(manifest.deps.test_depend, vec!["ament_cmake_gtest"]);
        assert_eq!(manifest.build_type, None);
    }

    #[test]
    fn parses_build_type() {
        let manifest = parse_file(&fixture_path("package_xml/with_build_type.xml")).unwrap();
        assert_eq!(manifest.build_type.as_deref(), Some("ament_cmake"));
    }

    #[test]
    fn all_deps_deduplicates() {
        let manifest = parse_str(&fixture("package_xml/with_dup_deps.xml"), &dummy_path()).unwrap();
        let all = manifest.deps.all();
        assert_eq!(all.iter().filter(|&&d| d == "rclcpp").count(), 1);
        assert!(all.contains(&"sensor_msgs"));
    }
}
