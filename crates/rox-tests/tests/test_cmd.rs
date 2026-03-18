use rox_tests::{PackageProject, TestEnv, assert_args_contain};

fn env() -> TestEnv {
    TestEnv::new()
}

#[test]
fn test_default_invokes_colcon_test() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["test", "--no-resolve"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    // At least the `colcon test` call; `colcon test-result` follows.
    assert!(
        calls
            .iter()
            .any(|c| c.args.first().map(String::as_str) == Some("test"))
    );
    assert!(
        calls
            .iter()
            .any(|c| c.args.first().map(String::as_str) == Some("test-result")),
        "colcon test-result should be called after tests"
    );
}

#[test]
fn test_packages_select() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["test", "--no-resolve", "-p", "my_pkg"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    let test_call = calls
        .iter()
        .find(|c| c.args.first().map(String::as_str) == Some("test"))
        .expect("colcon test call");
    assert_args_contain(test_call, &["--packages-select", "my_pkg"]);
}

#[test]
fn test_failure_propagates() {
    let proj = PackageProject::new("my_pkg");

    env()
        .cmd(proj.root())
        .env("FAKE_COLCON_EXIT", "1")
        .args(["test", "--no-resolve"])
        .assert()
        .failure();
}

#[test]
fn test_retest_until_pass_retries() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    // All calls fail; rox test should exit non-zero but make 3 colcon test attempts.
    te.cmd(proj.root())
        .env("FAKE_COLCON_EXIT", "1")
        .args(["test", "--no-resolve", "--retest-until-pass", "2"])
        .assert()
        .failure();

    let test_calls: Vec<_> = te
        .colcon_calls()
        .into_iter()
        .filter(|c| c.args.first().map(String::as_str) == Some("test"))
        .collect();
    assert_eq!(
        test_calls.len(),
        3,
        "expected 3 colcon test attempts (1 + 2 retries)"
    );
}
