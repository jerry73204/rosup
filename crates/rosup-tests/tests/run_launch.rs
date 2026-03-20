use rosup_tests::{PackageProject, TestEnv, assert_args_contain};

fn env() -> TestEnv {
    TestEnv::new()
}

// ── rosup run ─────────────────────────────────────────────────────────────────

#[test]
fn run_invokes_ros2_run() {
    let proj = PackageProject::new("my_pkg").with_install_dir();
    let te = env();

    te.cmd(proj.root())
        .args(["run", "my_pkg", "my_node"])
        .assert()
        .success();

    let calls = te.ros2_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(&calls[0], &["run", "my_pkg", "my_node"]);
}

#[test]
fn run_forwards_args_after_separator() {
    let proj = PackageProject::new("my_pkg").with_install_dir();
    let te = env();

    te.cmd(proj.root())
        .args([
            "run",
            "my_pkg",
            "my_node",
            "--",
            "--ros-args",
            "-p",
            "param:=value",
        ])
        .assert()
        .success();

    let calls = te.ros2_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(
        &calls[0],
        &[
            "run",
            "my_pkg",
            "my_node",
            "--",
            "--ros-args",
            "-p",
            "param:=value",
        ],
    );
}

#[test]
fn run_fails_without_install_dir() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["run", "my_pkg", "my_node"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("rosup build"));
}

#[test]
fn run_propagates_exit_code() {
    let proj = PackageProject::new("my_pkg").with_install_dir();
    let te = env();

    te.cmd(proj.root())
        .env("FAKE_ROS2_EXIT", "42")
        .args(["run", "my_pkg", "my_node"])
        .assert()
        .failure();
}

// ── rosup launch ──────────────────────────────────────────────────────────────

#[test]
fn launch_invokes_ros2_launch() {
    let proj = PackageProject::new("my_pkg").with_install_dir();
    let te = env();

    te.cmd(proj.root())
        .args(["launch", "my_pkg", "launch.py"])
        .assert()
        .success();

    let calls = te.ros2_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(&calls[0], &["launch", "my_pkg", "launch.py"]);
}

#[test]
fn launch_forwards_args_after_separator() {
    let proj = PackageProject::new("my_pkg").with_install_dir();
    let te = env();

    te.cmd(proj.root())
        .args(["launch", "my_pkg", "launch.py", "--", "arg:=value"])
        .assert()
        .success();

    let calls = te.ros2_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(
        &calls[0],
        &["launch", "my_pkg", "launch.py", "--", "arg:=value"],
    );
}

#[test]
fn launch_fails_without_install_dir() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["launch", "my_pkg", "launch.py"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("rosup build"));
}

#[test]
fn launch_propagates_exit_code() {
    let proj = PackageProject::new("my_pkg").with_install_dir();
    let te = env();

    te.cmd(proj.root())
        .env("FAKE_ROS2_EXIT", "1")
        .args(["launch", "my_pkg", "launch.py"])
        .assert()
        .failure();
}
