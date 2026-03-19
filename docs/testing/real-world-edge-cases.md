# Real-World Edge Cases

Discovered by testing rosup against three production workspaces:

- **Autoware 1.5.0** — 455 packages, auto-discovery mode
- **AutoSDV** — 113 packages, nested sub-workspaces, sensor drivers
- **cuda_ndt_matcher** — 5 packages, mixed Rust + ROS workspace

---

## Edge Cases to Cover in Tests

### 1. Nested test packages pollute auto-discovery

**Scenario:** A package contains a `tests/comparison/` directory with a full
copy of another project (66 `package.xml` files) for benchmarking. No
`COLCON_IGNORE` marker exists.

**Expected:** Colcon (and rosup) discovers all 66 packages. This matches
Colcon's behavior. The fix is for the user to add a `COLCON_IGNORE` marker
or use `exclude` patterns.

**Test idea:** Verify that `exclude = ["src/**/tests/**"]` correctly filters
deeply nested test packages from discovery.

### 2. Packages-do-not-nest rule with sub-workspaces

**Scenario:** A workspace member `src/localization/cuda_ndt_matcher/` is
itself a ROS package (has `package.xml`). Inside it,
`src/cuda_ndt_matcher/src/cuda_ndt_matcher/` also has a `package.xml`.

**Expected:** Only the outer package is discovered. The
"packages do not nest" rule in `colcon_scan` stops recursion at the first
`package.xml`. The inner packages are invisible unless they are above the
outer package in the directory tree.

**Test idea:** Create a package with `package.xml` at depth 1, and another
`package.xml` inside a child directory. Verify only the outer is found.
(Already covered by `colcon_scan_does_not_recurse_into_packages`.)

### 3. `COLCON_IGNORE` protecting vendored/external directories

**Scenario:** `external/` contains vendored dependencies with their own
`package.xml` files and a `COLCON_IGNORE` at the `external/` root to prevent
Colcon from discovering them.

**Expected:** The marker prevents all packages under `external/` from
appearing in the scan, regardless of how many nested packages exist.

**Test idea:** Create `external/` with `COLCON_IGNORE` and several child
packages. Verify zero are discovered. Then remove the marker and verify they
appear. (Partially covered by existing tests, but not with nested packages.)

### 4. Non-ROS directories in `src/` (Rust, CUDA, Python)

**Scenario:** `src/` contains directories without `package.xml` — pure Rust
crates (`cuda_ffi/`), CUDA libraries (`ndt_cuda/`), or Python packages.

**Expected:** These are silently skipped by `colcon_scan` since only
directories containing `package.xml` are recorded as members.

**Test idea:** Add non-ROS directories alongside ROS packages in a test
workspace. Verify they don't appear in discovery results.

### 5. Interleaved repeated XML elements

**Scenario:** A `package.xml` has `<depend>` elements interspersed with other
dependency types:

```xml
<depend>rclcpp</depend>
<exec_depend>sensor_msgs</exec_depend>
<depend>tf2</depend>
```

**Expected:** Both `rclcpp` and `tf2` appear in `deps.depend`. The
event-based parser handles this correctly (the old serde parser did not).

**Status:** Already covered by `interleaved_repeated_depend_elements` test
and the `interleaved_depend.xml` fixture.

### 6. Workspace member depends on sibling package

**Scenario:** `pkg_launch` depends on `pkg_core`, and both are workspace
members. In workspace mode, `collect_dep_names` should exclude `pkg_core`
from the external resolver because Colcon builds it from source.

**Expected:** `pkg_core` does not appear in the resolver's dependency list.

**Test idea:** Create a workspace with two packages where one depends on the
other. Run `collect_dep_names` and verify the sibling dep is excluded.

### 7. Large workspace discovery count (450+ packages)

**Scenario:** The Autoware workspace has 455 packages under `src/`.

**Expected:** `colcon_scan` discovers all of them in under a second. The
discovered members are sorted lexicographically.

**Status:** Performance is confirmed fast. No unit test needed for this
specific count, but the sorting guarantee should be tested.

### 8. `exclude` with deep glob patterns

**Scenario:** `exclude = ["src/**/tests/**"]` should filter packages nested
multiple levels deep under any `tests/` directory.

**Expected:** Packages under `src/localization/cuda_ndt_matcher/tests/` are
excluded from discovery even though they're 4 levels deep.

**Test idea:** Create a deep directory tree with packages at various depths.
Apply `exclude = ["**/tests/**"]` and verify correct filtering.

### 9. HTTP 404 from rosdistro for invalid distro name

**Scenario:** `rosup search --distro nonexistent` downloads a 404 page and
tries to gunzip it.

**Expected (current):** "failed to decompress cache: invalid gzip header" —
misleading.

**Expected (fixed):** "distro `nonexistent` not found" or similar.

**Test idea:** Unit test `fetch_and_store` with a mock HTTP response
returning 404. Verify a clear error, not a gzip error.

### 10. Overlay path does not exist on this machine

**Scenario:** `rosup.toml` has `overlays = ["/opt/autoware/1.5.0"]` but
the binary is not installed on this dev machine.

**Expected (current):** Every rosup command fails (even `search`, `sync`).

**Expected (fixed):** Commands that don't need overlays should still work.

**Test idea:** Configure an overlay pointing to a non-existent path. Verify
`sync`, `search`, and `add` still work (or produce a warning, not a hard
error). Verify `build` and `test` do fail (they need overlays).

### 11. `find_member_xml` in auto-discovery mode

**Scenario:** Workspace uses auto-discovery (`members` absent). User runs
`rosup add dep -p pkg_name`.

**Expected (current):** Always fails — `find_member_xml` skips auto-discovery.

**Expected (fixed):** Discovers the package via `colcon_scan` or
`project.members()`.

**Test idea:** Create an auto-discovery workspace. Call the equivalent of
`find_member_xml` with a valid package name. Verify it finds the package.

### 12. `rosup clone` destination

**Scenario:** User runs `rosup clone nav2_core` from the workspace root.
The repository is cloned into `$CWD/nav2_core/`. Colcon does not mandate
`src/` — packages can live anywhere in the workspace tree.

**Expected:** Clone lands at `$CWD/<repo_name>`. This is correct behavior.
Users who prefer `src/` should `cd src/` first or use `--destination src/`
(planned, see KI-005/KI-011).
