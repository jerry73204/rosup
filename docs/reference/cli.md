# CLI Reference

## Global Behavior

rox searches for `rox.toml` starting from the current directory, walking up
to parent directories. The directory containing `rox.toml` is the **project
root**. All commands operate relative to the project root.

If no `rox.toml` is found, most commands will error with a hint to run
`rox init`.

All commands that need dependencies (build, test, run, launch) resolve them
automatically before proceeding.

---

## Commands

### `rox init`

Initialize a new `rox.toml` for an existing package or workspace.

```
rox init [--workspace]
```

**Behavior:**

- Without `--workspace`: looks for `package.xml` in the current directory.
  Generates a single-package `rox.toml` with `[package]` populated from
  `package.xml`.
- With `--workspace`: scans for packages under `src/` (or current directory).
  Generates a workspace `rox.toml` with discovered members.
- If both a `src/` directory and a `package.xml` exist, defaults to workspace
  mode (common colcon layout).

**Examples:**

```bash
# Inside a single package
cd ~/nav2_core
rox init

# Inside a colcon workspace
cd ~/my_ws
rox init
```

### `rox clone`

Clone a package from the ROS index and set it up for development.

```
rox clone <package_name> [--distro <distro>]
```

**Behavior:**

1. Looks up `<package_name>` in the rosdistro distribution cache.
2. Clones the source repository.
3. Generates `rox.toml` via `rox init`.

**Examples:**

```bash
rox clone nav2_core
rox clone nav2_core --distro jazzy
```

### `rox build`

Build packages. Resolves dependencies first, then delegates to `colcon build`.

```
rox build [options]
```

| Option              | Description                                |
|---------------------|--------------------------------------------|
| `-p, --packages`    | Build only the specified packages          |
| `--deps`            | Also build dependencies of selected packages |
| `--cmake-args`      | Additional CMake arguments (overrides rox.toml) |
| `--no-resolve`      | Skip dependency resolution                 |
| `--release`         | Shorthand for `-DCMAKE_BUILD_TYPE=Release` |
| `--debug`           | Shorthand for `-DCMAKE_BUILD_TYPE=Debug`   |

**Examples:**

```bash
rox build
rox build -p my_robot_nav
rox build -p my_robot_nav --deps
rox build --release
```

### `rox test`

Run tests. Resolves dependencies first, then delegates to `colcon test`.

```
rox test [options]
```

| Option              | Description                                |
|---------------------|--------------------------------------------|
| `-p, --packages`    | Test only the specified packages           |
| `--retest-until-pass` | Retry failed tests up to N times         |
| `--no-resolve`      | Skip dependency resolution                 |

**Examples:**

```bash
rox test
rox test -p my_robot_nav
```

### `rox add`

Add a dependency to a package's `package.xml`.

```
rox add <dep_name> [options]
```

| Option              | Description                                |
|---------------------|--------------------------------------------|
| `-p, --package`     | Target package (workspace mode; defaults to current dir) |
| `--build`           | Add as `<build_depend>` only               |
| `--exec`            | Add as `<exec_depend>` only                |
| `--test`            | Add as `<test_depend>` only                |
| `--dev`             | Add as `<build_depend>` + `<test_depend>`  |

Without a type flag, adds as `<depend>` (build + build_export + exec).

**Behavior:**

1. Validates the dependency exists in rosdistro or rosdep.
2. Adds the appropriate XML element to `package.xml`.
3. Resolves and installs the dependency.

**Examples:**

```bash
rox add nav2_core
rox add nav2_core --exec
rox add ament_cmake_gtest --test
rox add sensor_msgs -p my_robot_nav
```

### `rox remove`

Remove a dependency from a package's `package.xml`.

```
rox remove <dep_name> [options]
```

| Option              | Description                                |
|---------------------|--------------------------------------------|
| `-p, --package`     | Target package (workspace mode)            |

Removes all dependency tags (`<depend>`, `<build_depend>`, etc.) matching the
given name.

**Examples:**

```bash
rox remove nav2_core
rox remove nav2_core -p my_robot_nav
```

### `rox search`

Search the ROS package index.

```
rox search <query> [options]
```

| Option              | Description                                |
|---------------------|--------------------------------------------|
| `--distro`          | Filter to a specific ROS distribution      |
| `--limit`           | Max results to display (default: 20)       |

**Examples:**

```bash
rox search navigation
rox search "costmap" --distro jazzy
```

### `rox run`

Run a ROS node. Delegates to `ros2 run`.

```
rox run <package_name> <executable_name> [-- <args>...]
```

Sources the workspace install before execution.

**Examples:**

```bash
rox run my_robot_nav nav_node
rox run my_robot_nav nav_node -- --ros-args -p use_sim_time:=true
```

### `rox launch`

Launch a ROS launch file. Delegates to `ros2 launch`.

```
rox launch <package_name> <launch_file> [-- <args>...]
```

Sources the workspace install before execution.

**Examples:**

```bash
rox launch my_robot_bringup robot.launch.py
rox launch my_robot_bringup robot.launch.py -- use_sim_time:=true
```

### `rox resolve`

Manually trigger dependency resolution without building.

```
rox resolve [options]
```

| Option              | Description                                |
|---------------------|--------------------------------------------|
| `--dry-run`         | Show what would be installed, don't install |
| `--source-only`     | Force source pull for all deps             |

**Examples:**

```bash
rox resolve
rox resolve --dry-run
```

### `rox clean`

Remove build artifacts.

```
rox clean [options]
```

| Option              | Description                                |
|---------------------|--------------------------------------------|
| `--all`             | Remove build/, install/, and log/          |
| `-p, --packages`    | Clean only the specified packages          |

**Examples:**

```bash
rox clean
rox clean --all
rox clean -p my_robot_nav
```
