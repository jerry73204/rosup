//! Fake `colcon` shim for integration tests.
//!
//! Records the invocation as JSON to `$SHIM_LOG_DIR/colcon-<pid>.json`.
//! Exits with `$FAKE_COLCON_EXIT` (default 0).
//! When `FAKE_COLCON_CREATE_ARTIFACTS=1`, creates stub install-base content.
//! When `FAKE_COLCON_FAIL_FIRST_N=<n>`, fails on the first n calls then succeeds.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let pid = std::process::id();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();

    let log_dir = env::var("SHIM_LOG_DIR").unwrap_or_default();

    // Record invocation.
    if !log_dir.is_empty() {
        let record = serde_json::json!({
            "tool": "colcon",
            "args": &args,
            "env": {
                "AMENT_PREFIX_PATH": env::var("AMENT_PREFIX_PATH").unwrap_or_default(),
            }
        });
        let path = PathBuf::from(&log_dir).join(format!("colcon-{pid}-{nanos}.json"));
        let _ = fs::write(&path, record.to_string());
    }

    // Optionally create stub install artifacts.
    if env::var("FAKE_COLCON_CREATE_ARTIFACTS").as_deref() == Ok("1") {
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Some(pos) = args_ref.iter().position(|a| *a == "--install-base")
            && let Some(base) = args_ref.get(pos + 1)
        {
            let _ = fs::create_dir_all(PathBuf::from(base).join("share").join("ament_index"));
        }
    }

    // FAKE_COLCON_FAIL_FIRST_N: count prior invocations and fail if still within n.
    if let Ok(n_str) = env::var("FAKE_COLCON_FAIL_FIRST_N")
        && let Ok(n) = n_str.parse::<usize>()
    {
        let prior = if log_dir.is_empty() {
            0
        } else {
            fs::read_dir(&log_dir)
                .map(|d| {
                    d.filter_map(|e| e.ok())
                        .filter(|e| {
                            e.file_name()
                                .to_str()
                                .map(|s| s.starts_with("colcon-"))
                                .unwrap_or(false)
                        })
                        .count()
                })
                .unwrap_or(0)
        };
        // prior includes the file we just wrote, so prior-1 = calls before this one.
        if prior.saturating_sub(1) < n {
            std::process::exit(1);
        }
    }

    let exit_code: i32 = env::var("FAKE_COLCON_EXIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    std::process::exit(exit_code);
}
