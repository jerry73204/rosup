use rox_tests::{TestEnv, project::write_package_xml};
use std::fs;
use tempfile::TempDir;

fn env() -> TestEnv {
    TestEnv::new()
}

#[test]
fn init_package_mode() {
    let dir = TempDir::new().unwrap();
    write_package_xml(dir.path(), "my_pkg", &[]);

    env()
        .cmd(dir.path())
        .args(["init"])
        .assert()
        .success()
        .stdout(predicates::str::contains("package"));

    let toml = fs::read_to_string(dir.path().join("rox.toml")).unwrap();
    assert!(toml.contains("[package]"));
    assert!(toml.contains("name = \"my_pkg\""));
}

#[test]
fn init_workspace_mode() {
    let dir = TempDir::new().unwrap();
    let pkg_a = dir.path().join("src/pkg_a");
    let pkg_b = dir.path().join("src/pkg_b");
    fs::create_dir_all(&pkg_a).unwrap();
    fs::create_dir_all(&pkg_b).unwrap();
    write_package_xml(&pkg_a, "pkg_a", &[]);
    write_package_xml(&pkg_b, "pkg_b", &[]);

    env()
        .cmd(dir.path())
        .args(["init"])
        .assert()
        .success()
        .stdout(predicates::str::contains("workspace"));

    let toml = fs::read_to_string(dir.path().join("rox.toml")).unwrap();
    assert!(toml.contains("[workspace]"));
    assert!(toml.contains("pkg_a"));
    assert!(toml.contains("pkg_b"));
}

#[test]
fn init_adds_gitignore_entry() {
    let dir = TempDir::new().unwrap();
    write_package_xml(dir.path(), "my_pkg", &[]);

    env().cmd(dir.path()).args(["init"]).assert().success();

    let gitignore = fs::read_to_string(dir.path().join(".gitignore")).unwrap();
    assert!(gitignore.lines().any(|l| l.trim() == ".rox/"));
}

#[test]
fn init_no_overwrite_without_force() {
    let dir = TempDir::new().unwrap();
    write_package_xml(dir.path(), "my_pkg", &[]);
    fs::write(dir.path().join("rox.toml"), "[package]\nname = \"old\"\n").unwrap();

    env()
        .cmd(dir.path())
        .args(["init"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("already exists"));
}

#[test]
fn init_force_overwrites() {
    let dir = TempDir::new().unwrap();
    write_package_xml(dir.path(), "my_pkg", &[]);
    fs::write(dir.path().join("rox.toml"), "[package]\nname = \"old\"\n").unwrap();

    env()
        .cmd(dir.path())
        .args(["init", "--force"])
        .assert()
        .success();

    let toml = fs::read_to_string(dir.path().join("rox.toml")).unwrap();
    assert!(toml.contains("name = \"my_pkg\""));
}

#[test]
fn init_no_package_xml_exits_nonzero() {
    let dir = TempDir::new().unwrap();
    env().cmd(dir.path()).args(["init"]).assert().failure();
}
