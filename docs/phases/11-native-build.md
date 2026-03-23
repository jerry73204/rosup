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

## 11.3 Ament index and environment scripts

**File:** `crates/rosup-core/src/build/ament_index.rs` (new),
`crates/rosup-core/src/build/environment.rs` (new)

Generate the ament index entries and environment hook scripts that colcon
produces after a package is installed.

### Ament index

- [ ] Write `<install>/share/ament_index/resource_index/packages/<name>`
  (empty marker file).
- [ ] Write `<install>/share/colcon-core/packages/<name>` containing
  colon-separated runtime dependency names.

### Environment scripts

- [ ] Generate `<install>/share/<name>/local_setup.bash` — sets
  `AMENT_PREFIX_PATH`, `PATH`, `LD_LIBRARY_PATH`, `PYTHONPATH`,
  `CMAKE_PREFIX_PATH` for this package.
- [ ] Generate `local_setup.sh` (POSIX sh), `local_setup.zsh`.
- [ ] Generate `package.dsv` (declarative setup) — colcon's internal
  format for chaining environment hooks.
- [ ] Generate `package.bash`, `package.sh`, `package.zsh` — full package
  scripts that source the runtime deps then local_setup.

### Acceptance criteria

- [ ] On a built package, `diff` rosup's generated scripts vs colcon's.
  They must produce identical `AMENT_PREFIX_PATH` after sourcing.
- [ ] `ros2 run <pkg> <node>` works after sourcing rosup's install scripts.

---

## 11.4 BuildTask trait and ament_cmake task

**Files:** `crates/rosup-core/src/build/task/mod.rs` (new),
`crates/rosup-core/src/build/task/ament_cmake.rs` (new)

### BuildTask trait

- [ ] Define the trait:
  ```rust
  pub trait BuildTask: Send + Sync {
      fn build(&self, ctx: &BuildContext) -> Result<()>;
      fn test(&self, ctx: &TestContext) -> Result<()>;
  }
  ```
- [ ] `BuildContext` struct: pkg name, source path, build base, install base,
  symlink_install, cmake_args, dep install paths, env vars.

### ament_cmake task

- [ ] Configure: `cmake <source> -B <build> -DCMAKE_INSTALL_PREFIX=<install>
  -DCMAKE_PREFIX_PATH=<deps> [cmake_args]`
- [ ] If `symlink_install`: add `-DAMENT_CMAKE_SYMLINK_INSTALL=1`.
- [ ] Build: `cmake --build <build> --parallel <jobs>`.
- [ ] Install: `cmake --install <build>`.
- [ ] Call ament_index + environment script generation from 11.3.
- [ ] Incremental: skip configure if `CMakeCache.txt` exists and source
  hasn't changed (match colcon's behavior).

### Acceptance criteria

- [ ] Build a real ament_cmake package (e.g. `autoware_core`) natively.
- [ ] Installed files match colcon's output layout.
- [ ] `ros2 run` works on the built package.

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

## 11.10 Compatibility test suite

**File:** `crates/rosup-tests/tests/native_build.rs` (new)

Verify colcon-compatible output.

- [ ] Build a minimal ament_cmake package natively, verify ament index
  and env scripts match colcon.
- [ ] Build a minimal ament_python package natively, verify install layout.
- [ ] Build a multi-package workspace, verify topological order.
- [ ] Build with `--symlink-install`, verify symlinks created.
- [ ] Build with `--continue-on-error`, verify failed pkg doesn't block
  independent packages.
- [ ] Test PyO3 bridge with a mock Python plugin.

### Acceptance criteria

- [ ] Test suite runs in CI alongside existing tests.
- [ ] Autoware workspace builds identically with `--native` and `--colcon`.

---

## Rollout strategy

```
Phase 11.1–11.3  │  Foundation: topo sort, selection, env scripts
                  │  No user-visible change yet. Can test in isolation.
                  │
Phase 11.4–11.5  │  Core tasks: ament_cmake, ament_python
                  │  Can build real packages but not wired into CLI.
                  │
Phase 11.6       │  Executor: parallel builds
                  │  Feature-complete for built-in types.
                  │
Phase 11.7       │  PyO3 bridge: existing plugin compat
                  │  All package types supported.
                  │
Phase 11.8       │  Subprocess protocol: future-proof plugin system
                  │  Optional, not blocking.
                  │
Phase 11.9       │  CLI integration: --native flag
                  │  User can opt in to native builds.
                  │
Phase 11.10      │  Compat tests: verify against colcon
                  │  Make --native the default.
```

### Milestone: default to `--native`

Switch the default from `--colcon` to `--native` when:

1. Autoware (455 packages) builds identically with both
2. All integration tests pass with `--native`
3. PyO3 bridge handles `ros.ament_cargo` correctly
4. No user-reported regressions after 2 weeks of opt-in testing
