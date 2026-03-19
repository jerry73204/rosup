# Feature: Package and Node Listing (`rosup list`)

## Problem

There is no way to inspect the contents of a rosup workspace without building
it first. Developers need to discover:

- Which packages are in the workspace
- Which executable nodes each package provides
- Which launch files are available

Colcon itself offers no pre-build introspection equivalent to `cargo build
--bin`. rosup can implement this by parsing `package.xml`, `CMakeLists.txt`,
`setup.py`, and launch file paths statically, without invoking the build
system.

## Proposed Command

```
rosup list [subcommand] [options]
```

### Subcommands

| Subcommand        | Description                                      |
|-------------------|--------------------------------------------------|
| `rosup list pkgs` | List packages in the workspace                   |
| `rosup list nodes`| List executable nodes (static analysis)          |
| `rosup list launches` | List launch files                            |

All subcommands default to listing everything in the workspace. Use filters to
narrow results.

### Common Options

| Option              | Description                                       |
|---------------------|---------------------------------------------------|
| `-p, --package <name>` | Filter to a specific package                  |
| `--json`            | Emit JSON output for scripting                    |
| `-q, --quiet`       | Print only names, one per line                    |

## `rosup list pkgs`

Lists all workspace members with basic metadata from `package.xml`.

```
$ rosup list pkgs
NAME                            VERSION   BUILD TYPE
autoware_core                   1.5.0     ament_cmake
autoware_msgs                   1.5.0     ament_cmake
autoware_rviz_plugins           1.5.0     ament_cmake
...  (453 total)
```

Fields shown: `name`, `version`, `build_type` (from `<export><build_type>`).

## `rosup list nodes`

Discovers executable targets by statically parsing build system files. Does
**not** require the workspace to be built.

### ament_cmake packages

Parse `CMakeLists.txt` for:

```cmake
add_executable(<name> ...)
```

Example output:

```
$ rosup list nodes
PACKAGE                         NODE
autoware_core                   core_node
autoware_rviz_plugins           rviz_panel_node
...

$ rosup list nodes -p autoware_core
PACKAGE                         NODE
autoware_core                   core_node
autoware_core                   core_diagnostics
```

### ament_python packages

Parse `setup.py` (or `setup.cfg`) for `entry_points['console_scripts']`:

```python
entry_points={
    'console_scripts': [
        'talker = my_py_pkg.talker:main',
    ],
}
```

### Limitations

- Static parsing cannot capture dynamically generated targets
  (`foreach`, generator expressions, etc.).
- Only top-level `add_executable` calls are detected; CMake functions that wrap
  it internally will be missed.
- Results are labelled `(static)` in verbose output to indicate they are
  derived without building.

## `rosup list launches`

Finds launch files by scanning known locations without parsing them.

Searched paths within each package directory:

- `launch/*.launch.py`
- `launch/*.launch.xml`
- `launch/*.launch.yaml`

Example output:

```
$ rosup list launches
PACKAGE                         LAUNCH FILE
my_robot_bringup                robot.launch.py
my_robot_bringup                sensors.launch.xml
autoware_core                   planning.launch.py
...

$ rosup list launches -p my_robot_bringup
PACKAGE                         LAUNCH FILE
my_robot_bringup                robot.launch.py
my_robot_bringup                sensors.launch.xml
```

## JSON Output

All subcommands support `--json` for scripting:

```json
[
  {
    "package": "autoware_core",
    "nodes": ["core_node", "core_diagnostics"],
    "launches": ["planning.launch.py"]
  }
]
```

## Implementation Notes

- Implemented as a `Command::List { subcommand, package, json, quiet }` variant
  in `rosup-cli`.
- CMakeLists.txt parsing: a simple line regex `add_executable\((\w+)` is
  sufficient for the initial implementation. A proper CMake AST parser is out
  of scope.
- setup.py parsing: use a regex for `'(\w+)\s*=\s*[\w.]+:main'` in the
  `console_scripts` block. If `setup.cfg` is present, parse its
  `[options.entry_points]` section.
- No dependency on building or sourcing the workspace.
