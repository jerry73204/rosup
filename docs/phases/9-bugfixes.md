# Phase 9 — Bug Fixes

Fix bugs discovered by real-world testing against Autoware 1.5.0 (455
packages), AutoSDV (113 packages), and cuda_ndt_matcher (5 packages).

---

## 9.1 Fix `find_member_xml` in auto-discovery workspaces (KI-007) — DONE

**Files:** `crates/rosup-cli/src/main.rs`

`find_member_xml()` only iterates `ws.members.iter().flatten()`, which
yields nothing when `members` is `None`. This breaks `rosup add -p <pkg>`
and `rosup remove -p <pkg>` in auto-discovery workspaces.

### Implementation

- [x] When `ws.members` is `None`, call `colcon_scan` (which already
  handles auto-discovery) to get the member list.
- [x] Search the discovered members by package name.
- [x] Return the `package.xml` path for the matching member.

### Acceptance criteria

- [x] `rosup add rclcpp -p pkg_a` succeeds in an auto-discovery workspace.
- [x] `rosup remove rclcpp -p pkg_a` succeeds in an auto-discovery workspace.
- [x] `rosup add rclcpp -p nonexistent` still fails with a clear error.
- [x] Existing glob-mode (`members = ["src/*"]`) behavior unchanged.
- [x] Integration test: init auto-discovery workspace, `add -p`, verify
  `package.xml` is modified.

---

## 9.2 Check HTTP status before decompressing rosdistro cache (KI-008) — DONE

**File:** `crates/rosup-core/src/resolver/rosdistro.rs`

`fetch_and_store()` downloaded the rosdistro cache without checking the HTTP
status code. A 404 response (HTML) was fed to `GzDecoder`, producing
"invalid gzip header".

### Implementation

- [x] After `reqwest::blocking::get(&url)`, check `response.status()`.
- [x] If not success, return `RosdistroError::DistroNotFound(String, u16)`
  with the distro name and HTTP status code.
- [x] Add the new variant to `RosdistroError`.

### Acceptance criteria

- [x] `rosup search --distro nonexistent` produces:
  `distro "nonexistent" not found in rosdistro index (HTTP 404)`.
- [x] Valid distro names still work (verified via existing search tests).

---

## 9.3 Make overlay validation non-fatal for non-build commands (KI-009) — DONE

**File:** `crates/rosup-cli/src/main.rs`

`apply_overlays_from_cwd()` ran at startup for every command. If an overlay
prefix lacked `setup.sh`, even `search`, `sync`, and `add` failed.

### Implementation

- [x] Removed the eager `apply_overlays_from_cwd()` call from `main()`.
- [x] Moved overlay activation into `cmd_build` and `cmd_test` — the only
  commands that need a sourced ROS environment for colcon.
- [x] All other commands (`search`, `clone`, `init`, `sync`, `add`,
  `remove`, `clean`, `resolve`) no longer trigger overlay validation.

### Acceptance criteria

- [x] `rosup sync` works when overlays point to a non-existent path.
- [x] `rosup add rclcpp` works when overlays point to a non-existent path.
- [x] `rosup search nav --distro humble` works when overlays point to a
  non-existent path.
- [x] `rosup build` still fails with a clear error when overlays are
  misconfigured.
- [x] All existing tests pass.

---

## 9.4 Move existence check before distro prompt in `cmd_init` (KI-010) — DONE

**File:** `crates/rosup-cli/src/main.rs`

`cmd_init` prompted for the ROS distro interactively before checking if
`rosup.toml` already exists. The user typed a value for nothing.

### Implementation

- [x] In `cmd_init`, check `cwd.join("rosup.toml").exists() && !force`
  **before** resolving `ros_distro` (the prompt/env check).
- [x] Return an error early.

### Acceptance criteria

- [x] Running `rosup init` in a directory with an existing `rosup.toml`
  immediately reports the error without prompting for distro.
- [x] `rosup init --force` still prompts (or uses env) and overwrites.
- [x] Existing `init_no_overwrite_without_force` integration test covers
  this scenario.

---

## 9.5 Add `ROS_DISTRO` env fallback to `resolve_distro` (KI-012) — DONE

**File:** `crates/rosup-cli/src/main.rs`

`resolve_distro()` (used by `search` and `clone`) did not fall back to
`ROS_DISTRO` from the environment, unlike `Resolver::new()` and `cmd_init`.

### Implementation

- [x] After checking the CLI flag and `rosup.toml`, added
  `std::env::var("ROS_DISTRO")` as a third fallback.
- [x] Updated the error message to mention `ROS_DISTRO` as an option.

### Acceptance criteria

- [x] `ROS_DISTRO=humble rosup search nav` works without `--distro`.
- [x] `--distro` flag still takes precedence over env.
- [x] `rosup.toml` `ros-distro` still takes precedence over env.
- [x] Resolution order: `--distro` → `rosup.toml` → `ROS_DISTRO` → error.

---

## 9.6 Add `--destination` flag to `rosup clone` (KI-005) — DONE

**File:** `crates/rosup-cli/src/main.rs`

`rosup clone` always cloned into `$CWD`. There was no way to specify a
different target directory.

### Implementation

- [x] Added `--destination <dir>` flag to the `Clone` command.
- [x] When set, clones into `<dir>/<repo_name>` instead of
  `$CWD/<repo_name>`.
- [x] When not set, behavior is unchanged (`$CWD`).

### Acceptance criteria

- [x] `rosup clone nav2_core --distro humble --destination src/` clones
  into `src/nav2_core/`.
- [x] `rosup clone nav2_core --distro humble` still clones into
  `$CWD/nav2_core/` (no behavior change).
- [x] `docs/reference/cli.md` updated.
- [x] KI-005 closed.
