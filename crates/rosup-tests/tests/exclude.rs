use rosup_tests::{TestEnv, project::write_package_xml};
use std::fs;
use tempfile::TempDir;

fn env() -> TestEnv {
    TestEnv::new()
}

fn add_pkg(root: &std::path::Path, rel: &str, name: &str) {
    let dir = root.join(rel);
    fs::create_dir_all(&dir).unwrap();
    write_package_xml(&dir, name, &[]);
}

fn make_auto_discovery_workspace(dir: &std::path::Path) {
    fs::write(dir.join("rosup.toml"), "[workspace]\n").unwrap();
}

// ── exclude ──────────────────────────────────────────────────────────────────

#[test]
fn exclude_adds_pattern_to_toml() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/isaac_pkg", "isaac_pkg");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "src/isaac_*"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Added"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("exclude"));
    assert!(toml.contains("src/isaac_*"));
}

#[test]
fn exclude_pattern_removes_packages_from_discovery() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/isaac_pkg", "isaac_pkg");
    make_auto_discovery_workspace(dir.path());

    // Add exclude, then sync --lock --dry-run to see what's discovered.
    env()
        .cmd(dir.path())
        .args(["exclude", "src/isaac_*"])
        .assert()
        .success();

    let output = env()
        .cmd(dir.path())
        .args(["sync", "--lock", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();

    assert!(stdout.contains("pkg_a"), "pkg_a should be discovered");
    assert!(
        !stdout.contains("isaac_pkg"),
        "isaac_pkg should be excluded"
    );
}

#[test]
fn exclude_list_shows_current_patterns() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/test_*\", \"src/vendor_*\"]\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["exclude", "--list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("src/test_*"))
        .stdout(predicates::str::contains("src/vendor_*"));
}

#[test]
fn exclude_list_empty_shows_message() {
    let dir = TempDir::new().unwrap();
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "--list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No exclude patterns"));
}

#[test]
fn exclude_no_args_shows_list() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/skip_*\"]\n",
    )
    .unwrap();

    // No pattern and no --list flag — should still list.
    env()
        .cmd(dir.path())
        .args(["exclude"])
        .assert()
        .success()
        .stdout(predicates::str::contains("src/skip_*"));
}

#[test]
fn exclude_duplicate_pattern_is_idempotent() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "src/test_*"])
        .assert()
        .success();

    // Add the same pattern again.
    env()
        .cmd(dir.path())
        .args(["exclude", "src/test_*"])
        .assert()
        .success()
        .stdout(predicates::str::contains("already"));

    // Should appear only once in TOML.
    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert_eq!(
        toml.matches("src/test_*").count(),
        1,
        "pattern should appear exactly once"
    );
}

#[test]
fn exclude_preserves_other_toml_content() {
    let dir = TempDir::new().unwrap();
    let original = "[workspace]\n\n[resolve]\nros-distro = \"humble\"\n";
    fs::write(dir.path().join("rosup.toml"), original).unwrap();

    env()
        .cmd(dir.path())
        .args(["exclude", "src/skip_*"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("src/skip_*"), "pattern should be added");
    assert!(
        toml.contains("ros-distro = \"humble\""),
        "resolve section should be preserved"
    );
}

#[test]
fn exclude_fails_on_package_project() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[package]\nname = \"my_pkg\"\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["exclude", "src/test_*"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("workspace"));
}

// ── include ──────────────────────────────────────────────────────────────────

#[test]
fn include_removes_pattern_from_toml() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/isaac_*\", \"src/test_*\"]\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["include", "src/isaac_*"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Removed"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(!toml.contains("src/isaac_*"), "pattern should be removed");
    assert!(toml.contains("src/test_*"), "other pattern should remain");
}

#[test]
fn include_last_pattern_removes_exclude_key() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/only_one\"]\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["include", "src/only_one"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(
        !toml.contains("exclude"),
        "exclude key should be removed when empty"
    );
}

#[test]
fn include_nonexistent_pattern_fails() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/test_*\"]\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["include", "src/nonexistent_*"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn exclude_then_include_roundtrip() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/isaac_pkg", "isaac_pkg");
    make_auto_discovery_workspace(dir.path());

    // Exclude.
    env()
        .cmd(dir.path())
        .args(["exclude", "src/isaac_*"])
        .assert()
        .success();

    // Verify excluded from discovery.
    let output = env()
        .cmd(dir.path())
        .args(["sync", "--lock", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(!String::from_utf8(output).unwrap().contains("isaac_pkg"));

    // Include (re-add).
    env()
        .cmd(dir.path())
        .args(["include", "src/isaac_*"])
        .assert()
        .success();

    // Verify re-discovered.
    let output = env()
        .cmd(dir.path())
        .args(["sync", "--lock", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(String::from_utf8(output).unwrap().contains("isaac_pkg"));
}
