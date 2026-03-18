# rosup.toml Reference

`rosup.toml` is the project manifest for rosup. There is exactly one per project,
located at the project root.

A `rosup.toml` operates in one of two modes depending on which top-level section
is present:

- **`[package]`** — single-package mode
- **`[workspace]`** — workspace mode

These two sections are mutually exclusive.

---

## Single-Package Mode

```toml
[package]
name = "my_robot_nav"      # must match <name> in package.xml
```

### `[package]`

| Field  | Type   | Required | Description                          |
|--------|--------|----------|--------------------------------------|
| `name` | string | yes      | Package name. Must match package.xml |

---

## Workspace Mode

```toml
[workspace]
members = [
    "src/my_robot_description",
    "src/my_robot_bringup",
    "src/my_robot_navigation",
]
exclude = ["src/experimental_*"]
```

### `[workspace]`

| Field     | Type     | Required | Description                                      |
|-----------|----------|----------|--------------------------------------------------|
| `members` | string[] | yes      | Glob patterns or paths to member package dirs     |
| `exclude` | string[] | no       | Glob patterns to exclude from member discovery    |

---

## Common Sections

The following sections apply to both modes.

### `[resolve]`

Controls how dependencies are resolved.

```toml
[resolve]
ros-distro = "jazzy"
source-preference = "binary"   # "binary" | "source" | "auto"
```

| Field               | Type   | Default    | Description                                    |
|---------------------|--------|------------|------------------------------------------------|
| `ros-distro`        | string | auto       | ROS distribution to resolve against. Auto-detected from `ROS_DISTRO` env var if omitted. |
| `ros-prefix`        | string | auto       | Path to the base ROS install (e.g. `/opt/ros/humble`). Auto-detected from `AMENT_PREFIX_PATH` if omitted. Use for non-standard install locations. |
| `source-preference` | enum   | `"auto"`   | `"auto"`: try binary first, fall back to source. `"binary"`: prefer rosdep. `"source"`: force source pull. |

### `[resolve.overrides.<dep_name>]`

Override resolution for a specific dependency. Forces source pull from the
given repository instead of using rosdep or the default rosdistro entry.

```toml
[resolve.overrides.nav2_core]
git = "https://github.com/myfork/navigation2.git"
branch = "my-fix"
rev = "abc1234"     # optional: pin to exact commit
```

| Field    | Type   | Required | Description                          |
|----------|--------|----------|--------------------------------------|
| `git`    | string | yes      | Git repository URL                   |
| `branch` | string | no       | Branch name. Default: repo default   |
| `rev`    | string | no       | Exact commit hash. Overrides branch  |

### `[build]`

Build configuration passed to colcon.

```toml
[build]
cmake-args = ["-DCMAKE_BUILD_TYPE=Release"]
parallel-jobs = 8
symlink-install = true
```

| Field              | Type     | Default | Description                              |
|--------------------|----------|---------|------------------------------------------|
| `cmake-args`       | string[] | `[]`    | Additional CMake arguments               |
| `parallel-jobs`    | integer  | auto    | Max parallel build jobs                  |
| `symlink-install`  | bool     | `true`  | Use symlink install (colcon default)     |

### `[build.overrides.<pkg_name>]`

Per-package build overrides. Only valid in workspace mode.

```toml
[build.overrides.my_msgs]
cmake-args = ["-DCMAKE_BUILD_TYPE=Debug"]
```

Fields are the same as `[build]`.

### `[test]`

Test configuration passed to colcon.

```toml
[test]
parallel-jobs = 4
retest-until-pass = 2
```

| Field               | Type    | Default | Description                                |
|---------------------|---------|---------|--------------------------------------------|
| `parallel-jobs`     | integer | auto    | Max parallel test jobs                     |
| `retest-until-pass` | integer | `0`     | Retry failed tests up to N times           |

---

## Full Example — Workspace

```toml
[workspace]
members = ["src/*"]
exclude = ["src/experimental_*"]

[resolve]
ros-distro = "jazzy"
# ros-prefix = "/opt/ros/jazzy"   # uncomment for non-standard install paths
source-preference = "auto"

[resolve.overrides.nav2_core]
git = "https://github.com/myfork/navigation2.git"
branch = "fix-costmap"

[build]
cmake-args = ["-DCMAKE_BUILD_TYPE=RelWithDebInfo"]
symlink-install = true
parallel-jobs = 8

[build.overrides.my_msgs]
cmake-args = ["-DCMAKE_BUILD_TYPE=Debug"]
```

## Full Example — Single Package

```toml
[package]
name = "my_robot_nav"

[resolve]
ros-distro = "jazzy"

[build]
cmake-args = ["-DCMAKE_BUILD_TYPE=Release"]
```
