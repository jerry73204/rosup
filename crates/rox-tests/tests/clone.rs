use rox_tests::TestEnv;
use std::fs;
use tempfile::TempDir;

fn env_with_cache() -> (TestEnv, TempDir) {
    let dir = TempDir::new().unwrap();
    let te = TestEnv::new().with_cache();
    (te, dir)
}

#[test]
fn clone_invokes_git_clone() {
    let (te, dir) = env_with_cache();
    te.cmd(dir.path())
        .args(["clone", "solo_pkg", "--distro", "humble"])
        .assert()
        .success();

    let git_calls = te.git_calls();
    let clone_call = git_calls
        .iter()
        .find(|c| c.args.contains(&"clone".to_string()))
        .expect("git clone call");
    // Should include the repo URL and clone into solo_pkg dir.
    assert!(
        clone_call.args.iter().any(|a| a.contains("solo_pkg")),
        "git clone args should mention solo_pkg: {:?}",
        clone_call.args
    );
}

#[test]
fn clone_creates_rox_toml() {
    let (te, dir) = env_with_cache();
    // fake-git creates package.xml at dest root by default.
    te.cmd(dir.path())
        .args(["clone", "solo_pkg", "--distro", "humble"])
        .assert()
        .success();

    // solo_pkg repo is named "solo_pkg" in the fixture.
    let dest = dir.path().join("solo_pkg");
    assert!(
        dest.join("rox.toml").exists(),
        "rox.toml should be created in cloned dir"
    );
}

#[test]
fn clone_unknown_package_exits_nonzero() {
    let (te, dir) = env_with_cache();
    te.cmd(dir.path())
        .args(["clone", "totally_unknown_pkg_xyz", "--distro", "humble"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("not found"));
}

#[test]
fn clone_workspace_mode_for_multi_package_repo() {
    let (te, dir) = env_with_cache();
    // sensor_msgs is in common_interfaces repo (multi-package).
    // FAKE_GIT_PKG_XMLS will create pkg_a/ and pkg_b/ subdirs with package.xml.
    te.cmd(dir.path())
        .env("FAKE_GIT_PKG_XMLS", "sensor_msgs,std_msgs")
        .args(["clone", "sensor_msgs", "--distro", "humble"])
        .assert()
        .success();

    let dest = dir.path().join("common_interfaces");
    let toml = fs::read_to_string(dest.join("rox.toml")).unwrap();
    assert!(
        toml.contains("[workspace]"),
        "multi-package clone should produce workspace rox.toml"
    );
}
