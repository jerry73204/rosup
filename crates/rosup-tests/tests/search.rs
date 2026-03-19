use rosup_tests::TestEnv;
use tempfile::TempDir;

fn env_with_cache() -> (TestEnv, TempDir) {
    // We need the project dir to be a valid rosup project for the CLI to not complain
    // about missing rosup.toml when searching. Actually rosup search doesn't require a
    // project — we just need any dir as cwd.
    let dir = TempDir::new().unwrap();
    let te = TestEnv::new().with_cache();
    (te, dir)
}

#[test]
fn search_returns_matches() {
    let (te, dir) = env_with_cache();
    te.cmd(dir.path())
        .args(["search", "rclcpp", "--distro", "humble"])
        .assert()
        .success()
        .stdout(predicates::str::contains("rclcpp"));
}

#[test]
fn search_no_matches_message() {
    let (te, dir) = env_with_cache();
    te.cmd(dir.path())
        .args(["search", "xyz_no_such_thing_at_all", "--distro", "humble"])
        .assert()
        .success()
        .stdout(predicates::str::contains("No packages found"));
}

#[test]
fn search_limit_flag() {
    let (te, dir) = env_with_cache();
    let output = te
        .cmd(dir.path())
        .args(["search", "rclcpp", "--distro", "humble", "--limit", "2"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let lines: Vec<_> = String::from_utf8(output)
        .unwrap()
        .lines()
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .into_iter()
        .map(str::to_owned)
        .collect();
    assert_eq!(lines.len(), 2, "expected exactly 2 result lines");
}

/// `rosup search` reads `ros-distro` from `rosup.toml` when neither `--distro`
/// nor `ROS_DISTRO` is set.
#[test]
fn search_reads_distro_from_rosup_toml() {
    use rosup_tests::project::write_rosup_toml;
    let dir = tempfile::TempDir::new().unwrap();
    write_rosup_toml(
        dir.path(),
        "[package]\nname = \"my_pkg\"\n\n[resolve]\nros-distro = \"humble\"\n",
    );
    let te = TestEnv::new().with_cache();
    // No --distro flag and ROS_DISTRO is unset (TestEnv removes it).
    te.cmd(dir.path())
        .args(["search", "rclcpp"])
        .assert()
        .success()
        .stdout(predicates::str::contains("rclcpp"));
}

#[test]
fn search_missing_distro_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    let te = TestEnv::new();
    // No --distro and no ROS_DISTRO env set (TestEnv removes it).
    te.cmd(dir.path())
        .args(["search", "rclcpp"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("ROS_DISTRO"));
}

/// `ROS_DISTRO` env is consulted as a fallback when no `--distro` flag and
/// no `rosup.toml` `ros-distro` is set.
#[test]
fn search_falls_back_to_ros_distro_env() {
    let dir = TempDir::new().unwrap();
    let te = TestEnv::new().with_cache();
    // TestEnv removes ROS_DISTRO, but we re-add it explicitly.
    te.cmd(dir.path())
        .env("ROS_DISTRO", "humble")
        .args(["search", "rclcpp"])
        .assert()
        .success()
        .stdout(predicates::str::contains("rclcpp"));
}
