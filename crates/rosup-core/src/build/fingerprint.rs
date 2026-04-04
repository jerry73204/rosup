//! Incremental builds via fingerprinting.
//!
//! Computes a fingerprint for each package from its inputs (source files,
//! package.xml, CMakeLists.txt, build config, dependency fingerprints).
//! If the fingerprint hasn't changed since the last successful build,
//! the package is skipped entirely.
//!
//! This is a **coarse gate** above cmake's own incremental build:
//!
//! ```text
//! fingerprint unchanged → skip entirely (0ms)
//! fingerprint changed   → run cmake → cmake skips unchanged targets (~0.1s)
//! no previous fingerprint → full build
//! ```

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use sha2::{Digest, Sha256};

/// Fingerprint file name stored in the build directory.
const FINGERPRINT_FILE: &str = ".rosup_fingerprint";

// ── Fingerprint ─────────────────────────────────────────────────────────────

/// A fingerprint capturing all inputs that affect a package's build output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    /// SHA-256 of `package.xml` content.
    pub package_xml: String,
    /// SHA-256 of `CMakeLists.txt` content (empty if not present).
    pub cmake_lists: String,
    /// SHA-256 over all source file mtimes + sizes (sorted by path).
    pub source_hash: String,
    /// SHA-256 of build configuration (cmake_args, symlink_install, etc.).
    pub build_config: String,
    /// SHA-256 of dependency fingerprint hashes (sorted by dep name).
    pub dep_hash: String,
}

/// Inputs needed to compute a fingerprint.
pub struct FingerprintInputs<'a> {
    /// Path to the package source directory.
    pub source_path: &'a Path,
    /// CMake args from rosup.toml.
    pub cmake_args: &'a [String],
    /// Whether symlink install is enabled.
    pub symlink_install: bool,
    /// Fingerprint hashes of dependencies, keyed by dep name.
    /// Only workspace member deps are included (external deps are assumed
    /// stable).
    pub dep_fingerprints: &'a BTreeMap<String, String>,
}

impl Fingerprint {
    /// Compute a fingerprint from package inputs.
    pub fn compute(inputs: &FingerprintInputs<'_>) -> Self {
        let package_xml = hash_file_content(&inputs.source_path.join("package.xml"));
        let cmake_lists = hash_file_content(&inputs.source_path.join("CMakeLists.txt"));
        let source_hash = hash_source_tree(inputs.source_path);
        let build_config = hash_build_config(inputs.cmake_args, inputs.symlink_install);
        let dep_hash = hash_dep_fingerprints(inputs.dep_fingerprints);

        Fingerprint {
            package_xml,
            cmake_lists,
            source_hash,
            build_config,
            dep_hash,
        }
    }

    /// Load a fingerprint from the build directory.
    ///
    /// Returns `None` if the file doesn't exist or is malformed.
    pub fn load(build_base: &Path) -> Option<Self> {
        let path = build_base.join(FINGERPRINT_FILE);
        let content = std::fs::read_to_string(&path).ok()?;
        parse_fingerprint(&content)
    }

    /// Save the fingerprint to the build directory.
    pub fn save(&self, build_base: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(build_base)?;
        let content = serialize_fingerprint(self);
        std::fs::write(build_base.join(FINGERPRINT_FILE), content)
    }

    /// Compute a single summary hash of this fingerprint (for dep_hash).
    pub fn summary_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(&self.package_xml);
        hasher.update(&self.cmake_lists);
        hasher.update(&self.source_hash);
        hasher.update(&self.build_config);
        hasher.update(&self.dep_hash);
        hex_digest(hasher)
    }
}

/// Check whether a package needs to be rebuilt.
///
/// Returns `true` if the package should be rebuilt (fingerprint changed
/// or doesn't exist).
pub fn should_rebuild(inputs: &FingerprintInputs<'_>, build_base: &Path) -> bool {
    let new_fp = Fingerprint::compute(inputs);
    match Fingerprint::load(build_base) {
        None => true,
        Some(old_fp) => old_fp != new_fp,
    }
}

// ── Hashing helpers ─────────────────────────────────────────────────────────

/// SHA-256 hash of a file's content. Returns empty string if file doesn't exist.
fn hash_file_content(path: &Path) -> String {
    match std::fs::read(path) {
        Ok(bytes) => {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            hex_digest(hasher)
        }
        Err(_) => String::new(),
    }
}

/// Hash the source tree by walking and hashing mtime+size of each file.
///
/// Uses mtime+size instead of full content hash for performance.
/// Files are sorted by path for determinism.
fn hash_source_tree(source_path: &Path) -> String {
    let mut entries: BTreeMap<String, String> = BTreeMap::new();

    // Walk common source directories
    for subdir in [
        "src", "include", "launch", "config", "param", "resource", "msg", "srv", "action",
    ] {
        let dir = source_path.join(subdir);
        if dir.is_dir() {
            walk_dir(&dir, source_path, &mut entries);
        }
    }

    // Also include setup.py, setup.cfg for Python packages
    for file in ["setup.py", "setup.cfg", "pyproject.toml"] {
        let path = source_path.join(file);
        if path.is_file()
            && let Some(entry) = mtime_size_entry(&path)
        {
            entries.insert(file.to_owned(), entry);
        }
    }

    let mut hasher = Sha256::new();
    for (path, entry) in &entries {
        hasher.update(path.as_bytes());
        hasher.update(b":");
        hasher.update(entry.as_bytes());
        hasher.update(b"\n");
    }
    hex_digest(hasher)
}

/// Recursively walk a directory, collecting mtime+size for each file.
fn walk_dir(dir: &Path, base: &Path, entries: &mut BTreeMap<String, String>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, base, entries);
        } else if path.is_file()
            && let Some(mtime_entry) = mtime_size_entry(&path)
        {
            let rel = path
                .strip_prefix(base)
                .unwrap_or(&path)
                .display()
                .to_string();
            entries.insert(rel, mtime_entry);
        }
    }
}

/// Get "mtime_nanos:size" string for a file.
fn mtime_size_entry(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(format!("{}:{}", mtime.as_nanos(), meta.len()))
}

/// Hash build configuration.
fn hash_build_config(cmake_args: &[String], symlink_install: bool) -> String {
    let mut hasher = Sha256::new();
    hasher.update(if symlink_install {
        b"symlink:1"
    } else {
        b"symlink:0"
    });
    for arg in cmake_args {
        hasher.update(b"\n");
        hasher.update(arg.as_bytes());
    }
    hex_digest(hasher)
}

/// Hash dependency fingerprints (sorted by dep name for determinism).
fn hash_dep_fingerprints(dep_fps: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    for (name, hash) in dep_fps {
        hasher.update(name.as_bytes());
        hasher.update(b":");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    hex_digest(hasher)
}

/// Convert a SHA-256 hasher to a hex string.
fn hex_digest(hasher: Sha256) -> String {
    let result = hasher.finalize();
    let mut s = String::with_capacity(64);
    for byte in result.iter() {
        write!(s, "{byte:02x}").unwrap();
    }
    s
}

// ── Serialization ───────────────────────────────────────────────────────────

fn serialize_fingerprint(fp: &Fingerprint) -> String {
    format!(
        "[fingerprint]\n\
         package_xml = \"{}\"\n\
         cmake_lists = \"{}\"\n\
         source_hash = \"{}\"\n\
         build_config = \"{}\"\n\
         dep_hash = \"{}\"\n",
        fp.package_xml, fp.cmake_lists, fp.source_hash, fp.build_config, fp.dep_hash,
    )
}

fn parse_fingerprint(content: &str) -> Option<Fingerprint> {
    let mut package_xml = None;
    let mut cmake_lists = None;
    let mut source_hash = None;
    let mut build_config = None;
    let mut dep_hash = None;

    for line in content.lines() {
        let line = line.trim();
        if let Some((key, value)) = line.split_once(" = ") {
            let value = value.trim_matches('"');
            match key {
                "package_xml" => package_xml = Some(value.to_owned()),
                "cmake_lists" => cmake_lists = Some(value.to_owned()),
                "source_hash" => source_hash = Some(value.to_owned()),
                "build_config" => build_config = Some(value.to_owned()),
                "dep_hash" => dep_hash = Some(value.to_owned()),
                _ => {}
            }
        }
    }

    Some(Fingerprint {
        package_xml: package_xml?,
        cmake_lists: cmake_lists?,
        source_hash: source_hash?,
        build_config: build_config?,
        dep_hash: dep_hash?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_test_package(dir: &Path) {
        std::fs::write(
            dir.join("package.xml"),
            "<package><name>test</name></package>",
        )
        .unwrap();
        std::fs::write(
            dir.join("CMakeLists.txt"),
            "cmake_minimum_required(VERSION 3.8)\nproject(test)\n",
        )
        .unwrap();
        let src = dir.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("main.cpp"), "int main() { return 0; }").unwrap();
    }

    static NO_DEPS: std::sync::LazyLock<BTreeMap<String, String>> =
        std::sync::LazyLock::new(BTreeMap::new);

    fn make_inputs(source_path: &Path) -> FingerprintInputs<'_> {
        FingerprintInputs {
            source_path,
            cmake_args: &[],
            symlink_install: false,
            dep_fingerprints: &NO_DEPS,
        }
    }

    // ── Compute ─────────────────────────────────────────────────────────

    #[test]
    fn compute_deterministic() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());

        let fp1 = Fingerprint::compute(&inputs);
        let fp2 = Fingerprint::compute(&inputs);
        assert_eq!(fp1, fp2);
    }

    #[test]
    fn compute_detects_package_xml_change() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());

        let fp1 = Fingerprint::compute(&inputs);
        std::fs::write(
            tmp.path().join("package.xml"),
            "<package><name>changed</name></package>",
        )
        .unwrap();
        let fp2 = Fingerprint::compute(&inputs);

        assert_ne!(fp1.package_xml, fp2.package_xml);
        assert_eq!(fp1.cmake_lists, fp2.cmake_lists);
    }

    #[test]
    fn compute_detects_cmake_change() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());

        let fp1 = Fingerprint::compute(&inputs);
        std::fs::write(tmp.path().join("CMakeLists.txt"), "project(changed)\n").unwrap();
        let fp2 = Fingerprint::compute(&inputs);

        assert_ne!(fp1.cmake_lists, fp2.cmake_lists);
    }

    #[test]
    fn compute_detects_source_file_change() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());

        let fp1 = Fingerprint::compute(&inputs);
        // Touch source file — change mtime
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(tmp.path().join("src/main.cpp"), "int main() { return 1; }").unwrap();
        let fp2 = Fingerprint::compute(&inputs);

        assert_ne!(fp1.source_hash, fp2.source_hash);
    }

    #[test]
    fn compute_detects_new_source_file() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());

        let fp1 = Fingerprint::compute(&inputs);
        std::fs::write(tmp.path().join("src/util.cpp"), "void util() {}").unwrap();
        let fp2 = Fingerprint::compute(&inputs);

        assert_ne!(fp1.source_hash, fp2.source_hash);
    }

    #[test]
    fn compute_detects_cmake_args_change() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());

        let args1: Vec<String> = vec!["-DFOO=1".into()];
        let args2: Vec<String> = vec!["-DFOO=2".into()];
        let deps = BTreeMap::new();

        let fp1 = Fingerprint::compute(&FingerprintInputs {
            source_path: tmp.path(),
            cmake_args: &args1,
            symlink_install: false,
            dep_fingerprints: &deps,
        });
        let fp2 = Fingerprint::compute(&FingerprintInputs {
            source_path: tmp.path(),
            cmake_args: &args2,
            symlink_install: false,
            dep_fingerprints: &deps,
        });

        assert_ne!(fp1.build_config, fp2.build_config);
    }

    #[test]
    fn compute_detects_symlink_install_change() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let deps = BTreeMap::new();

        let fp1 = Fingerprint::compute(&FingerprintInputs {
            source_path: tmp.path(),
            cmake_args: &[],
            symlink_install: false,
            dep_fingerprints: &deps,
        });
        let fp2 = Fingerprint::compute(&FingerprintInputs {
            source_path: tmp.path(),
            cmake_args: &[],
            symlink_install: true,
            dep_fingerprints: &deps,
        });

        assert_ne!(fp1.build_config, fp2.build_config);
    }

    #[test]
    fn compute_detects_dep_fingerprint_change() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());

        let mut deps1 = BTreeMap::new();
        deps1.insert("dep_a".into(), "hash_old".into());
        let mut deps2 = BTreeMap::new();
        deps2.insert("dep_a".into(), "hash_new".into());

        let fp1 = Fingerprint::compute(&FingerprintInputs {
            source_path: tmp.path(),
            cmake_args: &[],
            symlink_install: false,
            dep_fingerprints: &deps1,
        });
        let fp2 = Fingerprint::compute(&FingerprintInputs {
            source_path: tmp.path(),
            cmake_args: &[],
            symlink_install: false,
            dep_fingerprints: &deps2,
        });

        assert_ne!(fp1.dep_hash, fp2.dep_hash);
    }

    #[test]
    fn compute_missing_cmake_lists() {
        let tmp = TempDir::new().unwrap();
        // Only package.xml, no CMakeLists.txt (ament_python package)
        std::fs::write(tmp.path().join("package.xml"), "<package/>").unwrap();
        let inputs = make_inputs(tmp.path());

        let fp = Fingerprint::compute(&inputs);
        assert!(fp.cmake_lists.is_empty());
        assert!(!fp.package_xml.is_empty());
    }

    // ── Save / Load ─────────────────────────────────────────────────────

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());

        let fp = Fingerprint::compute(&inputs);
        let build_dir = tmp.path().join("build");
        fp.save(&build_dir).unwrap();

        let loaded = Fingerprint::load(&build_dir).unwrap();
        assert_eq!(fp, loaded);
    }

    #[test]
    fn load_returns_none_for_missing() {
        let tmp = TempDir::new().unwrap();
        assert!(Fingerprint::load(tmp.path()).is_none());
    }

    #[test]
    fn load_returns_none_for_malformed() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join(FINGERPRINT_FILE), "garbage").unwrap();
        assert!(Fingerprint::load(tmp.path()).is_none());
    }

    // ── should_rebuild ──────────────────────────────────────────────────

    #[test]
    fn should_rebuild_no_previous_fingerprint() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());
        let build_dir = tmp.path().join("build");

        assert!(should_rebuild(&inputs, &build_dir));
    }

    #[test]
    fn should_rebuild_false_when_unchanged() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());
        let build_dir = tmp.path().join("build");

        // Save current fingerprint
        let fp = Fingerprint::compute(&inputs);
        fp.save(&build_dir).unwrap();

        // Should not need rebuild
        assert!(!should_rebuild(&inputs, &build_dir));
    }

    #[test]
    fn should_rebuild_true_when_source_changed() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());
        let build_dir = tmp.path().join("build");

        // Save current fingerprint
        let fp = Fingerprint::compute(&inputs);
        fp.save(&build_dir).unwrap();

        // Modify source
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(tmp.path().join("src/main.cpp"), "int main() { return 42; }").unwrap();

        assert!(should_rebuild(&inputs, &build_dir));
    }

    #[test]
    fn should_rebuild_true_when_package_xml_changed() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());
        let build_dir = tmp.path().join("build");

        let fp = Fingerprint::compute(&inputs);
        fp.save(&build_dir).unwrap();

        // Add a new dep to package.xml
        std::fs::write(
            tmp.path().join("package.xml"),
            "<package><name>test</name><depend>new_dep</depend></package>",
        )
        .unwrap();

        assert!(should_rebuild(&inputs, &build_dir));
    }

    // ── summary_hash ────────────────────────────────────────────────────

    #[test]
    fn summary_hash_deterministic() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());

        let fp = Fingerprint::compute(&inputs);
        assert_eq!(fp.summary_hash(), fp.summary_hash());
        assert_eq!(fp.summary_hash().len(), 64); // SHA-256 hex
    }

    #[test]
    fn summary_hash_changes_with_fingerprint() {
        let tmp = TempDir::new().unwrap();
        make_test_package(tmp.path());
        let inputs = make_inputs(tmp.path());

        let fp1 = Fingerprint::compute(&inputs);
        std::fs::write(
            tmp.path().join("package.xml"),
            "<package><name>different</name></package>",
        )
        .unwrap();
        let fp2 = Fingerprint::compute(&inputs);

        assert_ne!(fp1.summary_hash(), fp2.summary_hash());
    }

    // ── Python package source detection ─────────────────────────────────

    #[test]
    fn detects_setup_py_changes() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("package.xml"), "<package/>").unwrap();
        std::fs::write(
            tmp.path().join("setup.py"),
            "from setuptools import setup\nsetup()",
        )
        .unwrap();
        let inputs = make_inputs(tmp.path());

        let fp1 = Fingerprint::compute(&inputs);
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::write(
            tmp.path().join("setup.py"),
            "from setuptools import setup\nsetup(name='changed')",
        )
        .unwrap();
        let fp2 = Fingerprint::compute(&inputs);

        assert_ne!(fp1.source_hash, fp2.source_hash);
    }
}
