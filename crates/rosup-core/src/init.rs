use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::package_xml;

#[derive(Debug, Error)]
pub enum InitError {
    #[error("rosup.toml already exists at {0}. Use --force to overwrite.")]
    AlreadyExists(PathBuf),
    #[error("no package.xml found in {0}")]
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

/// Outcome of a successful `rox init`.
#[derive(Debug)]
pub struct InitResult {
    pub rosup_toml_path: PathBuf,
    pub mode: InitMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitMode {
    Package,
    Workspace,
}

/// Run `rox init` in `dir`.
///
/// - If `force_workspace` is true, scan for member packages regardless of
///   whether `package.xml` is present.
/// - If `force` is true, overwrite an existing `rosup.toml`.
pub fn init(dir: &Path, force_workspace: bool, force: bool) -> Result<InitResult, InitError> {
    let rosup_toml = dir.join("rosup.toml");

    if rosup_toml.exists() && !force {
        return Err(InitError::AlreadyExists(rosup_toml));
    }

    let has_package_xml = dir.join("package.xml").exists();
    let has_src = dir.join("src").is_dir();

    let mode = if force_workspace || has_src {
        InitMode::Workspace
    } else if has_package_xml {
        InitMode::Package
    } else {
        return Err(InitError::NoPackageXml(dir.to_owned()));
    };

    let content = match mode {
        InitMode::Package => generate_package_toml(dir)?,
        InitMode::Workspace => generate_workspace_toml(dir)?,
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

fn generate_package_toml(dir: &Path) -> Result<String, InitError> {
    let manifest = package_xml::parse_file(&dir.join("package.xml"))?;
    Ok(format!("[package]\nname = \"{}\"\n", manifest.name))
}

fn generate_workspace_toml(dir: &Path) -> Result<String, InitError> {
    let scan_root = if dir.join("src").is_dir() {
        dir.join("src")
    } else {
        dir.to_owned()
    };

    let members = collect_members(dir, &scan_root)?;
    if members.is_empty() {
        return Err(InitError::NoMembers(scan_root));
    }

    let member_lines: String = members.iter().map(|m| format!("  \"{}\",\n", m)).collect();

    Ok(format!("[workspace]\nmembers = [\n{member_lines}]\n"))
}

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

/// Walk `scan_root` for directories containing `package.xml` and return their
/// paths relative to `workspace_root`.
fn collect_members(workspace_root: &Path, scan_root: &Path) -> Result<Vec<String>, InitError> {
    let mut members = Vec::new();
    visit_dir(scan_root, workspace_root, &mut members)?;
    members.sort();
    Ok(members)
}

fn visit_dir(
    dir: &Path,
    workspace_root: &Path,
    members: &mut Vec<String>,
) -> Result<(), InitError> {
    if dir.join("package.xml").exists() {
        let rel = dir
            .strip_prefix(workspace_root)
            .unwrap_or(dir)
            .display()
            .to_string();
        members.push(rel);
        // Don't recurse into packages — ROS packages don't nest.
        return Ok(());
    }

    let entries = std::fs::read_dir(dir).map_err(|e| InitError::Io {
        path: dir.to_owned(),
        source: e,
    })?;

    for entry in entries.flatten() {
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            visit_dir(&entry.path(), workspace_root, members)?;
        }
    }

    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_helpers::copy_fixture;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn init_single_package() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");

        let result = init(tmp.path(), false, false).unwrap();
        assert_eq!(result.mode, InitMode::Package);
        let content = fs::read_to_string(&result.rosup_toml_path).unwrap();
        assert!(content.contains("[package]"));
        assert!(content.contains("name = \"my_pkg\""));
    }

    #[test]
    fn init_workspace_via_src_dir() {
        let tmp = TempDir::new().unwrap();
        let pkg_a = tmp.path().join("src/pkg_a");
        let pkg_b = tmp.path().join("src/pkg_b");
        fs::create_dir_all(&pkg_a).unwrap();
        fs::create_dir_all(&pkg_b).unwrap();
        copy_fixture("package_xml/pkg_a.xml", &pkg_a, "package.xml");
        copy_fixture("package_xml/pkg_b.xml", &pkg_b, "package.xml");

        let result = init(tmp.path(), false, false).unwrap();
        assert_eq!(result.mode, InitMode::Workspace);
        let content = fs::read_to_string(&result.rosup_toml_path).unwrap();
        assert!(content.contains("[workspace]"));
        assert!(content.contains("src/pkg_a"));
        assert!(content.contains("src/pkg_b"));
    }

    #[test]
    fn init_fails_if_rosup_toml_exists_without_force() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/stub_pkg.xml", tmp.path(), "package.xml");
        fs::write(tmp.path().join("rosup.toml"), "[package]\nname = \"p\"\n").unwrap();

        let err = init(tmp.path(), false, false).unwrap_err();
        assert!(matches!(err, InitError::AlreadyExists(_)));
    }

    #[test]
    fn init_force_overwrites() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");
        fs::write(tmp.path().join("rosup.toml"), "[package]\nname = \"old\"\n").unwrap();

        init(tmp.path(), false, true).unwrap();
        let content = fs::read_to_string(tmp.path().join("rosup.toml")).unwrap();
        assert!(content.contains("name = \"my_pkg\""));
    }

    #[test]
    fn init_errors_with_no_package_xml_or_src() {
        let tmp = TempDir::new().unwrap();
        let err = init(tmp.path(), false, false).unwrap_err();
        assert!(matches!(err, InitError::NoPackageXml(_)));
    }

    #[test]
    fn init_creates_gitignore_with_rox_entry() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");

        init(tmp.path(), false, false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.lines().any(|l| l.trim() == ".rosup/"));
    }

    #[test]
    fn init_appends_to_existing_gitignore() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");
        fs::write(tmp.path().join(".gitignore"), "build/\ninstall/\n").unwrap();

        init(tmp.path(), false, false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert!(content.contains("build/"));
        assert!(content.contains("install/"));
        assert!(content.lines().any(|l| l.trim() == ".rosup/"));
    }

    #[test]
    fn init_does_not_duplicate_rox_in_gitignore() {
        let tmp = TempDir::new().unwrap();
        copy_fixture("package_xml/with_build_type.xml", tmp.path(), "package.xml");
        fs::write(tmp.path().join(".gitignore"), ".rosup/\n").unwrap();

        init(tmp.path(), false, false).unwrap();

        let content = fs::read_to_string(tmp.path().join(".gitignore")).unwrap();
        assert_eq!(content.lines().filter(|l| l.trim() == ".rosup/").count(), 1);
    }
}
