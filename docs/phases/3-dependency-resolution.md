# Phase 3 — Dependency Resolution

Implement the three-tier dependency resolution pipeline: ament environment,
rosdep binary, and rosdistro source pull.

## Work Items

### 3.1 rosdistro Cache

- [ ] Download and cache distribution YAML from `repo.ros2.org/rosdistro_cache/`
- [ ] Parse distribution cache: map package names to source repo URLs and versions
- [ ] Cache expiry / manual refresh (`rox resolve --refresh`)
- [ ] Store cache in `~/.rox/cache/`

### 3.2 Ament Environment Check

- [ ] Query `ament_index` (or `AMENT_PREFIX_PATH`) for installed packages
- [ ] Mark satisfied dependencies as resolved

### 3.3 rosdep Binary Resolution

- [ ] Shell out to `rosdep resolve <dep>` to check if a binary is available
- [ ] Shell out to `rosdep install` for actual installation
- [ ] Handle `--simulate` / dry-run mode

### 3.4 Source Pull Resolution

- [ ] Look up source URL and branch from rosdistro cache
- [ ] Clone into `~/.rox/src/{pkg}` (or update existing clone)
- [ ] Build source-pulled packages with colcon into `~/.rox/install/`

### 3.5 Override Handling

- [ ] Override source URL, branch, rev for specific deps from `[resolve.overrides]` in `rox.toml`

### 3.6 `rox resolve` Command

- [ ] Run full resolution pipeline
- [ ] `--dry-run`: print resolution plan without installing
- [ ] `--source-only`: skip rosdep, force source pull

### 3.7 Resolution Integration

- [ ] Run resolution as a pre-step for `build`, `test`, `run`, `launch`
- [ ] Implement `--no-resolve` flag to skip resolution

## Acceptance Criteria

- [ ] `rox resolve --dry-run` lists each dep with its resolution method (ament/binary/source)
- [ ] Deps already in ament environment are detected and skipped
- [ ] rosdep-resolvable deps are installed via system package manager
- [ ] Deps not in rosdep fall back to source clone from rosdistro
- [ ] `[resolve.overrides.X]` in `rox.toml` overrides the source URL for dep X
- [ ] Source-pulled packages are stored in `~/.rox/src/` and built in `~/.rox/install/`
- [ ] Resolution runs automatically before `rox build` unless `--no-resolve` is passed
- [ ] `rox resolve` with no args resolves all deps for all workspace members
- [ ] Errors are clear when a dep is not found in any tier
- [ ] Repeated resolution is idempotent (no re-clone, no re-install if already satisfied)
