# Phase 10 — Edge Case Tests

Add unit and integration tests for scenarios discovered during real-world
testing. See `docs/testing/real-world-edge-cases.md` for full context.

---

## 10.1 Deep `exclude` glob patterns

**File:** `crates/rosup-core/src/project.rs` (unit tests)

Verify that `exclude` patterns with `**` correctly filter packages nested
multiple levels deep.

- [ ] Fixture: create `src/a/pkg_a/package.xml`,
  `src/a/tests/comparison/pkg_x/package.xml`,
  `src/b/tests/bench/pkg_y/package.xml`.
- [ ] Test: auto-discovery with `exclude = ["**/tests/**"]` finds only
  `src/a/pkg_a`.
- [ ] Test: glob mode `members = ["src/**"]` with
  `exclude = ["**/tests/**"]` finds only `src/a/pkg_a`.

### Acceptance criteria

- [ ] Both auto-discovery and glob mode correctly apply deep `exclude`
  patterns.
- [ ] Packages at various nesting depths under `tests/` are all excluded.

---

## 10.2 `COLCON_IGNORE` with nested packages

**File:** `crates/rosup-core/src/init.rs` (unit tests)

Verify that a `COLCON_IGNORE` marker at a parent directory prevents all
descendant packages from being discovered.

- [ ] Fixture: create `external/COLCON_IGNORE`,
  `external/vendored_a/package.xml`,
  `external/vendored_b/sub/package.xml`.
- [ ] Test: `colcon_scan` discovers zero packages under `external/`.
- [ ] Test: removing the marker causes both packages to appear.

### Acceptance criteria

- [ ] A single `COLCON_IGNORE` at the parent blocks all nested packages.
- [ ] Existing ignore marker tests still pass.

---

## 10.3 Non-ROS directories alongside ROS packages

**File:** `crates/rosup-core/src/init.rs` (unit tests)

Verify that directories without `package.xml` (Rust crates, CUDA code,
Python packages) are silently excluded from discovery.

- [ ] Fixture: create `src/pkg_a/package.xml`, `src/cuda_ffi/Cargo.toml`
  (no `package.xml`), `src/ndt_cuda/CMakeLists.txt` (no `package.xml`).
- [ ] Test: `colcon_scan` discovers only `src/pkg_a`.

### Acceptance criteria

- [ ] Directories with `Cargo.toml`, `CMakeLists.txt`, `setup.py` but
  no `package.xml` are not reported as members.

---

## 10.4 Sibling workspace member excluded from resolver

**File:** `crates/rosup-cli/src/main.rs` or
`crates/rosup-tests/tests/resolve.rs` (integration test)

Verify that `collect_dep_names` excludes dependencies whose names match
workspace member package names.

- [ ] Fixture: create a workspace with `pkg_core` and `pkg_launch`.
  `pkg_launch/package.xml` has `<depend>pkg_core</depend>`.
- [ ] Test: `collect_dep_names` returns deps from both packages but does
  **not** include `pkg_core` (since it is a workspace member).
- [ ] Test: external deps (e.g. `rclcpp`) are still included.

### Acceptance criteria

- [ ] In-workspace dependencies are excluded from the external resolver.
- [ ] External dependencies are preserved.

---

## 10.5 Auto-discovery sorting guarantee

**File:** `crates/rosup-core/src/init.rs` (unit tests)

Verify that `colcon_scan` returns paths sorted lexicographically regardless
of filesystem iteration order.

- [ ] Fixture: create packages `src/z_pkg`, `src/a_pkg`, `src/m_pkg`.
- [ ] Test: `colcon_scan` returns `["src/a_pkg", "src/m_pkg", "src/z_pkg"]`.

### Acceptance criteria

- [ ] Results are sorted lexicographically, not by filesystem order.
- [ ] Already implicitly covered but deserves an explicit assertion.

---

## 10.6 `add -p` and `remove -p` in auto-discovery workspace

**File:** `crates/rosup-tests/tests/manifest.rs` (integration tests)

End-to-end test for the fix in Phase 9.1.

- [ ] Test: create a workspace with `[workspace]` (auto-discovery),
  two packages. Run `rosup add rclcpp -p pkg_a`. Verify `package.xml`
  is updated.
- [ ] Test: run `rosup remove rclcpp -p pkg_a`. Verify `package.xml`
  is updated.
- [ ] Test: `rosup add rclcpp -p nonexistent` fails with a clear error.

### Acceptance criteria

- [ ] `-p` flag works in auto-discovery workspaces.
- [ ] Error message for non-existent package is clear.

---

## 10.7 Overlay misconfiguration does not block non-build commands

**File:** `crates/rosup-tests/tests/init.rs` or new
`crates/rosup-tests/tests/overlay.rs`

End-to-end test for the fix in Phase 9.3.

- [ ] Test: create `rosup.toml` with `overlays = ["/nonexistent/path"]`.
  Run `rosup sync`. Verify it succeeds (or warns).
- [ ] Test: same setup. Run `rosup add rclcpp`. Verify it succeeds.

### Acceptance criteria

- [ ] Non-build commands survive misconfigured overlays.

---

## 10.8 `init` existence check ordering

**File:** `crates/rosup-tests/tests/init.rs`

End-to-end test for the fix in Phase 9.4.

- [ ] Test: create an existing `rosup.toml`. Run `rosup init` (without
  `--ros-distro`). Verify it fails immediately with "already exists"
  without blocking on stdin.

### Acceptance criteria

- [ ] `rosup init` on an existing project fails before any interactive
  prompt.

---

## 10.9 `ROS_DISTRO` env fallback for `search` and `clone`

**File:** `crates/rosup-tests/tests/search.rs`

End-to-end test for the fix in Phase 9.5.

- [ ] Test: set `ROS_DISTRO=humble` in the process env. Run
  `rosup search nav` (no `--distro`). Verify it returns results.
- [ ] Test: unset `ROS_DISTRO`, no `rosup.toml`, no `--distro`. Verify
  it fails with a clear error mentioning `--distro` and `ROS_DISTRO`.

### Acceptance criteria

- [ ] `ROS_DISTRO` env is consulted when no `--distro` flag and no
  `rosup.toml` `ros-distro` is set.
