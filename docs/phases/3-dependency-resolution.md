# Phase 3 — Dependency Resolution

Implement the three-tier dependency resolution pipeline: ament environment,
rosdep binary, and rosdistro source pull.

## Work Items

### 3.1 rosdistro Cache

- [x] Download and cache distribution YAML from `repo.ros2.org/rosdistro_cache/`
- [x] Parse distribution cache: map package names to source repo URLs and versions
- [x] Cache expiry / manual refresh (`rox resolve --refresh`)
- [x] Store cache in `~/.rox/cache/`

### 3.2 Ament Environment Check

- [x] Query `ament_index` (or `AMENT_PREFIX_PATH`) for installed packages
- [x] Mark satisfied dependencies as resolved

### 3.3 rosdep Binary Resolution

- [x] Shell out to `rosdep resolve <dep>` to check if a binary is available
- [x] Shell out to `rosdep install` for actual installation
- [x] Handle `--simulate` / dry-run mode

### 3.4 Source Pull Resolution

- [x] Look up source URL and branch from rosdistro cache
- [x] Clone into `~/.rox/src/{pkg}` (or update existing clone)
- [ ] Build source-pulled packages with colcon into `~/.rox/install/` (Phase 4)

### 3.5 Override Handling

- [x] Override source URL, branch, rev for specific deps from `[resolve.overrides]` in `rox.toml`

### 3.6 `rox resolve` Command

- [x] Run full resolution pipeline
- [x] `--dry-run`: print resolution plan without installing
- [x] `--source-only`: skip rosdep, force source pull

### 3.7 Resolution Integration

- [x] Run resolution as a pre-step for `build`, `test`, `run`, `launch`
- [x] Implement `--no-resolve` flag to skip resolution

## Acceptance Criteria

- [x] `rox resolve --dry-run` lists each dep with its resolution method (ament/binary/source)
- [x] Deps already in ament environment are detected and skipped
- [x] rosdep-resolvable deps are installed via system package manager
- [x] Deps not in rosdep fall back to source clone from rosdistro
- [x] `[resolve.overrides.X]` in `rox.toml` overrides the source URL for dep X
- [x] Source-pulled packages are stored in `~/.rox/src/`
- [x] Resolution runs automatically before `rox build` unless `--no-resolve` is passed
- [x] `rox resolve` with no args resolves all deps for all workspace members
- [x] Errors are clear when a dep is not found in any tier
- [x] Repeated resolution is idempotent (no re-clone, no re-install if already satisfied)
