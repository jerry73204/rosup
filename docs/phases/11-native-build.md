# Phase 11 — Native Build Engine

Replace colcon delegation with a native Rust build engine. Maintain full
compatibility with colcon's output format (ament index, environment scripts,
install layout) and support existing Python plugins via PyO3 fallback.

See design docs:
- `docs/design/native-builder.md` — full architecture with DSV parser,
  fingerprinting, plugin system, and parallel executor.
- `docs/design/builder-architecture.md` — how colcon builds packages
  (step-by-step trace with performance measurements).
- `docs/design/colcon-install-layout.md` — what files colcon generates
  (ament layer vs colcon layer).
- `docs/design/colcon-rewrite.md` — scope and compatibility strategy.
- `docs/design/pyo3-plugin-bridge.md` — PyO3 feasibility study.

---

## 11.1 Dependency graph and topological ordering — DONE

**Files:** `crates/rosup-core/src/build/topo.rs`

- [x] `PackageNode` struct: name, path, build_type, build_deps, all_deps.
- [x] `BuildGraph` struct: nodes, in-degree map, reverse adjacency.
- [x] `BuildGraph::new(nodes) -> Result<BuildGraph>`: validates no cycles.
- [x] `BuildGraph::ready() -> Vec<&PackageNode>`: packages with in-degree 0,
  sorted alphabetically.
- [x] `BuildGraph::mark_done(name)`: decrements dependents' in-degree.
- [x] `BuildGraph::drain_sequential()`: full sequential drain.
- [x] External deps silently ignored (only workspace members tracked).
- [x] 8 unit tests: linear chain, diamond, parallel roots, wide graph,
  cycle detection, external deps, mark_done releases dependents,
  complex multi-layer graph.

---

## 11.2 Package selection — DONE

**File:** `crates/rosup-core/src/build/selection.rs`

- [x] `SelectionFilter` struct: `select`, `skip`, `up_to` fields.
- [x] `apply_selection(packages, filter) -> Vec<PackageNode>`.
- [x] `select` → only named packages (unknown names silently ignored).
- [x] `skip` → everything except named packages.
- [x] `up_to` → named packages + transitive dep closure (BFS).
- [x] `up_to` + `skip` combination works.
- [x] Preserves input order.
- [x] 8 unit tests: no filter, select, skip, up_to transitive, up_to + skip,
  unknown package, diamond deps, order preservation.

---

## 11.3 Ament index and environment scripts — DONE

**Files:** `crates/rosup-core/src/build/ament_index.rs`,
`crates/rosup-core/src/build/environment.rs`

### Ament index (`ament_index.rs`)

- [x] `write_package_marker(install_base, name)` — writes ament index
  marker at `share/ament_index/resource_index/packages/<name>`.
- [x] `write_runtime_deps(install_base, name, deps)` — writes colcon
  deps file at `share/colcon-core/packages/<name>` (colon-separated).

### Environment scripts (`environment.rs`)

- [x] `generate_environment_scripts(install_base, name, python_version)`.
- [x] Per-hook DSV + shell files: `ament_prefix_path`, `library_path`,
  `path`, and optional `pythonpath`.
- [x] `local_setup.dsv` — lists all hooks.
- [x] `local_setup.sh` — defines `ament_prepend_unique_value` and sources
  hooks (compatible with colcon's ament_package template).
- [x] `local_setup.bash` — bash wrapper.
- [x] `local_setup.zsh` — zsh wrapper.
- [x] `package.dsv` — references local_setup files.
- [x] DSV format matches colcon: `prepend-non-duplicate;VAR;suffix`.
- [x] 12 unit tests covering all generated files and formats.

---

## 11.4 BuildTask trait and ament_cmake task — DONE

**Files:** `crates/rosup-core/src/build/task/mod.rs`,
`crates/rosup-core/src/build/task/ament_cmake.rs`

### BuildTask trait

- [x] `BuildTask` trait: `build(&self, ctx) -> Result<()>` and
  `test(&self, ctx) -> Result<()>`.
- [x] `BuildContext` struct: pkg_name, source_path, build_base, install_base,
  symlink_install, cmake_args, parallel_jobs, dep_install_paths, env,
  python_version, runtime_deps.
- [x] `run_command()` helper: runs subprocess with env and cwd, inherits
  stdio, returns error on non-zero exit.
- [x] `TaskError` enum: CommandFailed, Io, Other.

### ament_cmake task

- [x] Configure: `cmake <source> -B<build> -DCMAKE_INSTALL_PREFIX=<install>
  -DCMAKE_PREFIX_PATH=<deps> -DAMENT_CMAKE_SYMLINK_INSTALL=1
  -DBUILD_TESTING=OFF [cmake_args]`.
- [x] Build: `cmake --build <build> --parallel <jobs>`.
- [x] Install: `cmake --install <build>`.
- [x] Copy `package.xml` to `share/<name>/package.xml`.
- [x] Write ament index + colcon runtime deps + environment scripts.
- [x] Incremental: skip configure if `CMakeCache.txt` exists.
- [x] Test: runs `ctest --test-dir <build> --output-on-failure`.
- [x] Integration tests (marked `#[ignore]` — require sourced ROS env):
  build minimal package, verify install layout, verify incremental skip.

---

## 11.5 ament_python task — DONE

**File:** `crates/rosup-core/src/build/task/ament_python.rs`

- [x] Install: `python3 -m pip install --no-deps --prefix <install> .`
  Uses `--editable` when `symlink_install` is true.
- [x] Copy `package.xml` to `share/<name>/package.xml` (symlink if
  `symlink_install`).
- [x] Write ament index marker (idempotent — some packages do it via
  setup.py `data_files`).
- [x] Write colcon runtime deps file.
- [x] Generate environment scripts including PYTHONPATH hook
  (`local/lib/python<ver>/dist-packages`).
- [x] Test: runs `python3 -m pytest` with PYTHONPATH set.
- [x] Integration test (marked `#[ignore]`): build minimal ament_python
  package, verify install layout + pythonpath.dsv.

---

## 11.5b Colcon hook layer (post-install)

**Files:** `crates/rosup-core/src/build/environment.rs` (extend),
`crates/rosup-core/src/build/install_scripts.rs` (new)

After `cmake --install` runs, ament_cmake generates the `environment/`
hooks and `local_setup.*` files. But colcon adds a second layer of hooks
in `hook/` plus top-level install scripts. See
`docs/design/colcon-install-layout.md` for the full comparison.

### Per-package colcon hooks (`hook/` directory)

These are generated by colcon plugins AFTER cmake install. rosup's native
builder needs to replicate them.

- [ ] `hook/cmake_prefix_path.{dsv,sh}` — `prepend-non-duplicate;CMAKE_PREFIX_PATH;`
  Only generated if install dir contains `*Config.cmake` or `*-config.cmake`.
- [ ] `hook/pkg_config_path.{dsv,sh}` — `prepend-non-duplicate;PKG_CONFIG_PATH;lib/pkgconfig`
- [ ] `hook/pkg_config_path_multiarch.{dsv,sh}` — multiarch variant.
  Suffix from `gcc -print-multiarch` (e.g. `lib/x86_64-linux-gnu/pkgconfig`).
- [ ] `hook/ros_package_path.{dsv,sh}` — `prepend-non-duplicate;ROS_PACKAGE_PATH;share`
- [ ] Update `package.dsv` to reference `hook/` files (currently only
  references `local_setup.*`).
- [ ] `.catkin` — empty marker file at install root.

### Additional ament index entries

- [ ] `share/ament_index/resource_index/parent_prefix_path/<name>` —
  value is the `AMENT_PREFIX_PATH` at build time (chained underlays).
- [ ] `share/ament_index/resource_index/package_run_dependencies/<name>` —
  empty for ament_cmake packages.

### Top-level install scripts

Generated once for the entire install prefix, not per-package. Colcon uses
a Python helper (`_local_setup_util_sh.py`) to discover packages from
`colcon-core/packages/` files and source them in topological order.

Two implementation approaches:

**A. Ship colcon's `_local_setup_util_sh.py` as-is** — copy the ~200-line
Python script into the install dir. The shell scripts call it via
`python3`. This is what colcon does and guarantees compatibility.

**B. Generate shell scripts directly from Rust** — rosup already has the
topological sort. Instead of the Python helper, generate a static
`local_setup.bash` that sources packages in the correct order. No Python
needed at runtime.

**C. Use PyO3 to run the Python helper** — call `_local_setup_util_sh.py`
from Rust via PyO3 and capture its output to generate the shell scripts.
Same result as A but integrated into rosup's process.

**Decision: Option A for compatibility, with Option B as a future
optimisation.** Ship the Python script unchanged so `source install/setup.bash`
works exactly like colcon's output. Users who don't have Python at runtime
can use Option B (a `--no-python-scripts` flag).

- [ ] Copy `_local_setup_util_sh.py` to `install/`.
- [ ] Generate `install/setup.{bash,sh,zsh}` — sources chained prefixes
  then `local_setup`.
- [ ] Generate `install/local_setup.{bash,sh,zsh}` — calls the Python
  helper to get topological ordering.
- [ ] Generate `install/.colcon_install_layout` containing `"isolated"`.
- [ ] Generate `install/COLCON_IGNORE` — prevents colcon from scanning
  the install directory.

### Acceptance criteria

- [ ] `source install/setup.bash` on a rosup-built workspace produces
  identical `AMENT_PREFIX_PATH`, `CMAKE_PREFIX_PATH`, `PATH`,
  `LD_LIBRARY_PATH`, `PKG_CONFIG_PATH` as colcon-built.
- [ ] `ros2 run <pkg> <node>` works after sourcing.

---

## 11.5c DSV parser and per-package environment builder

**Files:** `crates/rosup-core/src/build/dsv.rs` (new),
`crates/rosup-core/src/build/task/mod.rs` (extend `BuildContext`)

Parse DSV files natively in Rust to construct the build environment for
each package. Eliminates the 25-second bash subprocess overhead that
colcon incurs per build.

See `docs/design/native-builder.md` section 1.

### DSV parser

- [ ] `DsvEntry` struct: `action`, `variable`, `suffix`.
- [ ] `DsvAction` enum: `PrependNonDuplicate`, `PrependNonDuplicateIfExists`,
  `AppendNonDuplicate`, `Set`, `SetIfUnset`, `Source`.
- [ ] `parse_dsv_file(path) -> Vec<DsvEntry>` — parse semicolon-separated lines.
- [ ] Handle `source` entries: read the referenced `.dsv` file recursively
  (not shell files — only DSV-to-DSV references).

### Environment builder

- [ ] `build_package_env(dep_install_paths, base_env) -> HashMap<String, String>`
- [ ] For each dep (in topological order), read `local_setup.dsv` and
  `package.dsv`, apply each entry to the env HashMap.
- [ ] `prepend-non-duplicate`: prepend `<prefix>/<suffix>` to `$VAR` with
  colon separator, skip if already present.
- [ ] `prepend-non-duplicate-if-exists`: same but check path exists first.
- [ ] `set`: override `$VAR = <value>`.
- [ ] `set-if-unset`: set only if not already in env.

### Acceptance criteria

- [ ] `BuildContext.env` contains correct `CMAKE_PREFIX_PATH`,
  `AMENT_PREFIX_PATH`, `LD_LIBRARY_PATH`, `PKG_CONFIG_PATH`, `PYTHONPATH`.
- [ ] `find_package(<dep>)` works in cmake for all resolved deps.
- [ ] Diff test: DSV-built env matches bash-sourced env for a real
  workspace (Autoware).
- [ ] Performance: env build < 5ms for 455-package workspace (vs 25s).

---

## 11.5d Incremental builds via fingerprinting

**File:** `crates/rosup-core/src/build/fingerprint.rs` (new)

Skip packages whose inputs haven't changed since the last successful
build. Two-level caching: rosup fingerprint (coarse, skips cmake
entirely) + cmake's own tracking (fine-grained, skips unchanged targets).

See `docs/design/native-builder.md` section 2.

### Fingerprint inputs

- [ ] `package.xml` content hash (dep changes trigger rebuild).
- [ ] `CMakeLists.txt` content hash (build config changes).
- [ ] Source file mtimes + sizes (faster than full content hash).
- [ ] Build config hash (cmake_args, symlink_install from rosup.toml).
- [ ] Dependency fingerprint hash (dep rebuild triggers downstream).

### Fingerprint storage

- [ ] `build/<pkg>/.rosup_fingerprint` — TOML file with per-input hashes.
- [ ] `Fingerprint::compute(pkg) -> Fingerprint` — walk source tree.
- [ ] `Fingerprint::load(path) -> Option<Fingerprint>` — read from disk.
- [ ] `should_rebuild(pkg, build_base) -> bool` — compare old vs new.

### Build decision logic

```
fingerprint unchanged → skip entirely (0ms)
fingerprint changed → run cmake → cmake skips unchanged targets (~0.1s)
no previous fingerprint → full build
```

### Acceptance criteria

- [ ] Second `rosup build` (no changes) skips all packages and finishes
  in < 1s for a 5-package workspace.
- [ ] Changing one source file rebuilds only that package + dependents.
- [ ] Changing `rosup.toml` cmake-args triggers rebuild of all packages.
- [ ] Adding a new dep to `package.xml` triggers rebuild.

---

## 11.6 Parallel executor

**File:** `crates/rosup-core/src/build/executor.rs` (new)

Dep-aware parallel job scheduler.

- [ ] `ParallelExecutor::execute(jobs, max_workers, on_error)`.
- [ ] Jobs are spawned when all deps are complete.
- [ ] `max_workers` limits concurrency (default: CPU count).
- [ ] `on_error` modes: `stop` (default), `continue` (`--continue-on-error`).
- [ ] Progress reporting: package name + status (building/done/failed).
- [ ] Use tokio or std threads (no Python GIL involved for built-in tasks).

### Acceptance criteria

- [ ] Builds Autoware (455 packages) faster than colcon (or equal).
- [ ] `--continue-on-error` correctly skips dependents of failed packages.
- [ ] Deterministic output order for reproducibility.

---

## 11.7 PyO3 plugin bridge

**File:** `crates/rosup-core/src/build/task/pyo3_bridge.rs` (new)

Load existing colcon Python plugins for package types not handled natively.

- [ ] Discover plugins via `importlib.metadata.entry_points(group=
  "colcon_core.task.build")`.
- [ ] Construct `PackageDescriptor`, `argparse.Namespace`, `TaskContext`
  from Rust data via PyO3.
- [ ] Implement `RosupTaskContext.put_event_into_queue()` — forward to
  Rust progress callback.
- [ ] Call `asyncio.run(task.build())` synchronously from Rust.
- [ ] Map Python exceptions to `BuildError`.
- [ ] Compile as optional feature: `[features] pyo3 = ["dep:pyo3"]`.
- [ ] Without the feature, step 3 in task resolution is skipped.

### Acceptance criteria

- [ ] `rosup build` on a workspace with `ros.ament_cargo` packages uses
  the colcon-cargo-ros2 plugin transparently.
- [ ] `rosup build` without Python installed skips PyO3 and errors
  clearly for unsupported build types.

---

## 11.8 Subprocess plugin protocol

**File:** `crates/rosup-core/src/build/task/subprocess.rs` (new)

For future native plugins that don't need Python.

- [ ] Define JSON protocol: `BuildRequest` → stdin, exit code → result.
- [ ] Discover `rosup-task-<build_type>` on `$PATH`.
- [ ] Spawn subprocess, pipe JSON, collect exit code.
- [ ] Document the protocol for plugin authors.

### Acceptance criteria

- [ ] A minimal `rosup-task-echo` test plugin works end-to-end.
- [ ] Protocol is documented in `docs/design/`.

---

## 11.9 Integration: wire into `rosup build`

**File:** `crates/rosup-cli/src/main.rs`

Replace the current colcon delegation with the native engine.

- [ ] Add `--native` / `--colcon` flags (default: `--native`).
- [ ] `--native`: discovery → topo sort → selection → executor → tasks.
- [ ] `--colcon`: current behavior (preserved as escape hatch).
- [ ] Task resolution: built-in → subprocess → PyO3 → error.
- [ ] Stale dep cleanup (11.4 already handles ament index).
- [ ] `--continue-on-error` and `--merge-install` exposed as CLI flags.

### Acceptance criteria

- [ ] `rosup build --native` produces identical output to `rosup build --colcon`
  on the Autoware workspace.
- [ ] `rosup build --native` is faster (measured with `hyperfine`).
- [ ] All existing integration tests pass with both `--native` and `--colcon`.

---

## 11.10 Testing infrastructure and compatibility suite

### Test levels

Tests are split into tiers based on what they need to run:

| Tier | What | Deps | Run with |
|------|------|------|----------|
| **Unit** | Pure logic: topo sort, selection, ament_index, env scripts | None | `just test-unit` |
| **Integration** | Spawn rosup binary, use shims | Shims built | `just test-integration` |
| **ROS** | Invoke cmake/colcon, need ament_cmake | Sourced ROS env | `just test-ros` |
| **Workspace** | Full workspace build, compare to colcon | Real workspace | `just test-workspace <path>` |

### nextest configuration (`.config/nextest.toml`)

- [x] `integration` test group — rosup-tests crate, max 8 threads.
- [x] `native-build` test group — ament_cmake tests, max 1 thread,
  60s timeout.
- [x] `#[ignore]` for ROS-dependent tests — `just test-ros` runs them
  with `--run-ignored ignored-only`.

### justfile recipes

- [x] `just test-unit` — unit tests only (no external deps).
- [x] `just test-integration` — builds shims + rosup, runs rosup-tests.
- [x] `just test` — unit + integration (fast, no ROS needed).
- [x] `just test-ros` — ROS-dependent tests (requires sourced env).
- [x] `just test-all` — CI + ROS tests.
- [x] `just test-workspace <path>` — test against real workspace.

### Unit tests per module (already done)

| Module | Tests | Coverage |
|--------|-------|----------|
| `build/topo.rs` | 8 | linear, diamond, parallel, cycle, external deps |
| `build/selection.rs` | 8 | select, skip, up_to, combinations, order |
| `build/ament_index.rs` | 3 | marker, runtime deps, empty deps |
| `build/environment.rs` | 9 | all scripts and DSV formats |
| `build/task/ament_cmake.rs` | 2 `#[ignore]` | full build, incremental |

### Compatibility tests (TODO)

- [ ] `crates/rosup-tests/tests/native_build.rs`: build a minimal
  ament_cmake package with `--native`, then source the install and
  verify `AMENT_PREFIX_PATH` matches colcon's output.
- [ ] Build a minimal ament_python package natively, verify install layout.
- [ ] Build a multi-package workspace, verify topological order matches
  `colcon list --topological-order`.
- [ ] Build with `--symlink-install`, verify symlinks created.
- [ ] Build with `--continue-on-error`, verify failed pkg doesn't block
  independent packages.
- [ ] Test PyO3 bridge with a mock Python plugin (requires Python).
- [ ] Diff test: source colcon's install vs rosup's install, compare
  `AMENT_PREFIX_PATH`, `PATH`, `LD_LIBRARY_PATH`, `PYTHONPATH`.

### Workspace smoke tests

- [ ] Autoware 1.5.0 (455 packages) builds identically with
  `--native` and `--colcon`.
- [ ] AutoSDV with excludes builds correctly.
- [ ] cuda_ndt_matcher with mixed Rust+ROS builds correctly.

---

## Rollout strategy

```
Phase 11.1–11.3  │  Foundation: topo sort, selection, env scripts
                  │  No user-visible change yet. Tested in isolation.
                  │
Phase 11.4–11.5  │  Core tasks: ament_cmake, ament_python
                  │  Can build real packages. Test with `just test-ros`.
                  │
Phase 11.6       │  Executor: parallel builds
                  │  Feature-complete. Stress test with Autoware.
                  │
Phase 11.7       │  PyO3 bridge: existing plugin compat
                  │  Test with colcon-cargo-ros2 in cuda_ndt_matcher.
                  │
Phase 11.8       │  Subprocess protocol: future-proof plugin system
                  │  Test with a minimal rosup-task-echo binary.
                  │
Phase 11.9       │  CLI integration: --native flag
                  │  All justfile test recipes must pass.
                  │
Phase 11.10      │  Compat tests + diff against colcon
                  │  Make --native the default.
```

### Milestone: default to `--native`

Switch the default from `--colcon` to `--native` when:

1. `just test-all` passes (unit + integration + ROS)
2. `just test-workspace ~/repos/autoware/1.5.0-ws --distro humble` passes
3. PyO3 bridge handles `ros.ament_cargo` in cuda_ndt_matcher
4. Diff test shows identical env after sourcing rosup vs colcon install
