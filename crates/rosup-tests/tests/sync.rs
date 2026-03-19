use rosup_tests::{TestEnv, project::write_package_xml};
use std::fs;
use tempfile::TempDir;

fn env() -> TestEnv {
    TestEnv::new()
}

/// Create a minimal workspace with an explicit members list.
fn make_locked_workspace(dir: &std::path::Path, members: &[&str]) -> String {
    let member_lines: String = members.iter().map(|m| format!("  \"{m}\",\n")).collect();
    let toml = format!("[workspace]\nmembers = [\n{member_lines}]\n");
    fs::write(dir.join("rosup.toml"), &toml).unwrap();
    toml
}

/// Scaffold a package directory under `root/<rel>`.
fn add_pkg(root: &std::path::Path, rel: &str, name: &str) {
    let dir = root.join(rel);
    fs::create_dir_all(&dir).unwrap();
    write_package_xml(&dir, name, &[]);
}

#[test]
fn sync_updates_changed_entries() {
    let dir = TempDir::new().unwrap();
    // Start with pkg_a only in the locked list.
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/pkg_b", "pkg_b");
    make_locked_workspace(dir.path(), &["src/pkg_a"]);

    env().cmd(dir.path()).args(["sync"]).assert().success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("pkg_a"), "pkg_a should be present");
    assert!(toml.contains("pkg_b"), "pkg_b should be added");
}

#[test]
fn sync_dry_run_does_not_modify_file() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/pkg_b", "pkg_b");
    let original = make_locked_workspace(dir.path(), &["src/pkg_a"]);

    env()
        .cmd(dir.path())
        .args(["sync", "--dry-run"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert_eq!(toml, original, "file must not be modified with --dry-run");
}

#[test]
fn sync_check_exits_nonzero_when_stale() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/pkg_b", "pkg_b");
    make_locked_workspace(dir.path(), &["src/pkg_a"]);

    env()
        .cmd(dir.path())
        .args(["sync", "--check"])
        .assert()
        .failure();
}

#[test]
fn sync_check_exits_zero_when_up_to_date() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    make_locked_workspace(dir.path(), &["src/pkg_a"]);

    env()
        .cmd(dir.path())
        .args(["sync", "--check"])
        .assert()
        .success();
}

#[test]
fn sync_lock_on_auto_discovery_workspace() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    add_pkg(dir.path(), "src/pkg_b", "pkg_b");
    fs::write(dir.path().join("rosup.toml"), "[workspace]\n").unwrap();

    env()
        .cmd(dir.path())
        .args(["sync", "--lock"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("members"), "members key should be written");
    assert!(toml.contains("pkg_a"));
    assert!(toml.contains("pkg_b"));
}

#[test]
fn sync_auto_discovery_without_lock_is_noop() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    let original = "[workspace]\n";
    fs::write(dir.path().join("rosup.toml"), original).unwrap();

    env()
        .cmd(dir.path())
        .args(["sync"])
        .assert()
        .success()
        .stdout(predicates::str::contains("auto-discovery"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert_eq!(toml, original, "file must not be modified");
}

/// Misconfigured overlays must not block non-build commands like sync.
#[test]
fn sync_succeeds_with_bad_overlay() {
    let dir = TempDir::new().unwrap();
    add_pkg(dir.path(), "src/pkg_a", "pkg_a");
    make_locked_workspace(dir.path(), &["src/pkg_a"]);
    // Append a bad overlay to the existing rosup.toml.
    let mut toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    toml.push_str("\n[resolve]\noverlays = [\"/nonexistent/overlay/path\"]\n");
    fs::write(dir.path().join("rosup.toml"), &toml).unwrap();

    env()
        .cmd(dir.path())
        .args(["sync", "--check"])
        .assert()
        .success();
}
