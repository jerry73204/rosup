use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::package_xml;

#[derive(Debug, Error)]
pub enum InitError {
    #[error("rosup.toml already exists at {0}. Use --force to overwrite.")]
    AlreadyExists(PathBuf),
    #[error("no package.xml found in {0} — pass --workspace to initialise a workspace")]
    NoPackageXml(PathBuf),
    #[error("no package.xml files found under {0}")]
    NoMembers(PathBuf),
    #[error("failed to write {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(transparent)]
    PackageXml(#[from] package_xml::PackageXmlError),
}

/// Outcome of a successful `rosup init`.
#[derive(Debug)]
pub struct InitResult {
    pub rosup_toml_path: PathBuf,
    pub mode: InitMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitMode {
    Package,
    /// Workspace with auto-discovery (no `members` key).
    Workspace,
    /// Workspace with an explicit locked member list.
    WorkspaceLocked,
}

/// Run `rosup init` in `dir`.
///
/// - `force_workspace`: create a workspace manifest instead of a package one.
/// - `lock`: when `force_workspace` is true, populate an explicit `members`
///   list via `colcon_scan` instead of writing an empty `[workspace]`.
/// - `ros_distro`: when `Some`, append a `[resolve]` section with `ros-distro`
///   and a base overlay for `/opt/ros/<distro>`.
/// - `force`: overwrite an existing `rosup.toml`.
pub fn init(
    dir: &Path,
    force_workspace: bool,
    lock: bool,
    ros_distro: Option<&str>,
    force: bool,
) -> Result<InitResult, InitError> {
    let rosup_toml = dir.join("rosup.toml");

    if rosup_toml.exists() && !force {
        return Err(InitError::AlreadyExists(rosup_toml));
    }

    let has_package_xml = dir.join("package.xml").exists();

    // Workspace mode must be declared explicitly via force_workspace (--workspace flag or
    // multi-package clone detection). A src/ directory alone is not sufficient — many
    // single ROS packages have src/ for C++ source files.
    let mode = if force_workspace {
        if lock {
            InitMode::WorkspaceLocked
        } else {
            InitMode::Workspace
        }
    } else if has_package_xml {
        InitMode::Package
    } else {
        return Err(InitError::NoPackageXml(dir.to_owned()));
    };

    let content = match mode {
        InitMode::Package => generate_package_toml(dir, ros_distro)?,
        InitMode::Workspace => generate_workspace_toml(ros_distro),
        InitMode::WorkspaceLocked => generate_locked_workspace_toml(dir, ros_distro)?,
    };

    std::fs::write(&rosup_toml, content).map_err(|e| InitError::Io {
        path: rosup_toml.clone(),
        source: e,
    })?;

    update_gitignore(dir)?;

    Ok(InitResult {
        rosup_toml_path: rosup_toml,
        mode,
    })
}

fn generate_package_toml(dir: &Path, ros_distro: Option<&str>) -> Result<String, InitError> {
    let manifest = package_xml::parse_file(&dir.join("package.xml"))?;
    let mut out = format!("[package]\nname = \"{}\"\n", manifest.name);
    if let Some(distro) = ros_distro {
        out.push_str(&resolve_section(distro));
    }
    Ok(out)
}

fn generate_workspace_toml(ros_distro: Option<&str>) -> String {
    let mut out = "[workspace]\n".to_owned();
    if let Some(distro) = ros_distro {
        out.push_str(&resolve_section(distro));
    }
    out
}

fn generate_locked_workspace_toml(
    dir: &Path,
    ros_distro: Option<&str>,
) -> Result<String, InitError> {
    let members = colcon_scan(dir, &[])?;
    if members.is_empty() {
        return Err(InitError::NoMembers(dir.to_owned()));
    }
    let mut out = format_workspace_toml(&members);
    if let Some(distro) = ros_distro {
        out.push_str(&resolve_section(distro));
    }
    Ok(out)
}

/// Format a `[resolve]` section with `ros-distro` and the base ROS overlay.
fn resolve_section(distro: &str) -> String {
    format!("\n[resolve]\nros-distro = \"{distro}\"\noverlays = [\"/opt/ros/{distro}\"]\n")
}

/// Format a `[workspace]` TOML block with an explicit member list.
pub fn format_workspace_toml(members: &[String]) -> String {
    let member_lines: String = members.iter().map(|m| format!("  \"{m}\",\n")).collect();
    format!("[workspace]\nmembers = [\n{member_lines}]\n")
}

// ── Colcon discovery scanner ───────────────────────────────────────────────────

/// Directory names that Colcon always skips.
const COLCON_SKIP_DIRS: &[&str] = &["build", "install", "log", ".rosup"];

/// File markers whose presence causes Colcon to skip a directory and all
/// of its descendants.
const COLCON_IGNORE_MARKERS: &[&str] = &["COLCON_IGNORE", "AMENT_IGNORE", "CATKIN_IGNORE"];

/// Recursively discover ROS packages using Colcon's discovery rules.
///
/// Returns paths relative to `workspace_root`, sorted lexicographically.
/// The `exclude` slice contains directory path prefixes; any discovered
/// package whose relative path starts with an exclude prefix is omitted.
pub fn colcon_scan(workspace_root: &Path, exclude: &[String]) -> Result<Vec<String>, InitError> {
    let mut members = Vec::new();
    colcon_visit(workspace_root, workspace_root, exclude, &mut members)?;
    members.sort();
    Ok(members)
}

/// Check whether `rel_path` is under any excluded prefix.
pub fn is_excluded(rel_path: &str, exclude: &[String]) -> bool {
    exclude
        .iter()
        .any(|ex| rel_path == ex || rel_path.starts_with(&format!("{ex}/")))
}

fn colcon_visit(
    dir: &Path,
    workspace_root: &Path,
    exclude: &[String],
    members: &mut Vec<String>,
) -> Result<(), InitError> {
    // A directory containing package.xml is a ROS package — record it and
    // stop recursing (packages do not nest).
    if dir != workspace_root && dir.join("package.xml").exists() {
        let rel = dir
            .strip_prefix(workspace_root)
            .unwrap_or(dir)
            .display()
            .to_string();
        if !is_excluded(&rel, exclude) {
            members.push(rel);
        }
        return Ok(());
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            return Err(InitError::Io {
                path: dir.to_owned(),
                source: e,
            });
        }
    };

    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden directories (dot-prefixed), except the workspace root itself.
        if name_str.starts_with('.') {
            continue;
        }

        // Skip standard Colcon output directories.
        if COLCON_SKIP_DIRS.contains(&name_str.as_ref()) {
            continue;
        }

        // Skip if any ignore marker is present inside this directory.
        if COLCON_IGNORE_MARKERS.iter().any(|m| path.join(m).exists()) {
            continue;
        }

        subdirs.push(path);
    }

    // Sort for deterministic output independent of filesystem ordering.
    subdirs.sort();

    for subdir in subdirs {
        colcon_visit(&subdir, workspace_root, exclude, members)?;
    }

    Ok(())
}

// ── rosup sync / exclude TOML patching ───────────────────────────────────────
//
// Uses `toml_edit` for format-preserving TOML manipulation instead of
// hand-rolled string patching.

/// Update the `members` key under `[workspace]` in `rosup.toml`.
///
/// - `Some(values)` — set `members = [...]` (replace or insert).
/// - `None` — remove the `members` key (switch to auto-discovery).
pub fn patch_members(toml_text: &str, members: Option<&[String]>) -> String {
    let mut doc = toml_text
        .parse::<toml_edit::DocumentMut>()
        .expect("rosup.toml should be valid TOML");

    let ws = doc
        .get_mut("workspace")
        .and_then(|v| v.as_table_like_mut())
        .expect("[workspace] section must exist");

    match members {
        Some(values) => {
            let mut arr = toml_edit::Array::new();
            for v in values {
                arr.push(v.as_str());
            }
            ws.insert("members", toml_edit::value(arr));
        }
        None => {
            ws.remove("members");
        }
    }

    doc.to_string()
}

/// Update the `exclude` key under `[workspace]` in `rosup.toml`.
///
/// - Non-empty `values` — set `exclude = [...]` (replace or insert).
/// - Empty `values` — remove the `exclude` key entirely.
pub fn patch_exclude(toml_text: &str, values: &[String]) -> String {
    let mut doc = toml_text
        .parse::<toml_edit::DocumentMut>()
        .expect("rosup.toml should be valid TOML");

    let ws = doc
        .get_mut("workspace")
        .and_then(|v| v.as_table_like_mut())
        .expect("[workspace] section must exist");

    if values.is_empty() {
        ws.remove("exclude");
    } else {
        let mut arr = toml_edit::Array::new();
        for v in values {
            arr.push(v.as_str());
        }
        ws.insert("exclude", toml_edit::value(arr));
    }

    doc.to_string()
}

// ── gitignore helper ───────────────────────────────────────────────────────────

/// Ensure `.rosup/` is listed in `<dir>/.gitignore`, appending if necessary.
fn update_gitignore(dir: &Path) -> Result<(), InitError> {
    let gitignore = dir.join(".gitignore");
    let entry = ".rosup/";

    if gitignore.exists() {
        let content = std::fs::read_to_string(&gitignore).map_err(|e| InitError::Io {
            path: gitignore.clone(),
            source: e,
        })?;
        if content
            .lines()
            .any(|l| l.trim() == ".rosup" || l.trim() == ".rosup/")
        {
            return Ok(());
        }
        let separator = if content.ends_with('\n') { "" } else { "\n" };
        let new_content = format!("{content}{separator}{entry}\n");
        std::fs::write(&gitignore, new_content).map_err(|e| InitError::Io {
            path: gitignore,
            source: e,
        })?;
    } else {
        std::fs::write(&gitignore, format!("{entry}\n")).map_err(|e| InitError::Io {
            path: gitignore,
            source: e,
        })?;
    }

    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::{copy_fixture, copy_fixture_dir};
    use std::fs;
    use tempfile::TempDir;

    // ── init ──────────────────────────────────────────────────────────────────

    #[test]
    fn init_single_package() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");

        let result = init(tmp.path(), false, false, None, false).unwrap();
        assert_eq!(result.mode, InitMode::Package);
        let content = fs::read_to_string(&result.rosup_toml_path).unwrap();
        assert!(content.contains("[package]"));
        assert!(content.contains("name = \"my_pkg\""));
    }

    #[test]
    fn init_package_with_distro_writes_resolve_section() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");

        let result = init(tmp.path(), false, false, Some("humble"), false).unwrap();
        let content = fs::read_to_string(&result.rosup_toml_path).unwrap();
        assert!(content.contains("[resolve]"));
        assert!(content.contains("ros-distro = \"humble\""));
        assert!(content.contains("\"/opt/ros/humble\""));
    }

    #[test]
    fn init_workspace_with_distro_writes_resolve_section() {
        let tmp = TempDir::new().unwrap();

        let result = init(tmp.path(), true, false, Some("jazzy"), false).unwrap();
        let content = fs::read_to_string(&result.rosup_toml_path).unwrap();
        assert!(content.contains("[workspace]"));
        assert!(content.contains("[resolve]"));
        assert!(content.contains("ros-distro = \"jazzy\""));
        assert!(content.contains("\"/opt/ros/jazzy\""));
    }

    #[test]
    fn init_workspace_auto_discovery() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src/pkg_a")).unwrap();

        let result = init(tmp.path(), true, false, None, false).unwrap();
        assert_eq!(result.mode, InitMode::Workspace);
        let content = fs::read_to_string(&result.rosup_toml_path).unwrap();
        assert!(content.contains("[workspace]"));
        assert!(!content.contains("members"));
    }

    #[test]
    fn init_workspace_locked_scans_src_dir() {
        let tmp = TempDir::new().unwrap();
        let pkg_a = tmp.path().join("src/pkg_a");
        let pkg_b = tmp.path().join("src/pkg_b");
        fs::create_dir_all(&pkg_a).unwrap();
        fs::create_dir_all(&pkg_b).unwrap();
        copy_fixture("package_xml/pkg_a.xml", &pkg_a, "package.xml");
        copy_fixture("package_xml/pkg_b.xml", &pkg_b, "package.xml");

        let result = init(tmp.path(), true, true, None, false).unwrap();
        assert_eq!(result.mode, InitMode::WorkspaceLocked);
        let content = fs::read_to_string(&result.rosup_toml_path).unwrap();
        assert!(content.contains("[workspace]"));
        assert!(content.contains("src/pkg_a"));
        assert!(content.contains("src/pkg_b"));
    }

    #[test]
    fn init_src_dir_alone_does_not_trigger_workspace() {
        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("src/pkg_a")).unwrap();

        // Without force_workspace, src/ alone is not enough — must be explicit.
        let err = init(tmp.path(), false, false, None, false).unwrap_err();
        assert!(matches!(err, InitError::NoPackageXml(_)));
    }

    #[test]
    fn init_fails_if_rosup_toml_exists_without_force() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/stub_pkg.xml", tmp.path(), "package.xml");
        fs::write(tmp.path().join("rosup.toml"), "[package]\nname = \"p\"\n").unwrap();

        let err = init(tmp.path(), false, false, None, false).unwrap_err();
        assert!(matches!(err, InitError::AlreadyExists(_)));
    }

    #[test]
    fn init_force_overwrites() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");
        fs::write(tmp.path().join("rosup.toml"), "[package]\nname = \"old\"\n").unwrap();

        init(tmp.path(), false, false, None, true).unwrap();
        let content = fs::read_to_string(tmp.path().join("rosup.toml")).unwrap();
        assert!(content.contains("name = \"my_pkg\""));
    }

    #[test]
    fn init_errors_with_no_package_xml() {
        let tmp = TempDir::new().unwrap();
        let err = init(tmp.path(), false, false, None, false).unwrap_err();
        assert!(matches!(err, InitError::NoPackageXml(_)));
    }

    #[test]
    fn init_creates_gitignore_with_rosup_entry() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");

        init(tmp.path(), false, false, None, false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.lines().any(|l| l.trim() == ".rosup/"));
    }

    #[test]
    fn init_appends_to_existing_gitignore() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");
        fs::write(tmp.path().join(".gitignore"), "build/\ninstall/\n").unwrap();

        init(tmp.path(), false, false, None, false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains("build/"));
        assert!(content.contains("install/"));
        assert!(content.lines().any(|l| l.trim() == ".rosup/"));
    }

    #[test]
    fn init_does_not_duplicate_rosup_in_gitignore() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");
        fs::write(tmp.path().join(".gitignore"), ".rosup/\n").unwrap();

        init(tmp.path(), false, false, None, false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(content.lines().filter(|l| l.trim() == ".rosup/").count(), 1);
    }

    // ── colcon_scan ───────────────────────────────────────────────────────────

    /// Create a directory with a `package.xml` fixture inside it.
    /// Content is taken from the stub fixture; the name arg is unused by
    /// colcon_scan (it only checks file existence), but kept for readability.
    fn make_pkg(dir: &Path, _name: &str) {
        fs::create_dir_all(dir).unwrap();
        copy_fixture("package_xml/stub_pkg.xml", dir, "package.xml");
    }

    #[test]
    fn colcon_scan_finds_packages_under_src() {
        let tmp = TempDir::new().unwrap();
        make_pkg(&tmp.path().join("src/pkg_a"), "pkg_a");
        make_pkg(&tmp.path().join("src/pkg_b"), "pkg_b");

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/pkg_a", "src/pkg_b"]);
    }

    #[test]
    fn colcon_scan_respects_colcon_ignore() {
        let tmp = TempDir::new().unwrap();
        make_pkg(&tmp.path().join("src/good"), "good");
        let ignored = tmp.path().join("src/ignored");
        make_pkg(&ignored, "ignored");
        fs::write(ignored.join("COLCON_IGNORE"), "").unwrap();

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/good"]);
    }

    #[test]
    fn colcon_scan_respects_ament_ignore() {
        let tmp = TempDir::new().unwrap();
        make_pkg(&tmp.path().join("src/good"), "good");
        let ignored = tmp.path().join("src/ignored");
        make_pkg(&ignored, "ignored");
        fs::write(ignored.join("AMENT_IGNORE"), "").unwrap();

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/good"]);
    }

    #[test]
    fn colcon_scan_respects_catkin_ignore() {
        let tmp = TempDir::new().unwrap();
        make_pkg(&tmp.path().join("src/good"), "good");
        let ignored = tmp.path().join("src/ignored");
        make_pkg(&ignored, "ignored");
        fs::write(ignored.join("CATKIN_IGNORE"), "").unwrap();

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/good"]);
    }

    #[test]
    fn colcon_scan_skips_output_dirs() {
        let tmp = TempDir::new().unwrap();
        make_pkg(&tmp.path().join("src/good"), "good");
        // These are standard colcon output dirs and must be skipped.
        for dir in &["build", "install", "log", ".rosup"] {
            make_pkg(&tmp.path().join(dir).join("fake_pkg"), "fake_pkg");
        }

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/good"]);
    }

    #[test]
    fn colcon_scan_skips_hidden_dirs() {
        let tmp = TempDir::new().unwrap();
        make_pkg(&tmp.path().join("src/good"), "good");
        make_pkg(&tmp.path().join(".hidden/pkg"), "pkg");

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/good"]);
    }

    #[test]
    fn colcon_scan_does_not_recurse_into_packages() {
        let tmp = TempDir::new().unwrap();
        // pkg_outer has a package.xml; nested/ inside it also has one.
        // Only pkg_outer should appear.
        let outer = tmp.path().join("src/pkg_outer");
        make_pkg(&outer, "pkg_outer");
        make_pkg(&outer.join("nested"), "nested");

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/pkg_outer"]);
    }

    #[test]
    fn colcon_scan_exclude_filters_results() {
        let tmp = TempDir::new().unwrap();
        make_pkg(&tmp.path().join("src/pkg_a"), "pkg_a");
        make_pkg(&tmp.path().join("src/experimental_pkg"), "experimental_pkg");

        let exclude = vec!["src/experimental_pkg".to_owned()];
        let members = colcon_scan(tmp.path(), &exclude).unwrap();
        assert_eq!(members, vec!["src/pkg_a"]);
    }

    #[test]
    fn colcon_ignore_blocks_all_nested_packages() {
        let tmp = TempDir::new().unwrap();
        copy_fixture_dir("workspaces/colcon_ignore_nested", tmp.path());

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/good"]);
    }

    #[test]
    fn non_ros_directories_excluded_from_scan() {
        let tmp = TempDir::new().unwrap();
        copy_fixture_dir("workspaces/mixed_non_ros", tmp.path());

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/pkg_a"]);
    }

    #[test]
    fn colcon_scan_results_sorted_lexicographically() {
        let tmp = TempDir::new().unwrap();
        copy_fixture_dir("workspaces/sort_order", tmp.path());

        let members = colcon_scan(tmp.path(), &[]).unwrap();
        assert_eq!(members, vec!["src/a_pkg", "src/m_pkg", "src/z_pkg"]);
    }

    // ── patch_members ─────────────────────────────────────────────────────────

    #[test]
    fn patch_members_replaces_existing_block() {
        let original = "[workspace]\nmembers = [\n  \"src/old\",\n]\n";
        let result = patch_members(
            original,
            Some(&["src/new_a".to_owned(), "src/new_b".to_owned()]),
        );
        assert!(result.contains("src/new_a"));
        assert!(result.contains("src/new_b"));
        assert!(!result.contains("src/old"));
        assert!(result.starts_with("[workspace]"));
    }

    #[test]
    fn patch_members_inserts_when_absent() {
        let original = "[workspace]\n";
        let result = patch_members(original, Some(&["src/pkg_a".to_owned()]));
        assert!(result.contains("members = ["));
        assert!(result.contains("src/pkg_a"));
        assert!(result.starts_with("[workspace]"));
    }

    #[test]
    fn patch_members_none_is_noop() {
        let original = "[workspace]\n";
        let result = patch_members(original, None);
        assert_eq!(result, original);
    }

    #[test]
    fn patch_members_preserves_surrounding_fields() {
        let original = "[workspace]\nmembers = [\n  \"src/old\",\n]\nexclude = [\"src/skip\"]\n";
        let result = patch_members(original, Some(&["src/new".to_owned()]));
        assert!(result.contains("src/new"));
        assert!(result.contains("exclude = [\"src/skip\"]"));
    }
}
