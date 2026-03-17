# Phase 5 — Search and Clone

Implement `rox search` and `rox clone` for discovering and fetching packages
from the ROS index.

## Work Items

- [ ] Implement `rox search`
  - [ ] Load rosdistro cache (download if missing)
  - [ ] Substring / fuzzy match package names against query
  - [ ] Display results: package name, description, source URL, status
  - [ ] Support `--distro` filter
  - [ ] Support `--limit` to cap result count
- [ ] Implement `rox clone`
  - [ ] Look up package in rosdistro cache
  - [ ] Resolve source repository URL and branch
  - [ ] Clone repository to current directory
  - [ ] Handle multi-package repos (rosdistro repos often contain multiple packages)
  - [ ] Run `rox init` in the cloned directory
  - [ ] Support `--distro` to select distribution

## Acceptance Criteria

- [ ] `rox search navigation` returns relevant packages (nav2_core, navigation2, etc.)
- [ ] `rox search costmap --distro jazzy` filters results to jazzy distribution
- [ ] `rox search` with no matches prints a clear message
- [ ] `rox clone nav2_core` clones the correct repo and creates `rox.toml`
- [ ] `rox clone nav2_core --distro jazzy` uses the jazzy-specific source URL
- [ ] `rox clone` of a nonexistent package prints a helpful error
- [ ] Multi-package repos are cloned fully (user can pick which to build via `-p`)
- [ ] Cache is downloaded on first search/clone if not present
