use rosup_tests::{TestEnv, project::write_package_xml};
use std::fs;
use tempfile::TempDir;

fn env() -> TestEnv {
    TestEnv::new()
}

#[test]
fn init_package_mode() {
    let dir = TempDir::new().unwrap();
    write_package_xml(dir.path(), "my_pkg", &[]);

    env()
        .cmd(dir.path())
        .args(["init", "--ros-distro", "humble"])
        .assert()
        .success()
        .stdout(predicates::str::contains("package"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("[package]"));
    assert!(toml.contains("name = \"my_pkg\""));
    assert!(toml.contains("ros-distro = \"humble\""));
    assert!(toml.contains("\"/opt/ros/humble\""));
}

#[test]
fn init_workspace_mode_auto_discovery() {
    let dir = TempDir::new().unwrap();

    env()
        .cmd(dir.path())
        .args(["init", "--workspace", "--ros-distro", "jazzy"])
        .assert()
        .success()
        .stdout(predicates::str::contains("auto-discovery"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("[workspace]"));
    // Auto-discovery mode must NOT write a members key.
    assert!(!toml.contains("members"));
    assert!(toml.contains("ros-distro = \"jazzy\""));
}

#[test]
fn init_workspace_mode_locked() {
    let dir = TempDir::new().unwrap();
    let pkg_a = dir.path().join("src/pkg_a");
    let pkg_b = dir.path().join("src/pkg_b");
    fs::create_dir_all(&pkg_a).unwrap();
    fs::create_dir_all(&pkg_b).unwrap();
    write_package_xml(&pkg_a, "pkg_a", &[]);
    write_package_xml(&pkg_b, "pkg_b", &[]);

    env()
        .cmd(dir.path())
        .args(["init", "--workspace", "--lock", "--ros-distro", "humble"])
        .assert()
        .success()
        .stdout(predicates::str::contains("locked"));

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("[workspace]"));
    assert!(toml.contains("pkg_a"));
    assert!(toml.contains("pkg_b"));
    assert!(toml.contains("ros-distro = \"humble\""));
}

#[test]
fn init_adds_gitignore_entry() {
    let dir = TempDir::new().unwrap();
    write_package_xml(dir.path(), "my_pkg", &[]);

    env()
        .cmd(dir.path())
        .args(["init", "--ros-distro", "humble"])
        .assert()
        .success();

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.lines().any(|l| l.trim() == ".rosup/"));
}

#[test]
fn init_no_overwrite_without_force() {
    let dir = TempDir::new().unwrap();
    write_package_xml(dir.path(), "my_pkg", &[]);
    fs::write(dir.path().join("rosup.toml"), "[package]\nname = \"old\"\n").unwrap();

    env()
        .cmd(dir.path())
        .args(["init", "--ros-distro", "humble"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"));
}

#[test]
fn init_force_overwrites() {
    let dir = TempDir::new().unwrap();
    write_package_xml(dir.path(), "my_pkg", &[]);
    fs::write(dir.path().join("rosup.toml"), "[package]\nname = \"old\"\n").unwrap();

    env()
        .cmd(dir.path())
        .args(["init", "--ros-distro", "humble", "--force"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rosup.toml")).unwrap();
    assert!(toml.contains("name = \"my_pkg\""));
}

#[test]
fn init_no_package_xml_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    env().cmd(dir.path()).args(["init"]).assert().failure();
}

/// Verify that `init` checks for existing rosup.toml BEFORE prompting for
/// the distro. Without --ros-distro and without ROS_DISTRO, if the existence
/// check runs first the command fails immediately without blocking on stdin.
#[test]
fn init_existence_check_before_distro_prompt() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("rosup.toml"), "[package]\nname = \"old\"\n").unwrap();

    // No --ros-distro, no ROS_DISTRO in env (TestEnv removes it).
    // If the existence check ran after the prompt, this would hang on stdin.
    // Instead it should fail immediately with "already exists".
    env()
        .cmd(dir.path())
        .args(["init"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"));
}
