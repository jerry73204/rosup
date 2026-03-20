//! Fake `ros2` shim for integration tests.
//!
//! Records the invocation as JSON to `$SHIM_LOG_DIR/ros2-<pid>.json`.
//! Exits with `$FAKE_ROS2_EXIT` (default 0).

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

    if !log_dir.is_empty() {
        let record = serde_json::json!({
            "tool": "ros2",
            "args": &args,
            "env": {
                "AMENT_PREFIX_PATH": env::var("AMENT_PREFIX_PATH").unwrap_or_default(),
            }
        });
        let path = PathBuf::from(&log_dir).join(format!("ros2-{pid}-{nanos}.json"));
        let _ = fs::write(&path, record.to_string());
    }

    let exit_code: i32 = env::var("FAKE_ROS2_EXIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    std::process::exit(exit_code);
}
