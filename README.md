# rosup

A Cargo-style package manager for ROS 2. rosup handles dependency resolution,
source pulls, and build orchestration so you can focus on writing nodes.

```
rosup new my_robot_nav --ros-distro humble --node navigator
cd my_robot_nav
rosup add rclcpp nav2_core
rosup build
```

## Features

- **Package scaffolding** — `rosup new` generates a ready-to-build package with
  `package.xml`, `CMakeLists.txt` (or `setup.py`), and `rosup.toml`
- **Automatic dependency resolution** — resolves deps from the ament environment,
  rosdep (binary), and rosdistro source repos before every build
- **Source dependency caching** — bare git clones in `~/.rosup/` are shared across
  projects; each project gets an isolated worktree
- **Workspace support** — manage single packages or multi-package Colcon workspaces
  with one manifest
- **Ament overlay activation** — declare prefix paths in `rosup.toml` and rosup
  sources them before every invocation

## Requirements

- Rust toolchain (for installation)
- ROS 2 (Humble, Jazzy, or later) with `colcon` and `rosdep`
- Git

## Installation

```sh
cargo install rosup
```

## Quick start

### Create a new package

```sh
rosup new my_pkg --ros-distro humble
cd my_pkg
```

This creates:

```
my_pkg/
├── rosup.toml
├── package.xml
├── CMakeLists.txt
├── src/
└── include/my_pkg/
```

`rosup.toml` is pre-populated with your distro and overlay:

```toml
[package]
name = "my_pkg"

[resolve]
ros-distro = "humble"
overlays = ["/opt/ros/humble"]
```

### Add a starter node

```sh
rosup new my_pkg --ros-distro humble --node talker
```

Generates `src/talker.cpp` with a minimal `rclcpp::Node` subclass and wires
`add_executable` in `CMakeLists.txt`.

For Python:

```sh
rosup new my_py_pkg --ros-distro humble --build-type ament_python --node listener
```

### Add and remove dependencies

```sh
rosup add rclcpp                  # <depend> (build + exec)
rosup add std_msgs --exec         # <exec_depend> only
rosup add ament_lint_auto --test  # <test_depend> only
rosup remove std_msgs
```

`package.xml` is updated in place — comments and formatting are preserved.

### Build

```sh
rosup build
```

rosup resolves all dependencies declared in `package.xml`, installs binary deps
via rosdep, pulls any source deps into `.rosup/src/`, builds the dep layer, then
builds your workspace.

Select specific packages:

```sh
rosup build -p my_pkg -p my_msgs
rosup build --release               # -DCMAKE_BUILD_TYPE=Release
```

### Test

```sh
rosup test
rosup test -p my_pkg
rosup test --retest-until-pass 3    # retry flaky tests up to 3 times
```

### Resolve dependencies

```sh
rosup resolve                  # resolve and install (may prompt for sudo)
rosup resolve -y               # non-interactive (no prompts, for CI/scripts)
rosup resolve --dry-run        # show plan without installing
rosup resolve --source-only    # force source pull, skip rosdep
```

`rosup build` checks deps but does not install them — run `rosup resolve` first.

### Exclude workspace packages

```sh
rosup exclude 'src/localization/autoware_isaac_*'   # by path glob
rosup exclude --pkg autoware_isaac_localization_launch  # by package name
rosup exclude --list                                    # show excludes
rosup include 'src/localization/autoware_isaac_*'       # re-include
```

### Ignore external dependencies

```sh
rosup ignore catkin            # ignore a ROS 1 dep in package.xml
rosup ignore blickfeld-scanner # ignore a vendor-specific dep
rosup ignore --list            # show ignored deps
rosup unignore catkin          # stop ignoring
```

### Register dependency sources

```sh
# Use Autoware's .repos file as a dependency source
rosup source add --repos autoware.repos --name autoware

# Use a git repo fork
rosup source add --git https://github.com/myfork/nav2.git --branch humble --name my-nav2

# List and remove
rosup source list
rosup source remove my-nav2
```

Packages from registered sources are resolved before rosdep and rosdistro.
Only repos that provide packages needed by the workspace are cloned.

### Search the ROS package index

```sh
rosup search nav2
rosup search rclcpp --distro jazzy --limit 5
```

### Clone a package for development

```sh
rosup clone gscam --distro humble
```

Clones the upstream source from rosdistro into the current directory and
initialises a `rosup.toml`.

### Clean build artifacts

```sh
rosup clean               # remove build/ and log/
rosup clean --all         # also remove install/
rosup clean --deps        # remove .rosup/build/ and .rosup/install/
autoware_launch/autoware.launch.xml  ns=/rosup clean --deps --src  # also remove .rosup/src/ worktrees
```

## rosup.toml reference

One `rosup.toml` lives at the project root. It operates in either **package**
or **workspace** mode — not both.

### Single package

```toml
[package]
name = "my_robot_nav"    # must match <name> in package.xml
```

### Workspace

```toml
[workspace]
members = [
  "src/my_robot_description",
  "src/my_robot_bringup",
  "src/my_robot_nav",
]
```

### `[resolve]`

```toml
[resolve]
ros-distro = "humble"          # or auto-detected from ROS_DISTRO env

# Ament prefix paths, sourced underlay-first.
# rosup new pre-populates this with /opt/ros/<distro>.
overlays = [
  "/opt/ros/humble",
  "/opt/autoware/1.5.0",
]

# "auto" (default) tries binary first, falls back to source.
# "binary" forces rosdep; "source" forces source pull.
source-preference = "auto"

# Ignore deps that are erroneous or platform-specific.
ignore-deps = ["catkin", "roscpp", "blickfeld-scanner"]

# Additional source registries (checked before rosdep/rosdistro).
[[resolve.sources]]
name = "autoware"
repos = "autoware.repos"

# Direct git repo as a source.
# [[resolve.sources]]
# name = "my-nav2-fork"
# git = "https://github.com/myfork/navigation2.git"
# branch = "humble"

# Override resolution for a specific dep.
[resolve.overrides.nav2_core]
git    = "https://github.com/myfork/navigation2.git"
branch = "my-fix"
# rev = "abc1234"   # pin to a specific commit
```

### `[build]`

```toml
[build]
cmake-args      = ["-DCMAKE_BUILD_TYPE=Release"]
parallel-jobs   = 8
symlink-install = true

# Per-package overrides (workspace mode only).
[build.overrides.my_msgs]
cmake-args = ["-DCMAKE_BUILD_TYPE=Debug"]
```

### `[test]`

```toml
[test]
parallel-jobs    = 4
retest-until-pass = 2
```

## How dependency resolution works

For each dep in `package.xml`, checked in priority order:

1. **Workspace member** — another package in the same workspace? Colcon builds it.
2. **User override** — `[resolve.overrides]` in `rosup.toml` (patched fork, pin).
3. **Ament environment** — already on `AMENT_PREFIX_PATH`? Done.
4. **rosdep** — resolves to a system package and installs via apt/pip.
5. **rosdistro source** — clones the upstream repo into `.rosup/src/`, backed by
   a bare clone cache in `~/.rosup/src/`.
6. **Unresolved** — error. Use `rosup ignore <dep>` to suppress.

Source deps are built as a separate dep layer (`.rosup/build/`, `.rosup/install/`)
before your workspace, so they are never mixed with your own build artifacts.

## Project layout

```
my_workspace/
├── rosup.toml
├── src/
│   ├── my_robot/
│   │   ├── package.xml
│   │   └── CMakeLists.txt
│   └── my_msgs/
│       ├── package.xml
│       └── CMakeLists.txt
├── build/        # colcon build output
├── install/      # colcon install prefix
├── log/
└── .rosup/       # managed by rosup, git-ignored
    ├── src/      # source dep worktrees
    ├── build/    # dep layer build artifacts
    └── install/  # dep layer install prefix
```

Global cache shared across all projects:

```
~/.rosup/
├── cache/        # rosdistro YAML cache
└── src/          # bare git clones (object store)
```

## Commands

| Command                | Description                                              |
|------------------------|----------------------------------------------------------|
| `rosup new <name>`     | Scaffold a new ROS 2 package                             |
| `rosup init`           | Create `rosup.toml` for an existing package or workspace |
| `rosup clone <pkg>`    | Clone a package from the ROS index                       |
| `rosup add <dep>`      | Add a dependency to `package.xml`                        |
| `rosup remove <dep>`   | Remove a dependency from `package.xml`                   |
| `rosup resolve`        | Resolve and install deps (may need sudo)                 |
| `rosup build`          | Check deps and build (never installs)                    |
| `rosup test`           | Check deps and run tests                                 |
| `rosup search <query>` | Search the ROS package index                             |
| `rosup sync`           | Sync workspace member list in `rosup.toml`               |
| `rosup exclude`        | Exclude workspace packages from discovery                |
| `rosup include`        | Re-include excluded packages                             |
| `rosup source`         | Manage dependency source registries (.repos / git)       |
| `rosup ignore <dep>`   | Ignore an external dependency during resolution          |
| `rosup unignore <dep>` | Stop ignoring a dependency                               |
| `rosup clean`          | Remove build artifacts                                   |

Pass `--help` to any command for the full flag list.

## License

MIT
