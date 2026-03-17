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

- [ ] Detect auto mode: workspace if `src/` dir exists, single-package if
  `package.xml` exists in CWD, error otherwise
- [ ] Single-package mode: read `<name>` and `<build_type>` from `package.xml`;
  generate `rox.toml` with `[package]`
- [ ] Workspace mode: scan `src/` recursively for `package.xml` files; generate
  `rox.toml` with `[workspace].members`
- [ ] `--workspace` flag forces workspace mode even without a `src/` dir
- [ ] Refuse to overwrite an existing `rox.toml` without `--force`
- [ ] Print the path of the generated `rox.toml` on success

### 2.2 `rox add`

- [ ] Parse the target `package.xml` into a DOM (preserving comments and
  whitespace structure)
- [ ] Default tag: `<depend>` (build + build_export + exec)
- [ ] `--build` → `<build_depend>`
- [ ] `--exec` → `<exec_depend>`
- [ ] `--test` → `<test_depend>`
- [ ] `--dev` → `<build_depend>` + `<test_depend>`
- [ ] `-p <pkg>` targets a specific member package in workspace mode; defaults
  to the package in CWD
- [ ] Detect and reject duplicate entries (same dep name in the same tag type)
- [ ] Insert new elements in canonical order (after the last existing element of
  the same or preceding dep type)
- [ ] Write back the file preserving existing comments, indentation style, and
  the `<?xml?>` / `<?xml-model?>` processing instructions
- [ ] Print confirmation: `Added <depend>sensor_msgs</depend> to package.xml`

### 2.3 `rox remove`

- [ ] Parse the target `package.xml`
- [ ] Remove **all** dependency tags (`<depend>`, `<build_depend>`,
  `<exec_depend>`, `<test_depend>`, etc.) whose text content matches the given
  name
- [ ] `-p <pkg>` for workspace mode
- [ ] Warn if no matching dependency was found (do not error)
- [ ] Write back the file preserving everything else
- [ ] Print each removed element: `Removed <depend>sensor_msgs</depend>`

---

## Acceptance Criteria

- [ ] `rox init` in a dir with `package.xml` produces valid `rox.toml` in
  package mode with correct name and build_type
- [ ] `rox init` in a dir with `src/` containing packages produces valid
  `rox.toml` in workspace mode with all discovered members listed
- [ ] `rox init` refuses to overwrite an existing `rox.toml` without `--force`
- [ ] Generated `rox.toml` is valid and parses without error
- [ ] `rox add sensor_msgs` inserts `<depend>sensor_msgs</depend>` in canonical
  position
- [ ] `rox add ament_cmake_gtest --test` inserts `<test_depend>` specifically
- [ ] `rox add` with `-p my_pkg` modifies the correct package in a workspace
- [ ] Adding an already-present dependency prints a message and makes no change
- [ ] `rox remove sensor_msgs` removes all tags whose content is `sensor_msgs`
- [ ] Written XML preserves the original `<?xml?>` declaration, `<?xml-model?>`
  PI, comments, and indentation style
- [ ] `rox remove nonexistent_dep` prints a warning, exits 0, makes no change
