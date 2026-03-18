use rosup_tests::project::write_package_xml;
use rosup_tests::{PackageProject, TestEnv};

/// Create a package project with a dep and the humble cache in the fake home.
fn setup(dep: &str) -> (PackageProject, TestEnv) {
    let proj = PackageProject::with_toml(
        "my_pkg",
        "[package]\nname = \"my_pkg\"\n\n[resolve]\nros-distro = \"humble\"\n",
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

/// Resolve works with only a `package.xml` — no `rosup.toml` required.
/// The distro comes from `ROS_DISTRO` env since there is no manifest to read it from.
#[test]
fn resolve_without_manifest_uses_package_xml() {
    let dir = tempfile::TempDir::new().unwrap();
    write_package_xml(dir.path(), "bare_pkg", &["solo_pkg"]);

    let te = TestEnv::new().with_cache();
    te.cmd(dir.path())
        .env("ROS_DISTRO", "humble")
        .args(["resolve", "--dry-run"])
        .assert()
        .success()
        .stdout(predicates::str::contains("would resolve solo_pkg"));
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
