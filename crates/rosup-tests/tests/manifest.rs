use rosup_tests::{PackageProject, TestEnv, WorkspaceProject};
use std::fs;

fn env() -> TestEnv {
    TestEnv::new()
}

#[test]
fn add_depend_default() {
    let proj = PackageProject::new("my_pkg");
    env()
        .cmd(proj.root())
        .args(["add", "sensor_msgs"])
        .assert()
        .success();

    let xml = fs::read_to_string(proj.root().join("package.xml")).unwrap();
    assert!(xml.contains("<depend>sensor_msgs</depend>"));
}

#[test]
fn add_build_depend() {
    let proj = PackageProject::new("my_pkg");
    env()
        .cmd(proj.root())
        .args(["add", "--build", "ament_cmake"])
        .assert()
        .success();

    let xml = fs::read_to_string(proj.root().join("package.xml")).unwrap();
    assert!(xml.contains("<build_depend>ament_cmake</build_depend>"));
    assert!(!xml.contains("<depend>ament_cmake</depend>"));
}

#[test]
fn add_exec_depend() {
    let proj = PackageProject::new("my_pkg");
    env()
        .cmd(proj.root())
        .args(["add", "--exec", "rclcpp"])
        .assert()
        .success();

    let xml = fs::read_to_string(proj.root().join("package.xml")).unwrap();
    assert!(xml.contains("<exec_depend>rclcpp</exec_depend>"));
}

#[test]
fn add_dev_inserts_both_tags() {
    let proj = PackageProject::new("my_pkg");
    env()
        .cmd(proj.root())
        .args(["add", "--dev", "gtest"])
        .assert()
        .success();

    let xml = fs::read_to_string(proj.root().join("package.xml")).unwrap();
    assert!(xml.contains("<build_depend>gtest</build_depend>"));
    assert!(xml.contains("<test_depend>gtest</test_depend>"));
}

#[test]
fn add_duplicate_exits_nonzero() {
    let proj = PackageProject::with_deps("my_pkg", &["sensor_msgs"]);
    env()
        .cmd(proj.root())
        .args(["add", "sensor_msgs"])
        .assert()
        .failure();
}

#[test]
fn remove_existing_dep() {
    let proj = PackageProject::with_deps("my_pkg", &["sensor_msgs"]);
    env()
        .cmd(proj.root())
        .args(["remove", "sensor_msgs"])
        .assert()
        .success();

    let xml = fs::read_to_string(proj.root().join("package.xml")).unwrap();
    assert!(!xml.contains("sensor_msgs"));
}

#[test]
fn remove_nonexistent_warns() {
    let proj = PackageProject::new("my_pkg");
    env()
        .cmd(proj.root())
        .args(["remove", "unknown_pkg"])
        .assert()
        .success()
        .stderr(predicates::str::contains("warning"));
}

#[test]
fn add_with_package_flag_in_auto_discovery_workspace() {
    let ws = WorkspaceProject::auto_discovery(&["pkg_a", "pkg_b"]);
    env()
        .cmd(ws.root())
        .args(["add", "sensor_msgs", "-p", "pkg_a"])
        .assert()
        .success();

    let xml = fs::read_to_string(ws.root().join("src/pkg_a/package.xml")).unwrap();
    assert!(xml.contains("<depend>sensor_msgs</depend>"));
    // pkg_b should be unchanged.
    let xml_b = fs::read_to_string(ws.root().join("src/pkg_b/package.xml")).unwrap();
    assert!(!xml_b.contains("sensor_msgs"));
}

#[test]
fn remove_with_package_flag_in_auto_discovery_workspace() {
    let ws = WorkspaceProject::auto_discovery(&["pkg_a"]);
    // First add a dep, then remove it.
    env()
        .cmd(ws.root())
        .args(["add", "rclcpp", "-p", "pkg_a"])
        .assert()
        .success();
    env()
        .cmd(ws.root())
        .args(["remove", "rclcpp", "-p", "pkg_a"])
        .assert()
        .success();

    let xml = fs::read_to_string(ws.root().join("src/pkg_a/package.xml")).unwrap();
    assert!(!xml.contains("rclcpp"));
}

#[test]
fn add_with_package_flag_nonexistent_package_fails() {
    let ws = WorkspaceProject::auto_discovery(&["pkg_a"]);
    env()
        .cmd(ws.root())
        .args(["add", "rclcpp", "-p", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("nonexistent"));
}
