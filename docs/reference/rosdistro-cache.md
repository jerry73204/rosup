# rosdistro Cache Format

## Overview

The rosdistro cache is a gzip-compressed YAML file published by the ROS
infrastructure at:

```
https://repo.ros2.org/rosdistro_cache/{distro}-cache.yaml.gz
```

For example, the Humble cache is at:

```
https://repo.ros2.org/rosdistro_cache/humble-cache.yaml.gz
```

rox decompresses this file and stores it at `~/.rox/cache/{distro}-cache.yaml`.
A companion timestamp file (`~/.rox/cache/{distro}-cache.timestamp`) records
when the cache was last fetched. The cache is considered stale after 24 hours
and is re-fetched automatically. Use `rox resolve --refresh` to force a
re-download.

## Top-Level Structure

```yaml
distribution_file:
  - type: distribution
    version: 2
    release_platforms:
      ubuntu:
        - jammy
    repositories:
      <repo_name>:
        source: { ... }
        release: { ... }
        status: maintained
```

`distribution_file` is a list; rox uses the first element.

## Repository Entry

Each key under `repositories` is the **repository name** (not necessarily the
package name). A repository may contain one or more ROS packages.

```yaml
repositories:
  rclcpp:
    source:
      type: git
      url: https://github.com/ros2/rclcpp.git
      version: humble          # branch or tag
    release:
      packages:
        - rclcpp
        - rclcpp_action
        - rclcpp_components
        - rclcpp_lifecycle
      url: https://github.com/ros2-gbp/rclcpp-release.git
      version: 16.0.18-1
    status: maintained
```

### `source` section

| Field     | Description                                      |
|-----------|--------------------------------------------------|
| `type`    | VCS type (`git`, `svn`, …). rox only supports `git`. |
| `url`     | Clone URL for the source repository.             |
| `version` | Branch or tag to check out.                      |

### `release` section

| Field      | Description                                                          |
|------------|----------------------------------------------------------------------|
| `packages` | List of ROS package names provided by this repository. **This is the key field for mapping package names to source repos.** |
| `url`      | Release repository URL (binary package releases). Not used by rox.  |
| `version`  | Release version string. Not used by rox.                            |

If `release.packages` is absent or empty, rox falls back to treating the
repository name itself as the single package name.

## Package-to-Repository Mapping

rox builds an index of `package_name → PackageSource` at load time:

1. Iterate over all repositories in `distribution_file[0].repositories`.
2. Skip entries with no `source` section or with `source.type != "git"`.
3. Read `release.packages` to enumerate all package names in the repo.
4. Map each package name to `(repo_name, source.url, source.version)`.

This allows rox to resolve a dependency like `sensor_msgs` to
`https://github.com/ros2/common_interfaces.git @ humble`, even though the repo
is named `common_interfaces`.

## Example: Single-package Repository

```yaml
solo_pkg:
  source:
    type: git
    url: https://github.com/example/solo_pkg.git
    version: main
  release:
    packages:
      - solo_pkg
```

## Example: Multi-package Repository

```yaml
common_interfaces:
  source:
    type: git
    url: https://github.com/ros2/common_interfaces.git
    version: humble
  release:
    packages:
      - sensor_msgs
      - std_msgs
      - geometry_msgs
      - nav_msgs
      - visualization_msgs
```

## Non-git Repositories

Entries with `source.type != "git"` (e.g. `svn`) are silently skipped during
index building. rox only supports git-based source pulls.
