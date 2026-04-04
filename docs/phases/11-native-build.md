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

## 11.5b Colcon hook layer (post-install) — DONE

**Files:** `crates/rosup-core/src/build/post_install.rs`,
`crates/rosup-core/src/build/install_scripts.rs` (new)

After `cmake --install` runs, ament_cmake generates the `environment/`
hooks and `local_setup.*` files. But colcon adds a second layer of hooks
in `hook/` plus top-level install scripts. See
`docs/design/colcon-install-layout.md` for the full comparison.

### Per-package colcon hooks (`hook/` directory)

These are generated by colcon plugins AFTER cmake install. rosup's native
builder replicates them in `post_install.rs`.

- [x] `hook/cmake_prefix_path.{dsv,sh}` — `prepend-non-duplicate;CMAKE_PREFIX_PATH;`
  Only generated if install dir contains `*Config.cmake` or `*-config.cmake`.
- [x] `hook/pkg_config_path.{dsv,sh}` — `prepend-non-duplicate;PKG_CONFIG_PATH;lib/pkgconfig`
- [x] `hook/pkg_config_path_multiarch.{dsv,sh}` — multiarch variant.
  Suffix from `gcc -print-multiarch` (e.g. `lib/x86_64-linux-gnu/pkgconfig`).
- [x] `hook/ros_package_path.{dsv,sh}` — `prepend-non-duplicate;ROS_PACKAGE_PATH;share`
- [x] `package.dsv` overwritten with `source;share/<pkg>/hook/*.{dsv,sh}`
  entries (colcon intentionally replaces ament's package.dsv).
- [x] `package.{bash,sh,zsh}` — colcon shell templates. `package.sh`
  defines `_colcon_prepend_unique_value()` and sources hook/*.sh.
- [x] `.catkin` — empty marker file at install root.

### Additional ament index entries

- [x] `share/ament_index/resource_index/parent_prefix_path/<name>` —
  value is the `AMENT_PREFIX_PATH` at build time (chained underlays).
- [x] `share/ament_index/resource_index/package_run_dependencies/<name>` —
  semicolon-separated runtime deps (empty for no-dep packages).

### Top-level install scripts

Generated once for the entire install prefix, not per-package.

**Decision: Ship the Python helper unchanged.** Both ament and colcon
use dynamic package discovery at source-time — scanning
`share/colcon-core/packages/` for marker files. A static script would
miss packages installed after the build. The Python helper (~0.5s at
source-time) is acceptable for a one-time interactive operation.

- [x] `_local_setup_util_sh.py` — colcon's Python helper shipped verbatim
  via `include_str!` for dynamic package discovery + topo-order sourcing.
- [x] `setup.{bash,sh,zsh}` — prefix chain scripts (sources chained
  underlays then `local_setup`). `setup.sh` has build-time fallback path.
- [x] `local_setup.{bash,sh,zsh}` — prefix scripts that add
  `COLCON_PREFIX_PATH`, find Python, and call `_local_setup_util_sh.py`
  to discover and source packages.
- [x] `.colcon_install_layout` containing `"isolated"`.
- [x] `COLCON_IGNORE` — empty marker.
- [x] 7 unit tests for install_scripts, 10 unit tests for post_install.

### Acceptance criteria

- [ ] `source install/setup.bash` on a rosup-built workspace produces
  identical `AMENT_PREFIX_PATH`, `CMAKE_PREFIX_PATH`, `PATH`,
  `LD_LIBRARY_PATH`, `PKG_CONFIG_PATH` as colcon-built.
- [ ] `ros2 run <pkg> <node>` works after sourcing.
- [ ] Installing a new package into `install/` and re-sourcing
  `setup.bash` picks it up without rebuilding.

---

## 11.5c DSV parser and per-package environment builder — DONE

**Files:** `crates/rosup-core/src/build/dsv.rs` (new)

Parse DSV files natively in Rust to construct the build environment for
each package. Eliminates the 25-second bash subprocess overhead that
colcon incurs per build.

See `docs/design/native-builder.md` section 1.

### DSV parser

- [x] `DsvEntry` struct: `action`, `variable`, `suffix`.
- [x] `DsvAction` enum: `PrependNonDuplicate`, `PrependNonDuplicateIfExists`,
  `AppendNonDuplicate`, `Set`, `SetIfUnset`, `Source`.
- [x] `parse_dsv_file(path) -> Vec<DsvEntry>` — parse semicolon-separated lines.
- [x] Handle `source` entries: `.dsv` references followed recursively;
  `.sh`/`.bash`/`.zsh` references check for sibling `.dsv` file and use
  that instead (handles ament's convention where `local_setup.dsv`
  references `.sh` files but sibling `.dsv` files hold the env directives).

### Environment builder

- [x] `build_package_env(dep_install_paths, base_env) -> HashMap<String, String>`
- [x] For each dep (in topological order), read `local_setup.dsv` and
  `package.dsv`, apply each entry to the env HashMap.
- [x] `prepend-non-duplicate`: prepend `<prefix>/<suffix>` to `$VAR` with
  colon separator, skip if already present.
- [x] `prepend-non-duplicate-if-exists`: same but check path exists first.
- [x] `append-non-duplicate`: append instead of prepend, skip duplicates.
- [x] `set`: override `$VAR = <value>`.
- [x] `set-if-unset`: set only if not already in env.
- [x] 20 unit tests: parsing, recursive collection, prepend/append helpers,
  set/set-if-unset, path existence checks, full env builds with ament
  and colcon layouts, multi-dep ordering.
- [x] Integration test: builds env from real `/opt/ros/humble` DSV files,
  verifies AMENT_PREFIX_PATH, LD_LIBRARY_PATH, PATH (marked `#[ignore]`).

### Acceptance criteria

- [x] Builds correct `CMAKE_PREFIX_PATH`, `AMENT_PREFIX_PATH`,
  `LD_LIBRARY_PATH`, `PKG_CONFIG_PATH` from DSV files.
- [x] Handles both ament layout (`.sh` → sibling `.dsv`) and colcon
  layout (direct `.dsv` references in `package.dsv`).
- [ ] Diff test: DSV-built env matches bash-sourced env for a real
  workspace (Autoware) — deferred to 11.10 compatibility suite.
- [ ] Performance benchmark — deferred to 11.10.

---

## 11.5d Incremental builds via fingerprinting — DONE

**File:** `crates/rosup-core/src/build/fingerprint.rs` (new)

Skip packages whose inputs haven't changed since the last successful
build. Two-level caching: rosup fingerprint (coarse, skips cmake
entirely) + cmake's own tracking (fine-grained, skips unchanged targets).

See `docs/design/native-builder.md` section 2.

### Fingerprint inputs

- [x] `package.xml` content hash (SHA-256 — dep changes trigger rebuild).
- [x] `CMakeLists.txt` content hash (SHA-256 — build config changes).
  Empty string if not present (ament_python packages).
- [x] Source file mtimes + sizes (faster than full content hash).
  Walks `src/`, `include/`, `launch/`, `config/`, `param/`, `resource/`,
  `msg/`, `srv/`, `action/` dirs + `setup.py`, `setup.cfg`, `pyproject.toml`.
  Files sorted by path for determinism.
- [x] Build config hash (cmake_args, symlink_install from rosup.toml).
- [x] Dependency fingerprint hash (dep rebuild triggers downstream).
  Uses `summary_hash()` of each workspace dep's fingerprint.

### Fingerprint storage

- [x] `build/<pkg>/.rosup_fingerprint` — TOML-like file with per-input
  SHA-256 hashes.
- [x] `Fingerprint::compute(inputs) -> Fingerprint` — walk source tree.
- [x] `Fingerprint::load(build_base) -> Option<Fingerprint>` — read from disk.
- [x] `Fingerprint::save(build_base)` — write to disk.
- [x] `should_rebuild(inputs, build_base) -> bool` — compare old vs new.
- [x] `Fingerprint::summary_hash() -> String` — single 64-char hex digest
  for use in dep_hash of downstream packages.

### Build decision logic

```
fingerprint unchanged → skip entirely (0ms)
fingerprint changed → run cmake → cmake skips unchanged targets (~0.1s)
no previous fingerprint → full build
```

### Tests

19 unit tests:
- Deterministic compute, detects changes in package.xml, CMakeLists.txt,
  source files (content + new file), cmake_args, symlink_install,
  dep fingerprints, missing CMakeLists.txt.
- Save/load roundtrip, missing file, malformed file.
- should_rebuild: no previous, unchanged, source changed, package.xml changed.
- summary_hash: deterministic, changes with fingerprint.
- Python package: detects setup.py changes.

### Acceptance criteria

- [x] Changing one source file triggers rebuild (tested).
- [x] Changing `rosup.toml` cmake-args triggers rebuild (tested).
- [x] Adding a new dep to `package.xml` triggers rebuild (tested).
- [x] Dep rebuild propagates downstream via dep_hash (tested).
- [ ] End-to-end: second `rosup build` skips all packages — deferred to
  11.9 (requires executor integration).

---

## 11.6 Parallel executor — DONE

**File:** `crates/rosup-core/src/build/executor.rs` (new),
`crates/rosup-core/src/build/topo.rs` (extended with `mark_dispatched`,
`mark_failed`)

Dep-aware parallel job scheduler using `std::thread::scope`.

- [x] `Executor::execute(graph, build_fn, progress_fn) -> BuildSummary`.
- [x] Jobs are spawned when all deps are complete.
- [x] `max_workers` limits concurrency.
- [x] `OnError::Stop`: no new jobs spawned after failure, drains in-flight.
- [x] `OnError::SkipDownstream`: removes failed package's transitive
  dependents from graph, continues building independent packages.
- [x] `ProgressEvent` enum: `Starting` (index/total/active), `Finished`
  (cached flag, duration), `Failed` (error message).
- [x] `BuildResult` enum: `Success`, `Cached`, `Failed`, `Skipped`.
- [x] `BuildSummary` with `succeeded()`, `cached()`, `failed()`,
  `skipped()`, `has_failures()`, `total_duration`.
- [x] `BuildOutcome` enum returned by user's build function:
  `Built` or `Cached`.
- [x] Uses `std::thread::scope` — no async runtime needed, threads can
  borrow the build function.
- [x] Three-phase dispatch: `mark_dispatched` (in-flight), `mark_done`
  (success, releases dependents), `mark_failed` (removes downstream).

### Tests

12 unit tests:
- Basic: executes all, respects dep order, independent packages.
- Cached: tracks cached count.
- Stop: prevents new jobs after failure.
- SkipDownstream: continues independent, cascades through deps.
- Parallelism: verifies multiple workers used concurrently.
- Progress: events emitted for start/done/fail.
- Diamond graph: correct ordering with parallelism.
- Empty graph: no-op.
- Summary counts: all result types in one test.

### Acceptance criteria

- [x] `--continue-on-error` correctly skips dependents of failed packages.
- [x] Parallel execution uses multiple workers (verified with atomic
  counter).
- [ ] Autoware performance comparison — deferred to 11.9 integration.

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
| `build/ament_index.rs` | 1 | marker |
| `build/environment.rs` | 9 | all scripts and DSV formats |
| `build/post_install.rs` | 10 | hooks, DSV, shell scripts, ament index |
| `build/install_scripts.rs` | 7 | all top-level files, chains, fallback |
| `build/dsv.rs` | 20 + 1 `#[ignore]` | parsing, recursive DSV, env builder, real ROS |
| `build/fingerprint.rs` | 19 | compute, save/load, should_rebuild, summary |
| `build/executor.rs` | 12 | parallel, ordering, error modes, progress |
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
