# CLI Reference

## Global Behavior

rosup searches for `rosup.toml` starting from the current directory, walking up
to parent directories. The directory containing `rosup.toml` is the **project
root**. All commands operate relative to the project root.

If no `rosup.toml` is found, most commands will error with a hint to run
`rosup init`.

All commands that need dependencies (build, test, run, launch) resolve them
automatically before proceeding. They also require the base ROS environment to
be sourced (`source /opt/ros/{distro}/setup.bash`) and will warn if it is not.

---

## Commands

### `rosup init`

Initialize a new `rosup.toml` for an existing package or workspace.

```
rosup init [--workspace] [--lock] [--force]
```

| Option        | Description                                                    |
|---------------|----------------------------------------------------------------|
| `--workspace` | Create a workspace manifest instead of a package one           |
| `--lock`      | Write an explicit locked member list (only with `--workspace`) |
| `--force`     | Overwrite an existing `rosup.toml`                             |

**Behavior:**

- Without `--workspace`: looks for `package.xml` in the current directory.
  Generates a single-package `rosup.toml` with `[package]` populated from
  `package.xml`.
- With `--workspace` (no `--lock`): writes an empty `[workspace]` section,
  enabling Colcon auto-discovery. No `members` key is written.
- With `--workspace --lock`: runs Colcon auto-discovery and writes the full
  `members` list. The list is a snapshot; use `rosup sync` to update it later.
- Adds `.rosup/` to `.gitignore`.

**Examples:**

```bash
# Inside a single package
cd ~/nav2_core && rosup init

# Workspace with auto-discovery (recommended for large workspaces)
cd ~/my_ws && rosup init --workspace

# Workspace with explicit pinned member list
cd ~/my_ws && rosup init --workspace --lock
```

---

### `rosup sync`

Synchronise the `members` list in `rosup.toml` with the current workspace
directory structure.

```
rosup sync [--dry-run] [--check] [--lock]
```

| Option      | Description                                                         |
|-------------|---------------------------------------------------------------------|
| `--dry-run` | Print what would change; do not write                               |
| `--check`   | Exit non-zero if the member list is out of date (CI mode)           |
| `--lock`    | Convert an auto-discovery workspace to an explicit member list      |

**Behavior:**

- Requires a workspace project; errors if `rosup.toml` has `[package]`.
- Runs Colcon auto-discovery to compute the current ground-truth member list.
- Compares against the `members` key (if any) in `rosup.toml`.
- Updates the `members` block in place, preserving all other `rosup.toml`
  content including comments.
- If `members` is absent (auto-discovery mode) and `--lock` is not set,
  prints an informational message and exits without modifying the file.

**Examples:**

```bash
# Update member list after adding/removing packages
rosup sync

# Preview changes without writing
rosup sync --dry-run

# CI: fail if member list is stale
rosup sync --check

# Convert auto-discovery to explicit list
rosup sync --lock
```

---

### `rosup clone`

Clone a package from the ROS index and set it up for development.

```
rosup clone <package_name> [--distro <distro>]
```

1. Looks up `<package_name>` in the rosdistro distribution cache.
2. Clones the source repository to the current directory.
3. Generates `rosup.toml` via `rosup init`.

**Examples:**

```bash
rosup clone nav2_core
rosup clone nav2_core --distro jazzy
```

---

### `rosup build`

Build packages. Resolves dependencies first, builds the dep layer if needed,
then delegates to `colcon build`.

```
rosup build [options]
```

| Option            | Description                                          |
|-------------------|------------------------------------------------------|
| `-p, --packages`  | Build only the specified packages                    |
| `--deps`          | Also build dependencies of selected packages         |
| `--cmake-args`    | Additional CMake arguments (appended to rosup.toml)    |
| `--no-resolve`    | Skip dependency resolution and dep layer build       |
| `--rebuild-deps`  | Force rebuild of the dep layer from source           |
| `--release`       | Shorthand for `-DCMAKE_BUILD_TYPE=Release`           |
| `--debug`         | Shorthand for `-DCMAKE_BUILD_TYPE=Debug`             |

**Build order:**

1. Warn if base ROS environment (`AMENT_PREFIX_PATH`) is not set.
2. Resolve dependencies (ament check → rosdep install → source clone).
3. Build dep layer: colcon builds source-pulled packages into `.rosup/install/`.
4. Build user workspace: colcon builds project packages with `.rosup/install/`
   on `AMENT_PREFIX_PATH`.

**Examples:**

```bash
rosup build
rosup build -p my_robot_nav
rosup build -p my_robot_nav --deps
rosup build --release
rosup build --rebuild-deps
```

---

### `rosup test`

Run tests. Resolves dependencies first, then delegates to `colcon test`.

```
rosup test [options]
```

| Option                | Description                                |
|-----------------------|--------------------------------------------|
| `-p, --packages`      | Test only the specified packages           |
| `--retest-until-pass` | Retry failed tests up to N times           |
| `--no-resolve`        | Skip dependency resolution                 |

**Examples:**

```bash
rosup test
rosup test -p my_robot_nav
rosup test --retest-until-pass 3
```

---

### `rosup add`

Add a dependency to a package's `package.xml`.

```
rosup add <dep_name> [options]
```

| Option          | Description                                              |
|-----------------|----------------------------------------------------------|
| `-p, --package` | Target package (workspace mode; defaults to current dir) |
| `--build`       | Add as `<build_depend>` only                             |
| `--exec`        | Add as `<exec_depend>` only                              |
| `--test`        | Add as `<test_depend>` only                              |
| `--dev`         | Add as `<build_depend>` + `<test_depend>`                |

Without a type flag, adds as `<depend>` (build + build_export + exec).

**Examples:**

```bash
rosup add nav2_core
rosup add nav2_core --exec
rosup add ament_cmake_gtest --test
rosup add sensor_msgs -p my_robot_nav
```

---

### `rosup remove`

Remove a dependency from a package's `package.xml`.

```
rosup remove <dep_name> [options]
```

| Option          | Description                     |
|-----------------|---------------------------------|
| `-p, --package` | Target package (workspace mode) |

Removes all dependency tags (`<depend>`, `<build_depend>`, etc.) matching the
given name.

**Examples:**

```bash
rosup remove nav2_core
rosup remove nav2_core -p my_robot_nav
```

---

### `rosup search`

Search the ROS package index.

```
rosup search <query> [options]
```

| Option    | Description                                  |
|-----------|----------------------------------------------|
| `--distro` | Filter to a specific ROS distribution       |
| `--limit`  | Max results to display (default: 20)        |

**Examples:**

```bash
rosup search navigation
rosup search "costmap" --distro jazzy
```

---

### `rosup run`

Run a ROS node. Sources the workspace and dep layer, then delegates to
`ros2 run`.

```
rosup run <package_name> <executable_name> [-- <args>...]
```

**Examples:**

```bash
rosup run my_robot_nav nav_node
rosup run my_robot_nav nav_node -- --ros-args -p use_sim_time:=true
```

---

### `rosup launch`

Launch a ROS launch file. Sources the workspace and dep layer, then delegates
to `ros2 launch`.

```
rosup launch <package_name> <launch_file> [-- <args>...]
```

**Examples:**

```bash
rosup launch my_robot_bringup robot.launch.py
rosup launch my_robot_bringup robot.launch.py -- use_sim_time:=true
```

---

### `rosup resolve`

Manually trigger dependency resolution without building.

```
rosup resolve [options]
```

| Option          | Description                                      |
|-----------------|--------------------------------------------------|
| `--dry-run`     | Show the resolution plan without installing      |
| `--source-only` | Force source pull for all deps (skip rosdep)     |
| `--refresh`     | Force re-download of the rosdistro cache         |

**Examples:**

```bash
rosup resolve
rosup resolve --dry-run
rosup resolve --source-only --refresh
```

---

### `rosup clean`

Remove build artifacts.

```
rosup clean [options]
```

| Option          | Description                                                    |
|-----------------|----------------------------------------------------------------|
| `--all`         | Also remove `install/` (user workspace)                        |
| `--deps`        | Remove `.rosup/build/` and `.rosup/install/` (dep layer artifacts) |
| `--deps --src`  | Also remove `.rosup/src/` worktrees (bare clones kept in `~/.rosup/`) |
| `-p, --packages` | Clean only the specified packages from `build/`               |

Default (no flags): removes `build/` and `log/` only.

**Examples:**

```bash
rosup clean
rosup clean --all
rosup clean --deps
rosup clean -p my_robot_nav
```
