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

## 9.2 Check HTTP status before decompressing rosdistro cache (KI-008)

**File:** `crates/rosup-core/src/resolver/rosdistro.rs`

`fetch_and_store()` downloads the rosdistro cache without checking the HTTP
status code. A 404 response (HTML) is fed to `GzDecoder`, producing
"invalid gzip header".

### Implementation

- [ ] After `reqwest::blocking::get(&url)`, check `response.status()`.
- [ ] If not `200 OK`, return a new `RosdistroError::DistroNotFound(String)`
  with the distro name.
- [ ] Add the new variant to `RosdistroError`.

### Acceptance criteria

- [ ] `rosup search --distro nonexistent` produces:
  `distro "nonexistent" not found in rosdistro index` (or similar).
- [ ] Valid distro names still work.
- [ ] Unit test: mock or test against a known-bad distro name.

---

## 9.3 Make overlay validation non-fatal for non-build commands (KI-009)

**File:** `crates/rosup-cli/src/main.rs`

`apply_overlays_from_cwd()` runs at startup for every command. If an overlay
prefix lacks `setup.sh`, even `search`, `sync`, and `add` fail.

### Implementation

- [ ] Move overlay activation out of `main()` startup. Instead, call it
  only from commands that need it (`build`, `test`, `resolve`, `run`,
  `launch`).
- [ ] For `search`, `clone`, `init`, `sync`, `add`, `remove`, `clean`:
  do not activate overlays. These commands work without a sourced
  environment.
- [ ] Alternatively: keep the startup call but downgrade the error to a
  `tracing::warn!` and return an empty env map, so non-build commands
  still work.

### Acceptance criteria

- [ ] `rosup sync` works when overlays point to a non-existent path.
- [ ] `rosup add rclcpp` works when overlays point to a non-existent path.
- [ ] `rosup search nav --distro humble` works when overlays point to a
  non-existent path.
- [ ] `rosup build` still fails with a clear error when overlays are
  misconfigured.
- [ ] Integration test: configure a bad overlay, run `sync`, verify success.
  Run `build`, verify failure.

---

## 9.4 Move existence check before distro prompt in `cmd_init` (KI-010)

**File:** `crates/rosup-cli/src/main.rs`

`cmd_init` prompts for the ROS distro interactively before checking if
`rosup.toml` already exists. The user types a value for nothing.

### Implementation

- [ ] In `cmd_init`, check `cwd.join("rosup.toml").exists() && !force`
  **before** resolving `ros_distro` (the prompt/env check).
- [ ] Return the `AlreadyExists` error early.

### Acceptance criteria

- [ ] Running `rosup init` in a directory with an existing `rosup.toml`
  immediately reports the error without prompting for distro.
- [ ] `rosup init --force` still prompts (or uses env) and overwrites.
- [ ] Integration test: create `rosup.toml`, run `rosup init`, verify no
  prompt and immediate failure.

---

## 9.5 Add `ROS_DISTRO` env fallback to `resolve_distro` (KI-012)

**File:** `crates/rosup-cli/src/main.rs`

`resolve_distro()` (used by `search` and `clone`) does not fall back to
`ROS_DISTRO` from the environment, unlike `Resolver::new()` and `cmd_init`.

### Implementation

- [ ] After checking the CLI flag and `rosup.toml`, add
  `std::env::var("ROS_DISTRO").ok()` as a third fallback.
- [ ] Update the error message to mention `ROS_DISTRO` as an option.

### Acceptance criteria

- [ ] `ROS_DISTRO=humble rosup search nav` works without `--distro`.
- [ ] `--distro` flag still takes precedence over env.
- [ ] `rosup.toml` `ros-distro` still takes precedence over env.
- [ ] Integration test: set `ROS_DISTRO` env, run `search`, verify success.

---

## 9.6 Add `--destination` flag to `rosup clone` (KI-005)

**File:** `crates/rosup-cli/src/main.rs`

`rosup clone` always clones into `$CWD`. There is no way to specify a
different target directory. Colcon does not mandate `src/` — packages can
live anywhere — so rosup should not assume a convention. Instead, just give
users explicit control.

### Implementation

- [ ] Add `--destination <dir>` flag to the `Clone` command.
- [ ] When set, clone into `<dir>/<repo_name>` instead of
  `$CWD/<repo_name>`.
- [ ] When not set, behavior is unchanged (`$CWD`).

### Acceptance criteria

- [ ] `rosup clone nav2_core --distro humble --destination src/` clones
  into `src/nav2_core/`.
- [ ] `rosup clone nav2_core --distro humble` still clones into
  `$CWD/nav2_core/` (no behavior change).
- [ ] Update `docs/reference/cli.md` to document `--destination`.
- [ ] Update `docs/known-issues.md`: close KI-005.
