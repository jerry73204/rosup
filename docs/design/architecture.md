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
1. Is it already available in the current ament environment?
   └─ yes → done
   └─ no  → continue

2. Can rosdep resolve it to a system binary package?
   └─ yes → install via system package manager → done
   └─ no  → continue

3. Look up source URL in rosdistro distribution cache.
   └─ found → clone/update in ~/.rox/src/{pkg} → build → done
   └─ not found → error
```

Users can override this per-dependency via `[resolve.overrides]` in `rox.toml`.

## Global State

```
~/.rox/
├── cache/          # rosdistro distribution cache (YAML)
├── src/            # source-pulled dependency packages
└── config.toml     # global user settings (optional)
```

## Underlying Tools

rox delegates to existing ROS 2 tools rather than reimplementing them:

| Task                  | Delegated to            |
|-----------------------|-------------------------|
| Build orchestration   | colcon build            |
| Test orchestration    | colcon test             |
| Binary dep install    | rosdep → apt/dnf/brew   |
| Source repo lookup     | rosdistro YAML cache    |
| Source repo cloning   | git                     |
| Package launch        | ros2 run / ros2 launch  |
