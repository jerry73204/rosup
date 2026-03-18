use rox_tests::TestEnv;
use tempfile::TempDir;

fn env_with_cache() -> (TestEnv, TempDir) {
    // We need the project dir to be a valid rox project for the CLI to not complain
    // about missing rox.toml when searching. Actually rox search doesn't require a
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

#[test]
fn search_missing_distro_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    let te = TestEnv::new();
    // No --distro and no ROS_DISTRO env set (TestEnv removes it).
    te.cmd(dir.path())
        .args(["search", "rclcpp"])
        .assert()
        .failure();
}
