//! Fake `rosdep` shim for integration tests.
//!
//! Records invocations to `$SHIM_LOG_DIR/rosdep-<pid>.json`.
//!
//! `resolve <pkg> --rosdistro <distro>`:
//!   - If `FAKE_ROSDEP_PACKAGES` is unset or empty → succeeds for any package.
//!   - If `FAKE_ROSDEP_PACKAGES=a,b,c` → succeeds only for those packages.
//!
//! `install --default-yes ...` → always succeeds.

use std::env;
use std::fs;
use std::path::PathBuf;
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
        let record = serde_json::json!({ "tool": "rosdep", "args": &args });
        let path = PathBuf::from(&log_dir).join(format!("rosdep-{pid}-{nanos}.json"));
        let _ = fs::write(&path, record.to_string());
    }

    let subcommand = args_ref.first().copied().unwrap_or("");

    match subcommand {
        "resolve" => {
            let pkg = args_ref.get(1).copied().unwrap_or("");
            let distro = args_ref
                .iter()
                .position(|a| *a == "--rosdistro")
                .and_then(|i| args_ref.get(i + 1).copied())
                .unwrap_or("humble");

            let whitelist = env::var("FAKE_ROSDEP_PACKAGES").unwrap_or_default();
            let resolvable: Vec<&str> = whitelist
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();

            let resolves = resolvable.is_empty() || resolvable.contains(&pkg);

            if resolves {
                let dashed = pkg.replace('_', "-");
                println!("#apt\nros-{distro}-{dashed}");
                std::process::exit(0);
            } else {
                eprintln!("ERROR: No definition for [{pkg}]");
                std::process::exit(1);
            }
        }
        "install" => {
            println!("Installing packages: done.");
            std::process::exit(0);
        }
        _ => {
            std::process::exit(1);
        }
    }
}
