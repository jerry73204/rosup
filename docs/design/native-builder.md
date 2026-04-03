# Native Builder Architecture

A Rust-native build engine replacing colcon delegation, with DSV-based
environment setup, incremental builds via fingerprinting, and colcon
plugin compatibility.

---

## Overview

```
rosup build
  │
  ├─ 1. Discover packages    (colcon_scan — existing)
  ├─ 2. Parse deps & topo    (BuildGraph — existing)
  ├─ 3. Apply selection      (SelectionFilter — existing)
  ├─ 4. Fingerprint check    (skip unchanged packages) ← NEW
  ├─ 5. For each ready package (parallel executor):
  │     ├─ 5a. Build env from DSV files             ← NEW
  │     ├─ 5b. Dispatch to build task
  │     │      ├─ Built-in: ament_cmake, ament_python, cmake
  │     │      ├─ Subprocess: rosup-task-<type>
  │     │      └─ PyO3: colcon Python plugins
  │     ├─ 5c. Generate colcon hooks (hook/ dir)     ← NEW
  │     └─ 5d. Update fingerprint
  └─ 6. Generate top-level install scripts           ← NEW
```

---

## 1. DSV Parser and Environment Builder

### DSV format

Each `.dsv` file contains semicolon-separated entries:

```
action;VARIABLE;suffix
```

### All DSV actions (from colcon source)

| Action                            | Meaning                                                             |
|-----------------------------------|---------------------------------------------------------------------|
| `prepend-non-duplicate`           | Prepend `<prefix>/<suffix>` to `$VAR` (skip if already present)     |
| `prepend-non-duplicate-if-exists` | Same, but only if the path exists on disk                           |
| `append-non-duplicate`            | Append instead of prepend                                           |
| `set`                             | Set `$VAR = <suffix>` (override)                                    |
| `set-if-unset`                    | Set `$VAR = <suffix>` only if not already set                       |
| `source`                          | Source a shell script (used in shell-level DSV, not in env builder) |

### Rust implementation

```rust
/// A parsed DSV entry.
pub struct DsvEntry {
    pub action: DsvAction,
    pub variable: String,
    pub suffix: String,  // empty = the prefix itself
}

pub enum DsvAction {
    PrependNonDuplicate,
    PrependNonDuplicateIfExists,
    AppendNonDuplicate,
    Set,
    SetIfUnset,
    Source,  // skipped in env builder (shell-only)
}

/// Build the environment for a package by reading its deps' DSV files.
///
/// For each dep (in topological order), reads `local_setup.dsv` and
/// `package.dsv`, then applies each entry to the env HashMap.
pub fn build_package_env(
    dep_install_paths: &[(String, PathBuf)],
    base_env: &HashMap<String, String>,
) -> HashMap<String, String> {
    let mut env = base_env.clone();

    for (dep_name, install_path) in dep_install_paths {
        // Read local_setup.dsv (ament hooks: AMENT_PREFIX_PATH, PATH, LD_LIBRARY_PATH)
        let local_dsv = install_path.join("share").join(dep_name).join("local_setup.dsv");
        apply_dsv_file(&local_dsv, install_path, &mut env);

        // Read package.dsv (colcon hooks: CMAKE_PREFIX_PATH, PKG_CONFIG_PATH)
        let pkg_dsv = install_path.join("share").join(dep_name).join("package.dsv");
        apply_dsv_file(&pkg_dsv, install_path, &mut env);
    }

    env
}

fn apply_dsv_file(
    dsv_path: &Path,
    prefix: &Path,
    env: &mut HashMap<String, String>,
) {
    // Read and parse each line
    // For `source` entries: read the referenced .dsv file recursively
    // For other entries: apply the action to env
}
```

This replaces the 25-second bash subprocess with a ~1ms Rust operation.

---

## 2. Incremental Builds via Fingerprinting

### What to fingerprint per package

| Input                             | How to hash                                    | Why                                     |
|-----------------------------------|------------------------------------------------|-----------------------------------------|
| `package.xml`                     | SHA-256 of content                             | Dep changes trigger rebuild             |
| `CMakeLists.txt`                  | SHA-256 of content                             | Build config changes                    |
| Source files (`src/`, `include/`) | SHA-256 of each file's mtime+size              | Faster than full content hash           |
| `rosup.toml` build config         | Hash of `cmake_args`, `symlink_install`        | Config changes trigger rebuild          |
| Dependency fingerprints           | Hash of dep install paths + their fingerprints | Dep rebuild triggers downstream rebuild |

### Fingerprint file

Stored at `build/<pkg>/.rosup_fingerprint`:

```toml
[fingerprint]
package_xml = "sha256:abc123..."
cmake_lists = "sha256:def456..."
source_hash = "sha256:789..."
build_config = "sha256:..."
dep_hash = "sha256:..."
timestamp = "2026-04-02T12:00:00Z"
```

### Build decision

```rust
fn should_rebuild(pkg: &PackageNode, build_base: &Path) -> bool {
    let fp_path = build_base.join(&pkg.name).join(".rosup_fingerprint");
    let old_fp = Fingerprint::load(&fp_path);
    let new_fp = Fingerprint::compute(pkg);

    match old_fp {
        None => true,      // never built
        Some(old) => old != new,  // changed
    }
}
```

### Interaction with cmake's own caching

cmake has its own incremental build via `CMakeCache.txt` and dependency
tracking. Our fingerprint is a **coarser gate**: if the fingerprint
hasn't changed, we skip the entire cmake invocation (configure + build +
install). If it has changed, we run cmake which then uses its own fine-
grained tracking to do minimal work.

```
rosup fingerprint unchanged → skip entirely (0ms)
rosup fingerprint changed → run cmake → cmake skips unchanged targets (~0.1s)
```

This two-level approach gives us the best of both worlds.

---

## 3. Plugin System for 3rd Party Build Types

### Task dispatch order

```rust
fn get_build_task(build_type: &str) -> Box<dyn BuildTask> {
    match build_type {
        // 1. Built-in tasks (compiled into rosup)
        "ament_cmake" | "ros.ament_cmake" => Box::new(AmentCmakeBuildTask),
        "ament_python" | "ros.ament_python" => Box::new(AmentPythonBuildTask),
        "cmake" => Box::new(CmakeBuildTask),

        // 2. Subprocess plugins (rosup-task-<type> on $PATH)
        _ if find_subprocess_plugin(build_type).is_some() => {
            Box::new(SubprocessTask::new(build_type))
        }

        // 3. PyO3 bridge (colcon Python plugins)
        #[cfg(feature = "pyo3")]
        _ if find_pyo3_plugin(build_type).is_some() => {
            Box::new(PyO3Task::new(build_type))
        }

        // 4. Error
        _ => panic!("unsupported build type: {build_type}"),
    }
}
```

### Built-in tasks share a common post-install step

After any built-in task completes cmake/pip install, a shared
`post_install()` function generates the colcon hook layer:

```rust
fn post_install(ctx: &BuildContext) -> Result<()> {
    // Generate hook/ directory
    generate_colcon_hooks(&ctx.install_base, &ctx.pkg_name)?;
    // Write colcon-core/packages/<name>
    write_runtime_deps(&ctx.install_base, &ctx.pkg_name, &ctx.runtime_deps)?;
    // Write ament index if not already done by cmake
    ensure_ament_index(&ctx.install_base, &ctx.pkg_name)?;
    // Write .catkin marker
    write_catkin_marker(&ctx.install_base)?;
    // Update fingerprint
    Fingerprint::save(&ctx.build_base, &ctx.pkg_name, &new_fingerprint)?;
    Ok(())
}
```

### Subprocess plugin protocol

External plugins are standalone binaries. rosup passes build context
as JSON on stdin:

```json
{
  "action": "build",
  "pkg_name": "my_rust_pkg",
  "source_path": "/ws/src/my_rust_pkg",
  "build_base": "/ws/build/my_rust_pkg",
  "install_base": "/ws/install/my_rust_pkg",
  "symlink_install": true,
  "env": {
    "CMAKE_PREFIX_PATH": "/ws/install/dep_a:/opt/ros/humble",
    "AMENT_PREFIX_PATH": "..."
  }
}
```

The plugin does its work (cargo build, etc.) and exits with 0/1.
rosup runs `post_install()` afterwards to generate hooks.

### PyO3 plugin bridge

For existing colcon Python plugins (e.g. `ros.ament_cargo`), rosup
loads them via PyO3:

```rust
fn build_via_pyo3(build_type: &str, ctx: &BuildContext) -> Result<()> {
    Python::with_gil(|py| {
        // Load plugin class from entry points
        let task = load_colcon_plugin(py, build_type)?;
        // Construct TaskContext
        let context = build_task_context(py, ctx)?;
        task.call_method1("set_context", (context,))?;
        // Run build synchronously
        let asyncio = py.import("asyncio")?;
        asyncio.call_method1("run", (task.call_method0("build")?,))?;
        Ok(())
    })?;
    // Run our post_install to add colcon hooks
    post_install(ctx)
}
```

---

## 4. Parallel Executor

### Design

```rust
pub struct Executor {
    max_workers: usize,
    on_error: OnError,
}

pub enum OnError {
    Stop,           // default: stop on first failure
    SkipDownstream, // --continue-on-error: skip dependents of failed
}

impl Executor {
    pub fn run(
        &self,
        graph: &mut BuildGraph,
        build_fn: impl Fn(&PackageNode) -> Result<()> + Send + Sync,
    ) -> Vec<BuildResult> {
        let pool = ThreadPool::new(self.max_workers);
        let (tx, rx) = channel();
        let mut results = Vec::new();

        loop {
            // Get packages with all deps satisfied
            let ready = graph.ready();
            if ready.is_empty() && graph.is_empty() {
                break;
            }

            // Spawn ready packages
            for pkg in ready {
                let tx = tx.clone();
                let pkg = pkg.clone();
                pool.execute(move || {
                    let result = build_fn(&pkg);
                    tx.send((pkg.name.clone(), result)).unwrap();
                });
            }

            // Wait for any completion
            let (name, result) = rx.recv().unwrap();
            match &result {
                Ok(()) => {
                    graph.mark_done(&name);
                }
                Err(e) => {
                    match self.on_error {
                        OnError::Stop => {
                            results.push(BuildResult::Failed(name, e));
                            break;
                        }
                        OnError::SkipDownstream => {
                            // Remove this package and all dependents
                            graph.mark_failed(&name);
                            results.push(BuildResult::Failed(name, e));
                        }
                    }
                }
            }
            results.push(BuildResult::Success(name));
        }

        results
    }
}
```

### Progress output

```
[1/455] Building autoware_cmake...
[2/455] Building autoware_lint_common...
[3/455] Building autoware_utils_math... (parallel with 2)
...
[455/455] Building autoware_launch...

Summary: 455 packages finished [42.3s] (23 cached)
```

Packages skipped by fingerprint show as "cached" in the summary.

---

## 5. Top-Level Install Scripts

After all packages are built, generate the scripts that users source
to activate the workspace (`source install/setup.bash`).

### Why dynamic discovery is required

Both ament and colcon use **dynamic package discovery at source-time**:

- **Ament** (`/opt/ros/humble/`): `_local_setup_util.py` scans
  `share/ament_index/resource_index/packages/` for marker files.
  When `apt install ros-humble-nav2-core` adds a new marker, the next
  `source setup.bash` picks it up automatically.

- **Colcon** (workspace `install/`): `_local_setup_util_sh.py` scans
  `share/colcon-core/packages/` or `<pkg>/share/colcon-core/packages/<pkg>`.
  Manually building and installing a new package into the prefix is
  picked up on next source.

A static shell script that hardcodes `source pkg_a/package.sh &&
source pkg_b/package.sh` would **lose this dynamic discovery**. Users
who install additional packages after the build would not see them.

### Decision: Python helper for source-time, DSV parser for build-time

| Context                                         | Method             | Dynamic?              | Performance |
|-------------------------------------------------|--------------------|-----------------------|-------------|
| **Build-time** env setup (step 5a)              | DSV parser in Rust | No (exact deps known) | ~1ms        |
| **Source-time** env setup (`source setup.bash`) | Python helper      | Yes (scans directory) | ~0.5s       |

The DSV parser handles build-time (where performance matters and deps
are known). The Python helper handles source-time (where dynamic
discovery matters and the ~0.5s cost is acceptable for a one-time
interactive operation).

### Implementation

- Copy colcon's `_local_setup_util_sh.py` to `install/` unchanged.
  This guarantees compatibility with colcon's ecosystem (e.g.
  `colcon test-result`, IDE integrations that source the workspace).
- Generate `install/setup.{bash,sh,zsh}` — sources chained prefixes
  then calls `local_setup`.
- Generate `install/local_setup.{bash,sh,zsh}` — invokes the Python
  helper to get topological ordering.
- Generate `install/.colcon_install_layout` containing `"isolated"`.
- Generate `install/COLCON_IGNORE`.

---

## 6. Full Build Flow

```
rosup build
  │
  ├─ Load rosup.toml
  ├─ Discover packages (colcon_scan)
  ├─ Parse package.xml → PackageNode (name, build_type, deps)
  ├─ Apply selection (--packages-select/skip/up-to)
  ├─ Build dependency graph (BuildGraph)
  ├─ Load base env (source overlays via DSV)
  │
  ├─ For each package (parallel, respecting deps):
  │   ├─ Check fingerprint → skip if unchanged
  │   ├─ Build env from deps' DSV files (Rust, ~1ms)
  │   ├─ Dispatch to build task:
  │   │   ├─ ament_cmake: cmake configure → build → install
  │   │   ├─ ament_python: pip install
  │   │   └─ others: subprocess or PyO3
  │   ├─ Post-install: generate hook/ + colcon-core/packages/
  │   └─ Save fingerprint
  │
  ├─ Generate top-level setup.bash (static, no Python)
  └─ Print summary (built/cached/failed/skipped)
```

---

## Implementation Order

| Phase | What                                 | Deps    | Effort |
|-------|--------------------------------------|---------|--------|
| A     | DSV parser + env builder             | none    | Small  |
| B     | Fingerprint + skip logic             | none    | Medium |
| C     | Colcon hook generation (`hook/` dir) | A       | Small  |
| D     | Top-level install scripts            | A, C    | Medium |
| E     | Wire into executor loop              | A, B, C | Medium |
| F     | Parallel executor                    | E       | Medium |
| G     | Subprocess plugin protocol           | E       | Small  |
| H     | PyO3 bridge                          | E       | Medium |

Start with A (DSV parser) — it unblocks everything else and delivers
the biggest performance win immediately.
