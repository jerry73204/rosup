# Colcon Rewrite in Rust — Design Proposal

## Why rewrite

Colcon is ~27 Python packages with a plugin system based on `setuptools`
entry points. It works but has drawbacks:

- **Startup overhead** — Python import + plugin discovery takes 0.5–1s
  before any work begins. On a 5-package workspace, colcon's overhead
  dominates actual build time.
- **Plugin fragility** — Python version upgrades, pip conflicts, and
  virtualenv mismatches routinely break colcon installs.
- **No binary distribution** — colcon is pip-installed, not a standalone
  binary. Users must maintain a Python environment.
- **Limited parallelism** — Python's GIL constrains the executor. The
  parallel executor uses asyncio subprocess, which is fine for I/O-bound
  work (waiting on cmake) but adds overhead for coordination.

rosup is already a Rust binary. Embedding colcon's build logic in Rust
eliminates the Python dependency for the core build pipeline while keeping
the option to load Python plugins for exotic package types.

---

## Scope: what to rewrite, what to keep

### Rewrite in Rust (core pipeline)

| Component                 | Why                                                   |
|---------------------------|-------------------------------------------------------|
| Package discovery         | Already done (`colcon_scan`)                          |
| Package identification    | Already done (`package_xml` parser)                   |
| Topological ordering      | Simple graph algorithm, no plugin needed              |
| Package selection         | `--packages-select`, `--packages-skip`, etc.          |
| Build tasks: ament_cmake  | Invokes `cmake` + `cmake --build` + `cmake --install` |
| Build tasks: ament_python | Invokes `pip install` or `python setup.py`            |
| Environment scripts       | Generate `local_setup.{bash,zsh,fish}`                |
| Ament index               | Write marker files to `share/ament_index/`            |
| Executor                  | Parallel dep-aware job scheduler                      |

### Keep as external (Python plugins via PyO3)

| Component                 | Why                                                   |
|---------------------------|-------------------------------------------------------|
| colcon-cargo-ros2         | Rust + ROS 2 message bindings — complex, already PyO3 |
| colcon-ros2-rust          | Alternative Rust build support                        |
| Custom enterprise plugins | Can't predict what users have installed               |
| catkin build type         | ROS 1 legacy, not worth rewriting                     |

---

## Architecture

### Everything is a plugin

In colcon, `ament_cmake` and `ament_python` are NOT part of the core —
they live in `colcon-ros` and `colcon-cmake` as plugins. The core only
defines `TaskExtensionPoint`. Every build type is a plugin, including the
"standard" ones.

rosup follows the same principle. The build core defines a trait, and all
build types implement it — including the built-in ones:

```rust
pub trait BuildTask: Send + Sync {
    /// Build the package from source.
    fn build(&self, ctx: &BuildContext) -> Result<()>;
    /// Run the package's tests.
    fn test(&self, ctx: &TestContext) -> Result<()>;
}
```

### Built-in tasks (compiled into rosup)

These cover ~95% of ROS 2 packages and have zero overhead:

- `AmentCmakeBuildTask` — cmake configure/build/install + ament index
- `AmentPythonBuildTask` — pip install + ament index
- `CmakeBuildTask` — plain cmake (non-ROS)

### External task plugins (loaded at runtime)

For package types not handled by built-in tasks (e.g. `ros.cargo_ros2`),
rosup loads external plugins.

#### Plugin loading options considered

| Option | Mechanism | Pros | Cons |
|--------|-----------|------|------|
| **A. Rust dylib** | `.so` loaded via `dlopen` | Fast, native | ABI instability across Rust versions |
| **B. WASM** | Sandboxed modules | Portable, safe | Slower, emerging ecosystem |
| **C. Subprocess** | Standalone binary, JSON protocol | Simple, language-agnostic, no ABI concerns | Process spawn overhead per package |
| **D. PyO3** | Load Python colcon plugins | Reuses existing ecosystem | Requires Python runtime |

#### Recommended: Option C (subprocess protocol)

Each plugin is a standalone binary named `rosup-task-<build_type>`:

```
rosup-task-cargo-ros2
rosup-task-catkin
rosup-task-custom-whatever
```

rosup discovers these on `$PATH` (like git's subcommand model). The
protocol is structured JSON on stdin/stdout:

```bash
# rosup calls:
echo '{
  "action": "build",
  "package_name": "my_pkg",
  "source_path": "/ws/src/my_pkg",
  "build_base": "/ws/build/my_pkg",
  "install_base": "/ws/install/my_pkg",
  "cmake_prefix_path": "/opt/ros/humble:...",
  "ament_prefix_path": "/opt/ros/humble:...",
  "symlink_install": true,
  "cmake_args": ["-DCMAKE_BUILD_TYPE=Release"],
  "env": { "ROS_DISTRO": "humble", ... }
}' | rosup-task-cargo-ros2

# Plugin returns exit code 0/1
# stdout: JSON build result or plain log output
```

**Why subprocess over PyO3:**

- No Python runtime required for the 95% case (ament_cmake/ament_python)
- Any language can implement a plugin (Rust, Python, Go, shell script)
- No ABI stability concerns
- `colcon-cargo-ros2` could ship a `rosup-task-cargo-ros2` binary alongside
  the existing colcon plugin — same Rust code, two frontends
- Plugin crashes don't bring down rosup

**Colcon fallback** — for plugins that haven't been ported to the subprocess
protocol, rosup can still delegate to colcon:

```rust
struct ColconFallbackTask {
    package_type: String,
}

impl BuildTask for ColconFallbackTask {
    fn build(&self, ctx: &BuildContext) -> Result<()> {
        // Invoke colcon for this single package
        Command::new("colcon")
            .args(["build", "--packages-select", &ctx.pkg.name,
                   "--build-base", &ctx.build_base,
                   "--install-base", &ctx.install_base])
            .status()?;
        Ok(())
    }
}
```

### Task resolution order

When rosup encounters a package with build type `T`:

1. Check built-in tasks (`ament_cmake`, `ament_python`, `cmake`)
2. Check `$PATH` for `rosup-task-T`
3. Fall back to `colcon build --packages-select` (if colcon is installed)
4. Error: "unsupported build type: T"

### Source tree layout

```
build/
  mod.rs              — public API, task trait, task resolution
  discovery.rs        — package discovery (existing colcon_scan)
  identification.rs   — package type detection (existing package_xml)
  topo.rs             — topological ordering (Kahn's algorithm)
  selection.rs        — --packages-select/skip/up-to logic
  executor.rs         — parallel dep-aware job scheduler
  task/
    mod.rs            — BuildTask trait + built-in registry
    ament_cmake.rs    — cmake configure/build/install
    ament_python.rs   — pip install + ament index
    cmake.rs          — plain cmake (non-ROS)
    subprocess.rs     — external plugin via JSON protocol
    colcon_fallback.rs — delegate to colcon CLI
  environment.rs      — generate setup.bash/local_setup.bash
  ament_index.rs      — write ament index entries
```

### CLI

```
rosup build [options]
  --native          Use rosup's built-in build tasks (default)
  --colcon          Delegate entirely to colcon (current behavior)
  --continue-on-error
  --merge-install
```

The `--colcon` flag preserves the current behavior as an escape hatch.
`--native` (default when implemented) uses the Rust build pipeline.

---

## Topological Ordering

Straightforward Kahn's algorithm:

```rust
pub fn topological_sort(packages: &[PackageNode]) -> Result<Vec<&PackageNode>> {
    // 1. Build adjacency list + in-degree map
    // 2. Queue all packages with in-degree 0
    // 3. Pop from queue, decrement neighbors' in-degree
    // 4. When neighbor reaches 0, add to queue
    // 5. Error if cycle detected (remaining nodes > 0)
}

pub struct PackageNode {
    pub name: String,
    pub path: PathBuf,
    pub build_type: BuildType,  // ament_cmake, ament_python, cmake, cargo_ros2
    pub deps: PackageDeps,      // build, run, test dep sets
}
```

Colcon sorts alphabetically within each "ready" batch for determinism.
We should match this.

---

## Build Tasks

### ament_cmake

The most common package type. Steps:

```rust
fn build_ament_cmake(ctx: &BuildContext) -> Result<()> {
    let build_dir = ctx.build_base.join(&ctx.pkg.name);
    let install_dir = ctx.install_base.join(&ctx.pkg.name);

    // 1. Configure
    let mut cmake_args = vec![
        ctx.pkg.path.display().to_string(),
        format!("-DCMAKE_INSTALL_PREFIX={}", install_dir.display()),
        "-DBUILD_TESTING=OFF".to_string(),  // unless testing
    ];
    cmake_args.extend(ctx.cmake_args.iter().cloned());
    if ctx.symlink_install {
        cmake_args.push("-DAMENT_CMAKE_SYMLINK_INSTALL=1".to_string());
    }
    // Set CMAKE_PREFIX_PATH from resolved deps
    let prefix_path = ctx.dep_install_paths.join(":");
    cmake_args.push(format!("-DCMAKE_PREFIX_PATH={prefix_path}"));

    run("cmake", &cmake_args, &build_dir, &ctx.env)?;

    // 2. Build
    run("cmake", &["--build", ".", "--parallel", &ctx.jobs], &build_dir, &ctx.env)?;

    // 3. Install
    run("cmake", &["--install", "."], &build_dir, &ctx.env)?;

    // 4. Write ament index
    write_ament_index(&install_dir, &ctx.pkg.name)?;

    // 5. Generate environment hooks
    generate_env_hooks(&install_dir, &ctx.pkg)?;

    Ok(())
}
```

### ament_python

```rust
fn build_ament_python(ctx: &BuildContext) -> Result<()> {
    let install_dir = ctx.install_base.join(&ctx.pkg.name);

    // 1. Install Python package
    if ctx.symlink_install {
        run("pip", &["install", "--editable", ".", "--prefix", &install_dir],
            &ctx.pkg.path, &ctx.env)?;
    } else {
        run("pip", &["install", ".", "--prefix", &install_dir],
            &ctx.pkg.path, &ctx.env)?;
    }

    // 2. Copy package.xml
    fs::copy(
        ctx.pkg.path.join("package.xml"),
        install_dir.join("share").join(&ctx.pkg.name).join("package.xml"),
    )?;

    // 3. Write ament index
    write_ament_index(&install_dir, &ctx.pkg.name)?;

    // 4. Generate environment hooks
    generate_env_hooks(&install_dir, &ctx.pkg)?;

    Ok(())
}
```

---

## Parallel Executor

```rust
pub struct ParallelExecutor {
    max_workers: usize,
}

impl ParallelExecutor {
    pub async fn execute(&self, jobs: Vec<Job>) -> Result<Vec<JobResult>> {
        let mut pending: HashMap<String, Job> = ...;
        let mut in_progress: FuturesUnordered<JoinHandle<JobResult>> = ...;
        let mut completed: HashSet<String> = HashSet::new();

        loop {
            // Find jobs whose deps are all completed
            let ready: Vec<Job> = pending.drain_filter(|name, job| {
                job.deps.iter().all(|d| completed.contains(d))
            }).map(|(_, j)| j).collect();

            // Spawn up to max_workers
            for job in ready {
                while in_progress.len() >= self.max_workers {
                    let result = in_progress.next().await;
                    completed.insert(result.name.clone());
                }
                in_progress.push(tokio::spawn(job.run()));
            }

            // Wait for at least one to complete
            if let Some(result) = in_progress.next().await {
                completed.insert(result.name.clone());
                if result.is_err() && !continue_on_error {
                    break;
                }
            }

            if pending.is_empty() && in_progress.is_empty() {
                break;
            }
        }
    }
}
```

---

## Environment Script Generation

After each package is installed, generate:

```
install/<pkg>/share/<pkg>/local_setup.bash
install/<pkg>/share/<pkg>/local_setup.sh
install/<pkg>/share/<pkg>/local_setup.zsh
install/<pkg>/share/ament_index/resource_index/packages/<pkg>
install/<pkg>/share/colcon-core/packages/<pkg>  (runtime deps list)
```

The `local_setup.bash` script:
```bash
# generated by rosup
AMENT_PREFIX_PATH="<install>/<pkg>:$AMENT_PREFIX_PATH"
PATH="<install>/<pkg>/bin:$PATH"
# ... library paths, python paths, etc.
```

The `share/colcon-core/packages/<pkg>` file:
```
dep1:dep2:dep3
```

This is the runtime dependency list used by prefix scripts to source deps
in the right order. Must match colcon's format for interoperability.

---

## Compatibility Strategy

### Phase 1: Hybrid (current + escape hatch)

Keep the current colcon delegation as the default. Add `--colcon` flag to
force it once native tasks exist. Start implementing `ament_cmake` task.

### Phase 2: Native for ament_cmake and ament_python

These two cover ~95% of ROS 2 packages. Test against real workspaces
(Autoware 455 packages) to verify output compatibility. Key test:
source colcon's output and rosup's output, diff the environment.

### Phase 3: Subprocess plugin protocol

Define the JSON stdin/stdout protocol. Port `colcon-cargo-ros2` to ship
a `rosup-task-cargo-ros2` binary alongside the existing colcon plugin.

### Phase 4: Colcon fallback for unported plugins

Any package type with no built-in task and no `rosup-task-*` binary on
`$PATH` falls back to:
```
colcon build --packages-select <pkg> --build-base <dir> --install-base <dir>
```

This preserves compatibility with all existing colcon plugins without
requiring any changes to them.

---

## What NOT to rewrite

- **rosdep** — system package manager integration. Keep using the `rosdep`
  CLI. It's well-tested and handles OS-specific package mapping.
- **bloom** — release tooling. Orthogonal to the build pipeline.
- **ament_cmake** CMake modules — the actual CMake code that ament provides
  (`ament_cmake_auto`, `ament_cmake_pytest`, etc.). rosup just invokes cmake;
  the cmake modules are part of the ROS install.
- **catkin build type** — ROS 1 legacy. Support via fallback only.

---

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Environment scripts don't match colcon's | Test by sourcing both and comparing env diff |
| ament_index format mismatch | Write integration test that colcon can read rosup's output |
| Missing cmake variables | Test against Autoware (heavy cmake usage) |
| Python package install differences | Test with ament_python packages that use launch files |
| Plugin loading breaks | `--colcon` escape hatch always available |
| Parallel executor race conditions | Stress test with large workspace + low worker count |
