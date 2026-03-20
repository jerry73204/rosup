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

/// Evaluate a REP-149 condition expression with `$ROS_VERSION=2`.
///
/// Handles simple comparisons like `$ROS_VERSION == 1`, `$ROS_VERSION != 2`,
/// and `$ROS_VERSION == 2`. For conditions we can't evaluate (complex
/// expressions with `and`/`or`/parentheses), returns `true` (conservative:
/// include the dep rather than silently dropping it).
fn eval_condition(condition: &str) -> bool {
    // Substitute known variables.
    let expr = condition.replace("$ROS_VERSION", "2");

    // If unresolved variables remain, we can't evaluate — include the dep.
    if expr.contains('$') {
        return true;
    }

    // Try to match simple: `<lhs> <op> <rhs>` patterns.
    let expr = expr.trim();
    for op in &["==", "!=", ">=", "<=", ">", "<"] {
        if let Some((lhs, rhs)) = expr.split_once(op) {
            let lhs = lhs.trim().trim_matches(|c| c == '"' || c == '\'');
            let rhs = rhs.trim().trim_matches(|c| c == '"' || c == '\'');
            return match *op {
                "==" => lhs == rhs,
                "!=" => lhs != rhs,
                ">=" => lhs >= rhs,
                "<=" => lhs <= rhs,
                ">" => lhs > rhs,
                "<" => lhs < rhs,
                _ => true,
            };
        }
    }

    // Can't parse — include the dep (conservative).
    true
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

    // Stack of open element names so we know the current path.
    // We only need to track depth-1 tags and `export > build_type`.
    // Track whether the current element has a ROS 1 condition.
    let mut tag_stack: Vec<String> = Vec::new();
    let mut skip_ros1: Vec<bool> = Vec::new();

    loop {
        match reader.read_event().map_err(map_err)? {
            Event::Start(e) | Event::Empty(e) => {
                let tag = std::str::from_utf8(e.name().as_ref())
                    .unwrap_or("")
                    .to_owned();
                // Evaluate `condition` attribute (REP-149). rosup only
                // supports ROS 2, so we substitute $ROS_VERSION=2 and
                // evaluate simple comparisons. Complex conditions we
                // can't evaluate are treated as true (conservative).
                let should_skip = e.attributes().flatten().any(|attr| {
                    attr.key.as_ref() == b"condition"
                        && !eval_condition(&String::from_utf8_lossy(&attr.value))
                });
                tag_stack.push(tag);
                skip_ros1.push(should_skip);
            }
            Event::End(_) => {
                tag_stack.pop();
                skip_ros1.pop();
            }
            Event::Text(e) => {
                // Skip elements with ROS 1 condition.
                if skip_ros1.last() == Some(&true) {
                    continue;
                }
                let text = e.unescape().map_err(map_err)?.into_owned();
                if text.is_empty() {
                    continue;
                }
                match tag_stack.as_slice() {
                    [.., parent, current] => {
                        if parent == "export" && current == "build_type" {
                            build_type = Some(text);
                            continue;
                        }
                        match current.as_str() {
                            "name" => name = Some(text),
                            "version" => version = Some(text),
                            "description" => description = Some(text),
                            "depend" => deps.depend.push(text),
                            "build_depend" => deps.build_depend.push(text),
                            "build_export_depend" => deps.build_export_depend.push(text),
                            "buildtool_depend" => deps.buildtool_depend.push(text),
                            "exec_depend" => deps.exec_depend.push(text),
                            "test_depend" => deps.test_depend.push(text),
                            "doc_depend" => deps.doc_depend.push(text),
                            _ => {}
                        }
                    }
                    [current] => match current.as_str() {
                        "name" => name = Some(text),
                        "version" => version = Some(text),
                        "description" => description = Some(text),
                        _ => {}
                    },
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

    #[test]
    fn interleaved_repeated_depend_elements() {
        let manifest = parse_str(
            &fixture("package_xml/interleaved_depend.xml"),
            &dummy_path(),
        )
        .unwrap();
        assert_eq!(manifest.deps.depend, vec!["rclcpp", "tf2"]);
        assert_eq!(manifest.deps.exec_depend, vec!["sensor_msgs"]);
    }

    #[test]
    fn conditional_deps_skip_ros1() {
        let manifest =
            parse_str(&fixture("package_xml/conditional_deps.xml"), &dummy_path()).unwrap();
        // ROS 2 deps should be present.
        assert!(manifest.deps.depend.contains(&"rclcpp".to_owned()));
        assert!(manifest.deps.depend.contains(&"sensor_msgs".to_owned()));
        assert!(manifest.deps.depend.contains(&"std_msgs".to_owned()));
        assert_eq!(
            manifest.deps.buildtool_depend,
            vec!["ament_cmake".to_owned()]
        );
        assert_eq!(manifest.build_type.as_deref(), Some("ament_cmake"));
        // ROS 1 deps should be absent.
        assert!(!manifest.deps.depend.contains(&"roscpp".to_owned()));
        assert!(!manifest.deps.depend.contains(&"roslib".to_owned()));
        assert!(
            !manifest
                .deps
                .buildtool_depend
                .contains(&"catkin".to_owned())
        );
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
}
