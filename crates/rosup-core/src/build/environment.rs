//! Environment script generation for installed packages.
//!
//! After a package is installed, colcon generates shell scripts that set
//! up the environment (AMENT_PREFIX_PATH, PATH, LD_LIBRARY_PATH, etc.).
//! These scripts are sourced by downstream packages and by users.
//!
//! We generate two layers:
//! - **DSV files** (declarative setup) — machine-readable, used by colcon
//!   and ament tooling to chain environments.
//! - **Shell scripts** (bash/sh/zsh) — human-readable, sourced by users.
//!
//! The DSV format is the source of truth. Shell scripts reference DSV
//! entries. This matches colcon's architecture.

use std::path::Path;

use super::ament_index::AmentIndexError;

fn write_file(path: &Path, content: &str) -> Result<(), AmentIndexError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AmentIndexError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    std::fs::write(path, content).map_err(|e| AmentIndexError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

/// An environment hook: a variable to set and the DSV action.
struct EnvHook {
    /// DSV action: `prepend-non-duplicate`, `prepend-non-duplicate-if-exists`.
    action: String,
    /// Environment variable name.
    var: String,
    /// Path suffix relative to the install prefix. Empty = the prefix itself.
    suffix: String,
    /// File basename (without extension).
    filename: String,
}

/// Standard environment hooks for a ROS 2 package.
fn standard_hooks() -> Vec<EnvHook> {
    vec![
        EnvHook {
            action: "prepend-non-duplicate".into(),
            var: "AMENT_PREFIX_PATH".into(),
            suffix: String::new(),
            filename: "ament_prefix_path".into(),
        },
        EnvHook {
            action: "prepend-non-duplicate".into(),
            var: "LD_LIBRARY_PATH".into(),
            suffix: "lib".into(),
            filename: "library_path".into(),
        },
        EnvHook {
            action: "prepend-non-duplicate-if-exists".into(),
            var: "PATH".into(),
            suffix: "bin".into(),
            filename: "path".into(),
        },
    ]
}

/// Generate all environment scripts for an installed package.
///
/// Creates:
/// - `share/<name>/environment/*.dsv` and `*.sh` — per-hook scripts
/// - `share/<name>/local_setup.dsv` — lists all hooks
/// - `share/<name>/local_setup.sh` — sources hooks via the DSV list
/// - `share/<name>/local_setup.bash` — bash wrapper
/// - `share/<name>/local_setup.zsh` — zsh wrapper
/// - `share/<name>/package.dsv` — references local_setup files
///
/// `has_python` adds a PYTHONPATH hook for packages that install Python
/// modules.
pub fn generate_environment_scripts(
    install_base: &Path,
    pkg_name: &str,
    python_version: Option<&str>,
) -> Result<(), AmentIndexError> {
    let share = install_base.join("share").join(pkg_name);
    let env_dir = share.join("environment");

    // Collect all hooks (standard + optional python).
    let mut hooks = standard_hooks();
    if let Some(pyver) = python_version {
        hooks.push(EnvHook {
            action: "prepend-non-duplicate".into(),
            var: "PYTHONPATH".into(),
            suffix: format!("local/lib/python{pyver}/dist-packages"),
            filename: "pythonpath".into(),
        });
    }

    // ── Per-hook DSV and shell files ─────────────────────────────────────

    let mut dsv_entries = Vec::new();

    for hook in hooks.iter() {
        // DSV file: `<action>;<VAR>;<suffix>`
        let dsv_content = format!("{};{};{}", hook.action, hook.var, hook.suffix);
        write_file(
            &env_dir.join(format!("{}.dsv", hook.filename)),
            &dsv_content,
        )?;

        // Shell file: uses ament_prepend_unique_value from local_setup.sh
        let sh_content = if hook.suffix.is_empty() {
            format!(
                "ament_prepend_unique_value {} \"$AMENT_CURRENT_PREFIX\"\n",
                hook.var
            )
        } else if hook.action.contains("if-exists") {
            format!(
                "if [ -d \"$AMENT_CURRENT_PREFIX/{}\" ]; then\n  \
                 ament_prepend_unique_value {} \"$AMENT_CURRENT_PREFIX/{}\"\nfi\n",
                hook.suffix, hook.var, hook.suffix
            )
        } else {
            format!(
                "ament_prepend_unique_value {} \"$AMENT_CURRENT_PREFIX/{}\"\n",
                hook.var, hook.suffix
            )
        };
        write_file(&env_dir.join(format!("{}.sh", hook.filename)), &sh_content)?;

        dsv_entries.push(format!(
            "source;share/{pkg_name}/environment/{}.sh",
            hook.filename
        ));
    }

    // ── local_setup.dsv ──────────────────────────────────────────────────

    write_file(&share.join("local_setup.dsv"), &dsv_entries.join("\n"))?;

    // ── local_setup.sh ───────────────────────────────────────────────────
    //
    // This is the big one. It defines the `ament_prepend_unique_value`
    // function and sources all hooks listed in local_setup.dsv.
    // We generate a minimal version that's compatible with colcon's.

    let hook_sources: String = dsv_entries
        .iter()
        .map(|entry| {
            // entry is "source;share/<pkg>/environment/<name>.sh"
            let sh_path = entry.strip_prefix("source;").unwrap_or(entry);
            format!(
                "ament_append_value AMENT_ENVIRONMENT_HOOKS \
                 \"$AMENT_CURRENT_PREFIX/{sh_path}\""
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let local_setup_sh = format!(
        r#": ${{AMENT_CURRENT_PREFIX:="{install_base}"}}

# append value to variable using colon separator
ament_append_value() {{
  eval _values=\"\$$1\"
  if [ -z "$_values" ]; then
    eval export $1=\"$2\"
  else
    eval export $1=\"\$$1:$2\"
  fi
  unset _values
}}

# prepend unique value to variable
ament_prepend_unique_value() {{
  eval _values=\"\$$1\"
  _duplicate=
  _old_IFS=$IFS; IFS=":"
  for _item in $_values; do
    [ -z "$_item" ] && continue
    [ "$_item" = "$2" ] && _duplicate=1
  done
  IFS=$_old_IFS
  if [ -z "$_duplicate" ]; then
    if [ -z "$_values" ]; then
      eval export $1=\"$2\"
    else
      eval export $1=\"$2:\$$1\"
    fi
  fi
  unset _duplicate _values _old_IFS
}}

unset AMENT_ENVIRONMENT_HOOKS
{hook_sources}

_old_IFS=$IFS; IFS=":"
for _hook in $AMENT_ENVIRONMENT_HOOKS; do
  [ -f "$_hook" ] && . "$_hook"
done
IFS=$_old_IFS
unset _hook _old_IFS AMENT_ENVIRONMENT_HOOKS AMENT_CURRENT_PREFIX
"#,
        install_base = install_base.display()
    );
    write_file(&share.join("local_setup.sh"), &local_setup_sh)?;

    // ── local_setup.bash ─────────────────────────────────────────────────

    let local_setup_bash = "\
_this_path=$(builtin cd \"$(dirname \"${BASH_SOURCE[0]}\")\" && pwd)\n\
AMENT_CURRENT_PREFIX=$(builtin cd \"$(dirname \"${BASH_SOURCE[0]}\")/..\" && pwd)\n\
. \"$_this_path/local_setup.sh\"\n\
unset _this_path\n";
    write_file(&share.join("local_setup.bash"), local_setup_bash)?;

    // ── local_setup.zsh ──────────────────────────────────────────────────

    let local_setup_zsh = "\
AMENT_CURRENT_PREFIX=$(builtin cd \"$(dirname \"${(%):-%N}\")/..\" && pwd)\n\
. \"$(dirname \"${(%):-%N}\")/local_setup.sh\"\n";
    write_file(&share.join("local_setup.zsh"), local_setup_zsh)?;

    // ── package.dsv ──────────────────────────────────────────────────────

    let package_dsv = format!(
        "source;share/{pkg_name}/local_setup.bash\n\
         source;share/{pkg_name}/local_setup.dsv\n\
         source;share/{pkg_name}/local_setup.sh\n\
         source;share/{pkg_name}/local_setup.zsh"
    );
    write_file(&share.join("package.dsv"), &package_dsv)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generates_standard_env_hooks() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", None).unwrap();

        // Check DSV files exist
        let env = tmp.path().join("share/my_pkg/environment");
        assert!(env.join("ament_prefix_path.dsv").exists());
        assert!(env.join("library_path.dsv").exists());
        assert!(env.join("path.dsv").exists());

        // Check shell files exist
        assert!(env.join("ament_prefix_path.sh").exists());
        assert!(env.join("library_path.sh").exists());
        assert!(env.join("path.sh").exists());

        // No pythonpath without python_version
        assert!(!env.join("pythonpath.dsv").exists());
    }

    #[test]
    fn generates_python_hook() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", Some("3.10")).unwrap();

        let env = tmp.path().join("share/my_pkg/environment");
        assert!(env.join("pythonpath.dsv").exists());

        let dsv = std::fs::read_to_string(env.join("pythonpath.dsv")).unwrap();
        assert!(dsv.contains("PYTHONPATH"));
        assert!(dsv.contains("python3.10"));

        let sh = std::fs::read_to_string(env.join("pythonpath.sh")).unwrap();
        assert!(sh.contains("PYTHONPATH"));
        assert!(sh.contains("python3.10"));
    }

    #[test]
    fn generates_local_setup_scripts() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", None).unwrap();

        let share = tmp.path().join("share/my_pkg");
        assert!(share.join("local_setup.sh").exists());
        assert!(share.join("local_setup.bash").exists());
        assert!(share.join("local_setup.zsh").exists());
        assert!(share.join("local_setup.dsv").exists());
        assert!(share.join("package.dsv").exists());
    }

    #[test]
    fn local_setup_dsv_lists_hooks() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", None).unwrap();

        let dsv = std::fs::read_to_string(tmp.path().join("share/my_pkg/local_setup.dsv")).unwrap();
        assert!(dsv.contains("ament_prefix_path.sh"));
        assert!(dsv.contains("library_path.sh"));
        assert!(dsv.contains("path.sh"));
    }

    #[test]
    fn local_setup_sh_defines_helper_functions() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", None).unwrap();

        let sh = std::fs::read_to_string(tmp.path().join("share/my_pkg/local_setup.sh")).unwrap();
        assert!(sh.contains("ament_prepend_unique_value"));
        assert!(sh.contains("ament_append_value"));
        assert!(sh.contains("AMENT_ENVIRONMENT_HOOKS"));
    }

    #[test]
    fn package_dsv_references_local_setup() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", None).unwrap();

        let dsv = std::fs::read_to_string(tmp.path().join("share/my_pkg/package.dsv")).unwrap();
        assert!(dsv.contains("local_setup.bash"));
        assert!(dsv.contains("local_setup.sh"));
        assert!(dsv.contains("local_setup.zsh"));
    }

    #[test]
    fn ament_prefix_dsv_format() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", None).unwrap();

        let dsv = std::fs::read_to_string(
            tmp.path()
                .join("share/my_pkg/environment/ament_prefix_path.dsv"),
        )
        .unwrap();
        assert_eq!(dsv, "prepend-non-duplicate;AMENT_PREFIX_PATH;");
    }

    #[test]
    fn library_path_dsv_format() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", None).unwrap();

        let dsv =
            std::fs::read_to_string(tmp.path().join("share/my_pkg/environment/library_path.dsv"))
                .unwrap();
        assert_eq!(dsv, "prepend-non-duplicate;LD_LIBRARY_PATH;lib");
    }

    #[test]
    fn path_dsv_format() {
        let tmp = TempDir::new().unwrap();
        generate_environment_scripts(tmp.path(), "my_pkg", None).unwrap();

        let dsv =
            std::fs::read_to_string(tmp.path().join("share/my_pkg/environment/path.dsv")).unwrap();
        assert_eq!(dsv, "prepend-non-duplicate-if-exists;PATH;bin");
    }
}
