# Phase 8 — Workspace Auto-Discovery

Make `members` optional in `[workspace]`. When omitted, rosup discovers
packages using Colcon's own rules. When present, entries are glob patterns
(not explicit paths). `rosup init --workspace` and a new `rosup sync` command
replace the old explicit-list approach.

---

## 8.1 Config: make `members` optional

**File:** `crates/rosup-core/src/config.rs`

- [x] Change `WorkspaceConfig::members` from `Vec<String>` to
  `Option<Vec<String>>` with `#[serde(default)]`.
- [x] An empty `[workspace]` section (no `members` key) must parse
  successfully — this is the auto-discovery signal.
- [x] Update the existing `workspace_mode` config test: add a case for
  `members` absent (auto-discovery).

**Acceptance criteria:**
- `toml::from_str("[workspace]\n")` succeeds and yields
  `WorkspaceConfig { members: None, exclude: [] }`.
- `toml::from_str("[workspace]\nmembers = [\"src/*\"]\n")` yields
  `WorkspaceConfig { members: Some(["src/*"]), exclude: [] }`.

---

## 8.2 Colcon discovery scanner

**File:** `crates/rosup-core/src/init.rs` (extend `collect_members` /
add `colcon_scan`)

Implement a recursive directory walker that exactly replicates Colcon's
package discovery rules.

### Ignore markers (skip directory and all descendants)
- [x] `COLCON_IGNORE` present in the directory → skip
- [x] `AMENT_IGNORE` present in the directory → skip
- [x] `CATKIN_IGNORE` present in the directory → skip

### Always-skipped directory names
- [x] `build`, `install`, `log` (standard Colcon output dirs)
- [x] `.rosup` (rosup internal)
- [x] Any directory whose name starts with `.` (hidden dirs)

### Discovery rule
- [x] When `package.xml` is found in a directory: record the directory
  as a member and **do not recurse further** (ROS packages do not nest).
- [x] Paths are returned relative to the workspace root, sorted
  lexicographically.

### Signature
```rust
pub fn colcon_scan(
    workspace_root: &Path,
    exclude: &[glob::Pattern],
) -> Result<Vec<String>, InitError>
```

**Acceptance criteria (unit tests in `init.rs`):**
- [x] Basic scan finds packages under `src/`.
- [x] Directory containing `COLCON_IGNORE` and all its children are skipped.
- [x] `AMENT_IGNORE` and `CATKIN_IGNORE` are also respected.
- [x] Directories named `build`, `install`, `log`, `.rosup`, `.hidden`
  are skipped.
- [x] Nested packages are not discovered (stops at first `package.xml`).
- [x] `exclude` patterns remove matching results.

---

## 8.3 Member discovery in `project.rs`

**File:** `crates/rosup-core/src/project.rs`

Update `discover_members` to dispatch on whether `members` is set.

### Auto-discovery (`members = None`)
- [x] Call `colcon_scan(root, &excludes)` from 8.2.
- [x] Parse `package.xml` for each discovered path.

### Glob mode (`members = Some(patterns)`)
- [x] Expand each glob pattern relative to `workspace_root`.
- [x] For each matched directory: check for `package.xml` directly inside
  it (do not recurse; the glob controls depth via `*` vs `**`).
- [x] Apply `exclude` patterns (relative to workspace root).
- [x] **Warn** (via `tracing::warn!`) when a glob pattern matches zero
  directories. Do not error.

**Acceptance criteria (unit tests in `project.rs`):**
- [x] `members = None` discovers all packages via Colcon rules.
- [x] `members = None` with `exclude` omits matched paths.
- [x] `members = ["src/*"]` discovers direct children of `src/` only.
- [x] `members = ["src/**"]` discovers all descendants of `src/`.
- [x] A glob that matches nothing emits a warning and returns an empty
  result for that pattern (other patterns still resolved).
- [x] Existing `discover_workspace_members` test continues to pass.

---

## 8.4 `rosup init` changes

**Files:** `crates/rosup-core/src/init.rs`,
`crates/rosup-cli/src/main.rs`

### `rosup init --workspace` (no `--lock`)
- [x] Generate `[workspace]\n` with **no** `members` key — enables
  auto-discovery.
- [x] `rosup init --workspace` without `--lock` must not call
  `colcon_scan`; just write the empty section.

### `rosup init --workspace --lock`
- [x] Run `colcon_scan` and write the full explicit member list.
- [x] Format: `members = [\n  "path",\n  ...\n]\n`.

### CLI changes
- [x] Add `--lock` flag to the `Init` command (only meaningful with
  `--workspace`).
- [x] If `--lock` is passed without `--workspace`, emit a warning and
  ignore it.
- [x] Update `cmd_init` output message: distinguish empty-workspace vs
  locked-workspace.

**Acceptance criteria (integration tests in `rosup-tests/tests/init.rs`):**
- [x] `rosup init --workspace` generates a `rosup.toml` with
  `[workspace]` and **no** `members = ` line.
- [x] `rosup init --workspace --lock` generates a `rosup.toml` with a
  populated `members` list.
- [x] Existing `init_workspace_mode` integration test updated to use
  `--lock` if it expects an explicit list.

---

## 8.5 `rosup sync` command

**Files:** `crates/rosup-cli/src/main.rs`,
`crates/rosup-core/src/init.rs`

New command for users who maintain a pinned member list.

```
rosup sync [--dry-run] [--check] [--lock]
```

| Option      | Description                                                        |
|-------------|--------------------------------------------------------------------|
| `--dry-run` | Print what would change; do not write                              |
| `--check`   | Exit non-zero if the member list is out of date (CI mode)          |
| `--lock`    | Convert auto-discovery `[workspace]` to an explicit member list     |

### Behaviour

- [x] Load the project; error if not a workspace.
- [x] Run `colcon_scan` to get the current ground-truth member list.
- [x] Compare against the existing `members` list in `rosup.toml`
  (if any).
- [x] In `--dry-run` / `--check` mode: print added/removed entries;
  `--check` exits non-zero if there are any changes.
- [x] Otherwise: write the updated `members` list back to `rosup.toml`.

### TOML update strategy

Preserve all other content in `rosup.toml`. Use a targeted line-level
patch:

- [x] Locate the `members = [` block in the file text (may be absent if
  in auto-discovery mode).
- [x] Replace/insert the `members = [\n  ...\n]` block, keeping all
  other lines intact.
- [x] If `members` is absent and `--lock` is set: insert after the
  `[workspace]` header line.
- [x] If `members` is absent and `--lock` is not set: no-op (already
  in auto-discovery mode).

**Acceptance criteria (integration tests in
`rosup-tests/tests/sync.rs`):**
- [x] `rosup sync` on an explicit-list workspace updates changed entries.
- [x] `rosup sync --dry-run` prints the diff and does not modify the file.
- [x] `rosup sync --check` exits non-zero when the list is stale.
- [x] `rosup sync --check` exits zero when the list is up to date.
- [x] `rosup sync --lock` on an auto-discovery workspace writes the full
  member list.
- [x] `rosup sync` on an auto-discovery workspace (no `--lock`) is a
  no-op with an informational message.

---

## 8.6 Documentation updates

- [x] Update `docs/reference/rosup-toml.md`: document optional `members`,
  auto-discovery behaviour, glob semantics (`*` vs `**`), `exclude`.
- [x] Update `docs/reference/cli.md`: document `rosup init --lock` and
  new `rosup sync` command.
- [x] Update `docs/features/workspace-rescan.md`: mark design as
  superseded by this phase (implemented).
- [x] Update `docs/known-issues.md`: close KI-001.
