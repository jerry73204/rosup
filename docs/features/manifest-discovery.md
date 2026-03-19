# Feature: Manifest Discovery (Cargo Convention)

## Problem

rosup currently requires `rosup.toml` to be in the current working directory
(or a parent directory). Cargo's convention is more flexible: it searches
upward from the current directory and also supports `--manifest-path` for
explicit override. This enables running rosup commands from inside a package
subdirectory or from a script with an absolute path.

## Current Behaviour

rosup walks up from `$CWD` until it finds `rosup.toml`. If not found, it
errors. There is no way to specify a manifest path explicitly.

## Proposed Behaviour (Cargo Convention)

### 1. Upward search from `$CWD`

Unchanged. rosup walks up from the current directory until `rosup.toml` is
found or the filesystem root is reached.

### 2. `--manifest-path` global flag

```bash
rosup --manifest-path /path/to/rosup.toml build
rosup --manifest-path ~/my_ws/rosup.toml list pkgs
```

Bypasses the upward search entirely. The directory containing the specified
file is used as the project root.

Mirrors Cargo exactly:

```bash
cargo --manifest-path /path/to/Cargo.toml build
```

### 3. Stop at filesystem boundaries

Cargo stops upward search at filesystem boundaries (different device IDs) and
at the user's home directory. rosup should do the same to avoid accidentally
picking up a `rosup.toml` in an unrelated parent directory.

### 4. Workspace root vs package root

In Cargo, a package inside a workspace can be addressed either from its own
directory or from the workspace root. rosup should mirror this:

- Running `rosup build` from inside `src/my_pkg/` should find the workspace
  `rosup.toml` at the workspace root and operate on the full workspace (or
  scope to `my_pkg` implicitly via the `-p` default; see below).
- Running `rosup build` from the workspace root operates on all packages.

#### Implicit `-p` scoping

When the current directory is inside a workspace member package, rosup may
optionally default `-p` to that package (like `cargo build` in a workspace
member defaults to the current package). This should be opt-in or clearly
documented, as it differs from colcon behaviour.

## Known Issue: Current Behaviour Inconsistency

The upward search is implemented but the stop conditions (filesystem boundary,
home directory) are not. This can cause unexpected behaviour when rosup is run
from inside `/home/<user>/` and finds a stale `rosup.toml` from a different
project.

**Workaround:** Always run rosup from the project root.

## Implementation Notes

- Add `--manifest-path: Option<PathBuf>` to the top-level `Cli` struct in
  `rosup-cli`.
- In `Project::load_from`, if `--manifest-path` is set, use its parent
  directory as the project root instead of searching.
- Add filesystem boundary detection to `Project::load_from` using
  `std::fs::metadata(...).dev()` (Unix) to stop the upward walk when the
  device ID changes.
- Stop the upward walk at `dirs::home_dir()` as an additional safety bound.
