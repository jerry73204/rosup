# Phase 4 — Build and Test

Implement `rosup build`, `rosup test`, and `rosup clean` by delegating to colcon
with configuration from `rosup.toml`. This phase also covers building the
per-project dep layer from source-pulled dependencies.

## Background

### Environment layering

colcon resolves upstream packages through `AMENT_PREFIX_PATH`. Each rosup build
uses a three-layer stack:

```
/opt/ros/{distro}/       ← base ROS install (user sources this)
<project>/.rosup/install/  ← dep layer: source-pulled deps built by rosup
<project>/install/       ← user workspace output
```

rosup constructs `AMENT_PREFIX_PATH` for every colcon invocation; the user is
only required to have sourced the base ROS install beforehand.

### Two-phase build

`rosup build` runs dep layer first, user workspace second:

1. **Dep layer** — `colcon build` inside `.rosup/` selecting only source-pulled
   packages, with `AMENT_PREFIX_PATH` pointing at the base ROS install.
2. **User workspace** — `colcon build` at the project root, with
   `AMENT_PREFIX_PATH` prepended with `<project>/.rosup/install`.

The dep layer is rebuilt only when source-pulled packages have changed
(worktree HEAD differs from the last recorded build). Use `--rebuild-deps` to
force a rebuild.

## Work Items

### 4.1 Environment Setup

- [x] Check that `AMENT_PREFIX_PATH` is set; if not, print a clear warning:
  `warning: ROS base environment not sourced — run: source /opt/ros/{distro}/setup.bash`
- [x] Detect the ROS base prefix from `AMENT_PREFIX_PATH` or fall back to
  `/opt/ros/{distro}` using the configured `ros_distro`
- [x] Build the `AMENT_PREFIX_PATH` string for colcon subprocesses:
  `<project>/.rosup/install : existing AMENT_PREFIX_PATH`
- [x] Propagate `CMAKE_PREFIX_PATH` consistently with `AMENT_PREFIX_PATH`

### 4.2 Dep Layer Build

- [x] After `rosup resolve`, collect the list of source-pulled repos from
  `ProjectStore`
- [x] Run `colcon build` for the dep layer:
  ```
  colcon build \
    --base-paths .rosup/src/* \
    --build-base .rosup/build \
    --install-base .rosup/install
  ```
  with env `AMENT_PREFIX_PATH = /opt/ros/{distro}`
- [x] Skip dep layer build if `.rosup/install` already satisfies all source deps
  (check ament index markers in `.rosup/install/`)
- [x] `--rebuild-deps` flag forces a clean rebuild of the dep layer
- [x] Stream colcon output to terminal in real time

### 4.3 `rosup build`

- [x] Run environment check (4.1), resolution (Phase 3), dep layer build (4.2)
- [x] Construct `colcon build` command from `[build]` config in `rosup.toml`:
  - `symlink-install` → `--symlink-install`
  - `parallel-jobs` → `--parallel-workers`
  - `cmake-args` → `--cmake-args`
- [x] `--release` → append `-DCMAKE_BUILD_TYPE=Release` to cmake-args
- [x] `--debug` → append `-DCMAKE_BUILD_TYPE=Debug` to cmake-args
- [x] `-p` / `--packages` → `--packages-select`
- [x] `--deps` → `--packages-up-to` (builds named packages and their deps)
- [x] `--cmake-args` CLI flag appends to rosup.toml cmake-args
- [x] `--no-resolve` skips resolution and dep layer build
- [x] Stream colcon output to terminal; propagate exit code

### 4.4 Per-Package Build Overrides

- [x] Read `[build.overrides.<pkg>]` from `rosup.toml`
- [x] Merge per-package `cmake-args` with workspace-level defaults
- [x] Packages with overrides get a separate `colcon build --packages-select`
  invocation with their specific cmake-args

### 4.5 `rosup test`

- [x] Run environment check and resolution (unless `--no-resolve`)
- [x] Construct `colcon test` command from `[test]` config:
  - `parallel-jobs` → `--parallel-workers`
- [x] `-p` / `--packages` → `--packages-select`
- [x] `--retest-until-pass N`: re-run failed tests up to N additional times
- [x] Run `colcon test-result --verbose` to print summary after tests
- [x] Stream colcon output to terminal; propagate exit code

### 4.6 `rosup clean`

- [x] Default: remove `build/` and `log/` (user workspace artifacts)
- [x] `--all`: also remove `install/` (user workspace install)
- [x] `--deps`: remove `.rosup/build/` and `.rosup/install/` (dep layer artifacts,
  keeps source worktrees intact)
- [x] `--deps --src`: also remove `.rosup/src/` worktrees (bare clones in
  `~/.rosup/src/` are unaffected)
- [x] `-p <pkg>`: remove only the specified package's artifacts from `build/`
  and `install/`

## Acceptance Criteria

- [x] `rosup build` warns (but does not exit) if `AMENT_PREFIX_PATH` is unset
- [x] `rosup build` resolves deps, builds dep layer, then builds user workspace
- [x] `rosup build` is a no-op on the dep layer when source deps are already built
- [x] `rosup build --rebuild-deps` rebuilds the dep layer from scratch
- [x] `rosup build --release` passes `-DCMAKE_BUILD_TYPE=Release` to colcon
- [x] `rosup build -p my_pkg` builds only `my_pkg`
- [x] `rosup build -p my_pkg --deps` builds `my_pkg` and its dependencies
- [x] `cmake-args` from `rosup.toml` are passed through to colcon
- [x] Per-package `[build.overrides]` are applied correctly
- [x] `rosup test` runs tests and prints a result summary
- [x] `rosup test -p my_pkg` tests only `my_pkg`
- [x] `rosup clean` removes `build/` and `log/`
- [x] `rosup clean --all` also removes `install/`
- [x] `rosup clean --deps` removes `.rosup/build/` and `.rosup/install/` only
- [x] Build output streams to terminal (not buffered until completion)
- [x] Non-zero colcon exit codes propagate as rosup exit codes
