# Colcon Build Architecture — Findings

Study of the colcon source code at `external/colcon-core/`, `colcon-ros/`,
and `colcon-cmake/`. Goal: understand how colcon builds packages so rosup
can achieve full compatibility.

---

## Overview

Colcon is a **plugin-based build orchestrator**. Its core is small — the
real logic is in plugins for package discovery, identification, selection,
building, and environment setup. Everything is an extension point.

```
Discovery → Identification → Topological Sort → Selection → Build Tasks
```

## 1. Package Discovery

**Entry point:** `colcon_core.package_discovery`

Two flavors:
- **PathPackageDiscovery** — the default. Walks `--paths` (default: `.`)
  recursively. Each directory is passed to identification extensions.
- **--base-paths** — similar but for specifying base directories.

**COLCON_IGNORE handling:** `IgnorePackageIdentification` (priority 1000)
checks for `COLCON_IGNORE` file and raises `IgnoreLocationException` to
skip the directory and all descendants. ROS plugin also checks
`AMENT_IGNORE` and `CATKIN_IGNORE`.

**rosup equivalent:** `colcon_scan()` in `init.rs` replicates this. Our
implementation matches colcon's behavior.

## 2. Package Identification

**Entry point:** `colcon_core.package_identification`

Extensions check if a directory is a package and set its type:

| Extension                   | Priority | Checks for                  | Sets type to                                |
|-----------------------------|----------|-----------------------------|---------------------------------------------|
| IgnorePackageIdentification | 1000     | `COLCON_IGNORE`             | (skips)                                     |
| RosPackageIdentification    | 150      | `package.xml`               | `ros.ament_cmake`, `ros.ament_python`, etc. |
| CmakePackageIdentification  | 100      | `CMakeLists.txt`            | `cmake`                                     |
| PythonPackageIdentification | 50       | `setup.py`/`pyproject.toml` | `python`                                    |

ROS identification uses `catkin_pkg` to parse `package.xml` and extract
the `<build_type>` from `<export>`. This determines which build task runs.

**rosup equivalent:** We parse `package.xml` and extract `build_type` in
`package_xml.rs`. We don't currently use `build_type` to select build
behavior since we delegate to colcon, but `rosup tree` would use it.

## 3. Dependency Resolution & Topological Ordering

**Entry point:** `colcon_core.topological_order`

Algorithm:
1. For each package, compute `recursive_dependencies` (transitive closure)
2. Iteratively find packages with no remaining deps (all deps built)
3. Yield those packages (alphabetically for determinism)
4. Remove them from other packages' dependency sets
5. Repeat until all packages ordered or cycle detected

**Dependency categories:** `build`, `run`, `test` — mapped from package.xml
tags:
- `<depend>` → build + run
- `<build_depend>` → build
- `<exec_depend>` → run
- `<test_depend>` → test
- `<buildtool_depend>` → build

**rosup implication:** When rosup invokes `colcon build`, colcon does its
own topo sort. rosup doesn't need to replicate this — just pass the right
packages and let colcon order them.

## 4. Package Selection

**Entry point:** `colcon_core.package_selection`

Key flags and their semantics:

| Flag                       | Behavior                                           |
|----------------------------|----------------------------------------------------|
| `--packages-select A B`    | Only build A and B (no deps)                       |
| `--packages-skip A B`      | Build everything except A and B                    |
| `--packages-up-to A`       | Build A and all its dependencies                   |
| `--packages-up-to-regex`   | Same with regex matching                           |
| `--packages-above A`       | Build A and everything that depends on A           |
| `--packages-skip-by-dep A` | Skip packages that depend on A                     |
| `--packages-ignore A`      | Ignore A during discovery (as if it doesn't exist) |

**rosup mapping:**
- `rosup build -p A` → `--packages-select A`
- `rosup build -p A --deps` → `--packages-up-to A`
- `[workspace] exclude` → `--packages-skip` (by name, after discovery)

## 5. The Build Verb

**Entry point:** `colcon_core.verb.build`

For each selected package:
1. Look up a **task extension** for the package type (e.g. `ros.ament_cmake`)
2. Create `BuildPackageArguments` with:
   - `path` — absolute source directory
   - `build_base` — per-package build directory
   - `install_base` — install prefix (per-package or merged)
   - `symlink_install` — bool
   - `cmake_args` — from CLI
   - `recursive_dependencies` — dict of dep_name → install_path
3. Create a `Job(identifier, dependencies, task, context)`
4. Submit to executor

**Important: `recursive_dependencies` mapping.** Each dependency is mapped
to its install path. The build task uses this to set `CMAKE_PREFIX_PATH`
and `AMENT_PREFIX_PATH` so cmake/ament can find the dependency.

## 6. Build Tasks

### ament_cmake (`colcon-ros/colcon_ros/task/ament_cmake/build.py`)

Wraps `CmakeBuildTask` with ROS-specific args:
- `-DAMENT_CMAKE_SYMLINK_INSTALL=1` (if `--symlink-install`)
- `-DAMENT_TEST_RESULTS_DIR=...`
- Custom `--ament-cmake-args`

### cmake (`colcon-cmake/colcon_cmake/task/cmake/build.py`)

1. `cmake <source_dir> -B <build_dir> <args>` — configure
2. `cmake --build <build_dir>` — build
3. `cmake --install <build_dir> --prefix <install_dir>` — install

### ament_python (`colcon-ros/colcon_ros/task/ament_python/build.py`)

1. `pip install` (or `pip install --editable` for symlink-install)
2. Installs `package.xml` to `share/{pkg}/package.xml`
3. Creates ament index marker at `share/ament_index/resource_index/packages/{pkg}`

## 7. Environment Setup

**How colcon makes deps visible to each build:**

For each package being built, colcon sets:
```
CMAKE_PREFIX_PATH=<dep1_install>:<dep2_install>:...
AMENT_PREFIX_PATH=<dep1_install>:<dep2_install>:...
```

Where `<dep_install>` is the install directory of each dependency.

**After the full build**, colcon generates `install/<pkg>/local_setup.{sh,bash,zsh}`
scripts. Sourcing these sets up the environment for that package and its
runtime deps.

**`.colcon/index/{pkg}` file:** Created at
`install/.colcon/index/{pkg}`, contains colon-separated runtime deps.
Used by prefix scripts to source deps in topological order.

## 8. Executor Model

- **Sequential:** Builds one package at a time in topo order.
- **Parallel:** Builds independent packages concurrently. Uses asyncio.
  `--parallel-workers N` controls concurrency.

**On-error behavior:**
- `interrupt` (default) — stop everything
- `--continue-on-error` — skip failed package's dependents, continue others

## 9. The `--symlink-install` Flag

Implemented at two levels:
1. **Core install function** (`colcon_core.task.install`): symlinks files
   instead of copying from source to install dir.
2. **ament_cmake**: passes `-DAMENT_CMAKE_SYMLINK_INSTALL=1` to cmake.
3. **ament_python**: uses `pip install --editable` when available.

---

## Implications for rosup

### What rosup currently does correctly

1. **Discovery** — `colcon_scan` matches colcon's ignore marker behavior
2. **Flag mapping** — `--packages-select`, `--packages-skip`, `--packages-up-to`,
   `--symlink-install`, `--cmake-args`, `--parallel-workers` are all passed through
3. **Dep layer** — `.rosup/install/` is prepended to `AMENT_PREFIX_PATH`
4. **Stale dep cleanup** — removes packages from `.rosup/install/` that
   became workspace members

### What rosup could improve

1. **No topo sort needed** — colcon handles this internally. rosup just needs
   to pass the right package names and flags.

2. **`--packages-ignore` vs `--packages-skip`** — Currently rosup uses
   `--packages-skip` for excluded workspace members. `--packages-ignore`
   might be more correct: it removes the package from discovery entirely,
   so colcon won't even warn about it. Worth investigating.

3. **`--merge-install`** — Not currently supported by rosup. Some workspaces
   use merged installs for simpler `AMENT_PREFIX_PATH`. Could be a future
   `[build]` config option.

4. **`--continue-on-error`** — Not currently exposed. Could be useful for
   large workspaces where one broken package shouldn't block the rest.

5. **Build type awareness** — rosup parses `build_type` from `package.xml`
   but doesn't use it. For `rosup tree`, knowing whether a package is
   `ament_cmake` vs `ament_python` would be useful metadata.

6. **Parallel workers** — Currently mapped from `[build] parallel-jobs`.
   Could auto-detect based on CPU count.

### Full compatibility checklist

| colcon feature            | rosup status                               |
|---------------------------|--------------------------------------------|
| `--base-paths`            | Used for dep layer build                   |
| `--build-base`            | Used for dep layer build                   |
| `--install-base`          | Used for dep layer build                   |
| `--symlink-install`       | Mapped from `[build] symlink-install`      |
| `--parallel-workers`      | Mapped from `[build] parallel-jobs`        |
| `--cmake-args`            | Mapped from `[build] cmake-args` + CLI     |
| `--packages-select`       | Mapped from `-p`                           |
| `--packages-up-to`        | Mapped from `-p --deps`                    |
| `--packages-skip`         | Mapped from `[workspace] exclude`          |
| `--continue-on-error`     | Not exposed                                |
| `--merge-install`         | Not supported                              |
| `--event-handlers`        | Not exposed                                |
| `--packages-ignore`       | Not used (could replace `--packages-skip`) |
| `--allow-overriding`      | Not used                                   |
| `--cmake-target`          | Not exposed                                |
| `--cmake-clean-cache`     | Not exposed                                |
| `--cmake-force-configure` | Not exposed                                |
