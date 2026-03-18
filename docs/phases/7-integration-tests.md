# Phase 7 — Integration Tests

Add a dedicated `rosup-tests` crate that exercises the rosup CLI end-to-end using
real processes, fake external tool shims, and fixture project trees. This
mirrors the `nros-tests` pattern from nano-ros: a single integration crate that
owns all cross-cutting test infrastructure.

## Background

### Why a separate crate

- Integration tests that spawn the `rosup` binary cannot live in `rosup-cli`
  without a circular build dependency.
- A dedicated crate keeps the main crates free of test-only dev-dependencies
  (`assert_cmd`, `rstest`, `predicates`) and the fake-binary build machinery.
- All shared fixture helpers are in `src/` (as a library), individual test
  scenarios in `tests/*.rs`, following the nano-ros layout exactly.

### Fake external tool shims

rosup delegates to three external tools: `colcon`, `git`, and `rosdep`. The
integration test suite needs to control their behavior without requiring a
real ROS installation. A `shims/` sub-crate provides thin Rust binaries for
each tool:

- **`fake-colcon`** — records invocation (args, env vars) to a temp JSON file;
  exits with a configurable code read from `FAKE_COLCON_EXIT` (default 0);
  optionally creates stub artifacts (empty `install/` dir) when
  `FAKE_COLCON_CREATE_ARTIFACTS=1`.
- **`fake-git`** — records invocations; creates stub bare clone directories
  and worktree stubs on `clone --bare` and `worktree add`; ignores `fetch`
  and `reset`.
- **`fake-rosdep`** — prints configurable `apt-get install` lines (driven by
  `FAKE_ROSDEP_PACKAGES`); exits 0 for resolvable packages, 1 otherwise.

Each test injects the shims directory ahead of the real tools by prepending it
to `PATH` in the `Command` environment.

### Project fixture helpers

`src/project.rs` provides typed builders for constructing project trees in a
`tempfile::TempDir`:

```
ProjectFixture::package("my_pkg")      // single package with package.xml + rosup.toml
ProjectFixture::workspace(members)     // workspace with N member packages
  .with_rox_toml(patch_fn)            // customise rosup.toml
  .with_dep_layer()                   // pre-create .rosup/install/ with stub content
  .build() -> TempDir
```

All XML written by the helpers is valid package.xml format 3 to ensure the
full parsing stack exercises real code.

## Work Items

### 7.1 Workspace Setup

- [x] Add `crates/rosup-tests/` to `[workspace].members` in root `Cargo.toml`
- [x] Add `crates/shims/` sub-crate to `[workspace].members` (produces
  `fake-colcon`, `fake-git`, `fake-rosdep` binaries)
- [x] `rosup-tests/Cargo.toml` dev-dependencies:
  - `assert_cmd = "2"` — spawn rosup binary, assert exit code and output
  - `rstest = "0.24"` — fixture injection and parameterised cases
  - `predicates = "3"` — composable output predicates
  - `tempfile = "3"` — isolated project trees
  - `once_cell = "1"` — cache `rosup` binary path and shim dir path
  - `serde_json = "1"` — parse shim invocation records
- [x] Create `.config/nextest.toml` with:
  - Default profile: 30 s slow threshold, 60 s terminate timeout
  - CI profile: fail-fast, shorter timeouts

### 7.2 Shim Binaries (`crates/shims/`)

- [x] `fake-colcon/main.rs`:
  - Serialize `argv` + selected env vars to `$SHIM_LOG_DIR/colcon-<pid>.json`
  - If `FAKE_COLCON_CREATE_ARTIFACTS=1`: `mkdir -p <install-base>/share/ament_index`
  - Exit with `$FAKE_COLCON_EXIT` (default 0)
- [x] `fake-git/main.rs`:
  - Record invocation
  - On `clone --bare <url> <path>`: `mkdir -p <path>/objects`
  - On `worktree add --detach <wt> <ref>`: `mkdir -p <wt>`
  - On `fetch` / `reset`: no-op
- [x] `fake-rosdep/main.rs`:
  - `resolve <pkg>`: print `apt-get install ros-<distro>-<pkg>` if pkg in
    `$FAKE_ROSDEP_PACKAGES`; exit 1 otherwise
  - `install --default-yes ...`: print `Installing packages...`; exit 0
- [x] `src/shims.rs` helper: `shim_dir() -> PathBuf` returning the directory
  containing the shim binaries (built into `target/…/debug/`)

### 7.3 Project Fixture Builder (`src/project.rs`)

- [x] `PackageFixture`: creates `package.xml` + `rosup.toml` in a `TempDir`
- [x] `WorkspaceFixture`: creates workspace `rosup.toml` + N member dirs each
  with their own `package.xml`
- [x] Helper `dep_layer_stub(root)`: writes enough content to `.rosup/install/`
  to make `build_dep_layer` believe the layer is already built
- [x] Helper `artifact_stub(root, pkg)`: creates
  `build/<pkg>/` and `install/<pkg>/` entries for clean tests

### 7.4 `src/lib.rs` — Shared Utilities

- [x] `rox_cmd(project_dir) -> assert_cmd::Command` — returns a
  `Command` for the `rosup` binary with `current_dir` set and shims on `PATH`
- [x] `shim_log(log_dir) -> Vec<ShimInvocation>` — read + parse all
  `*.json` files from the shim log dir
- [x] `assert_colcon_args(invocations, expected_args)` — assert a colcon
  invocation contains the expected argument subsequence
- [x] `unique_log_dir() -> TempDir` — per-test isolated shim log dir (avoids
  cross-test contamination when tests run in parallel)

### 7.5 Nextest Configuration (`.config/nextest.toml`)

- [x] Default profile:
  ```toml
  [profile.default]
  slow-timeout = { period = "30s", terminate-after = 2 }
  fail-fast = false
  ```
- [x] CI profile:
  ```toml
  [profile.ci]
  fail-fast = true
  slow-timeout = { period = "15s", terminate-after = 1 }
  ```
- [x] Justfile `ci` recipe passes `--profile ci` to nextest

### 7.6 `tests/init.rs` — `rosup init`

- [x] `init_package_mode`: `rosup init` in a dir with `package.xml` → creates
  `rosup.toml` with `[package]`, exits 0
- [x] `init_workspace_mode`: `rosup init` in a dir with `src/` containing
  multiple `package.xml` files → creates workspace `rosup.toml`
- [x] `init_adds_gitignore_entry`: after `rosup init`, `.gitignore` contains
  `.rosup/`
- [x] `init_no_overwrite_without_force`: second `rosup init` exits non-zero
  with "already exists" message
- [x] `init_force_overwrites`: `rosup init --force` succeeds on re-run

### 7.7 `tests/manifest.rs` — `rosup add` / `rosup remove`

- [x] `add_depend_default`: `rosup add sensor_msgs` inserts `<depend>sensor_msgs</depend>`
- [x] `add_build_depend`: `rosup add --build ament_cmake` inserts `<build_depend>`
- [x] `add_exec_depend`: `rosup add --exec rclcpp` inserts `<exec_depend>`
- [x] `add_dev_inserts_both_tags`: `rosup add --dev gtest` inserts
  `<build_depend>` + `<test_depend>`
- [x] `add_duplicate_exits_nonzero`: adding an already-present dep exits 1
- [x] `remove_existing_dep`: `rosup remove sensor_msgs` removes the tag, prints
  removed line
- [x] `remove_nonexistent_warns`: `rosup remove unknown_pkg` exits 0, prints
  warning
- [ ] `workspace_add_with_p_flag`: `rosup add -p pkg_a sensor_msgs` modifies
  only `pkg_a/package.xml`

### 7.8 `tests/resolve.rs` — `rosup resolve`

- [x] `dry_run_prints_plan`: `rosup resolve --dry-run` with fake cache lists
  each dep and resolution method; exits 0; does not call `rosdep install`
- [x] `dry_run_no_rosdep_install`: shim log shows no `install` subcommand
- [ ] `source_only_skips_rosdep`: `rosup resolve --source-only` never invokes
  `fake-rosdep`
- [x] `unresolved_dep_exits_nonzero`: a dep absent from both ament and cache
  exits 1 with "could not resolve" message
- [x] Uses an on-disk copy of `humble-cache-sample.yaml` via a stub
  `DistroCache` loader that reads from the fixture (no network)

### 7.9 `tests/build.rs` — `rosup build`

All tests use `fake-colcon`; dep layer is pre-stubbed so dep build is
skipped unless `--rebuild-deps` is passed.

- [x] `build_default`: `rosup build` invokes `colcon build`; exit 0 propagated
- [x] `build_release_flag`: `rosup build --release` → colcon args contain
  `-DCMAKE_BUILD_TYPE=Release`
- [x] `build_debug_flag`: `rosup build --debug` → args contain
  `-DCMAKE_BUILD_TYPE=Debug`
- [x] `build_packages_select`: `rosup build -p my_pkg` → `--packages-select my_pkg`
- [x] `build_packages_up_to`: `rosup build -p my_pkg --deps` → `--packages-up-to my_pkg`
- [x] `build_no_resolve_skips_resolver`: `rosup build --no-resolve` → no
  `fake-rosdep` invocations in the shim log
- [x] `build_rebuild_deps_rebuilds_dep_layer`: `rosup build --rebuild-deps` with
  a pre-existing `.rosup/install/` still calls `colcon build` for the dep layer
- [x] `build_colcon_failure_propagates`: `FAKE_COLCON_EXIT=1` → `rosup build`
  exits 1
- [x] `build_cmake_args_from_toml`: `cmake-args = ["-DFOO=bar"]` in `rosup.toml`
  → colcon invocation contains `-DFOO=bar`
- [x] `build_override_pkg_separate_invocation`: package with
  `[build.overrides.my_pkg]` triggers a separate colcon call with
  `--packages-select my_pkg`

### 7.10 `tests/test_cmd.rs` — `rosup test`

- [x] `test_default`: `rosup test` invokes `colcon test`; runs
  `colcon test-result --verbose` after; exit 0
- [x] `test_packages_select`: `rosup test -p my_pkg` → `--packages-select my_pkg`
- [x] `test_failure_propagates`: `FAKE_COLCON_EXIT=1` → `rosup test` exits 1
- [x] `test_retest_until_pass`: `--retest-until-pass 2` with failure on first
  two colcon calls but success on third → overall exit 0; shim log shows 3
  colcon invocations

### 7.11 `tests/clean.rs` — `rosup clean`

These tests create real directory trees and assert they are removed (no shims
needed).

- [x] `clean_default_removes_build_and_log`: `rosup clean` removes `build/` and
  `log/`; leaves `install/` intact
- [x] `clean_all_removes_install`: `rosup clean --all` also removes `install/`
- [x] `clean_deps_removes_dot_rox_build_and_install`: `rosup clean --deps`
  removes `.rosup/build/` and `.rosup/install/`; leaves `.rosup/src/` intact
- [x] `clean_deps_src_removes_worktrees`: `rosup clean --deps --src` removes
  `.rosup/src/` as well
- [x] `clean_package_scoped`: `rosup clean -p my_pkg` removes only
  `build/my_pkg/` and `install/my_pkg/`; leaves other packages intact
- [x] `clean_no_error_when_nothing_to_clean`: `rosup clean` in a fresh project
  exits 0

### 7.12 `tests/search.rs` — `rosup search`

Tests use a local cache YAML via `FAKE_CACHE_DIR` or by writing the fixture
into the GlobalStore path.

- [x] `search_returns_matches`: `rosup search rclcpp --distro humble` prints
  package names and URLs; exit 0
- [x] `search_no_matches_message`: `rosup search xyz_no_such_thing --distro
  humble` prints "No packages found" message; exit 0
- [x] `search_limit_flag`: `rosup search rclcpp --distro humble --limit 2`
  returns exactly 2 lines
- [x] `search_missing_distro_exits_nonzero`: `rosup search foo` (no `--distro`,
  no `ROS_DISTRO` env) → exit 1 with distro hint

### 7.13 `tests/clone.rs` — `rosup clone`

Uses `fake-git` shim; does not perform real clones.

- [x] `clone_invokes_git_clone`: `rosup clone sensor_msgs --distro humble`
  → shim log shows `git clone --branch <branch> <url> common_interfaces`
- [x] `clone_creates_rox_toml`: after clone, destination dir has `rosup.toml`
- [x] `clone_unknown_package_exits_nonzero`: `rosup clone no_such_pkg --distro
  humble` exits 1 with "not found" message
- [x] `clone_workspace_mode_for_multi_package_repo`: `fake-git` creates stub
  `pkg_a/package.xml` + `pkg_b/package.xml` in the cloned dir → `rosup.toml`
  contains `[workspace]`

## Acceptance Criteria

- [x] `just test` runs all 50+ integration tests without a ROS installation
- [x] All shim-dependent tests are hermetic: no network, no real colcon/git
- [x] Each test uses its own `TempDir`; no shared mutable state between tests
- [x] `just ci` passes with the new nextest CI profile
- [x] A failing colcon exit code causes `rosup build` / `rosup test` to exit
  non-zero in the integration test
- [x] The phase-5 search/clone acceptance criteria are verified by integration
  tests rather than manual testing
