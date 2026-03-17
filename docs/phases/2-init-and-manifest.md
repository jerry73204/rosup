# Phase 2 — Init and Manifest Management

Implement `rox init`, `rox add`, and `rox remove` — the commands that create
and modify project manifests.

## Background

See `docs/reference/package-xml.md` for the full format 3 spec. Key points
that affect this phase:

- Element order is **not significant** to the parser, but rox should write
  files following the canonical ordering convention.
- All text values are whitespace-normalized on read; rox should write clean,
  consistently indented XML.
- The `<export><build_type>` element is required for colcon to recognise a
  package as ROS 2. `rox init` must emit it.
- `<depend>` is the right default tag for `rox add` (covers build + build_export
  + exec). More specific tags (`--build`, `--exec`, `--test`) are opt-in.
- `condition` attribute: rox does not need to evaluate conditions — it is the
  user's responsibility to write correct condition expressions. rox preserves
  them on read/write.
- Comments in package.xml must be preserved on write.

---

## Work Items

### 2.1 `rox init`

- [x] Detect auto mode: workspace if `src/` dir exists, single-package if
  `package.xml` exists in CWD, error otherwise
- [x] Single-package mode: read `<name>` and `<build_type>` from `package.xml`;
  generate `rox.toml` with `[package]`
- [x] Workspace mode: scan `src/` recursively for `package.xml` files; generate
  `rox.toml` with `[workspace].members`
- [x] `--workspace` flag forces workspace mode even without a `src/` dir
- [x] Refuse to overwrite an existing `rox.toml` without `--force`
- [x] Print the path of the generated `rox.toml` on success

### 2.2 `rox add`

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

### 2.3 `rox remove`

- [x] Remove **all** dependency tags whose text content matches the given name
- [x] `-p <pkg>` for workspace mode
- [x] Warn if no matching dependency was found (do not error)
- [x] Write back the file preserving everything else
- [x] Print each removed element: `Removed <depend>sensor_msgs</depend>`

---

## Acceptance Criteria

- [x] `rox init` in a dir with `package.xml` produces valid `rox.toml` in
  package mode with correct name
- [x] `rox init` in a dir with `src/` containing packages produces valid
  `rox.toml` in workspace mode with all discovered members listed
- [x] `rox init` refuses to overwrite an existing `rox.toml` without `--force`
- [x] Generated `rox.toml` is valid and parses without error
- [x] `rox add sensor_msgs` inserts `<depend>sensor_msgs</depend>` in canonical
  position
- [x] `rox add ament_cmake_gtest --test` inserts `<test_depend>` specifically
- [x] `rox add` with `-p my_pkg` modifies the correct package in a workspace
- [x] Adding an already-present dependency prints a message and makes no change
- [x] `rox remove sensor_msgs` removes all tags whose content is `sensor_msgs`
- [x] Written XML preserves the original `<?xml?>` declaration, `<?xml-model?>`
  PI, comments, and indentation style
- [x] `rox remove nonexistent_dep` prints a warning, exits 0, makes no change
