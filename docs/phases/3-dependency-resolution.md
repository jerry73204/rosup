# Phase 3 — Dependency Resolution

Implement the three-tier dependency resolution pipeline: ament environment
check, rosdep binary install, and rosdistro source pull. Includes the storage
layer that backs source dependency management.

## Work Items

### 3.1 rosdistro Cache

- [x] Download and decompress distribution YAML from `repo.ros2.org/rosdistro_cache/`
- [x] Parse distribution cache: map package names → (repo name, source URL, branch)
- [x] Cache expiry (24 h TTL) and manual refresh (`rox resolve --refresh`)
- [x] Store cache YAML in `~/.rox/cache/{distro}-cache.yaml`
- [x] Document rosdistro cache format in `docs/reference/rosdistro-cache.md`

### 3.2 Ament Environment Check

- [x] Query `AMENT_PREFIX_PATH` for installed packages via ament index marker files
- [x] Mark satisfied dependencies as resolved (skip resolution for them)

### 3.3 rosdep Binary Resolution

- [x] Shell out to `rosdep resolve <dep> --rosdistro <distro>` to detect binary availability
- [x] Shell out to `rosdep install --default-yes` for actual installation
- [x] Dry-run mode: plan without executing

### 3.4 Source Pull Resolution

- [x] Look up repo name, source URL, and branch from rosdistro cache
- [x] Support user-defined overrides from `[resolve.overrides]` in `rox.toml`
- [ ] **Fix:** rewrite `source_pull` to use two-level bare clone + worktree model (see 3.7)
- [ ] **Fix:** group source deps by repository before any git operations

### 3.5 `rox resolve` Command

- [x] Run full resolution pipeline (ament → rosdep → source)
- [x] `--dry-run`: print resolution plan without installing or cloning
- [x] `--source-only`: skip rosdep, force source pull for all deps
- [x] `--refresh`: force re-download of the rosdistro cache
- [x] Collect deps from all workspace members in workspace mode

### 3.6 Resolution Integration

- [x] Run resolution as a pre-step for `rox build` and `rox test`
- [x] `--no-resolve` flag on `build` and `test` to skip resolution

### 3.7 Store Refactor (fixes to existing code)

The current `RoxDir` struct conflates global and per-project storage. Split it
into two types to match the two-level design.

- [x] Replace `RoxDir` with `GlobalStore` and `ProjectStore`:
  - `GlobalStore { cache: PathBuf, src: PathBuf }` — `~/.rox/cache/` and `~/.rox/src/`
  - `ProjectStore { src: PathBuf, build: PathBuf, install: PathBuf }` — `<root>/.rox/`
- [x] `GlobalStore::open()` — resolves from `~/.rox/`
- [x] `ProjectStore::open(root)` — resolves from `<root>/.rox/`
- [x] Both expose `ensure()` to create directories on first use
- [x] Update `Resolver` to hold both `GlobalStore` and `ProjectStore`
- [x] Rewrite `source_pull` to use bare clone + git worktree:
  1. Bare clone keyed by repo name into `~/.rox/src/<repo>.git`
     (`git clone --bare <url> <bare>`, or `git -C <bare> fetch --quiet origin`)
  2. Worktree per project: `git -C <bare> worktree add --detach <project>/.rox/src/<repo> <target>`
  3. On update: `git -C <worktree> reset --hard <target>`
  4. Rev pin: target is the commit hash instead of branch name
- [x] Group source-pull deps by repo name before git operations — multi-package repos
      produce one bare clone and one worktree regardless of how many packages are needed
- [x] `rox init` adds `.rox/` to the project `.gitignore`

## Acceptance Criteria

- [x] `rox resolve --dry-run` lists each dep with its resolution method
- [x] Deps already in ament environment are detected and skipped
- [x] rosdep-resolvable deps are installed via system package manager
- [x] Deps not in rosdep fall back to source clone from rosdistro
- [x] `[resolve.overrides.X]` in `rox.toml` overrides the source URL for dep X
- [x] Resolution runs automatically before `rox build` unless `--no-resolve` is passed
- [x] `rox resolve` with no args resolves all deps for all workspace members
- [x] Errors are clear when a dep is not found in any tier
- [x] `~/.rox/src/<repo>.git` is a bare clone; per-project `.rox/src/<repo>/` is a worktree
- [x] Two projects needing the same repo at different branches each get their own worktree
- [x] Multi-package repos produce one clone and one worktree (not one per package)
- [x] `.rox/` is added to `.gitignore` by `rox init`
- [ ] Repeated resolution is idempotent (no re-clone, no re-install if already satisfied)
