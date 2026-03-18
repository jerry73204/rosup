use rosup_tests::{PackageProject, TestEnv};
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
