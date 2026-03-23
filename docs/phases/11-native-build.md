# Phase 11 — Native Build Engine

Replace colcon delegation with a native Rust build engine. Maintain full
compatibility with colcon's output format (ament index, environment scripts,
install layout) and support existing Python plugins via PyO3 fallback.

See `docs/design/colcon-rewrite.md` and `docs/design/pyo3-plugin-bridge.md`
for background research.

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

## 11.5 ament_python task

**File:** `crates/rosup-core/src/build/task/ament_python.rs` (new)

- [ ] Install: `pip install [--editable] . --prefix <install>`.
- [ ] Copy `package.xml` to `<install>/share/<name>/package.xml`.
- [ ] Write ament index marker.
- [ ] Generate environment scripts.

### Acceptance criteria

- [ ] Build a real ament_python package natively.
- [ ] Python launch files and nodes are runnable.

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
