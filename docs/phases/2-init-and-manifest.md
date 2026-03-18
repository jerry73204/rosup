# Phase 2 — Init and Manifest Management

Implement `rosup init`, `rosup add`, and `rosup remove` — the commands that create
and modify project manifests.

## Background

See `docs/reference/package-xml.md` for the full format 3 spec. Key points
that affect this phase:

- Element order is **not significant** to the parser, but rosup should write
  files following the canonical ordering convention.
- All text values are whitespace-normalized on read; rosup should write clean,
  consistently indented XML.
- The `<export><build_type>` element is required for colcon to recognise a
  package as ROS 2. `rosup init` must emit it.
- `<depend>` is the right default tag for `rosup add` (covers build + build_export
  + exec). More specific tags (`--build`, `--exec`, `--test`) are opt-in.
- `condition` attribute: rosup does not need to evaluate conditions — it is the
  user's responsibility to write correct condition expressions. rosup preserves
  them on read/write.
- Comments in package.xml must be preserved on write.

---

## Work Items

### 2.1 `rosup init`

- [x] Detect auto mode: workspace if `src/` dir exists, single-package if
  `package.xml` exists in CWD, error otherwise
- [x] Single-package mode: read `<name>` and `<build_type>` from `package.xml`;
  generate `rosup.toml` with `[package]`
- [x] Workspace mode: scan `src/` recursively for `package.xml` files; generate
  `rosup.toml` with `[workspace].members`
- [x] `--workspace` flag forces workspace mode even without a `src/` dir
- [x] Refuse to overwrite an existing `rosup.toml` without `--force`
- [x] Print the path of the generated `rosup.toml` on success

### 2.2 `rosup add`

- [x] Default tag: `<depend>` (build + build_export + exec)
- [x] `--build` → `<build_depend>`
- [x] `--exec` → `<exec_depend>`
- [x] `--test` → `<test_depend>`
- [x] `--dev` → `<build_depend>` + `<test_depend>`
- [x] `-p <pkg>` targets a specific member package in workspace mode; defaults
  to the package in CWD
- [x] Detect and reject duplicate entries (same dep name in the same tag type)
- [x] Insert new elements in canonical order (after the last existing element of
  the same or preceding dep type)
- [x] Write back the file preserving existing comments, indentation style, and
  the `<?xml?>` / `<?xml-model?>` processing instructions
- [x] Print confirmation: `Added <depend>sensor_msgs</depend> to package.xml`

### 2.3 `rosup remove`

- [x] Remove **all** dependency tags whose text content matches the given name
- [x] `-p <pkg>` for workspace mode
- [x] Warn if no matching dependency was found (do not error)
- [x] Write back the file preserving everything else
- [x] Print each removed element: `Removed <depend>sensor_msgs</depend>`

---

## Acceptance Criteria

- [x] `rosup init` in a dir with `package.xml` produces valid `rosup.toml` in
  package mode with correct name
- [x] `rosup init` in a dir with `src/` containing packages produces valid
  `rosup.toml` in workspace mode with all discovered members listed
- [x] `rosup init` refuses to overwrite an existing `rosup.toml` without `--force`
- [x] Generated `rosup.toml` is valid and parses without error
- [x] `rosup add sensor_msgs` inserts `<depend>sensor_msgs</depend>` in canonical
  position
- [x] `rosup add ament_cmake_gtest --test` inserts `<test_depend>` specifically
- [x] `rosup add` with `-p my_pkg` modifies the correct package in a workspace
- [x] Adding an already-present dependency prints a message and makes no change
- [x] `rosup remove sensor_msgs` removes all tags whose content is `sensor_msgs`
- [x] Written XML preserves the original `<?xml?>` declaration, `<?xml-model?>`
  PI, comments, and indentation style
- [x] `rosup remove nonexistent_dep` prints a warning, exits 0, makes no change
