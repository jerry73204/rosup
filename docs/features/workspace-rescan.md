# Feature: Workspace Rescan (`rosup sync`)

## Problem

`rosup.toml` lists workspace members explicitly. In large Colcon workspaces
(e.g. Autoware with 450+ packages), the member list becomes unwieldy to
maintain by hand. Packages are added, removed, or moved between directories
regularly, and the manifest drifts out of sync.

Running `rosup init --workspace --force` would regenerate the full list but
overwrites any manual customisations (excludes, ordering).

## Proposed Command

```
rosup sync [--dry-run] [--check]
```

Re-scans the workspace for packages and updates the `members` list in
`rosup.toml` in place.

| Option      | Description                                                      |
|-------------|------------------------------------------------------------------|
| `--dry-run` | Print what would change without writing to `rosup.toml`          |
| `--check`   | Exit non-zero if the member list is out of date (for CI)         |

## Colcon Scan Behaviour

The scan must replicate Colcon's package discovery rules exactly so the member
list matches what `colcon build` would see.

### Ignore markers

Colcon skips a directory (and all its children) when it encounters any of the
following:

| Marker               | Notes                                        |
|----------------------|----------------------------------------------|
| `COLCON_IGNORE`      | Empty file; presence causes the dir to skip  |
| `AMENT_IGNORE`       | Legacy alias for `COLCON_IGNORE`             |
| `CATKIN_IGNORE`      | Legacy alias for `COLCON_IGNORE`             |
| `.git`               | Directories containing `.git` at depth > 0 are skipped by default in some colcon versions |

### Standard excluded directories

Colcon also skips directories named:

- `build`, `install`, `log` (standard Colcon output directories)
- `.rosup` (rosup internal directory)
- Hidden directories (`.`-prefixed) other than the workspace root

### Discovery depth

Colcon walks the directory tree recursively. When a directory containing
`package.xml` is found, it is added as a package and **not** descended into
further (ROS packages do not nest).

## Interaction with `exclude`

The existing `exclude` field in `[workspace]` is respected:

```toml
[workspace]
members = [...]           # managed by rosup sync
exclude = ["src/legacy/*"]   # preserved; sync never adds excluded paths
```

`rosup sync` will never add a path that matches an `exclude` glob, and will
never remove a path that is only excluded (not absent).

## Diff output

```
$ rosup sync --dry-run
~ rosup.toml members (453 → 455):
  + src/universe/autoware_new_pkg
  + src/universe/autoware_another_pkg
  - src/core/autoware_removed_pkg
Write rosup.toml? [y/N]
```

## Known Edge Cases

- **Symlinks**: follow symlinks into directories unless they form a cycle.
- **Out-of-tree builds**: the scan root is always the workspace root, not any
  build or install directory.
- **colcon.pkg**: packages declared via `colcon.pkg` (non-`package.xml` build
  systems) are out of scope for the initial implementation. rosup only tracks
  ROS packages.

## Implementation Notes

- Reuse `init::collect_members` scan logic in `rosup-core`.
- The update must be a targeted diff of the `members` array in the TOML file,
  preserving all other fields, comments, and formatting. Use a line-level
  patch to the `members = [...]` block, consistent with the approach used in
  `manifest.rs`.
- New command variant `Command::Sync` in `rosup-cli`.
