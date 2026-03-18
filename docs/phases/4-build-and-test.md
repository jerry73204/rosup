# Phase 4 — Build and Test

Implement `rox build`, `rox test`, and `rox clean` by delegating to colcon
with configuration from `rox.toml`. This phase also covers building the
per-project dep layer from source-pulled dependencies.

## Background

### Environment layering

colcon resolves upstream packages through `AMENT_PREFIX_PATH`. Each rox build
uses a three-layer stack:

```
/opt/ros/{distro}/       ← base ROS install (user sources this)
<project>/.rox/install/  ← dep layer: source-pulled deps built by rox
<project>/install/       ← user workspace output
```

rox constructs `AMENT_PREFIX_PATH` for every colcon invocation; the user is
only required to have sourced the base ROS install beforehand.

### Two-phase build

`rox build` runs dep layer first, user workspace second:

1. **Dep layer** — `colcon build` inside `.rox/` selecting only source-pulled
   packages, with `AMENT_PREFIX_PATH` pointing at the base ROS install.
2. **User workspace** — `colcon build` at the project root, with
   `AMENT_PREFIX_PATH` prepended with `<project>/.rox/install`.

The dep layer is rebuilt only when source-pulled packages have changed
(worktree HEAD differs from the last recorded build). Use `--rebuild-deps` to
force a rebuild.

## Work Items

### 4.1 Environment Setup

- [ ] Check that `AMENT_PREFIX_PATH` is set; if not, print a clear warning:
  `warning: ROS base environment not sourced — run: source /opt/ros/{distro}/setup.bash`
- [ ] Detect the ROS base prefix from `AMENT_PREFIX_PATH` or fall back to
  `/opt/ros/{distro}` using the configured `ros-distro`
- [ ] Build the `AMENT_PREFIX_PATH` string for colcon subprocesses:
  `<project>/.rox/install : existing AMENT_PREFIX_PATH`
- [ ] Propagate `CMAKE_PREFIX_PATH` consistently with `AMENT_PREFIX_PATH`

### 4.2 Dep Layer Build

- [ ] After `rox resolve`, collect the list of source-pulled repos from
  `ProjectStore`
- [ ] Run `colcon build` for the dep layer:
  ```
  colcon build \
    --packages-select <source-pulled-pkgs> \
    --base-paths .rox/src/* \
    --build-base .rox/build \
    --install-base .rox/install
  ```
  with env `AMENT_PREFIX_PATH = /opt/ros/{distro}`
- [ ] Skip dep layer build if `.rox/install` already satisfies all source deps
  (check ament index markers in `.rox/install/`)
- [ ] `--rebuild-deps` flag forces a clean rebuild of the dep layer
- [ ] Stream colcon output to terminal in real time

### 4.3 `rox build`

- [ ] Run environment check (4.1), resolution (Phase 3), dep layer build (4.2)
- [ ] Construct `colcon build` command from `[build]` config in `rox.toml`:
  - `symlink-install` → `--symlink-install`
  - `parallel-jobs` → `--parallel-workers`
  - `cmake-args` → `--cmake-args`
- [ ] `--release` → append `-DCMAKE_BUILD_TYPE=Release` to cmake-args
- [ ] `--debug` → append `-DCMAKE_BUILD_TYPE=Debug` to cmake-args
- [ ] `-p` / `--packages` → `--packages-select`
- [ ] `--deps` → `--packages-up-to` (builds named packages and their deps)
- [ ] `--cmake-args` CLI flag appends to rox.toml cmake-args
- [ ] `--no-resolve` skips resolution and dep layer build
- [ ] Stream colcon output to terminal; propagate exit code

### 4.4 Per-Package Build Overrides

- [ ] Read `[build.overrides.<pkg>]` from `rox.toml`
- [ ] Merge per-package `cmake-args` with workspace-level defaults
- [ ] Pass per-package args via `--cmake-args` with colcon's package-specific
  argument syntax

### 4.5 `rox test`

- [ ] Run environment check and resolution (unless `--no-resolve`)
- [ ] Construct `colcon test` command from `[test]` config:
  - `parallel-jobs` → `--parallel-workers`
- [ ] `-p` / `--packages` → `--packages-select`
- [ ] `--retest-until-pass N`: re-run failed tests up to N times
- [ ] Run `colcon test-result --verbose` to print summary after tests
- [ ] Stream colcon output to terminal; propagate exit code

### 4.6 `rox clean`

- [ ] Default: remove `build/` and `log/` (user workspace artifacts)
- [ ] `--all`: also remove `install/` (user workspace install)
- [ ] `--deps`: remove `.rox/build/` and `.rox/install/` (dep layer artifacts,
  keeps source worktrees intact)
- [ ] `--deps --src`: also remove `.rox/src/` worktrees (bare clones in
  `~/.rox/src/` are unaffected)
- [ ] `-p <pkg>`: remove only the specified package's artifacts from `build/`

## Acceptance Criteria

- [ ] `rox build` warns and exits early if `AMENT_PREFIX_PATH` is unset
- [ ] `rox build` resolves deps, builds dep layer, then builds user workspace
- [ ] `rox build` is a no-op on the dep layer when source deps are already built
- [ ] `rox build --rebuild-deps` rebuilds the dep layer from scratch
- [ ] `rox build --release` passes `-DCMAKE_BUILD_TYPE=Release` to colcon
- [ ] `rox build -p my_pkg` builds only `my_pkg`
- [ ] `rox build -p my_pkg --deps` builds `my_pkg` and its dependencies
- [ ] `cmake-args` from `rox.toml` are passed through to colcon
- [ ] Per-package `[build.overrides]` are applied correctly
- [ ] `rox test` runs tests and prints a result summary
- [ ] `rox test -p my_pkg` tests only `my_pkg`
- [ ] `rox clean` removes `build/` and `log/`
- [ ] `rox clean --all` also removes `install/`
- [ ] `rox clean --deps` removes `.rox/build/` and `.rox/install/` only
- [ ] Build output streams to terminal (not buffered until completion)
- [ ] Non-zero colcon exit codes propagate as rox exit codes
