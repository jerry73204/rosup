# Builder Architecture — How rosup interacts with Colcon components

## The Build Pipeline

```
                    rosup build
                        │
         ┌──────────────┼──────────────┐
         │              │              │
    Phase 1:       Phase 2:       Phase 3:
    Check deps     Dep layer      Workspace
                   build          build
         │              │              │
    resolver.plan()  colcon build   colcon build
                   (.rosup/src/)  (workspace)
```

## Phase 2 & 3 in Detail: How colcon builds packages

### Step 1: Preparation (once)

colcon discovers packages, topo-sorts them, and creates **Job** objects.
Each Job gets:
- A task extension for its package type (e.g. `AmentCmakeBuildTask`)
- A `TaskContext` containing the package descriptor, build args, and a
  `dependencies` dict mapping dep name → install path
- A set of dependency job names (for the executor scheduler)

```python
# From build.py _get_jobs():
recursive_dependencies = OrderedDict()
for dep_name in decorator.recursive_dependencies:
    dep_path = os.path.join(install_base, dep_name)  # isolated mode
    recursive_dependencies[dep_name] = dep_path

task_context = TaskContext(
    pkg=pkg, args=package_args,
    dependencies=recursive_dependencies)

job = Job(
    identifier=pkg.name,
    dependencies=set(recursive_dependencies.keys()),
    task=extension, task_context=task_context)
```

All jobs are created upfront. The executor then runs them respecting
dependency ordering.

**rosup equivalent:** `BuildGraph` in `build/topo.rs` + `BuildContext`.

### Step 2: Execute jobs (per package, in dependency order)

The executor runs each Job. For parallel execution, multiple independent
jobs can run simultaneously. Each job calls `task.build()` which runs
these steps:

#### 2a. Set up the build environment

**This is the key interaction.** The task generates a shell script that
sources all the package's dependencies' `package.sh` scripts. It then
executes this script and captures the resulting environment.

```python
# From CmakeBuildTask.build():
env = await get_command_environment(
    'build', args.build_base, self.context.dependencies)
```

`get_command_environment()` generates a temporary shell script:
```bash
# colcon_command_prefix_build.sh (auto-generated per package):
. install/pkg_a/share/pkg_a/package.sh
. install/pkg_b/share/pkg_b/package.sh
```

Then runs `. script.sh && env -0` in a subprocess and parses the
null-separated output into an environment dict.

The result: `CMAKE_PREFIX_PATH`, `AMENT_PREFIX_PATH`, `LD_LIBRARY_PATH`,
`PKG_CONFIG_PATH`, etc. — all properly layered from the dependency chain.

**rosup's current approach:** In colcon delegation mode, rosup constructs
`AMENT_PREFIX_PATH` manually. This works because colcon handles the per-
package env internally.

**rosup's native builder needs to:** Replicate this per-package environment
setup (see "Critical Path" below).

#### 2b. Run cmake configure + build + install

```
cmake <source_dir> -DCMAKE_INSTALL_PREFIX=<install> ...  # using env from 2a
cmake --build <build_dir>
cmake --install <build_dir>
```

ament_cmake generates during install:
- `share/<pkg>/environment/` — ament hooks
- `share/<pkg>/local_setup.{bash,sh,zsh,dsv}`
- `share/<pkg>/package.{bash,sh,zsh,dsv,xml}`
- `share/<pkg>/cmake/` — CMake config files
- `share/ament_index/` — ament index entries

**rosup equivalent:** `AmentCmakeBuildTask`. Already implemented.

#### 2c. Post-install: colcon adds its hooks

After cmake install, the build task:

1. **Creates colcon `hook/` scripts** — scans install dir and generates:
   - `hook/cmake_prefix_path.{dsv,sh}` — if `*Config.cmake` found
   - `hook/pkg_config_path.{dsv,sh}` — always
   - `hook/ros_package_path.{dsv,sh}` — always

2. **Calls `create_environment_scripts()`** — reads DSV files from both
   `environment/` and `hook/`, generates final `package.{bash,sh,zsh}`

3. **Writes `colcon-core/packages/<pkg>`** — runtime deps for topo sourcing

These files are generated per-package, immediately after that package's
build completes. This means the NEXT package in the build order can
source this package's `package.sh` in step 2a.

### Step 3: Top-level install scripts (once, after all packages)

After all jobs complete, `BuildVerb.main()` calls
`_create_prefix_scripts()`:

```python
# From build.py:
rc = execute_jobs(context, jobs, ...)
self._create_prefix_scripts(install_base, context.args.merge_install)
```

This generates:
- `install/setup.{bash,sh,zsh}` — sources chained prefixes (underlays)
  then `local_setup`
- `install/local_setup.{bash,sh,zsh}` — calls `_local_setup_util_sh.py`
- `install/_local_setup_util_sh.py` — Python script that reads
  `colcon-core/packages/` files, topo-sorts, reads DSV files, and
  generates shell export commands

```bash
# local_setup.bash invocation:
_colcon_ordered_commands="$(python3 _local_setup_util_sh.py sh bash)"
eval "${_colcon_ordered_commands}"
```

**Python is required at source-time** (when a user runs
`source install/setup.bash`), not just build-time.

## What rosup needs to implement for native builds

### Already done ✅

| Component                        | rosup module                 |
|----------------------------------|------------------------------|
| Package discovery                | `init::colcon_scan()`        |
| Package.xml parsing              | `package_xml.rs`             |
| Topological sort                 | `build/topo.rs`              |
| Package selection                | `build/selection.rs`         |
| ament index markers              | `build/ament_index.rs`       |
| ament env hooks (`environment/`) | `build/environment.rs`       |
| colcon runtime deps file         | `build/ament_index.rs`       |
| cmake configure/build/install    | `build/task/ament_cmake.rs`  |
| pip install                      | `build/task/ament_python.rs` |

### Not yet done ❌

| Component                              | Approach                                                                         |
|----------------------------------------|----------------------------------------------------------------------------------|
| **Build environment setup** (3a)       | Source dep `package.sh` scripts or construct env from DSV files                  |
| **colcon hooks** (`hook/` dir) (3d)    | Generate `cmake_prefix_path`, `pkg_config_path`, `ros_package_path` DSV+sh files |
| **`package.dsv` update**               | Include `hook/` entries alongside `local_setup` entries                          |
| **Top-level install scripts** (Step 4) | Copy `_local_setup_util_sh.py`, generate `setup.bash`                            |
| **Parallel executor**                  | Dep-aware job scheduler using `BuildGraph::ready()`/`mark_done()`                |

### The critical path: build environment (3a)

The most important missing piece is how rosup sets up the environment
for each package's cmake invocation. Currently it only sets
`AMENT_PREFIX_PATH`. It needs to also set `CMAKE_PREFIX_PATH`,
`LD_LIBRARY_PATH`, `PKG_CONFIG_PATH`, etc.

Two approaches:

**A. Source the package.sh scripts (like colcon)**
```rust
// Generate: ". install/dep_a/share/dep_a/package.sh && env -0"
// Run in bash, capture env
let env = source_dep_scripts(&dep_install_paths)?;
// Pass env to cmake
```

This is exactly what colcon does via `get_command_environment()`. It
sources the `package.sh` for each dependency, captures the resulting
environment, and passes it to cmake. Pros: guaranteed compatible.
Cons: requires bash subprocess per build.

**B. Build env from DSV files directly (faster)**
```rust
// For each dep in topo order, read its DSV files and apply
let mut env = base_env.clone();
for dep in dep_install_paths {
    for dsv_entry in read_dsv_files(dep) {
        apply_dsv_action(&mut env, dsv_entry);
    }
}
// Pass env to cmake
```

Read the DSV files programmatically and apply the actions
(`prepend-non-duplicate`, `set`, etc.) in Rust. Pros: no subprocess,
faster. Cons: must handle all DSV action types correctly.

### Performance: why this matters

Measured on a single-package workspace:

| Step                             | Time       |
|----------------------------------|------------|
| Direct cmake reconfigure (no-op) | 0.1s       |
| Direct cmake --build (no-op)     | 0.015s     |
| Direct cmake --install (no-op)   | 0.004s     |
| **Total cmake (no-op)**          | **~0.12s** |
| **colcon build (no-op)**         | **~25s**   |
| **colcon overhead**              | **~24.9s** |

The 25s comes from colcon's `get_command_environment()` which sources
`package.sh` scripts in a bash subprocess for each package. The Python
interpreter startup and ROS env chain sourcing dominate.

**Approach B (DSV parsing in Rust) would eliminate this entirely.**
The no-op rebuild goes from 25s to ~0.2s. This is the primary
motivation for the native builder.

### Recommendation

**Start with Approach B (DSV parsing).** The DSV format is simple
(`action;VAR;suffix`), we already know all the action types, and
the performance win is massive. Use diff tests against Approach A's
output to verify correctness.

Approach A (sourcing package.sh) remains as a `--compat` fallback for
edge cases where DSV parsing doesn't produce identical results.
