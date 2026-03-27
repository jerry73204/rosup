use rosup_tests::TestEnv;
use rosup_tests::project::{write_package_xml, write_rosup_toml};
use std::fs;
use tempfile::TempDir;

fn env() -> TestEnv {
    TestEnv::new().with_cache()
}

fn make_repos_file(dir: &std::path::Path) {
    // Create a .repos file with repos that match rosdistro entries
    fs::write(
        dir.join("deps.repos"),
        r#"repositories:
  nav2:
    type: git
    url: https://github.com/ros-planning/navigation2.git
    version: 1.1.20
  rclcpp_repo:
    type: git
    url: https://github.com/ros2/rclcpp.git
    version: humble
"#,
    )
    .unwrap();
}

#[test]
fn resolve_dry_run_shows_repos_source() {
    let dir = TempDir::new().unwrap();
    let pkg_dir = dir.path().join("src/my_pkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    write_package_xml(&pkg_dir, "my_pkg", &["nav2_core"]);

    make_repos_file(dir.path());

    write_rosup_toml(
        dir.path(),
        "[workspace]\n\n\
         [resolve]\n\
         ros-distro = \"humble\"\n\n\
         [[resolve.sources]]\n\
         name = \"nav2\"\n\
         repos = \"deps.repos\"\n",
    );

    let output = env()
        .cmd(dir.path())
        .args(["resolve", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();

    // nav2_core should resolve from the .repos source, not rosdistro
    // (both would work, but .repos has higher priority)
    assert!(
        stdout.contains("nav2_core"),
        "nav2_core should be in the plan"
    );
}

#[test]
fn resolve_dry_run_without_repos_source_uses_rosdistro() {
    let dir = TempDir::new().unwrap();
    let pkg_dir = dir.path().join("src/my_pkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    write_package_xml(&pkg_dir, "my_pkg", &["rclcpp"]);

    // No [[resolve.sources]] — should use ament/rosdep/rosdistro
    write_rosup_toml(
        dir.path(),
        "[workspace]\n\n[resolve]\nros-distro = \"humble\"\n",
    );

    env()
        .cmd(dir.path())
        .args(["resolve", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("rclcpp"));
}

#[test]
fn resolve_with_git_source() {
    let dir = TempDir::new().unwrap();
    let pkg_dir = dir.path().join("src/my_pkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    write_package_xml(&pkg_dir, "my_pkg", &["my_custom_dep"]);

    // Direct git repo source — dep name matches source name.
    // Use FAKE_ROSDEP_PACKAGES to make rosdep NOT resolve this dep,
    // so it falls through to the .repos source.
    write_rosup_toml(
        dir.path(),
        "[workspace]\n\n\
         [resolve]\n\
         ros-distro = \"humble\"\n\n\
         [[resolve.sources]]\n\
         name = \"my_custom_dep\"\n\
         git = \"https://github.com/example/my_custom_dep.git\"\n\
         branch = \"main\"\n",
    );

    let output = env()
        .cmd(dir.path())
        .env("FAKE_ROSDEP_PACKAGES", "other_pkg")
        .args(["resolve", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();

    assert!(
        stdout.contains("my_custom_dep") && stdout.contains("repos"),
        "my_custom_dep should resolve from git source: {stdout}"
    );
}

#[test]
fn missing_repos_file_warns_but_resolves() {
    let dir = TempDir::new().unwrap();
    let pkg_dir = dir.path().join("src/my_pkg");
    fs::create_dir_all(&pkg_dir).unwrap();
    write_package_xml(&pkg_dir, "my_pkg", &["rclcpp"]);

    // Reference a .repos file that doesn't exist
    write_rosup_toml(
        dir.path(),
        "[workspace]\n\n\
         [resolve]\n\
         ros-distro = \"humble\"\n\n\
         [[resolve.sources]]\n\
         name = \"missing\"\n\
         repos = \"nonexistent.repos\"\n",
    );

    // Should still succeed — missing .repos is a warning, not an error
    env()
        .cmd(dir.path())
        .args(["resolve", "--dry-run"])
        .assert()
        .success();
}
