//! DSV parser and per-package environment builder.
//!
//! Parses colcon/ament DSV (Declarative Setup) files and applies them to
//! build a `HashMap<String, String>` environment. This replaces the 25-second
//! bash subprocess that colcon uses to source `package.sh` for every package.
//!
//! DSV format: each line is `action;VARIABLE;suffix` (semicolon-separated).
//! See `docs/design/native-builder.md` section 1.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::trace;

// ── DSV types ───────────────────────────────────────────────────────────────

/// A parsed DSV entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DsvEntry {
    pub action: DsvAction,
    pub variable: String,
    /// Path suffix relative to the install prefix. Empty string means the
    /// prefix itself.
    pub suffix: String,
}

/// DSV action types (from colcon source).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DsvAction {
    PrependNonDuplicate,
    PrependNonDuplicateIfExists,
    AppendNonDuplicate,
    Set,
    SetIfUnset,
    /// Source another file. The `variable` field holds the relative path.
    /// Only DSV-to-DSV references are followed; shell scripts are skipped.
    Source,
}

impl DsvAction {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "prepend-non-duplicate" => Some(Self::PrependNonDuplicate),
            "prepend-non-duplicate-if-exists" => Some(Self::PrependNonDuplicateIfExists),
            "append-non-duplicate" => Some(Self::AppendNonDuplicate),
            "set" => Some(Self::Set),
            "set-if-unset" => Some(Self::SetIfUnset),
            "source" => Some(Self::Source),
            _ => None,
        }
    }
}

// ── DSV parser ──────────────────────────────────────────────────────────────

/// Parse a single DSV file into entries.
///
/// Skips empty lines, comment lines (starting with `#`), and lines with
/// unknown actions. For `source` entries, only `.dsv` references are
/// followed recursively; shell script references are ignored.
pub fn parse_dsv_file(path: &Path) -> Vec<DsvEntry> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    parse_dsv_content(&content)
}

/// Parse DSV content string into entries.
fn parse_dsv_content(content: &str) -> Vec<DsvEntry> {
    let mut entries = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.splitn(3, ';').collect();
        if parts.len() < 2 {
            continue;
        }
        let Some(action) = DsvAction::from_str(parts[0]) else {
            continue;
        };
        let variable = parts[1].to_owned();
        let suffix = if parts.len() > 2 {
            parts[2].to_owned()
        } else {
            String::new()
        };
        entries.push(DsvEntry {
            action,
            variable,
            suffix,
        });
    }
    entries
}

/// Recursively collect all environment-affecting DSV entries from a file.
///
/// `source` entries pointing to `.dsv` files are followed recursively.
/// Shell script references and non-DSV sources are skipped.
/// `prefix` is the install prefix used to resolve relative paths.
fn collect_dsv_entries(dsv_path: &Path, prefix: &Path) -> Vec<DsvEntry> {
    let entries = parse_dsv_file(dsv_path);
    let mut result = Vec::new();

    for entry in entries {
        if entry.action == DsvAction::Source {
            let ref_path = &entry.variable;
            let full = if Path::new(ref_path).is_absolute() {
                PathBuf::from(ref_path)
            } else {
                prefix.join(ref_path)
            };

            if ref_path.ends_with(".dsv") {
                // Direct .dsv reference — recurse
                result.extend(collect_dsv_entries(&full, prefix));
            } else {
                // .sh/.bash/.zsh reference — check for a sibling .dsv file.
                // In ament's layout, local_setup.dsv references
                // `environment/ament_prefix_path.sh` but the actual DSV data
                // is in `environment/ament_prefix_path.dsv`.
                let dsv_sibling = full.with_extension("dsv");
                if dsv_sibling.exists() {
                    result.extend(collect_dsv_entries(&dsv_sibling, prefix));
                }
                // If no sibling .dsv exists, skip (shell-only hook)
            }
        } else {
            result.push(entry);
        }
    }

    result
}

// ── Environment builder ─────────────────────────────────────────────────────

/// Build the environment for a package by reading its dependencies' DSV files.
///
/// For each dependency (in the order provided, which should be topological),
/// reads `local_setup.dsv` and `package.dsv`, then applies each entry to
/// the environment HashMap.
///
/// `dep_install_paths` is a list of `(dep_name, install_path)` pairs.
/// `base_env` is the starting environment (e.g. from the overlay chain).
pub fn build_package_env(
    dep_install_paths: &[(String, PathBuf)],
    base_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut env = base_env.clone();

    for (dep_name, install_path) in dep_install_paths {
        // Read local_setup.dsv (ament hooks: AMENT_PREFIX_PATH, PATH,
        // LD_LIBRARY_PATH, PYTHONPATH)
        let local_dsv = install_path
            .join("share")
            .join(dep_name)
            .join("local_setup.dsv");
        let entries = collect_dsv_entries(&local_dsv, install_path);
        for entry in &entries {
            apply_entry(entry, install_path, &mut env);
        }

        // Read package.dsv (colcon hooks: CMAKE_PREFIX_PATH,
        // PKG_CONFIG_PATH, ROS_PACKAGE_PATH)
        let pkg_dsv = install_path
            .join("share")
            .join(dep_name)
            .join("package.dsv");
        let entries = collect_dsv_entries(&pkg_dsv, install_path);
        for entry in &entries {
            apply_entry(entry, install_path, &mut env);
        }
    }

    env
}

/// Apply a single DSV entry to the environment.
fn apply_entry(entry: &DsvEntry, prefix: &Path, env: &mut HashMap<String, String>) {
    let value = resolve_value(&entry.suffix, prefix);

    match entry.action {
        DsvAction::PrependNonDuplicate => {
            prepend_non_duplicate(env, &entry.variable, &value);
        }
        DsvAction::PrependNonDuplicateIfExists => {
            if Path::new(&value).exists() {
                prepend_non_duplicate(env, &entry.variable, &value);
            } else {
                trace!(
                    "skip prepend-non-duplicate-if-exists: {} (path does not exist)",
                    value
                );
            }
        }
        DsvAction::AppendNonDuplicate => {
            append_non_duplicate(env, &entry.variable, &value);
        }
        DsvAction::Set => {
            env.insert(entry.variable.clone(), value);
        }
        DsvAction::SetIfUnset => {
            env.entry(entry.variable.clone()).or_insert(value);
        }
        DsvAction::Source => {
            // Source entries are handled by collect_dsv_entries, not here
        }
    }
}

/// Resolve a DSV suffix to a full path value.
///
/// If the suffix is empty, the prefix itself is the value.
/// If the suffix is a relative path, it's joined to the prefix.
/// If it's already absolute, it's used as-is.
fn resolve_value(suffix: &str, prefix: &Path) -> String {
    if suffix.is_empty() {
        prefix.display().to_string()
    } else if Path::new(suffix).is_absolute() {
        suffix.to_owned()
    } else {
        prefix.join(suffix).display().to_string()
    }
}

/// Prepend a value to a colon-separated variable, skipping duplicates.
fn prepend_non_duplicate(env: &mut HashMap<String, String>, var: &str, value: &str) {
    let existing = env.get(var).map(String::as_str).unwrap_or("");
    if existing.is_empty() {
        env.insert(var.to_owned(), value.to_owned());
    } else if existing.split(':').any(|v| v == value) {
        // Already present — skip
    } else {
        env.insert(var.to_owned(), format!("{value}:{existing}"));
    }
}

/// Append a value to a colon-separated variable, skipping duplicates.
fn append_non_duplicate(env: &mut HashMap<String, String>, var: &str, value: &str) {
    let existing = env.get(var).map(String::as_str).unwrap_or("");
    if existing.is_empty() {
        env.insert(var.to_owned(), value.to_owned());
    } else if existing.split(':').any(|v| v == value) {
        // Already present — skip
    } else {
        env.insert(var.to_owned(), format!("{existing}:{value}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── parse_dsv_content ───────────────────────────────────────────────

    #[test]
    fn parse_basic_entries() {
        let entries = parse_dsv_content(
            "prepend-non-duplicate;AMENT_PREFIX_PATH;\n\
             prepend-non-duplicate;LD_LIBRARY_PATH;lib\n\
             prepend-non-duplicate-if-exists;PATH;bin\n",
        );
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].action, DsvAction::PrependNonDuplicate);
        assert_eq!(entries[0].variable, "AMENT_PREFIX_PATH");
        assert_eq!(entries[0].suffix, "");
        assert_eq!(entries[1].suffix, "lib");
        assert_eq!(entries[2].action, DsvAction::PrependNonDuplicateIfExists);
        assert_eq!(entries[2].suffix, "bin");
    }

    #[test]
    fn parse_source_entries() {
        let entries = parse_dsv_content(
            "source;share/my_pkg/environment/ament_prefix_path.sh\n\
             source;share/my_pkg/environment/path.sh\n",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, DsvAction::Source);
        assert_eq!(
            entries[0].variable,
            "share/my_pkg/environment/ament_prefix_path.sh"
        );
    }

    #[test]
    fn parse_skips_comments_and_empty_lines() {
        let entries = parse_dsv_content(
            "# this is a comment\n\
             \n\
             prepend-non-duplicate;FOO;bar\n\
             \n",
        );
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].variable, "FOO");
    }

    #[test]
    fn parse_set_and_set_if_unset() {
        let entries = parse_dsv_content(
            "set;MY_VAR;my_value\n\
             set-if-unset;OTHER_VAR;default\n",
        );
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].action, DsvAction::Set);
        assert_eq!(entries[0].variable, "MY_VAR");
        assert_eq!(entries[0].suffix, "my_value");
        assert_eq!(entries[1].action, DsvAction::SetIfUnset);
    }

    #[test]
    fn parse_unknown_action_skipped() {
        let entries = parse_dsv_content("unknown-action;FOO;bar\n");
        assert!(entries.is_empty());
    }

    // ── collect_dsv_entries (recursive) ─────────────────────────────────

    #[test]
    fn collect_follows_dsv_sources_recursively() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();

        // Create ament-style layout: local_setup.dsv references .sh files,
        // but sibling .dsv files contain the actual env directives
        let env_dir = prefix.join("share/my_pkg/environment");
        std::fs::create_dir_all(&env_dir).unwrap();
        std::fs::write(
            env_dir.join("ament_prefix_path.dsv"),
            "prepend-non-duplicate;AMENT_PREFIX_PATH;",
        )
        .unwrap();
        std::fs::write(env_dir.join("ament_prefix_path.sh"), "# shell hook").unwrap();
        std::fs::write(
            env_dir.join("library_path.dsv"),
            "prepend-non-duplicate;LD_LIBRARY_PATH;lib",
        )
        .unwrap();
        std::fs::write(env_dir.join("library_path.sh"), "# shell hook").unwrap();

        // local_setup.dsv references .sh files (ament convention)
        // Our parser finds sibling .dsv files automatically
        let local_dsv = prefix.join("share/my_pkg/local_setup.dsv");
        std::fs::write(
            &local_dsv,
            "source;share/my_pkg/environment/ament_prefix_path.sh\n\
             source;share/my_pkg/environment/library_path.sh\n",
        )
        .unwrap();

        let entries = collect_dsv_entries(&local_dsv, prefix);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].variable, "AMENT_PREFIX_PATH");
        assert_eq!(entries[1].variable, "LD_LIBRARY_PATH");
    }

    #[test]
    fn collect_skips_shell_sources() {
        let tmp = TempDir::new().unwrap();
        let prefix = tmp.path();

        let dsv = prefix.join("test.dsv");
        std::fs::write(
            &dsv,
            "source;share/pkg/hook/cmake_prefix_path.sh\n\
             prepend-non-duplicate;FOO;bar\n",
        )
        .unwrap();

        let entries = collect_dsv_entries(&dsv, prefix);
        // Only the non-source entry should remain
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].variable, "FOO");
    }

    // ── prepend/append helpers ──────────────────────────────────────────

    #[test]
    fn prepend_to_empty() {
        let mut env = HashMap::new();
        prepend_non_duplicate(&mut env, "PATH", "/usr/bin");
        assert_eq!(env["PATH"], "/usr/bin");
    }

    #[test]
    fn prepend_non_duplicate_adds_to_front() {
        let mut env = HashMap::from([("PATH".into(), "/usr/bin".into())]);
        prepend_non_duplicate(&mut env, "PATH", "/opt/ros/bin");
        assert_eq!(env["PATH"], "/opt/ros/bin:/usr/bin");
    }

    #[test]
    fn prepend_non_duplicate_skips_existing() {
        let mut env = HashMap::from([("PATH".into(), "/opt/ros/bin:/usr/bin".into())]);
        prepend_non_duplicate(&mut env, "PATH", "/opt/ros/bin");
        // Should not change
        assert_eq!(env["PATH"], "/opt/ros/bin:/usr/bin");
    }

    #[test]
    fn append_non_duplicate_adds_to_back() {
        let mut env = HashMap::from([("PATH".into(), "/usr/bin".into())]);
        append_non_duplicate(&mut env, "PATH", "/opt/ros/bin");
        assert_eq!(env["PATH"], "/usr/bin:/opt/ros/bin");
    }

    #[test]
    fn append_non_duplicate_skips_existing() {
        let mut env = HashMap::from([("PATH".into(), "/usr/bin:/opt/ros/bin".into())]);
        append_non_duplicate(&mut env, "PATH", "/opt/ros/bin");
        assert_eq!(env["PATH"], "/usr/bin:/opt/ros/bin");
    }

    // ── apply_entry ─────────────────────────────────────────────────────

    #[test]
    fn apply_set_overrides() {
        let mut env = HashMap::from([("MY_VAR".into(), "old".into())]);
        let entry = DsvEntry {
            action: DsvAction::Set,
            variable: "MY_VAR".into(),
            suffix: "new_value".into(),
        };
        apply_entry(&entry, Path::new("/prefix"), &mut env);
        // Set with a non-empty suffix that's not an existing path →
        // resolve_value joins prefix + suffix
        assert_eq!(env["MY_VAR"], "/prefix/new_value");
    }

    #[test]
    fn apply_set_if_unset_does_not_override() {
        let mut env = HashMap::from([("MY_VAR".into(), "existing".into())]);
        let entry = DsvEntry {
            action: DsvAction::SetIfUnset,
            variable: "MY_VAR".into(),
            suffix: "default".into(),
        };
        apply_entry(&entry, Path::new("/prefix"), &mut env);
        assert_eq!(env["MY_VAR"], "existing");
    }

    #[test]
    fn apply_set_if_unset_sets_when_missing() {
        let mut env = HashMap::new();
        let entry = DsvEntry {
            action: DsvAction::SetIfUnset,
            variable: "MY_VAR".into(),
            suffix: "default".into(),
        };
        apply_entry(&entry, Path::new("/prefix"), &mut env);
        assert_eq!(env["MY_VAR"], "/prefix/default");
    }

    #[test]
    fn apply_prepend_if_exists_skips_missing_path() {
        let mut env = HashMap::new();
        let entry = DsvEntry {
            action: DsvAction::PrependNonDuplicateIfExists,
            variable: "PATH".into(),
            suffix: "bin".into(),
        };
        // /nonexistent/bin doesn't exist
        apply_entry(&entry, Path::new("/nonexistent"), &mut env);
        assert!(!env.contains_key("PATH"));
    }

    #[test]
    fn apply_prepend_if_exists_adds_existing_path() {
        let tmp = TempDir::new().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();

        let mut env = HashMap::new();
        let entry = DsvEntry {
            action: DsvAction::PrependNonDuplicateIfExists,
            variable: "PATH".into(),
            suffix: "bin".into(),
        };
        apply_entry(&entry, tmp.path(), &mut env);
        assert_eq!(env["PATH"], bin.display().to_string());
    }

    // ── build_package_env (integration) ─────────────────────────────────

    #[test]
    fn build_env_from_ament_layout() {
        let tmp = TempDir::new().unwrap();
        let pkg_install = tmp.path().join("pkg_a");
        std::fs::create_dir_all(&pkg_install).unwrap();

        // Create ament-style DSV files
        let share = pkg_install.join("share/pkg_a");
        let env_dir = share.join("environment");
        std::fs::create_dir_all(&env_dir).unwrap();

        std::fs::write(
            env_dir.join("ament_prefix_path.dsv"),
            "prepend-non-duplicate;AMENT_PREFIX_PATH;",
        )
        .unwrap();
        std::fs::write(
            env_dir.join("library_path.dsv"),
            "prepend-non-duplicate;LD_LIBRARY_PATH;lib",
        )
        .unwrap();

        // local_setup.dsv references the environment DSVs
        std::fs::write(
            share.join("local_setup.dsv"),
            "source;share/pkg_a/environment/ament_prefix_path.dsv\n\
             source;share/pkg_a/environment/library_path.dsv\n",
        )
        .unwrap();

        // package.dsv references hook DSVs
        let hook_dir = share.join("hook");
        std::fs::create_dir_all(&hook_dir).unwrap();
        std::fs::write(
            hook_dir.join("cmake_prefix_path.dsv"),
            "prepend-non-duplicate;CMAKE_PREFIX_PATH;",
        )
        .unwrap();
        std::fs::write(
            share.join("package.dsv"),
            "source;share/pkg_a/hook/cmake_prefix_path.dsv\n",
        )
        .unwrap();

        let deps = vec![("pkg_a".into(), pkg_install.clone())];
        let base_env = HashMap::new();
        let env = build_package_env(&deps, &base_env);

        let pkg_path = pkg_install.display().to_string();
        assert_eq!(env["AMENT_PREFIX_PATH"], pkg_path);
        assert_eq!(env["LD_LIBRARY_PATH"], format!("{pkg_path}/lib"));
        assert_eq!(env["CMAKE_PREFIX_PATH"], pkg_path);
    }

    #[test]
    fn build_env_multiple_deps_prepend_order() {
        let tmp = TempDir::new().unwrap();

        // Create two packages: pkg_a and pkg_b
        for pkg in ["pkg_a", "pkg_b"] {
            let install = tmp.path().join(pkg);
            let share = install.join("share").join(pkg);
            let env_dir = share.join("environment");
            std::fs::create_dir_all(&env_dir).unwrap();

            std::fs::write(
                env_dir.join("ament_prefix_path.dsv"),
                "prepend-non-duplicate;AMENT_PREFIX_PATH;",
            )
            .unwrap();
            std::fs::write(
                share.join("local_setup.dsv"),
                format!("source;share/{pkg}/environment/ament_prefix_path.dsv\n"),
            )
            .unwrap();
            // Empty package.dsv
            std::fs::write(share.join("package.dsv"), "").unwrap();
        }

        let deps = vec![
            ("pkg_a".into(), tmp.path().join("pkg_a")),
            ("pkg_b".into(), tmp.path().join("pkg_b")),
        ];
        let base_env = HashMap::from([("AMENT_PREFIX_PATH".into(), "/opt/ros/humble".into())]);
        let env = build_package_env(&deps, &base_env);

        // pkg_b prepended after pkg_a → pkg_b is at front
        let ament = &env["AMENT_PREFIX_PATH"];
        let parts: Vec<&str> = ament.split(':').collect();
        assert_eq!(parts.len(), 3);
        assert!(parts[0].ends_with("pkg_b"));
        assert!(parts[1].ends_with("pkg_a"));
        assert_eq!(parts[2], "/opt/ros/humble");
    }

    #[test]
    fn build_env_colcon_hook_dsv_format() {
        // Test with the exact format that our post_install.rs generates
        let tmp = TempDir::new().unwrap();
        let install = tmp.path().join("my_pkg");
        let share = install.join("share/my_pkg");
        let hook = share.join("hook");
        std::fs::create_dir_all(&hook).unwrap();

        // Hook DSVs (as generated by post_install.rs)
        std::fs::write(
            hook.join("cmake_prefix_path.dsv"),
            "prepend-non-duplicate;CMAKE_PREFIX_PATH;",
        )
        .unwrap();
        std::fs::write(
            hook.join("pkg_config_path.dsv"),
            "prepend-non-duplicate;PKG_CONFIG_PATH;lib/pkgconfig",
        )
        .unwrap();
        std::fs::write(
            hook.join("ros_package_path.dsv"),
            "prepend-non-duplicate;ROS_PACKAGE_PATH;share",
        )
        .unwrap();

        // package.dsv referencing hooks (as generated by post_install.rs)
        std::fs::write(
            share.join("package.dsv"),
            "source;share/my_pkg/hook/cmake_prefix_path.dsv\n\
             source;share/my_pkg/hook/cmake_prefix_path.sh\n\
             source;share/my_pkg/hook/ros_package_path.dsv\n\
             source;share/my_pkg/hook/ros_package_path.sh\n\
             source;share/my_pkg/hook/pkg_config_path.dsv\n\
             source;share/my_pkg/hook/pkg_config_path.sh\n",
        )
        .unwrap();

        // Empty local_setup.dsv
        std::fs::write(share.join("local_setup.dsv"), "").unwrap();

        let deps = vec![("my_pkg".into(), install.clone())];
        let env = build_package_env(&deps, &HashMap::new());

        let pkg_path = install.display().to_string();
        assert_eq!(env["CMAKE_PREFIX_PATH"], pkg_path);
        assert_eq!(env["PKG_CONFIG_PATH"], format!("{pkg_path}/lib/pkgconfig"));
        assert_eq!(env["ROS_PACKAGE_PATH"], format!("{pkg_path}/share"));
    }

    // ── Real-world test with /opt/ros/humble ────────────────────────────

    /// Integration test: build env from real /opt/ros/humble DSV files.
    /// Requires a ROS 2 Humble installation.
    #[test]
    #[ignore]
    fn build_env_from_real_ros_humble() {
        let humble = Path::new("/opt/ros/humble");
        if !humble.exists() {
            eprintln!("skipping: /opt/ros/humble not found");
            return;
        }

        // In /opt/ros/humble (ament merged install), the install_path
        // for all packages is the same: /opt/ros/humble
        let deps = vec![("rclcpp".into(), humble.to_owned())];
        let env = build_package_env(&deps, &HashMap::new());

        // rclcpp's local_setup.dsv references environment/*.sh, and our
        // parser finds sibling .dsv files: AMENT_PREFIX_PATH, LD_LIBRARY_PATH, PATH
        assert_eq!(env.get("AMENT_PREFIX_PATH").unwrap(), "/opt/ros/humble");
        assert_eq!(env.get("LD_LIBRARY_PATH").unwrap(), "/opt/ros/humble/lib");
        // PATH uses prepend-non-duplicate-if-exists — /opt/ros/humble/bin exists
        assert!(env.get("PATH").unwrap().contains("/opt/ros/humble/bin"));
    }
}
