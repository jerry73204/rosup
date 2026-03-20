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

// ── exclude with path ────────────────────────────────────────────────────────

#[test]
fn exclude_adds_concrete_path_to_toml() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/isaac_pkg", "isaac_pkg");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "src/isaac_pkg"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Excluded"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("src/isaac_pkg"));
}

#[test]
fn exclude_with_glob_expands_to_concrete_paths() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/isaac_a", "isaac_a");
    add_pkg(dir.path(), "src/isaac_b", "isaac_b");
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "src/isaac_*"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("src/isaac_a"), "expanded path a");
    assert!(toml.contains("src/isaac_b"), "expanded path b");
    assert!(!toml.contains("*"), "no globs stored");
}

#[test]
fn exclude_path_removes_packages_from_discovery() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/isaac_pkg", "isaac_pkg");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "src/isaac_pkg"])
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
fn exclude_list_shows_current_entries() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/test_pkg\", \"src/vendor_pkg\"]\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["exclude", "--list"])
        .assert()
        .success()
        .stdout(predicates::str::contains("src/test_pkg"))
        .stdout(predicates::str::contains("src/vendor_pkg"));
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
        "[workspace]\nexclude = [\"src/skip_pkg\"]\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["exclude"])
        .assert()
        .success()
        .stdout(predicates::str::contains("src/skip_pkg"));
}

#[test]
fn exclude_duplicate_is_idempotent() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "src/pkg_a"])
        .assert()
        .success();

    env()
        .cmd(dir.path())
        .args(["exclude", "src/pkg_a"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Already excluded"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert_eq!(
        toml.matches("src/pkg_a").count(),
        1,
        "should appear exactly once"
    );
}

#[test]
fn exclude_preserves_other_toml_content() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/skip_pkg", "skip_pkg");
    let original = "[workspace]\n\n[resolve]\nros-distro = \"humble\"\n";
    fs::write(dir.path().join("rosup.toml"), original).unwrap();

    env()
        .cmd(dir.path())
        .args(["exclude", "src/skip_pkg"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("src/skip_pkg"));
    assert!(toml.contains("ros-distro = \"humble\""));
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
        .args(["exclude", "src/test_pkg"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("workspace"));
}

#[test]
fn exclude_broader_path_subsumes_narrower() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/a/child_1", "child_1");
    add_pkg(dir.path(), "src/a/child_2", "child_2");
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/a/child_1\"]\n",
    )
    .unwrap();

    // Excluding the parent should replace the narrower child entry.
    env()
        .cmd(dir.path())
        .args(["exclude", "src/a"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("src/a"), "parent should be present");
    assert!(!toml.contains("src/a/child_1"), "child should be subsumed");
}

// ── --pkg flag ───────────────────────────────────────────────────────────────

#[test]
fn exclude_pkg_flag_looks_up_path_by_name() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/drivers/isaac_driver", "isaac_driver");
    add_pkg(dir.path(), "src/core/pkg_a", "pkg_a");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "--pkg", "isaac_driver"])
        .assert()
        .success()
        .stdout(predicates::str::contains("src/drivers/isaac_driver"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("src/drivers/isaac_driver"));
}

#[test]
fn exclude_pkg_flag_unknown_package_fails() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "--pkg", "nonexistent_pkg"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

// ── include ──────────────────────────────────────────────────────────────────

#[test]
fn include_removes_exact_entry() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/isaac_pkg\", \"src/test_pkg\"]\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["include", "src/isaac_pkg"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Included"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(!toml.contains("src/isaac_pkg"), "should be removed");
    assert!(toml.contains("src/test_pkg"), "other should remain");
}

#[test]
fn include_last_entry_removes_exclude_key() {
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
    assert!(!toml.contains("exclude"), "key should be removed");
}

#[test]
fn include_nonexistent_entry_fails() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/test_pkg\"]\n",
    )
    .unwrap();

    env()
        .cmd(dir.path())
        .args(["include", "src/nonexistent"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not excluded"));
}

#[test]
fn include_splits_broader_entry_into_siblings() {
    let dir = TempDir::new().unwrap();
    // Parent dir with two child packages.
    add_pkg(dir.path(), "src/isaac/launch", "launch_pkg");
    add_pkg(dir.path(), "src/isaac/bridge", "bridge_pkg");
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/isaac\"]\n",
    )
    .unwrap();

    // Re-include just bridge_pkg by path.
    env()
        .cmd(dir.path())
        .args(["include", "src/isaac/bridge"])
        .assert()
        .success()
        .stdout(predicates::str::contains("Included"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    // The broad "src/isaac" should be split: launch stays excluded, bridge removed.
    assert!(
        !toml.contains("\"src/isaac\""),
        "broad entry should be removed"
    );
    assert!(toml.contains("src/isaac/launch"), "sibling stays excluded");
    assert!(
        !toml.contains("src/isaac/bridge"),
        "target should be included"
    );
}

#[test]
fn include_pkg_flag_splits_broader_entry() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/isaac/launch", "launch_pkg");
    add_pkg(dir.path(), "src/isaac/bridge", "bridge_pkg");
    fs::write(
        dir.path().join("rosup.toml"),
        "[workspace]\nexclude = [\"src/isaac\"]\n",
    )
    .unwrap();

    // Re-include bridge_pkg by name.
    env()
        .cmd(dir.path())
        .args(["include", "--pkg", "bridge_pkg"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("src/isaac/launch"), "sibling stays excluded");
    assert!(
        !toml.contains("src/isaac/bridge"),
        "target should be included"
    );
}

// ── roundtrips ───────────────────────────────────────────────────────────────

#[test]
fn exclude_then_include_roundtrip() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/isaac_pkg", "isaac_pkg");
    make_auto_discovery_workspace(dir.path());

    // Exclude.
    env()
        .cmd(dir.path())
        .args(["exclude", "src/isaac_pkg"])
        .assert()
        .success();

    // Verify excluded.
    let output = env()
        .cmd(dir.path())
        .args(["sync", "--lock", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    assert!(!String::from_utf8(output).unwrap().contains("isaac_pkg"));

    // Include back.
    env()
        .cmd(dir.path())
        .args(["include", "src/isaac_pkg"])
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

#[test]
fn exclude_pkg_then_include_pkg_roundtrip() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/drivers/isaac_driver", "isaac_driver");
    add_pkg(dir.path(), "src/core/pkg_a", "pkg_a");
    make_auto_discovery_workspace(dir.path());

    env()
        .cmd(dir.path())
        .args(["exclude", "--pkg", "isaac_driver"])
        .assert()
        .success();

    env()
        .cmd(dir.path())
        .args(["include", "--pkg", "isaac_driver"])
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
    assert!(String::from_utf8(output).unwrap().contains("isaac_driver"));
}
