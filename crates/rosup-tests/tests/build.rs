use rosup_tests::{PackageProject, TestEnv, assert_args_contain};

fn env() -> TestEnv {
    TestEnv::new()
}

#[test]
fn build_default_invokes_colcon() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["build", "--no-resolve"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].args[0], "build");
}

#[test]
fn build_release_flag() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["build", "--no-resolve", "--release"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(&calls[0], &["--cmake-args", "-DCMAKE_BUILD_TYPE=Release"]);
}

#[test]
fn build_debug_flag() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["build", "--no-resolve", "--debug"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(&calls[0], &["--cmake-args", "-DCMAKE_BUILD_TYPE=Debug"]);
}

#[test]
fn build_packages_select() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["build", "--no-resolve", "-p", "my_pkg"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(&calls[0], &["--packages-select", "my_pkg"]);
}

#[test]
fn build_packages_up_to() {
    let proj = PackageProject::new("my_pkg");
    let te = env();

    te.cmd(proj.root())
        .args(["build", "--no-resolve", "-p", "my_pkg", "--deps"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(&calls[0], &["--packages-up-to", "my_pkg"]);
}

#[test]
fn build_colcon_failure_propagates() {
    let proj = PackageProject::new("my_pkg");

    env()
        .cmd(proj.root())
        .env("FAKE_COLCON_EXIT", "1")
        .args(["build", "--no-resolve"])
        .assert()
        .failure();
}

#[test]
fn build_cmake_args_from_toml() {
    let toml = "[package]\nname = \"my_pkg\"\n\n[build]\ncmake_args = [\"-DFOO=bar\"]\n";
    let proj = PackageProject::with_toml("my_pkg", toml);
    let te = env();

    te.cmd(proj.root())
        .args(["build", "--no-resolve"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    assert_eq!(calls.len(), 1);
    assert_args_contain(&calls[0], &["--cmake-args", "-DFOO=bar"]);
}

#[test]
fn build_rebuild_deps_rebuilds_dep_layer() {
    // Pre-create .rosup/src/<repo> and .rosup/install/ so without --rebuild-deps
    // the dep layer would be skipped.
    let proj = PackageProject::new("my_pkg")
        .with_dep_src("some_dep")
        .with_dep_layer_stub();
    let te = env();

    te.cmd(proj.root())
        .args(["build", "--no-resolve", "--rebuild-deps"])
        .assert()
        .success();

    // Two colcon calls: dep layer + workspace.
    let calls = te.colcon_calls();
    assert_eq!(
        calls.len(),
        2,
        "expected dep layer + workspace colcon calls"
    );
    // First call has --base-paths (dep layer).
    assert_args_contain(&calls[0], &["--base-paths"]);
    // Second call is the workspace build.
    assert_eq!(calls[1].args[0], "build");
    assert!(!calls[1].args.contains(&"--base-paths".to_string()));
}

#[test]
fn build_override_pkg_separate_invocation() {
    let toml = "[package]\nname = \"my_pkg\"\n\n[build.overrides.my_pkg]\ncmake_args = [\"-DOVERRIDE=1\"]\n";
    let proj = PackageProject::with_toml("my_pkg", toml);
    let te = env();

    te.cmd(proj.root())
        .args(["build", "--no-resolve", "-p", "my_pkg"])
        .assert()
        .success();

    let calls = te.colcon_calls();
    // Only the override invocation (base skipped because all selected pkgs have overrides).
    assert_eq!(calls.len(), 1);
    assert_args_contain(&calls[0], &["--packages-select", "my_pkg", "-DOVERRIDE=1"]);
}
