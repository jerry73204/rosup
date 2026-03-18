use rosup_tests::{PackageProject, TestEnv};
use std::fs;

fn env() -> TestEnv {
    TestEnv::new()
}

#[test]
fn clean_default_removes_build_and_log() {
    let proj = PackageProject::new("my_pkg").with_artifacts();
    env().cmd(proj.root()).args(["clean"]).assert().success();

    assert!(!proj.root().join("build").exists());
    assert!(!proj.root().join("log").exists());
    assert!(
        proj.root().join("install").exists(),
        "install/ must survive"
    );
}

#[test]
fn clean_all_removes_install() {
    let proj = PackageProject::new("my_pkg").with_artifacts();
    env()
        .cmd(proj.root())
        .args(["clean", "--all"])
        .assert()
        .success();

    assert!(!proj.root().join("build").exists());
    assert!(!proj.root().join("log").exists());
    assert!(!proj.root().join("install").exists());
}

#[test]
fn clean_deps_removes_dot_rosup_artifacts() {
    let proj = PackageProject::new("my_pkg").with_dep_src("my_dep");
    // Also pre-create .rosup/build and .rosup/install.
    let rosup = proj.root().join(".rosup");
    fs::create_dir_all(rosup.join("build")).unwrap();
    fs::create_dir_all(rosup.join("install")).unwrap();

    env()
        .cmd(proj.root())
        .args(["clean", "--deps"])
        .assert()
        .success();

    assert!(!rosup.join("build").exists());
    assert!(!rosup.join("install").exists());
    assert!(
        rosup.join("src").exists(),
        ".rosup/src/ must survive --deps alone"
    );
}

#[test]
fn clean_deps_src_removes_worktrees() {
    let proj = PackageProject::new("my_pkg").with_dep_src("my_dep");
    let rosup = proj.root().join(".rosup");
    fs::create_dir_all(rosup.join("build")).unwrap();
    fs::create_dir_all(rosup.join("install")).unwrap();

    env()
        .cmd(proj.root())
        .args(["clean", "--deps", "--src"])
        .assert()
        .success();

    assert!(!rosup.join("build").exists());
    assert!(!rosup.join("install").exists());
    assert!(!rosup.join("src").exists());
}

#[test]
fn clean_package_scoped() {
    let proj = PackageProject::new("my_pkg")
        .with_package_artifacts("my_pkg")
        .with_package_artifacts("other_pkg");

    env()
        .cmd(proj.root())
        .args(["clean", "-p", "my_pkg"])
        .assert()
        .success();

    assert!(!proj.root().join("build/my_pkg").exists());
    assert!(!proj.root().join("install/my_pkg").exists());
    assert!(
        proj.root().join("build/other_pkg").exists(),
        "other_pkg must survive"
    );
    assert!(proj.root().join("install/other_pkg").exists());
}

#[test]
fn clean_no_error_when_nothing_to_clean() {
    let proj = PackageProject::new("my_pkg");
    env().cmd(proj.root()).args(["clean"]).assert().success();
}
