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
        source: quick_xml::Error,
    },
    #[error("package.xml at {path} is missing required field `{field}`")]
    MissingField { path: String, field: &'static str },
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

/// A single dependency with optional version constraints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dependency {
    pub name: String,
    /// Version constraints from package.xml attributes.
    pub version: VersionConstraint,
}

impl Dependency {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: VersionConstraint::default(),
        }
    }
}

impl Dependencies {
    /// Helper: extract names from a dep list (for assertions).
    pub fn names(deps: &[Dependency]) -> Vec<&str> {
        deps.iter().map(|d| d.name.as_str()).collect()
    }
}

/// Optional version constraint attributes from package.xml format 3.
///
/// Two attributes can be combined to describe a range
/// (e.g. `version_gte="1.1" version_lt="2.0"`).
/// Colcon parses these but does not enforce them at build time —
/// they are used by bloom for Debian dependency generation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VersionConstraint {
    pub version_eq: Option<String>,
    pub version_gte: Option<String>,
    pub version_gt: Option<String>,
    pub version_lte: Option<String>,
    pub version_lt: Option<String>,
}

impl VersionConstraint {
    pub fn is_empty(&self) -> bool {
        self.version_eq.is_none()
            && self.version_gte.is_none()
            && self.version_gt.is_none()
            && self.version_lte.is_none()
            && self.version_lt.is_none()
    }
}

impl std::fmt::Display for VersionConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::new();
        if let Some(v) = &self.version_eq {
            parts.push(format!("=={v}"));
        }
        if let Some(v) = &self.version_gte {
            parts.push(format!(">={v}"));
        }
        if let Some(v) = &self.version_gt {
            parts.push(format!(">{v}"));
        }
        if let Some(v) = &self.version_lte {
            parts.push(format!("<={v}"));
        }
        if let Some(v) = &self.version_lt {
            parts.push(format!("<{v}"));
        }
        write!(f, "{}", parts.join(", "))
    }
}

/// All dependency types from package.xml format 3.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Dependencies {
    /// `<depend>` — build + build_export + exec
    pub depend: Vec<Dependency>,
    pub build_depend: Vec<Dependency>,
    pub build_export_depend: Vec<Dependency>,
    pub buildtool_depend: Vec<Dependency>,
    pub exec_depend: Vec<Dependency>,
    pub test_depend: Vec<Dependency>,
    pub doc_depend: Vec<Dependency>,
}

impl Dependencies {
    /// All unique dependency names across all types.
    pub fn all(&self) -> Vec<&str> {
        let mut seen = std::collections::HashSet::new();
        let mut result = Vec::new();
        for dep in self
            .depend
            .iter()
            .chain(&self.build_depend)
            .chain(&self.build_export_depend)
            .chain(&self.buildtool_depend)
            .chain(&self.exec_depend)
            .chain(&self.test_depend)
            .chain(&self.doc_depend)
        {
            if seen.insert(dep.name.as_str()) {
                result.push(dep.name.as_str());
            }
        }
        result
    }
}

/// Evaluate a REP-149 condition expression with `$ROS_VERSION=2`.
///
/// Handles simple comparisons like `$ROS_VERSION == 1`, `$ROS_VERSION != 2`,
/// and `$ROS_VERSION == 2`. For conditions we can't evaluate (complex
/// expressions with `and`/`or`/parentheses), returns `true` (conservative:
/// include the dep rather than silently dropping it).
/// Evaluate a REP-149 condition attribute. Delegates to the full
/// expression parser in `condition.rs`.
fn eval_condition(condition: &str) -> bool {
    crate::condition::eval_condition(condition)
}

pub fn parse_file(path: &Path) -> Result<PackageManifest, PackageXmlError> {
    let content = std::fs::read_to_string(path).map_err(|e| PackageXmlError::Io {
        path: path.display().to_string(),
        source: e,
    })?;
    parse_str(&content, path)
}

/// Parse a package.xml string using quick_xml's event-based reader.
///
/// Using the event-based API (rather than serde deserialization) correctly
/// handles interleaved repeated sibling elements such as:
///
/// ```xml
/// <depend>rclcpp</depend>
/// <exec_depend>sensor_msgs</exec_depend>
/// <depend>tf2</depend>   <!-- second <depend> would fail with serde -->
/// ```
pub fn parse_str(xml: &str, path: &Path) -> Result<PackageManifest, PackageXmlError> {
    use quick_xml::events::Event;

    let path_str = path.display().to_string();
    let map_err = |e| PackageXmlError::Parse {
        path: path_str.clone(),
        source: e,
    };

    let mut reader = quick_xml::Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut name: Option<String> = None;
    let mut version: Option<String> = None;
    let mut description: Option<String> = None;
    let mut build_type: Option<String> = None;
    let mut deps = Dependencies::default();

    // Per-element state tracked alongside the tag stack.
    struct ElementState {
        tag: String,
        skip: bool,
        version: VersionConstraint,
    }

    let mut stack: Vec<ElementState> = Vec::new();

    loop {
        match reader.read_event().map_err(map_err)? {
            Event::Start(e) | Event::Empty(e) => {
                let tag = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_owned();
                let mut skip = false;
                let mut ver = VersionConstraint::default();
                for attr in e.attributes().flatten() {
                    let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
                    let val = String::from_utf8_lossy(&attr.value).into_owned();
                    match key {
                        "condition" => {
                            if !eval_condition(&val) {
                                skip = true;
                            }
                        }
                        "version_eq" => ver.version_eq = Some(val),
                        "version_gte" => ver.version_gte = Some(val),
                        "version_gt" => ver.version_gt = Some(val),
                        "version_lte" => ver.version_lte = Some(val),
                        "version_lt" => ver.version_lt = Some(val),
                        _ => {}
                    }
                }
                stack.push(ElementState {
                    tag,
                    skip,
                    version: ver,
                });
            }
            Event::End(_) => {
                stack.pop();
            }
            Event::Text(e) => {
                let state = match stack.last() {
                    Some(s) => s,
                    None => continue,
                };
                if state.skip {
                    continue;
                }
                let text = e.unescape().map_err(map_err)?.into_owned();
                if text.is_empty() {
                    continue;
                }
                let make_dep = |name: String, ver: &VersionConstraint| Dependency {
                    name,
                    version: ver.clone(),
                };
                // Check depth: [.., parent, current] or [current].
                let depth = stack.len();
                let current_tag = &state.tag;
                let parent_tag = if depth >= 2 {
                    Some(stack[depth - 2].tag.as_str())
                } else {
                    None
                };

                if parent_tag == Some("export") && current_tag == "build_type" {
                    build_type = Some(text);
                    continue;
                }
                match current_tag.as_str() {
                    "name" if parent_tag.is_some() => name = Some(text),
                    "version" if parent_tag.is_some() => version = Some(text),
                    "description" if parent_tag.is_some() => description = Some(text),
                    "depend" => deps.depend.push(make_dep(text, &state.version)),
                    "build_depend" => deps.build_depend.push(make_dep(text, &state.version)),
                    "build_export_depend" => {
                        deps.build_export_depend
                            .push(make_dep(text, &state.version));
                    }
                    "buildtool_depend" => {
                        deps.buildtool_depend.push(make_dep(text, &state.version));
                    }
                    "exec_depend" => deps.exec_depend.push(make_dep(text, &state.version)),
                    "test_depend" => deps.test_depend.push(make_dep(text, &state.version)),
                    "doc_depend" => deps.doc_depend.push(make_dep(text, &state.version)),
                    "name" if parent_tag.is_none() => name = Some(text),
                    "version" if parent_tag.is_none() => version = Some(text),
                    "description" if parent_tag.is_none() => description = Some(text),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(PackageManifest {
        name: name.ok_or_else(|| PackageXmlError::MissingField {
            path: path_str.clone(),
            field: "name",
        })?,
        version: version.unwrap_or_default(),
        description: description.unwrap_or_default(),
        build_type,
        deps,
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
        assert_eq!(
            Dependencies::names(&manifest.deps.depend),
            vec!["rclcpp", "sensor_msgs"]
        );
        assert_eq!(
            Dependencies::names(&manifest.deps.buildtool_depend),
            vec!["ament_cmake"]
        );
        assert_eq!(
            Dependencies::names(&manifest.deps.test_depend),
            vec!["ament_cmake_gtest"]
        );
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

    #[test]
    fn interleaved_repeated_depend_elements() {
        let manifest = parse_str(
            &fixture("package_xml/interleaved_depend.xml"),
            &dummy_path(),
        )
        .unwrap();
        assert_eq!(
            Dependencies::names(&manifest.deps.depend),
            vec!["rclcpp", "tf2"]
        );
        assert_eq!(
            Dependencies::names(&manifest.deps.exec_depend),
            vec!["sensor_msgs"]
        );
    }

    #[test]
    fn conditional_deps_skip_ros1() {
        let manifest =
            parse_str(&fixture("package_xml/conditional_deps.xml"), &dummy_path()).unwrap();
        let dep_names = Dependencies::names(&manifest.deps.depend);
        let bt_names = Dependencies::names(&manifest.deps.buildtool_depend);
        // ROS 2 deps should be present.
        assert!(dep_names.contains(&"rclcpp"));
        assert!(dep_names.contains(&"sensor_msgs"));
        assert!(dep_names.contains(&"std_msgs"));
        assert_eq!(bt_names, vec!["ament_cmake"]);
        assert_eq!(manifest.build_type.as_deref(), Some("ament_cmake"));
        // ROS 1 deps should be absent.
        assert!(!dep_names.contains(&"roscpp"));
        assert!(!dep_names.contains(&"roslib"));
        assert!(!bt_names.contains(&"catkin"));
    }

    #[test]
    fn parses_version_constraints() {
        let manifest = parse_str(
            &fixture("package_xml/with_version_constraints.xml"),
            &dummy_path(),
        )
        .unwrap();
        // genmsg has version_gte + version_lt
        let genmsg = &manifest.deps.build_depend[0];
        assert_eq!(genmsg.name, "genmsg");
        assert_eq!(genmsg.version.version_gte.as_deref(), Some("1.1"));
        assert_eq!(genmsg.version.version_lt.as_deref(), Some("2.0"));
        assert!(genmsg.version.version_eq.is_none());
        assert_eq!(genmsg.version.to_string(), ">=1.1, <2.0");

        // rclcpp has no constraints
        let rclcpp = &manifest.deps.depend[0];
        assert_eq!(rclcpp.name, "rclcpp");
        assert!(rclcpp.version.is_empty());

        // python_qt_binding has version_gte only
        let pqb = &manifest.deps.exec_depend[0];
        assert_eq!(pqb.name, "python_qt_binding");
        assert_eq!(pqb.version.version_gte.as_deref(), Some("0.3.0"));
        assert_eq!(pqb.version.to_string(), ">=0.3.0");
    }

    #[test]
    fn eval_condition_simple() {
        // ROS 2 conditions (should be true).
        assert!(eval_condition("$ROS_VERSION == 2"));
        assert!(eval_condition("$ROS_VERSION != 1"));

        // ROS 1 conditions (should be false).
        assert!(!eval_condition("$ROS_VERSION == 1"));
        assert!(!eval_condition("$ROS_VERSION != 2"));

        // Unknown variables — conservative, return true.
        assert!(eval_condition("$PLATFORM == jetson"));

        // Complex expressions we can't parse — conservative, return true.
        assert!(eval_condition("$ROS_VERSION == 2 and $ARCH == arm64"));
    }

    #[test]
    fn compound_and_or_conditions() {
        let manifest =
            parse_str(&fixture("package_xml/condition_and_or.xml"), &dummy_path()).unwrap();
        let dep_names = Dependencies::names(&manifest.deps.depend);
        assert!(dep_names.contains(&"rclcpp"), "ROS 2 AND condition true");
        assert!(!dep_names.contains(&"roscpp"), "ROS 1 AND condition false");
        assert!(dep_names.contains(&"sensor_msgs"), "OR with one true");
        assert!(
            !dep_names.contains(&"nonexistent_pkg"),
            "OR with both false"
        );
        assert!(dep_names.contains(&"std_msgs"), "unconditional");
    }

    #[test]
    fn parenthesised_conditions() {
        let manifest =
            parse_str(&fixture("package_xml/condition_parens.xml"), &dummy_path()).unwrap();
        let dep_names = Dependencies::names(&manifest.deps.depend);
        assert!(dep_names.contains(&"rclcpp"), "(F or T) and T = T");
        assert!(!dep_names.contains(&"roscpp"), "(F or T) and F = F");
        assert!(dep_names.contains(&"std_msgs"), "unconditional");
    }
}
