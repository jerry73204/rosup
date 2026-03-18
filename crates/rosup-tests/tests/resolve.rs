use rosup_tests::{PackageProject, TestEnv};

/// Create a package project with a dep and the humble cache in the fake home.
fn setup(dep: &str) -> (PackageProject, TestEnv) {
    let proj = PackageProject::with_toml(
        "my_pkg",
        "[package]\nname = \"my_pkg\"\n\n[resolve]\nros_distro = \"humble\"\n",
    );
    // Overwrite package.xml to include the dep.
    rosup_tests::project::write_package_xml(proj.root(), "my_pkg", &[dep]);
    let te = TestEnv::new().with_cache();
    (proj, te)
}

#[test]
fn dry_run_binary_resolution() {
    let (proj, te) = setup("solo_pkg");
    // Default fake-rosdep resolves everything → binary resolution.
    te.cmd(proj.root())
        .args(["resolve", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("would resolve solo_pkg"))
        .stdout(predicates::str::contains("binary"));
}

#[test]
fn dry_run_no_rosdep_install() {
    let (proj, te) = setup("solo_pkg");
    te.cmd(proj.root())
        .args(["resolve", "--dry-run"])
        .assert()
        .success();

    // No rosdep install call (only resolve).
    let rosdep_calls = te.rosdep_calls();
    assert!(
        rosdep_calls
            .iter()
            .all(|c| c.args.first().map(String::as_str) != Some("install")),
        "dry-run must not call rosdep install"
    );
}

#[test]
fn unresolved_dep_exits_nonzero() {
    let (proj, te) = setup("totally_unknown_dep_xyz");
    // Make rosdep fail for this dep by whitelisting something else.
    te.cmd(proj.root())
        .env("FAKE_ROSDEP_PACKAGES", "other_pkg")
        .args(["resolve", "--dry-run"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("unresolved"));
}
