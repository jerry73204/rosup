/// Line-level manipulation of package.xml that preserves comments, processing
/// instructions, and indentation.
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("dependency `{dep}` already present in <{tag}>")]
    AlreadyPresent { dep: String, tag: String },
    #[error("no <{0}> or </package> anchor found — is this a valid package.xml?")]
    NoAnchor(String),
}

/// Which dependency tag to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepTag {
    Depend,
    BuildDepend,
    ExecDepend,
    TestDepend,
    /// Expands to BuildDepend + TestDepend (two insertions).
    Dev,
}

impl DepTag {
    pub fn tag_names(self) -> &'static [&'static str] {
        match self {
            DepTag::Depend => &["depend"],
            DepTag::BuildDepend => &["build_depend"],
            DepTag::ExecDepend => &["exec_depend"],
            DepTag::TestDepend => &["test_depend"],
            DepTag::Dev => &["build_depend", "test_depend"],
        }
    }
}

/// Canonical ordering of dependency tags. Tags earlier in this list will appear
/// before tags later in the list.
const DEP_ORDER: &[&str] = &[
    "buildtool_depend",
    "buildtool_export_depend",
    "depend",
    "build_depend",
    "build_export_depend",
    "exec_depend",
    "test_depend",
    "doc_depend",
    "conflict",
    "replace",
    "group_depend",
    "member_of_group",
];

fn dep_rank(tag: &str) -> usize {
    DEP_ORDER
        .iter()
        .position(|&t| t == tag)
        .unwrap_or(usize::MAX)
}

/// Add a dependency to `package.xml` at `path`.
///
/// Returns the list of `<tag>name</tag>` strings that were inserted (one per
/// tag in the case of `DepTag::Dev`).
pub fn add_dep(path: &Path, dep: &str, tag: DepTag) -> Result<Vec<String>, ManifestError> {
    let content = read(path)?;
    let mut lines: Vec<String> = content.lines().map(str::to_owned).collect();
    let indent = detect_indent(&lines);

    let mut inserted = Vec::new();

    for tag_name in tag.tag_names() {
        // Duplicate check across all dep tag types for this name.
        if is_present(&lines, dep, tag_name) {
            return Err(ManifestError::AlreadyPresent {
                dep: dep.to_owned(),
                tag: tag_name.to_string(),
            });
        }

        let new_line = format!("{indent}<{tag_name}>{dep}</{tag_name}>");
        let insert_after = find_insert_pos(&lines, tag_name);
        lines.insert(insert_after + 1, new_line.clone());
        inserted.push(new_line.trim().to_owned());
    }

    write(path, &lines.join("\n"))?;
    Ok(inserted)
}

/// Remove all dependency tags for `dep` from `package.xml` at `path`.
///
/// Returns the list of `<tag>name</tag>` strings that were removed.
pub fn remove_dep(path: &Path, dep: &str) -> Result<Vec<String>, ManifestError> {
    let content = read(path)?;
    let lines: Vec<String> = content.lines().map(str::to_owned).collect();

    let mut removed = Vec::new();
    let mut kept = Vec::new();

    for line in &lines {
        if let Some(tag) = matches_any_dep_tag(line, dep) {
            removed.push(format!("<{tag}>{dep}</{tag}>"));
        } else {
            kept.push(line.clone());
        }
    }

    if !removed.is_empty() {
        write(path, &kept.join("\n"))?;
    }

    Ok(removed)
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn read(path: &Path) -> Result<String, ManifestError> {
    std::fs::read_to_string(path).map_err(|e| ManifestError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

fn write(path: &Path, content: &str) -> Result<(), ManifestError> {
    std::fs::write(path, content).map_err(|e| ManifestError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

/// Detect the indentation used by existing dep tags, defaulting to two spaces.
fn detect_indent(lines: &[String]) -> String {
    for tag in DEP_ORDER {
        for line in lines {
            let trimmed = line.trim_start();
            if trimmed.starts_with(&format!("<{tag}>")) || trimmed.starts_with(&format!("<{tag} "))
            {
                let spaces = line.len() - trimmed.len();
                return " ".repeat(spaces);
            }
        }
    }
    "  ".to_owned()
}

/// Return true if `dep` already appears inside `<tag_name>`.
fn is_present(lines: &[String], dep: &str, tag_name: &str) -> bool {
    let needle = format!("<{tag_name}>{dep}</{tag_name}>");
    lines.iter().any(|l| l.contains(&needle))
}

/// Find the line index after which to insert a new `<tag_name>` element.
///
/// Strategy: find the last existing line whose tag has canonical rank ≤ our
/// tag's rank. If none found, insert just before `<export>` or `</package>`.
fn find_insert_pos(lines: &[String], tag_name: &str) -> usize {
    let our_rank = dep_rank(tag_name);
    let mut best: Option<usize> = None;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // Match any opening dep tag.
        for &known in DEP_ORDER {
            if dep_rank(known) <= our_rank
                && (trimmed.starts_with(&format!("<{known}>"))
                    || trimmed.starts_with(&format!("<{known} ")))
            {
                // Walk forward to find the closing tag line.
                let close = format!("</{known}>");
                let end = lines[i..]
                    .iter()
                    .position(|l| l.contains(&close))
                    .map(|p| i + p)
                    .unwrap_or(i);
                best = Some(end);
            }
        }
    }

    if let Some(pos) = best {
        return pos;
    }

    // Fallback: insert before <export> or </package>.
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with("<export>") || t.starts_with("<export ") || t == "</package>" {
            return i.saturating_sub(1);
        }
    }

    lines.len().saturating_sub(1)
}

/// If `line` contains a dep tag wrapping exactly `dep`, return the tag name.
fn matches_any_dep_tag<'a>(line: &str, dep: &str) -> Option<&'a str> {
    for &tag in DEP_ORDER {
        let pattern = format!("<{tag}>{dep}</{tag}>");
        if line.contains(&pattern) {
            return Some(tag);
        }
    }
    None
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::copy_fixture;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn add_depend_inserts_after_existing_depend() {
        let tmp = TempDir::new().unwrap();
        let p = copy_fixture("package_xml/manifest_base.xml", tmp.path(), "package.xml");
        let inserted = add_dep(&p, "sensor_msgs", DepTag::Depend).unwrap();
        assert_eq!(inserted, vec!["<depend>sensor_msgs</depend>"]);
        let content = fs::read_to_string(&p).unwrap();
        let rclcpp_pos = content.find("<depend>rclcpp</depend>").unwrap();
        let sensor_pos = content.find("<depend>sensor_msgs</depend>").unwrap();
        assert!(
            sensor_pos > rclcpp_pos,
            "sensor_msgs should come after rclcpp"
        );
    }

    #[test]
    fn add_test_depend_inserts_in_canonical_order() {
        let tmp = TempDir::new().unwrap();
        let p = copy_fixture("package_xml/manifest_base.xml", tmp.path(), "package.xml");
        add_dep(&p, "my_test_lib", DepTag::TestDepend).unwrap();
        let content = fs::read_to_string(&p).unwrap();
        let gtest_pos = content
            .find("<test_depend>ament_cmake_gtest</test_depend>")
            .unwrap();
        let new_pos = content
            .find("<test_depend>my_test_lib</test_depend>")
            .unwrap();
        assert!(new_pos > gtest_pos);
    }

    #[test]
    fn add_dev_inserts_both_tags() {
        let tmp = TempDir::new().unwrap();
        let p = copy_fixture("package_xml/manifest_base.xml", tmp.path(), "package.xml");
        let inserted = add_dep(&p, "my_lib", DepTag::Dev).unwrap();
        assert_eq!(inserted.len(), 2);
        let content = fs::read_to_string(&p).unwrap();
        assert!(content.contains("<build_depend>my_lib</build_depend>"));
        assert!(content.contains("<test_depend>my_lib</test_depend>"));
    }

    #[test]
    fn add_duplicate_errors() {
        let tmp = TempDir::new().unwrap();
        let p = copy_fixture("package_xml/manifest_base.xml", tmp.path(), "package.xml");
        let err = add_dep(&p, "rclcpp", DepTag::Depend).unwrap_err();
        assert!(matches!(err, ManifestError::AlreadyPresent { .. }));
    }

    #[test]
    fn remove_dep_removes_all_matching_tags() {
        let tmp = TempDir::new().unwrap();
        let p = copy_fixture(
            "package_xml/manifest_multi_rclcpp.xml",
            tmp.path(),
            "package.xml",
        );
        let removed = remove_dep(&p, "rclcpp").unwrap();
        assert_eq!(removed.len(), 2);
        let content = fs::read_to_string(&p).unwrap();
        assert!(!content.contains("rclcpp"));
    }

    #[test]
    fn remove_nonexistent_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let p = copy_fixture("package_xml/manifest_base.xml", tmp.path(), "package.xml");
        let original = fs::read_to_string(&p).unwrap();
        let removed = remove_dep(&p, "nonexistent_pkg").unwrap();
        assert!(removed.is_empty());
        assert_eq!(fs::read_to_string(&p).unwrap(), original);
    }

    #[test]
    fn preserves_comments_and_processing_instructions() {
        let tmp = TempDir::new().unwrap();
        let p = copy_fixture(
            "package_xml/manifest_with_comments.xml",
            tmp.path(),
            "package.xml",
        );
        add_dep(&p, "sensor_msgs", DepTag::Depend).unwrap();
        let content = fs::read_to_string(&p).unwrap();
        assert!(content.contains("<?xml-model"));
        assert!(content.contains("<!-- package metadata -->"));
        assert!(content.contains("<!-- runtime deps -->"));
    }
}
