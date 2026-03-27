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
# members is optional — omit for Colcon auto-discovery
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
| `members` | string[] | no       | Glob patterns for member package dirs. When absent, rosup discovers packages using Colcon's rules. |
| `exclude` | string[] | no       | Directory path globs to exclude from workspace member discovery. Only affects workspace members — external packages cannot be excluded. |

### Auto-discovery (no `members`)

When `members` is omitted, rosup recursively scans the workspace root using
Colcon's package discovery rules:

- A directory containing `package.xml` is a ROS package. Recursion stops there
  (packages do not nest).
- Directories containing `COLCON_IGNORE`, `AMENT_IGNORE`, or `CATKIN_IGNORE`
  are skipped entirely (including descendants).
- `build/`, `install/`, `log/`, `.rosup/`, and hidden directories (`.git`, etc.)
  are always skipped.
- `exclude` patterns are applied after discovery.

```toml
# Minimal workspace manifest — auto-discovers all packages.
[workspace]
```

### Glob mode (`members` present)

Each entry in `members` is a glob pattern expanded relative to the workspace
root:

- `*` matches a single path segment (one directory level).
- `**` matches across directory boundaries (recursive).

```toml
[workspace]
members = ["src/*"]          # direct children of src/ only
# members = ["src/**"]       # all descendants of src/
exclude = ["src/experimental_*"]
```

A pattern that matches zero directories emits a warning but does not cause an
error. To convert auto-discovery output to a locked list, run `rosup sync --lock`.

### `rosup sync` — keeping the member list up to date

For workspaces with an explicit `members` list, `rosup sync` resynchronises it
with the current directory structure. See `rosup sync` in the CLI reference.

---

## Common Sections

The following sections apply to both modes.

### `[resolve]`

Controls how dependencies are resolved.

```toml
[resolve]
ros-distro = "humble"
overlays = [
    "/opt/ros/humble",
    "/opt/autoware/1.5.0",
]
source-preference = "binary"   # "binary" | "source" | "auto"
```

| Field               | Type     | Default  | Description                                    |
|---------------------|----------|----------|------------------------------------------------|
| `ros-distro`        | string   | auto     | ROS distribution to resolve against. Auto-detected from `ROS_DISTRO` env var if omitted. |
| `overlays`          | string[] | `[]`     | Ament install prefixes to activate, in underlay→overlay order. rosup sources `setup.sh` from each and applies the resulting environment to all subprocesses. |
| `source-preference` | enum     | `"auto"` | `"auto"`: try binary first, fall back to source. `"binary"`: prefer rosdep. `"source"`: force source pull. |

### `ignore-deps`

Dependency names to skip during resolution. Use this for deps that are
erroneous in upstream `package.xml` files you cannot modify, or deps only
available on other platforms. Only affects external deps — workspace members
cannot be ignored this way (use `[workspace] exclude` instead).

```toml
[resolve]
ignore-deps = ["roscpp", "catkin", "blickfeld-scanner"]
```

### `[[resolve.sources]]`

Register additional source registries — `.repos` files or direct git repos —
as dependency sources. Packages from these sources are resolved before rosdep
and rosdistro, acting like Cargo's `[patch]`.

Only repos that provide packages needed by the workspace are cloned.

```toml
# .repos file source (multiple repos)
[[resolve.sources]]
name = "autoware"
repos = "autoware.repos"     # path relative to workspace root

# Direct git repo source (Cargo-style)
[[resolve.sources]]
name = "my-nav2-fork"
git = "https://github.com/myfork/navigation2.git"
branch = "humble"            # or tag = "1.5.0" or rev = "abc123"
```

| Field    | Type   | Required | Description                                          |
|----------|--------|----------|------------------------------------------------------|
| `name`   | string | yes      | Human-readable name for this source                  |
| `repos`  | string | no*      | Path to a `.repos` file (relative to workspace root) |
| `git`    | string | no*      | Git clone URL                                        |
| `branch` | string | no       | Branch to checkout (with `git`)                      |
| `tag`    | string | no       | Tag to checkout (with `git`)                         |
| `rev`    | string | no       | Exact commit SHA (with `git`)                        |

\* Exactly one of `repos` or `git` must be set.

Manage sources via the CLI:

```bash
rosup source add --repos autoware.repos --name autoware
rosup source add --git https://github.com/myfork/nav2.git --branch humble --name my-nav2
rosup source list
rosup source remove my-nav2
```

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
ros-distro = "humble"
overlays = ["/opt/ros/humble", "/opt/autoware/1.5.0"]
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
ros-distro = "humble"
overlays = ["/opt/ros/humble"]

[build]
cmake-args = ["-DCMAKE_BUILD_TYPE=Release"]
```
