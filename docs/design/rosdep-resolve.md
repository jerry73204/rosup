# rosdep resolve vs rosdep install

## Background

rosup uses `rosdep` for two operations:

1. **Query** — `rosdep resolve <key> --rosdistro <distro>` to check if a
   dependency is available as a binary package and discover its system
   package name.
2. **Install** — install the resolved binary dependencies via the system
   package manager.

Previously, rosup handled step 2 by constructing a temporary `package.xml`
with the dependency keys and calling `rosdep install --from-paths <tmpdir>
--ignore-src`. This design doc evaluates whether using `rosdep resolve`
exclusively and calling installers directly is a better approach.

## How `rosdep resolve` works

```
$ rosdep resolve rclcpp --rosdistro humble
#apt
ros-humble-rclcpp
```

Output format: `#<installer>\n<package1>\n<package2>\n...`

- Returns the installer name (apt, pip, etc.) and the exact system package
  names.
- Works with any rosdep key — ROS package names, pure system keys (`asio`,
  `libpcl-dev`), pip packages.
- Fails cleanly with "No definition" when a key has no rosdep rule.
- Does not require a `package.xml` — takes keys directly as arguments.

## How `rosdep install` works

```
$ rosdep install --from-paths <dir> --ignore-src --rosdistro humble
```

- Discovers `package.xml` files under `<dir>`, extracts dependency keys.
- Resolves each key through rosdep rules, then invokes the installer.
- `--ignore-src` skips keys that match packages found in `<dir>`.
- `--default-yes` passes `-y` to apt.
- `--simulate` shows what would be installed without doing it.

### The temporary package.xml workaround

rosup previously wrote a fake `package.xml` with `<exec_depend>` entries for
each key, then pointed `rosdep install --from-paths` at the temp directory.
This was necessary because `rosdep install` operates on directories, not
individual keys.

## Comparison

| Aspect | `rosdep resolve` + direct install | `rosdep install --from-paths` |
|--------|-----------------------------------|-------------------------------|
| Input | Individual keys | Directory with package.xml |
| Temp files | None | Fake package.xml required |
| Error granularity | Per-key failure with reason | Opaque "install failed" |
| Dry-run output | Exact `apt-get install <pkgs>` | `rosdep install --simulate` |
| Installer control | Full (batching, ordering) | Delegated to rosdep |
| Key type support | All (ROS, system, pip) | All (via package.xml) |
| Pure system keys | Works directly | Works (via exec_depend) |
| Multi-installer | Groups by installer (apt/pip) | Handles internally |
| Traceability | Key → system package mapping | Not visible |

## Edge cases tested

| Key type | `rosdep resolve` behavior |
|----------|--------------------------|
| ROS package (`rclcpp`) | `#apt\nros-humble-rclcpp` |
| Pure system key (`asio`) | `#apt\nlibasio-dev` |
| Multi-package key | `#apt\npkg1\npkg2` |
| No rosdep rule | Exit 1, "No definition" |
| Pip dependency (`empy`) | `#pip\nempy` |

All edge cases work correctly with `rosdep resolve`. The key insight is that
`rosdep resolve` operates on the same rosdep rules database as `rosdep
install` — the resolution logic is identical, only the execution differs.

## Decision

**Use `rosdep resolve` exclusively, call installers directly.**

Reasons:

1. **No temp files** — eliminates the fake `package.xml` workaround.
2. **Better error messages** — when installation fails, rosup can show
   which rosdep key mapped to which system package.
3. **Per-key failure handling** — `resolve_all()` collects failures
   individually rather than aborting on the first one.
4. **Dry-run clarity** — shows the exact `apt-get install` / `pip install`
   command that would run, not the opaque `rosdep install --simulate`.
5. **Installer grouping** — batches all apt packages into one call, all pip
   packages into another, reducing sudo prompts.

The old `rosdep install` function is retained as a fallback for unknown
installer types that rosup doesn't handle directly.

## Implementation

**File:** `crates/rosup-core/src/resolver/rosdep.rs`

New public API:

- `resolve_all(keys, distro)` — batch-resolve all keys, group by installer,
  collect failures separately.
- `install_direct(resolved, dry_run, yes)` — call `apt-get install` / `pip
  install` directly from grouped results.
- `ResolveAllResult` — result struct with `by_installer` map and `failures`.
- `InstalledPackage` — tracks the rosdep key → system package mapping.

The resolver's `execute()` now calls `resolve_all()` + `install_direct()`
instead of `rosdep::install()`.
