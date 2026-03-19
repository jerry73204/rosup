# Phase 10 — Edge Case Tests — DONE

Add unit and integration tests for scenarios discovered during real-world
testing. See `docs/testing/real-world-edge-cases.md` for full context.

---

## 10.1 Deep `exclude` glob patterns — DONE

**File:** `crates/rosup-core/src/project.rs` (unit tests)

- [x] `deep_exclude_filters_nested_test_packages_auto_discovery`: packages
  under `src/a/tests/comparison/` and `src/b/tests/bench/` excluded by
  `exclude = ["**/tests/**"]` in auto-discovery mode.
- [x] `deep_exclude_filters_nested_test_packages_glob_mode`: same with
  `members = ["src/**"]` glob mode.

---

## 10.2 `COLCON_IGNORE` with nested packages — DONE

**File:** `crates/rosup-core/src/init.rs` (unit tests)

- [x] `colcon_ignore_blocks_all_nested_packages`: `external/COLCON_IGNORE`
  prevents `external/vendored_a/` and `external/vendored_b/sub/` from
  being discovered.

---

## 10.3 Non-ROS directories alongside ROS packages — DONE

**File:** `crates/rosup-core/src/init.rs` (unit tests)

- [x] `non_ros_directories_excluded_from_scan`: `src/cuda_ffi/` (has
  `Cargo.toml`) and `src/ndt_cuda/` (has `CMakeLists.txt`) are excluded
  because they lack `package.xml`.

---

## 10.4 Sibling workspace member excluded from resolver — DONE

**File:** `crates/rosup-tests/tests/resolve.rs` (integration test)

- [x] `workspace_sibling_deps_excluded_from_resolve`: workspace with
  `pkg_core` and `pkg_launch` (depends on `pkg_core` + `rclcpp`).
  `resolve --dry-run` resolves `rclcpp` but does not attempt to resolve
  `pkg_core`.

---

## 10.5 Auto-discovery sorting guarantee — DONE

**File:** `crates/rosup-core/src/init.rs` (unit tests)

- [x] `colcon_scan_results_sorted_lexicographically`: packages `z_pkg`,
  `a_pkg`, `m_pkg` created in reverse order; result is
  `["src/a_pkg", "src/m_pkg", "src/z_pkg"]`.

---

## 10.6 `add -p` and `remove -p` in auto-discovery workspace — DONE

**File:** `crates/rosup-tests/tests/manifest.rs` (integration tests)

Implemented during Phase 9.1.

- [x] `add_with_package_flag_in_auto_discovery_workspace`
- [x] `remove_with_package_flag_in_auto_discovery_workspace`
- [x] `add_with_package_flag_nonexistent_package_fails`

---

## 10.7 Overlay misconfiguration does not block non-build commands — DONE

**File:** `crates/rosup-tests/tests/sync.rs`

- [x] `sync_succeeds_with_bad_overlay`: workspace with
  `overlays = ["/nonexistent/overlay/path"]`; `sync --check` succeeds.

---

## 10.8 `init` existence check ordering — DONE

**File:** `crates/rosup-tests/tests/init.rs`

- [x] `init_existence_check_before_distro_prompt`: existing `rosup.toml`,
  no `--ros-distro`, no `ROS_DISTRO`; fails immediately with "already
  exists" without blocking on stdin.

---

## 10.9 `ROS_DISTRO` env fallback for `search` and `clone` — DONE

**File:** `crates/rosup-tests/tests/search.rs`

- [x] `search_falls_back_to_ros_distro_env`: `ROS_DISTRO=humble` set in
  env, no `--distro` flag; search returns results.
- [x] `search_missing_distro_exits_nonzero`: updated assertion to verify
  error message mentions `ROS_DISTRO`.
