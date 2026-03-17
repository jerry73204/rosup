# Phase 2 — Init and Manifest Management

Implement `rox init`, `rox add`, and `rox remove` — the commands that create
and modify project manifests.

## Work Items

- [ ] Implement `rox init` for single-package mode
  - [ ] Read `package.xml` in current directory
  - [ ] Generate `rox.toml` with `[package]` populated from package.xml name
  - [ ] Error if no `package.xml` found and `--workspace` not set
- [ ] Implement `rox init` for workspace mode
  - [ ] Scan `src/` for directories containing `package.xml`
  - [ ] Generate `rox.toml` with `[workspace].members` listing discovered paths
  - [ ] Auto-detect workspace mode when `src/` dir exists
- [ ] Implement `rox init` auto-detection (workspace vs single-package)
- [ ] Implement `rox add <dep>`
  - [ ] Parse target `package.xml`
  - [ ] Insert dependency element (`<depend>` by default)
  - [ ] Support `--build`, `--exec`, `--test`, `--dev` type flags
  - [ ] Support `-p` flag for targeting a specific package in workspace mode
  - [ ] Preserve existing XML formatting and comments
  - [ ] Reject duplicate dependencies
- [ ] Implement `rox remove <dep>`
  - [ ] Remove all matching dependency tags from `package.xml`
  - [ ] Support `-p` flag for workspace mode
  - [ ] Warn if dependency not found

## Acceptance Criteria

- [ ] `rox init` in a directory with `package.xml` produces valid `rox.toml` in package mode
- [ ] `rox init` in a directory with `src/` containing packages produces valid `rox.toml` in workspace mode
- [ ] `rox init` refuses to overwrite an existing `rox.toml` without `--force`
- [ ] `rox add sensor_msgs` inserts `<depend>sensor_msgs</depend>` into `package.xml`
- [ ] `rox add ament_cmake_gtest --test` inserts `<test_depend>` specifically
- [ ] `rox add` with `-p my_pkg` targets the correct package in a workspace
- [ ] `rox remove sensor_msgs` removes all dependency tags for `sensor_msgs`
- [ ] XML output preserves original formatting (indentation, comments, declaration)
- [ ] Adding an already-present dependency prints a message and does not duplicate
