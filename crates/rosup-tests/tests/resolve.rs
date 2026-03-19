use rosup_tests::project::{write_package_xml, write_rosup_toml};
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

/// In a workspace, deps that are sibling member packages should not be passed
/// to the external resolver — colcon builds them from source.
#[test]
fn workspace_sibling_deps_excluded_from_resolve() {
    let dir = tempfile::TempDir::new().unwrap();
    let pkg_core = dir.path().join("src/pkg_core");
    let pkg_launch = dir.path().join("src/pkg_launch");
    std::fs::create_dir_all(&pkg_core).unwrap();
    std::fs::create_dir_all(&pkg_launch).unwrap();
    write_package_xml(&pkg_core, "pkg_core", &[]);
    // pkg_launch depends on pkg_core (sibling) and rclcpp (external).
    write_package_xml(&pkg_launch, "pkg_launch", &["pkg_core", "rclcpp"]);
    write_rosup_toml(
        dir.path(),
        "[workspace]\n\n[resolve]\nros-distro = \"humble\"\n",
    );

    let te = TestEnv::new().with_cache();
    let output = te
        .cmd(dir.path())
        .args(["resolve", "--dry-run"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stdout = String::from_utf8(output).unwrap();

    // rclcpp (external) should appear in the resolution plan.
    assert!(
        stdout.contains("rclcpp"),
        "external dep rclcpp should be resolved"
    );
    // pkg_core (sibling member) should NOT appear — colcon handles it.
    assert!(
        !stdout.contains("pkg_core"),
        "sibling member pkg_core should be excluded from resolver"
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
        .stderr(predicates::str::contains("could not resolve"));
}
