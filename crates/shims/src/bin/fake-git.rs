//! Fake `git` shim for integration tests.
//!
//! Records invocations to `$SHIM_LOG_DIR/git-<pid>.json`.
//! Implements the minimal subset used by rosup:
//!   - `clone --bare <url> <dest>` → creates `<dest>/objects/`
//!   - `worktree add --detach <wt> <ref>` → creates `<wt>/`
//!   - `clone --branch <br> <url> <dest>` → creates `<dest>/` with stub package.xml
//!   - `fetch`, `reset` → no-op
//!
//! For regular clones, sets `FAKE_GIT_PKG_XMLS=pkg_a,pkg_b` to create
//! subdirectory package.xml files (workspace layout).  Without that env var a
//! single `package.xml` is written at the repo root (package layout).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();

    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    if let Ok(log_dir) = env::var("SHIM_LOG_DIR") {
        let record = serde_json::json!({ "tool": "git", "args": &args });
        let path = PathBuf::from(&log_dir).join(format!("git-{pid}-{nanos}.json"));
        let _ = fs::write(&path, record.to_string());
    }

    // Determine subcommand (skip leading -C <dir> pairs).
    let mut i = 0usize;
    while i < args_ref.len() && args_ref[i] == "-C" {
        i += 2; // skip -C <dir>
    }
    let subcommand = args_ref.get(i).copied().unwrap_or("");
    let rest = if i + 1 < args_ref.len() {
        &args_ref[i + 1..]
    } else {
        &[]
    };

    match subcommand {
        "clone" => {
            let is_bare = rest.contains(&"--bare");
            // Last non-flag argument is the destination path.
            let dest = rest.iter().rfind(|a| !a.starts_with('-')).copied();

            if let Some(dest) = dest {
                let dest_path = PathBuf::from(dest);
                if is_bare {
                    // Bare clone stub: just needs the directory to exist.
                    let _ = fs::create_dir_all(dest_path.join("objects"));
                } else {
                    // Regular clone: create package.xml file(s).
                    let _ = fs::create_dir_all(&dest_path);
                    let pkg_xmls = env::var("FAKE_GIT_PKG_XMLS").unwrap_or_default();
                    if pkg_xmls.is_empty() {
                        // Single-package repo: package.xml at root.
                        write_package_xml(&dest_path, "cloned_pkg");
                    } else {
                        // Multi-package repo: one subdir per package.
                        for pkg in pkg_xmls.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                            let pkg_dir = dest_path.join(pkg);
                            let _ = fs::create_dir_all(&pkg_dir);
                            write_package_xml(&pkg_dir, pkg);
                        }
                    }
                }
            }
        }
        "worktree" => {
            // git worktree add [--quiet] [--detach] <wt_path> <ref>
            if rest.first().copied() == Some("add") {
                let wt = rest[1..].iter().find(|a| !a.starts_with('-')).copied();
                if let Some(wt) = wt {
                    let _ = fs::create_dir_all(wt);
                }
            }
        }
        // fetch, reset, init → no-op
        _ => {}
    }

    let exit_code: i32 = env::var("FAKE_GIT_EXIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    std::process::exit(exit_code);
}

fn write_package_xml(dir: &Path, name: &str) {
    let xml = format!(
        r#"<?xml version="1.0"?>
<package format="3">
  <name>{name}</name>
  <version>0.0.1</version>
  <description>stub</description>
  <maintainer email="test@example.com">test</maintainer>
  <license>MIT</license>
</package>
"#
    );
    let _ = fs::write(dir.join("package.xml"), xml);
}
