# Phase 4 — Build and Test

Implement `rox build`, `rox test`, and `rox clean` by delegating to colcon
with configuration from `rox.toml`.

## Work Items

- [ ] Implement `rox build`
  - [ ] Run dependency resolution (phase 3)
  - [ ] Construct `colcon build` command from `[build]` config
  - [ ] Apply `cmake-args`, `parallel-jobs`, `symlink-install` from `rox.toml`
  - [ ] Support `-p` / `--packages` to build specific packages
  - [ ] Support `--deps` to include dependencies of selected packages
  - [ ] Support `--release` / `--debug` shorthands
  - [ ] Support `--cmake-args` CLI override
  - [ ] Stream colcon output to terminal in real time
- [ ] Implement `[build.overrides.<pkg>]` handling
  - [ ] Merge per-package cmake-args with workspace-level defaults
- [ ] Implement `rox test`
  - [ ] Run dependency resolution
  - [ ] Construct `colcon test` command from `[test]` config
  - [ ] Support `-p` / `--packages` to test specific packages
  - [ ] Support `--retest-until-pass`
  - [ ] Run `colcon test-result` to show summary
- [ ] Implement `rox clean`
  - [ ] Default: remove `build/` and `log/`
  - [ ] `--all`: also remove `install/`
  - [ ] `-p`: remove only the specified package's build artifacts

## Acceptance Criteria

- [ ] `rox build` resolves deps then runs `colcon build` successfully
- [ ] `rox build --release` passes `-DCMAKE_BUILD_TYPE=Release` to colcon
- [ ] `rox build -p my_pkg` builds only `my_pkg`
- [ ] `rox build -p my_pkg --deps` builds `my_pkg` and its dependencies
- [ ] `cmake-args` from `rox.toml` are passed through to colcon
- [ ] Per-package `[build.overrides]` are applied correctly
- [ ] `rox test` runs tests and prints a result summary
- [ ] `rox test -p my_pkg` tests only `my_pkg`
- [ ] `rox clean` removes `build/` and `log/`
- [ ] `rox clean --all` also removes `install/`
- [ ] Build output streams to terminal (not buffered until completion)
- [ ] Non-zero colcon exit codes propagate as rox exit codes
