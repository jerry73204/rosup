use rosup_tests::TestEnv;
use tempfile::TempDir;

/// Run `rosup new <name> [args...]` in a fresh temp dir.
/// Passes `--ros-distro humble` to skip the distro prompt and pipes a newline
/// so the build-type prompt selects the default (ament_cmake).
fn run_new(args: &[&str]) -> (assert_cmd::Command, TempDir) {
    let dir = TempDir::new().unwrap();
    let te = TestEnv::new();
    let mut cmd = te.cmd(dir.path());
    cmd.args(["new"])
        .args(args)
        .args(["--ros-distro", "humble"])
        .write_stdin("\n");
    (cmd, dir)
}

#[test]
fn new_ament_cmake_creates_expected_files() {
    let (mut cmd, dir) = run_new(&["my_pkg"]);
    cmd.assert().success();

    let pkg = dir.path().join("my_pkg");
    assert!(pkg.join("package.xml").exists());
    assert!(pkg.join("CMakeLists.txt").exists());
    assert!(pkg.join("rosup.toml").exists());
    assert!(pkg.join("src").is_dir());
    assert!(pkg.join("include/my_pkg").is_dir());
}

#[test]
fn new_ament_python_creates_expected_files() {
    let (mut cmd, dir) = run_new(&["my_py_pkg", "--build-type", "ament_python"]);
    cmd.assert().success();

    let pkg = dir.path().join("my_py_pkg");
    assert!(pkg.join("package.xml").exists());
    assert!(pkg.join("setup.py").exists());
    assert!(pkg.join("setup.cfg").exists());
    assert!(pkg.join("resource/my_py_pkg").exists());
    assert!(pkg.join("my_py_pkg/__init__.py").exists());
    assert!(pkg.join("rosup.toml").exists());
}

#[test]
fn new_with_node_flag_creates_cpp_node() {
    let (mut cmd, dir) = run_new(&["my_pkg", "--node", "talker"]);
    cmd.assert().success();

    let pkg = dir.path().join("my_pkg");
    assert!(pkg.join("src/talker.cpp").exists());
    let cmake = std::fs::read_to_string(pkg.join("CMakeLists.txt")).unwrap();
    assert!(cmake.contains("add_executable(talker"));
}

#[test]
fn new_python_with_node_flag_creates_py_node() {
    let (mut cmd, dir) = run_new(&[
        "my_py_pkg",
        "--build-type",
        "ament_python",
        "--node",
        "listener",
    ]);
    cmd.assert().success();

    let pkg = dir.path().join("my_py_pkg");
    assert!(pkg.join("my_py_pkg/listener.py").exists());
    let setup = std::fs::read_to_string(pkg.join("setup.py")).unwrap();
    assert!(setup.contains("listener = my_py_pkg.listener:main"));
}

#[test]
fn new_with_deps_appear_in_package_xml() {
    let (mut cmd, dir) = run_new(&["my_pkg", "--dep", "rclcpp", "--dep", "std_msgs"]);
    cmd.assert().success();

    let xml = std::fs::read_to_string(dir.path().join("my_pkg/package.xml")).unwrap();
    assert!(xml.contains("<depend>rclcpp</depend>"));
    assert!(xml.contains("<depend>std_msgs</depend>"));
}

#[test]
fn new_existing_directory_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("my_pkg")).unwrap();
    let te = TestEnv::new();
    te.cmd(dir.path())
        .args([
            "new",
            "my_pkg",
            "--ros-distro",
            "humble",
            "--build-type",
            "ament_cmake",
        ])
        .assert()
        .failure();
}

#[test]
fn new_default_build_type_is_ament_cmake() {
    let (mut cmd, dir) = run_new(&["my_pkg"]);
    cmd.assert().success();

    // CMakeLists.txt present → ament_cmake; no setup.py
    assert!(dir.path().join("my_pkg/CMakeLists.txt").exists());
    assert!(!dir.path().join("my_pkg/setup.py").exists());
}

#[test]
fn new_prompt_selects_ament_python_by_number() {
    let dir = TempDir::new().unwrap();
    let te = TestEnv::new();
    // First line answers the distro prompt; second answers the build-type prompt.
    te.cmd(dir.path())
        .args(["new", "my_pkg"])
        .write_stdin("humble\n2\n")
        .assert()
        .success();

    assert!(dir.path().join("my_pkg/setup.py").exists());
    assert!(!dir.path().join("my_pkg/CMakeLists.txt").exists());
}

#[test]
fn new_with_destination_flag() {
    let dir = TempDir::new().unwrap();
    let dest = dir.path().join("sub/dir");
    std::fs::create_dir_all(&dest).unwrap();
    let te = TestEnv::new();
    te.cmd(dir.path())
        .args([
            "new",
            "my_pkg",
            "--ros-distro",
            "humble",
            "--destination",
            dest.to_str().unwrap(),
        ])
        .write_stdin("\n")
        .assert()
        .success();

    assert!(dest.join("my_pkg/rosup.toml").exists());
}

#[test]
fn new_ros_distro_written_to_toml() {
    let (mut cmd, dir) = run_new(&["my_pkg"]);
    cmd.assert().success();

    let toml = std::fs::read_to_string(dir.path().join("my_pkg/rosup.toml")).unwrap();
    assert!(toml.contains("[resolve]"));
    assert!(toml.contains("ros-distro = \"humble\""));
    assert!(toml.contains("/opt/ros/humble"));
}

#[test]
fn new_empty_distro_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    let te = TestEnv::new();
    // Empty distro input followed by build-type — should fail.
    te.cmd(dir.path())
        .args(["new", "my_pkg"])
        .write_stdin("\nament_cmake\n")
        .assert()
        .failure();
}
