# Phase 5 — Search and Clone

Implement `rosup search` and `rosup clone` for discovering and fetching packages
from the ROS index.

## Work Items

### 5.1 `rosup search`

- [x] Load rosdistro cache (download if missing)
- [x] Substring / fuzzy match package names against query
- [x] Display results: package name, description, source URL, status
- [x] Support `--distro` filter
- [x] Support `--limit` to cap result count

### 5.2 `rosup clone`

- [x] Look up package in rosdistro cache
- [x] Resolve source repository URL and branch
- [x] Clone repository to current directory
- [x] Handle multi-package repos (rosdistro repos often contain multiple packages)
- [x] Run `rosup init` in the cloned directory
- [x] Support `--distro` to select distribution

## Acceptance Criteria

- [x] `rosup search navigation` returns relevant packages (nav2_core, navigation2, etc.)
- [x] `rosup search costmap --distro jazzy` filters results to jazzy distribution
- [x] `rosup search` with no matches prints a clear message
- [x] `rosup clone nav2_core` clones the correct repo and creates `rosup.toml`
- [x] `rosup clone nav2_core --distro jazzy` uses the jazzy-specific source URL
- [x] `rosup clone` of a nonexistent package prints a helpful error
- [x] Multi-package repos are cloned fully (user can pick which to build via `-p`)
- [x] Cache is downloaded on first search/clone if not present
