# Architecture

rox is a Cargo-like package manager for ROS 2. It wraps the existing ROS 2
toolchain (colcon, rosdep, ament) behind a single CLI that provides automatic
dependency resolution, a unified manifest (`rox.toml`), and a fluent developer
experience.

## Core Principles

1. **`package.xml` is the source of truth for dependencies.** rox reads and
   writes `package.xml` — it does not replace it. This keeps packages
   interoperable with the broader ROS ecosystem (bloom, rosdep, colcon).

2. **One `rox.toml` per project, at the root.** Whether the project is a
   single package or a multi-package workspace, there is exactly one `rox.toml`.
   No per-package sidecar files inside a workspace.

3. **Automatic dependency resolution.** Every command that needs dependencies
   (build, test, run) resolves them transparently before proceeding.

4. **Binary first, source as fallback.** Dependencies are resolved in order:
   ament environment → rosdep (binary) → source pull from rosdistro.

5. **Require base ROS environment.** rox expects the user to have sourced the
   base ROS installation (`/opt/ros/{distro}/setup.bash`). This sets
   `AMENT_PREFIX_PATH` and is the standard ROS 2 convention. rox warns clearly
   if the base environment is not detected and does not attempt to source it
   automatically.

## Project Modes

### Single-Package Mode

`rox.toml` sits next to `package.xml`. Suitable for developing or experimenting
with a single ROS package.

```
my_package/
├── rox.toml
├── package.xml
├── CMakeLists.txt
└── src/
```

Detected when `rox.toml` contains a `[package]` section.

### Workspace Mode

`rox.toml` sits at the workspace root. Member packages live under `src/` (or
other paths declared in `members`). Each member has its own `package.xml` but
no `rox.toml`.

```
my_workspace/
├── rox.toml
├── src/
│   ├── my_robot/
│   │   ├── package.xml
│   │   └── CMakeLists.txt
│   └── my_msgs/
│       ├── package.xml
│       └── CMakeLists.txt
├── build/
├── install/
└── log/
```

Detected when `rox.toml` contains a `[workspace]` section.

## Dependency Resolution

Resolution runs before build, test, and run commands. For each dependency
declared in `package.xml` files:

```
1. Is it already available in the current ament environment (AMENT_PREFIX_PATH)?
   └─ yes → done
   └─ no  → continue

2. Can rosdep resolve it to a system binary package?
   └─ yes → install via system package manager → done
   └─ no  → continue

3. Is there a user override in [resolve.overrides] in rox.toml?
   └─ yes → use override URL/branch
   └─ no  → look up source URL in rosdistro distribution cache

4. Clone/update source repo into per-project .rox/src/
   └─ found → clone → done
   └─ not found → error
```

### Source Package Resolution

ROS does not have a central source registry (no crates.io equivalent).
rosdistro is purely an index — it maps package names to git repository URLs
hosted on GitHub or other services. Source availability depends on the upstream
host.

Source packages are tracked at the **repository level**, not the package level.
A single repository often contains multiple ROS packages (e.g.,
`common_interfaces` contains `sensor_msgs`, `std_msgs`, `geometry_msgs`, etc.).
rox groups source-pulled dependencies by repository before cloning to avoid
redundant git operations.

Users can override any dependency via `[resolve.overrides]` in `rox.toml`,
which is the escape hatch for private or custom forks not listed in rosdistro.

## Storage Layout

### Global store (`~/.rox/`)

Shared across all projects. Contains only data that is safe to share:

```
~/.rox/
├── cache/                        # rosdistro YAML files (download cache)
│   ├── humble-cache.yaml
│   └── humble-cache.timestamp
└── src/                          # bare git clones (object store cache)
    ├── common_interfaces.git/    # bare clone — all objects for this repo
    └── rclcpp.git/
```

Bare clones act as a download cache: git objects are fetched once globally and
reused across all projects that need the same repository. The global store is
never a colcon workspace — it holds no build or install artifacts.

### Per-project store (`.rox/`)

One `.rox/` directory per project root, analogous to Cargo's `target/`. It is
git-ignored and owned entirely by that project.

```
~/repos/AutoSDV/
├── rox.toml
├── src/
│   └── my_pkg/package.xml
├── .rox/                         # per-project, git-ignored
│   ├── src/                      # git worktrees backed by ~/.rox/src/*.git
│   │   ├── common_interfaces/    # worktree @ humble branch
│   │   └── rclcpp/
│   ├── build/                    # colcon build artifacts for dep layer
│   └── install/                  # colcon install prefix for dep layer
├── build/                        # colcon build for user's packages
├── install/                      # colcon install for user's packages
└── log/
```

Git worktrees give each project an isolated working directory backed by the
shared object store. Two projects can check out different branches of the same
repository without conflicts or redundant downloads.

## Build Flow

`rox build` runs in two phases:

```
Phase 0: Check base ROS environment
  └─ AMENT_PREFIX_PATH set? → proceed
  └─ unset? → warn and suggest: source /opt/ros/{distro}/setup.bash

Phase 1: Resolve dependencies
  └─ ament check, rosdep install, source clone/update (see above)

Phase 2: Build dep layer (only if source deps exist)
  └─ colcon build \
       --packages-select <source-pulled-pkgs> \
       --build-base .rox/build \
       --install-base .rox/install
     env: AMENT_PREFIX_PATH = /opt/ros/{distro} + existing AMENT_PREFIX_PATH
  └─ skip if .rox/install already satisfies all source deps (incremental)

Phase 3: Build user workspace
  └─ colcon build [--symlink-install] [--packages-select ...]
     env: AMENT_PREFIX_PATH = <project>/.rox/install + /opt/ros/{distro} + existing
```

`rox run` and `rox launch` source `<project>/.rox/install/local_setup.bash`
and `<project>/install/local_setup.bash` before delegating to `ros2`.

## Underlying Tools

rox delegates to existing ROS 2 tools rather than reimplementing them:

| Task                      | Delegated to                  |
|---------------------------|-------------------------------|
| Build orchestration       | colcon build                  |
| Test orchestration        | colcon test                   |
| Binary dep install        | rosdep → apt/dnf/brew         |
| Source repo index         | rosdistro YAML cache          |
| Source repo object cache  | git (bare clones in ~/.rox/)  |
| Per-project working trees | git worktree                  |
| Package launch            | ros2 run / ros2 launch        |
