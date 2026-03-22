//! Typed builders for temporary rosup project trees.

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// A single-package project in a temporary directory.
pub struct PackageProject {
    pub dir: TempDir,
}

impl PackageProject {
    /// Create a package project with `package.xml` and `rosup.toml`.
    pub fn new(name: &str) -> Self {
        Self::with_deps(name, &[])
    }

    /// Create a package project with listed `<depend>` entries.
    pub fn with_deps(name: &str, deps: &[&str]) -> Self {
        let dir = TempDir::new().expect("tempdir");
        write_package_xml(dir.path(), name, deps);
        write_rosup_toml(dir.path(), &format!("[package]\nname = \"{name}\"\n"));
        Self { dir }
    }

    /// Create a package project with a custom rosup.toml body.
    pub fn with_toml(name: &str, toml_body: &str) -> Self {
        let dir = TempDir::new().expect("tempdir");
        write_package_xml(dir.path(), name, &[]);
        write_rosup_toml(dir.path(), toml_body);
        Self { dir }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Pre-populate `.rosup/install/` with a stub file so dep layer is seen as built.
    pub fn with_dep_layer_stub(self) -> Self {
        let install = self.dir.path().join(".rosup/install");
        fs::create_dir_all(&install).unwrap();
        fs::write(install.join(".stub"), "").unwrap();
        self
    }

    /// Pre-populate `.rosup/src/<name>/` so dep layer source exists.
    pub fn with_dep_src(self, repo_name: &str) -> Self {
        let src = self.dir.path().join(".rosup/src").join(repo_name);
        fs::create_dir_all(&src).unwrap();
        self
    }

    /// Pre-create build/log/install artifact directories.
    pub fn with_artifacts(self) -> Self {
        for dir in ["build", "log", "install"] {
            fs::create_dir_all(self.dir.path().join(dir)).unwrap();
        }
        self
    }

    /// Pre-create per-package artifact dirs under build/ and install/.
    pub fn with_package_artifacts(self, pkg: &str) -> Self {
        for dir in ["build", "install"] {
            fs::create_dir_all(self.dir.path().join(dir).join(pkg)).unwrap();
        }
        self
    }

    /// Pre-create `install/` so `rosup run`/`launch` sees the workspace as built.
    pub fn with_install_dir(self) -> Self {
        fs::create_dir_all(self.dir.path().join("install")).unwrap();
        self
    }
}

/// A workspace project with multiple member packages.
pub struct WorkspaceProject {
    pub dir: TempDir,
}

impl WorkspaceProject {
    /// Create workspace with explicit members in `src/<name>/`.
    pub fn new(members: &[&str]) -> Self {
        let dir = TempDir::new().expect("tempdir");
        let mut member_lines = String::new();
        for name in members {
            let pkg_dir = dir.path().join("src").join(name);
            fs::create_dir_all(&pkg_dir).unwrap();
            write_package_xml(&pkg_dir, name, &[]);
            member_lines.push_str(&format!("  \"src/{name}\",\n"));
        }
        let toml = format!("[workspace]\nmembers = [\n{member_lines}]\n");
        write_rosup_toml(dir.path(), &toml);
        Self { dir }
    }

    /// Create workspace with auto-discovery (no `members` key) and packages
    /// under `src/<name>/`.
    pub fn auto_discovery(members: &[&str]) -> Self {
        let dir = TempDir::new().expect("tempdir");
        for name in members {
            let pkg_dir = dir.path().join("src").join(name);
            fs::create_dir_all(&pkg_dir).unwrap();
            write_package_xml(&pkg_dir, name, &[]);
        }
        write_rosup_toml(dir.path(), "[workspace]\n");
        Self { dir }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Simulate a stale dep layer package in `.rosup/install/` that has the
    /// same name as a workspace member.
    pub fn with_stale_dep(self, pkg_name: &str) -> Self {
        let install = self.dir.path().join(".rosup/install");
        let ament_index = install.join("share/ament_index/resource_index/packages");
        fs::create_dir_all(&ament_index).unwrap();
        fs::write(ament_index.join(pkg_name), "").unwrap();
        // Create share/<name>/ and lib/<name>/ to simulate a full install.
        fs::create_dir_all(install.join("share").join(pkg_name)).unwrap();
        fs::write(
            install.join("share").join(pkg_name).join("package.xml"),
            "<package><name>stale</name></package>",
        )
        .unwrap();
        fs::create_dir_all(install.join("lib").join(pkg_name)).unwrap();
        self
    }
}

// ── helpers ───────────────────────────────────────────────────────────────────

pub fn write_package_xml(dir: &Path, name: &str, deps: &[&str]) {
    let dep_lines: String = deps
        .iter()
        .map(|d| format!("  <depend>{d}</depend>\n"))
        .collect();
    let xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.0.0</version>
  <description>test package</description>
  <maintainer email="test@example.com">Test</maintainer>
  <license>MIT</license>
{dep_lines}</package>
"#
    );
    fs::write(dir.join("package.xml"), xml).unwrap();
}

pub fn write_rosup_toml(dir: &Path, content: &str) {
    fs::write(dir.join("rosup.toml"), content).unwrap();
}

/// Path to the rosdistro fixture YAML bundled with rosup-core.
pub fn humble_cache_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/rosup-core/tests/fixtures/rosdistro/humble-cache-sample.yaml")
}

/// Populate a fake HOME's rosup cache with the sample humble YAML.
pub fn install_humble_cache(home: &Path) {
    let cache_dir = home.join(".rosup/cache");
    fs::create_dir_all(&cache_dir).unwrap();
    let content =
        fs::read_to_string(humble_cache_fixture()).expect("humble-cache-sample.yaml not found");
    fs::write(cache_dir.join("humble-cache.yaml"), &content).unwrap();
    // Write a far-future timestamp so the cache is never considered stale.
    fs::write(cache_dir.join("humble-cache.timestamp"), "9999999999").unwrap();
}
