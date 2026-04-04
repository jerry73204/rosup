//! Top-level install directory scripts.
//!
//! After all packages are built, these scripts are generated once for the
//! entire install prefix. They allow users to activate the workspace with
//! `source install/setup.bash`.
//!
//! Files generated:
//! - `setup.{bash,sh,zsh}` — prefix chain (sources underlays then local_setup)
//! - `local_setup.{bash,sh,zsh}` — calls `_local_setup_util_sh.py` to discover
//!   packages and source them in topological order
//! - `_local_setup_util_sh.py` — colcon's Python helper (shipped verbatim)
//! - `.colcon_install_layout` — "isolated"
//! - `COLCON_IGNORE` — empty marker to prevent workspace scanning

use std::path::Path;

use super::post_install::PostInstallError;

fn write_file(path: &Path, content: &str) -> Result<(), PostInstallError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| PostInstallError::Io {
            path: parent.display().to_string(),
            source: e,
        })?;
    }
    std::fs::write(path, content).map_err(|e| PostInstallError::Io {
        path: path.display().to_string(),
        source: e,
    })
}

/// Generate all top-level install scripts.
///
/// `chained_prefixes` are underlay paths to source before the workspace
/// (e.g. `["/opt/ros/humble"]`). These are sourced in order.
pub fn generate_install_scripts(
    install_dir: &Path,
    chained_prefixes: &[String],
) -> Result<(), PostInstallError> {
    let install_path = install_dir.display().to_string();

    // Prefix chain scripts
    write_setup_bash(install_dir, chained_prefixes)?;
    write_setup_sh(install_dir, chained_prefixes, &install_path)?;
    write_setup_zsh(install_dir, chained_prefixes)?;

    // Local setup scripts (call _local_setup_util_sh.py)
    write_local_setup_bash(install_dir)?;
    write_local_setup_sh(install_dir, &install_path)?;
    write_local_setup_zsh(install_dir)?;

    // Python helper
    write_file(
        &install_dir.join("_local_setup_util_sh.py"),
        LOCAL_SETUP_UTIL_PY,
    )?;

    // Markers
    write_file(&install_dir.join(".colcon_install_layout"), "isolated\n")?;
    write_file(&install_dir.join("COLCON_IGNORE"), "")?;

    Ok(())
}

// ── setup.bash ──────────────────────────────────────────────────────────────

fn write_setup_bash(
    install_dir: &Path,
    chained_prefixes: &[String],
) -> Result<(), PostInstallError> {
    let mut chains = String::new();
    for prefix in chained_prefixes {
        chains.push_str(&format!(
            "COLCON_CURRENT_PREFIX=\"{prefix}\"\n\
             _colcon_prefix_chain_bash_source_script \"$COLCON_CURRENT_PREFIX/local_setup.bash\"\n\n"
        ));
    }

    let content = format!(
        "{SETUP_BASH_HEADER}\
         # source chained prefixes\n\
         # setting COLCON_CURRENT_PREFIX avoids determining the prefix in the sourced script\n\
         {chains}\
         # source this prefix\n\
         # setting COLCON_CURRENT_PREFIX avoids determining the prefix in the sourced script\n\
         COLCON_CURRENT_PREFIX=\"$(builtin cd \"`dirname \"${{BASH_SOURCE[0]}}\"`\" > /dev/null && pwd)\"\n\
         _colcon_prefix_chain_bash_source_script \"$COLCON_CURRENT_PREFIX/local_setup.bash\"\n\n\
         unset COLCON_CURRENT_PREFIX\n\
         unset _colcon_prefix_chain_bash_source_script\n"
    );
    write_file(&install_dir.join("setup.bash"), &content)
}

const SETUP_BASH_HEADER: &str = "\
# generated from colcon_bash/shell/template/prefix_chain.bash.em

# This script extends the environment with the environment of other prefix
# paths which were sourced when this file was generated as well as all packages
# contained in this prefix path.

# function to source another script with conditional trace output
# first argument: the path of the script
_colcon_prefix_chain_bash_source_script() {
  if [ -f \"$1\" ]; then
    if [ -n \"$COLCON_TRACE\" ]; then
      echo \"# . \\\"$1\\\"\"
    fi
    . \"$1\"
  else
    echo \"not found: \\\"$1\\\"\" 1>&2
  fi
}

";

// ── setup.sh ────────────────────────────────────────────────────────────────

fn write_setup_sh(
    install_dir: &Path,
    chained_prefixes: &[String],
    install_path: &str,
) -> Result<(), PostInstallError> {
    let mut chains = String::new();
    for prefix in chained_prefixes {
        chains.push_str(&format!(
            "COLCON_CURRENT_PREFIX=\"{prefix}\"\n\
             _colcon_prefix_chain_sh_source_script \"$COLCON_CURRENT_PREFIX/local_setup.sh\"\n\n"
        ));
    }

    let content = SETUP_SH_TEMPLATE
        .replace("@INSTALL_PATH@", install_path)
        .replace("@CHAINS@", &chains);
    write_file(&install_dir.join("setup.sh"), &content)
}

const SETUP_SH_TEMPLATE: &str = r##"# generated from colcon_core/shell/template/prefix_chain.sh.em

# This script extends the environment with the environment of other prefix
# paths which were sourced when this file was generated as well as all packages
# contained in this prefix path.

# since a plain shell script can't determine its own path when being sourced
# either use the provided COLCON_CURRENT_PREFIX
# or fall back to the build time prefix (if it exists)
_colcon_prefix_chain_sh_COLCON_CURRENT_PREFIX=@INSTALL_PATH@
if [ ! -z "$COLCON_CURRENT_PREFIX" ]; then
  _colcon_prefix_chain_sh_COLCON_CURRENT_PREFIX="$COLCON_CURRENT_PREFIX"
elif [ ! -d "$_colcon_prefix_chain_sh_COLCON_CURRENT_PREFIX" ]; then
  echo "The build time path \"$_colcon_prefix_chain_sh_COLCON_CURRENT_PREFIX\" doesn't exist. Either source a script for a different shell or set the environment variable \"COLCON_CURRENT_PREFIX\" explicitly." 1>&2
  unset _colcon_prefix_chain_sh_COLCON_CURRENT_PREFIX
  return 1
fi

# function to source another script with conditional trace output
# first argument: the path of the script
_colcon_prefix_chain_sh_source_script() {
  if [ -f "$1" ]; then
    if [ -n "$COLCON_TRACE" ]; then
      echo "# . \"$1\""
    fi
    . "$1"
  else
    echo "not found: \"$1\"" 1>&2
  fi
}

# source chained prefixes
# setting COLCON_CURRENT_PREFIX avoids relying on the build time prefix of the sourced script
@CHAINS@
# source this prefix
# setting COLCON_CURRENT_PREFIX avoids relying on the build time prefix of the sourced script
COLCON_CURRENT_PREFIX="$_colcon_prefix_chain_sh_COLCON_CURRENT_PREFIX"
_colcon_prefix_chain_sh_source_script "$COLCON_CURRENT_PREFIX/local_setup.sh"

unset _colcon_prefix_chain_sh_COLCON_CURRENT_PREFIX
unset _colcon_prefix_chain_sh_source_script
unset COLCON_CURRENT_PREFIX
"##;

// ── setup.zsh ───────────────────────────────────────────────────────────────

fn write_setup_zsh(
    install_dir: &Path,
    chained_prefixes: &[String],
) -> Result<(), PostInstallError> {
    let mut chains = String::new();
    for prefix in chained_prefixes {
        chains.push_str(&format!(
            "COLCON_CURRENT_PREFIX=\"{prefix}\"\n\
             _colcon_prefix_chain_zsh_source_script \"$COLCON_CURRENT_PREFIX/local_setup.zsh\"\n\n"
        ));
    }

    let content = SETUP_ZSH_TEMPLATE.replace("@CHAINS@", &chains);
    write_file(&install_dir.join("setup.zsh"), &content)
}

const SETUP_ZSH_TEMPLATE: &str = r##"# generated from colcon_zsh/shell/template/prefix_chain.zsh.em

# This script extends the environment with the environment of other prefix
# paths which were sourced when this file was generated as well as all packages
# contained in this prefix path.

# function to source another script with conditional trace output
# first argument: the path of the script
_colcon_prefix_chain_zsh_source_script() {
  if [ -f "$1" ]; then
    if [ -n "$COLCON_TRACE" ]; then
      echo "# . \"$1\""
    fi
    . "$1"
  else
    echo "not found: \"$1\"" 1>&2
  fi
}

# source chained prefixes
# setting COLCON_CURRENT_PREFIX avoids determining the prefix in the sourced script
@CHAINS@
# source this prefix
# setting COLCON_CURRENT_PREFIX avoids determining the prefix in the sourced script
COLCON_CURRENT_PREFIX="$(builtin cd -q "`dirname "${(%):-%N}"`" > /dev/null && pwd)"
_colcon_prefix_chain_zsh_source_script "$COLCON_CURRENT_PREFIX/local_setup.zsh"

unset COLCON_CURRENT_PREFIX
unset _colcon_prefix_chain_zsh_source_script
"##;

// ── local_setup.bash ────────────────────────────────────────────────────────

fn write_local_setup_bash(install_dir: &Path) -> Result<(), PostInstallError> {
    write_file(&install_dir.join("local_setup.bash"), LOCAL_SETUP_BASH)
}

const LOCAL_SETUP_BASH: &str = r##"# generated from colcon_bash/shell/template/prefix.bash.em

# This script extends the environment with all packages contained in this
# prefix path.

# a bash script is able to determine its own path if necessary
if [ -z "$COLCON_CURRENT_PREFIX" ]; then
  _colcon_prefix_bash_COLCON_CURRENT_PREFIX="$(builtin cd "`dirname "${BASH_SOURCE[0]}"`" > /dev/null && pwd)"
else
  _colcon_prefix_bash_COLCON_CURRENT_PREFIX="$COLCON_CURRENT_PREFIX"
fi

# function to prepend a value to a variable
# which uses colons as separators
# duplicates as well as trailing separators are avoided
# first argument: the name of the result variable
# second argument: the value to be prepended
_colcon_prefix_bash_prepend_unique_value() {
  # arguments
  _listname="$1"
  _value="$2"

  # get values from variable
  eval _values=\"\$$_listname\"
  # backup the field separator
  _colcon_prefix_bash_prepend_unique_value_IFS="$IFS"
  IFS=":"
  # start with the new value
  _all_values="$_value"
  _contained_value=""
  # iterate over existing values in the variable
  for _item in $_values; do
    # ignore empty strings
    if [ -z "$_item" ]; then
      continue
    fi
    # ignore duplicates of _value
    if [ "$_item" = "$_value" ]; then
      _contained_value=1
      continue
    fi
    # keep non-duplicate values
    _all_values="$_all_values:$_item"
  done
  unset _item
  if [ -z "$_contained_value" ]; then
    if [ -n "$COLCON_TRACE" ]; then
      if [ "$_all_values" = "$_value" ]; then
        echo "export $_listname=$_value"
      else
        echo "export $_listname=$_value:\$$_listname"
      fi
    fi
  fi
  unset _contained_value
  # restore the field separator
  IFS="$_colcon_prefix_bash_prepend_unique_value_IFS"
  unset _colcon_prefix_bash_prepend_unique_value_IFS
  # export the updated variable
  eval export $_listname=\"$_all_values\"
  unset _all_values
  unset _values

  unset _value
  unset _listname
}

# add this prefix to the COLCON_PREFIX_PATH
_colcon_prefix_bash_prepend_unique_value COLCON_PREFIX_PATH "$_colcon_prefix_bash_COLCON_CURRENT_PREFIX"
unset _colcon_prefix_bash_prepend_unique_value

# check environment variable for custom Python executable
if [ -n "$COLCON_PYTHON_EXECUTABLE" ]; then
  if [ ! -f "$COLCON_PYTHON_EXECUTABLE" ]; then
    echo "error: COLCON_PYTHON_EXECUTABLE '$COLCON_PYTHON_EXECUTABLE' doesn't exist"
    return 1
  fi
  _colcon_python_executable="$COLCON_PYTHON_EXECUTABLE"
else
  # try the Python executable known at configure time
  _colcon_python_executable="/usr/bin/python3"
  # if it doesn't exist try a fall back
  if [ ! -f "$_colcon_python_executable" ]; then
    if ! /usr/bin/env python3 --version > /dev/null 2> /dev/null; then
      echo "error: unable to find python3 executable"
      return 1
    fi
    _colcon_python_executable=`/usr/bin/env python3 -c "import sys; print(sys.executable)"`
  fi
fi

# function to source another script with conditional trace output
# first argument: the path of the script
_colcon_prefix_sh_source_script() {
  if [ -f "$1" ]; then
    if [ -n "$COLCON_TRACE" ]; then
      echo "# . \"$1\""
    fi
    . "$1"
  else
    echo "not found: \"$1\"" 1>&2
  fi
}

# get all commands in topological order
_colcon_ordered_commands="$($_colcon_python_executable "$_colcon_prefix_bash_COLCON_CURRENT_PREFIX/_local_setup_util_sh.py" sh bash)"
unset _colcon_python_executable
if [ -n "$COLCON_TRACE" ]; then
  echo "$(declare -f _colcon_prefix_sh_source_script)"
  echo "# Execute generated script:"
  echo "# <<<"
  echo "${_colcon_ordered_commands}"
  echo "# >>>"
  echo "unset _colcon_prefix_sh_source_script"
fi
eval "${_colcon_ordered_commands}"
unset _colcon_ordered_commands

unset _colcon_prefix_sh_source_script

unset _colcon_prefix_bash_COLCON_CURRENT_PREFIX
"##;

// ── local_setup.sh ──────────────────────────────────────────────────────────

fn write_local_setup_sh(install_dir: &Path, install_path: &str) -> Result<(), PostInstallError> {
    let content = LOCAL_SETUP_SH_TEMPLATE.replace("@INSTALL_PATH@", install_path);
    write_file(&install_dir.join("local_setup.sh"), &content)
}

const LOCAL_SETUP_SH_TEMPLATE: &str = r##"# generated from colcon_core/shell/template/prefix.sh.em

# This script extends the environment with all packages contained in this
# prefix path.

# since a plain shell script can't determine its own path when being sourced
# either use the provided COLCON_CURRENT_PREFIX
# or fall back to the build time prefix (if it exists)
_colcon_prefix_sh_COLCON_CURRENT_PREFIX="@INSTALL_PATH@"
if [ -z "$COLCON_CURRENT_PREFIX" ]; then
  if [ ! -d "$_colcon_prefix_sh_COLCON_CURRENT_PREFIX" ]; then
    echo "The build time path \"$_colcon_prefix_sh_COLCON_CURRENT_PREFIX\" doesn't exist. Either source a script for a different shell or set the environment variable \"COLCON_CURRENT_PREFIX\" explicitly." 1>&2
    unset _colcon_prefix_sh_COLCON_CURRENT_PREFIX
    return 1
  fi
else
  _colcon_prefix_sh_COLCON_CURRENT_PREFIX="$COLCON_CURRENT_PREFIX"
fi

# function to prepend a value to a variable
# which uses colons as separators
# duplicates as well as trailing separators are avoided
# first argument: the name of the result variable
# second argument: the value to be prepended
_colcon_prefix_sh_prepend_unique_value() {
  # arguments
  _listname="$1"
  _value="$2"

  # get values from variable
  eval _values=\"\$$_listname\"
  # backup the field separator
  _colcon_prefix_sh_prepend_unique_value_IFS="$IFS"
  IFS=":"
  # start with the new value
  _all_values="$_value"
  _contained_value=""
  # iterate over existing values in the variable
  for _item in $_values; do
    # ignore empty strings
    if [ -z "$_item" ]; then
      continue
    fi
    # ignore duplicates of _value
    if [ "$_item" = "$_value" ]; then
      _contained_value=1
      continue
    fi
    # keep non-duplicate values
    _all_values="$_all_values:$_item"
  done
  unset _item
  if [ -z "$_contained_value" ]; then
    if [ -n "$COLCON_TRACE" ]; then
      if [ "$_all_values" = "$_value" ]; then
        echo "export $_listname=$_value"
      else
        echo "export $_listname=$_value:\$$_listname"
      fi
    fi
  fi
  unset _contained_value
  # restore the field separator
  IFS="$_colcon_prefix_sh_prepend_unique_value_IFS"
  unset _colcon_prefix_sh_prepend_unique_value_IFS
  # export the updated variable
  eval export $_listname=\"$_all_values\"
  unset _all_values
  unset _values

  unset _value
  unset _listname
}

# add this prefix to the COLCON_PREFIX_PATH
_colcon_prefix_sh_prepend_unique_value COLCON_PREFIX_PATH "$_colcon_prefix_sh_COLCON_CURRENT_PREFIX"
unset _colcon_prefix_sh_prepend_unique_value

# check environment variable for custom Python executable
if [ -n "$COLCON_PYTHON_EXECUTABLE" ]; then
  if [ ! -f "$COLCON_PYTHON_EXECUTABLE" ]; then
    echo "error: COLCON_PYTHON_EXECUTABLE '$COLCON_PYTHON_EXECUTABLE' doesn't exist"
    return 1
  fi
  _colcon_python_executable="$COLCON_PYTHON_EXECUTABLE"
else
  # try the Python executable known at configure time
  _colcon_python_executable="/usr/bin/python3"
  # if it doesn't exist try a fall back
  if [ ! -f "$_colcon_python_executable" ]; then
    if ! /usr/bin/env python3 --version > /dev/null 2> /dev/null; then
      echo "error: unable to find python3 executable"
      return 1
    fi
    _colcon_python_executable=`/usr/bin/env python3 -c "import sys; print(sys.executable)"`
  fi
fi

# function to source another script with conditional trace output
# first argument: the path of the script
_colcon_prefix_sh_source_script() {
  if [ -f "$1" ]; then
    if [ -n "$COLCON_TRACE" ]; then
      echo "# . \"$1\""
    fi
    . "$1"
  else
    echo "not found: \"$1\"" 1>&2
  fi
}

# get all commands in topological order
_colcon_ordered_commands="$($_colcon_python_executable "$_colcon_prefix_sh_COLCON_CURRENT_PREFIX/_local_setup_util_sh.py" sh)"
unset _colcon_python_executable
if [ -n "$COLCON_TRACE" ]; then
  echo "_colcon_prefix_sh_source_script() {
    if [ -f \"\$1\" ]; then
      if [ -n \"\$COLCON_TRACE\" ]; then
        echo \"# . \\\"\$1\\\"\"
      fi
      . \"\$1\"
    else
      echo \"not found: \\\"\$1\\\"\" 1>&2
    fi
  }"
  echo "# Execute generated script:"
  echo "# <<<"
  echo "${_colcon_ordered_commands}"
  echo "# >>>"
  echo "unset _colcon_prefix_sh_source_script"
fi
eval "${_colcon_ordered_commands}"
unset _colcon_ordered_commands

unset _colcon_prefix_sh_source_script

unset _colcon_prefix_sh_COLCON_CURRENT_PREFIX
"##;

// ── local_setup.zsh ─────────────────────────────────────────────────────────

fn write_local_setup_zsh(install_dir: &Path) -> Result<(), PostInstallError> {
    write_file(&install_dir.join("local_setup.zsh"), LOCAL_SETUP_ZSH)
}

const LOCAL_SETUP_ZSH: &str = r##"# generated from colcon_zsh/shell/template/prefix.zsh.em

# This script extends the environment with all packages contained in this
# prefix path.

# a zsh script is able to determine its own path if necessary
if [ -z "$COLCON_CURRENT_PREFIX" ]; then
  _colcon_prefix_zsh_COLCON_CURRENT_PREFIX="$(builtin cd -q "`dirname "${(%):-%N}"`" > /dev/null && pwd)"
else
  _colcon_prefix_zsh_COLCON_CURRENT_PREFIX="$COLCON_CURRENT_PREFIX"
fi

# function to convert array-like strings into arrays
# to workaround SH_WORD_SPLIT not being set
_colcon_prefix_zsh_convert_to_array() {
  local _listname=$1
  local _dollar="$"
  local _split="{="
  local _to_array="(\"$_dollar$_split$_listname}\")"
  eval $_listname=$_to_array
}

# function to prepend a value to a variable
# which uses colons as separators
# duplicates as well as trailing separators are avoided
# first argument: the name of the result variable
# second argument: the value to be prepended
_colcon_prefix_zsh_prepend_unique_value() {
  # arguments
  _listname="$1"
  _value="$2"

  # get values from variable
  eval _values=\"\$$_listname\"
  # backup the field separator
  _colcon_prefix_zsh_prepend_unique_value_IFS="$IFS"
  IFS=":"
  # start with the new value
  _all_values="$_value"
  _contained_value=""
  # workaround SH_WORD_SPLIT not being set
  _colcon_prefix_zsh_convert_to_array _values
  # iterate over existing values in the variable
  for _item in $_values; do
    # ignore empty strings
    if [ -z "$_item" ]; then
      continue
    fi
    # ignore duplicates of _value
    if [ "$_item" = "$_value" ]; then
      _contained_value=1
      continue
    fi
    # keep non-duplicate values
    _all_values="$_all_values:$_item"
  done
  unset _item
  if [ -z "$_contained_value" ]; then
    if [ -n "$COLCON_TRACE" ]; then
      if [ "$_all_values" = "$_value" ]; then
        echo "export $_listname=$_value"
      else
        echo "export $_listname=$_value:\$$_listname"
      fi
    fi
  fi
  unset _contained_value
  # restore the field separator
  IFS="$_colcon_prefix_zsh_prepend_unique_value_IFS"
  unset _colcon_prefix_zsh_prepend_unique_value_IFS
  # export the updated variable
  eval export $_listname=\"$_all_values\"
  unset _all_values
  unset _values

  unset _value
  unset _listname
}

# add this prefix to the COLCON_PREFIX_PATH
_colcon_prefix_zsh_prepend_unique_value COLCON_PREFIX_PATH "$_colcon_prefix_zsh_COLCON_CURRENT_PREFIX"
unset _colcon_prefix_zsh_prepend_unique_value
unset _colcon_prefix_zsh_convert_to_array

# check environment variable for custom Python executable
if [ -n "$COLCON_PYTHON_EXECUTABLE" ]; then
  if [ ! -f "$COLCON_PYTHON_EXECUTABLE" ]; then
    echo "error: COLCON_PYTHON_EXECUTABLE '$COLCON_PYTHON_EXECUTABLE' doesn't exist"
    return 1
  fi
  _colcon_python_executable="$COLCON_PYTHON_EXECUTABLE"
else
  # try the Python executable known at configure time
  _colcon_python_executable="/usr/bin/python3"
  # if it doesn't exist try a fall back
  if [ ! -f "$_colcon_python_executable" ]; then
    if ! /usr/bin/env python3 --version > /dev/null 2> /dev/null; then
      echo "error: unable to find python3 executable"
      return 1
    fi
    _colcon_python_executable=`/usr/bin/env python3 -c "import sys; print(sys.executable)"`
  fi
fi

# function to source another script with conditional trace output
# first argument: the path of the script
_colcon_prefix_sh_source_script() {
  if [ -f "$1" ]; then
    if [ -n "$COLCON_TRACE" ]; then
      echo "# . \"$1\""
    fi
    . "$1"
  else
    echo "not found: \"$1\"" 1>&2
  fi
}

# get all commands in topological order
_colcon_ordered_commands="$($_colcon_python_executable "$_colcon_prefix_zsh_COLCON_CURRENT_PREFIX/_local_setup_util_sh.py" sh zsh)"
unset _colcon_python_executable
if [ -n "$COLCON_TRACE" ]; then
  echo "$(declare -f _colcon_prefix_sh_source_script)"
  echo "# Execute generated script:"
  echo "# <<<"
  echo "${_colcon_ordered_commands}"
  echo "# >>>"
  echo "unset _colcon_prefix_sh_source_script"
fi
eval "${_colcon_ordered_commands}"
unset _colcon_ordered_commands

unset _colcon_prefix_sh_source_script

unset _colcon_prefix_zsh_COLCON_CURRENT_PREFIX
"##;

// ── _local_setup_util_sh.py ─────────────────────────────────────────────────

/// Colcon's Python helper script for dynamic package discovery.
///
/// This is shipped verbatim from colcon_core. It discovers packages from
/// `share/colcon-core/packages/` files, reads their runtime deps, sorts
/// them topologically, and outputs shell commands to source each package's
/// DSV/shell hooks.
const LOCAL_SETUP_UTIL_PY: &str = include_str!("install_scripts_local_setup_util_sh.py");

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn generates_all_top_level_files() {
        let tmp = TempDir::new().unwrap();
        generate_install_scripts(tmp.path(), &["/opt/ros/humble".into()]).unwrap();

        assert!(tmp.path().join("setup.bash").exists());
        assert!(tmp.path().join("setup.sh").exists());
        assert!(tmp.path().join("setup.zsh").exists());
        assert!(tmp.path().join("local_setup.bash").exists());
        assert!(tmp.path().join("local_setup.sh").exists());
        assert!(tmp.path().join("local_setup.zsh").exists());
        assert!(tmp.path().join("_local_setup_util_sh.py").exists());
        assert!(tmp.path().join(".colcon_install_layout").exists());
        assert!(tmp.path().join("COLCON_IGNORE").exists());
    }

    #[test]
    fn colcon_install_layout_is_isolated() {
        let tmp = TempDir::new().unwrap();
        generate_install_scripts(tmp.path(), &[]).unwrap();

        let content = std::fs::read_to_string(tmp.path().join(".colcon_install_layout")).unwrap();
        assert_eq!(content, "isolated\n");
    }

    #[test]
    fn setup_bash_chains_underlays() {
        let tmp = TempDir::new().unwrap();
        generate_install_scripts(
            tmp.path(),
            &["/opt/ros/humble".into(), "/my/overlay".into()],
        )
        .unwrap();

        let content = std::fs::read_to_string(tmp.path().join("setup.bash")).unwrap();
        assert!(content.contains("/opt/ros/humble"));
        assert!(content.contains("/my/overlay"));
        // Own prefix sourced last
        assert!(content.contains("BASH_SOURCE[0]"));
    }

    #[test]
    fn setup_sh_has_build_time_fallback() {
        let tmp = TempDir::new().unwrap();
        let path_str = tmp.path().display().to_string();
        generate_install_scripts(tmp.path(), &[]).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("setup.sh")).unwrap();
        assert!(content.contains(&path_str));
    }

    #[test]
    fn local_setup_bash_calls_python_helper() {
        let tmp = TempDir::new().unwrap();
        generate_install_scripts(tmp.path(), &[]).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("local_setup.bash")).unwrap();
        assert!(content.contains("_local_setup_util_sh.py"));
        assert!(content.contains("COLCON_PREFIX_PATH"));
    }

    #[test]
    fn python_helper_has_expected_content() {
        let tmp = TempDir::new().unwrap();
        generate_install_scripts(tmp.path(), &[]).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("_local_setup_util_sh.py")).unwrap();
        assert!(content.contains("def get_packages"));
        assert!(content.contains("def order_packages"));
        assert!(content.contains("colcon-core/packages"));
    }

    #[test]
    fn no_chained_prefixes() {
        let tmp = TempDir::new().unwrap();
        generate_install_scripts(tmp.path(), &[]).unwrap();

        let content = std::fs::read_to_string(tmp.path().join("setup.bash")).unwrap();
        // Should still have the function and own prefix
        assert!(content.contains("_colcon_prefix_chain_bash_source_script"));
        assert!(content.contains("BASH_SOURCE[0]"));
    }
}
