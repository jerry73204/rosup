# Dependency Model

How Colcon resolves dependencies, and how rosup layers on top.

---

## Colcon's Dependency Model

Colcon is a **build orchestrator**, not a package manager. It builds packages
in topological order based on declared dependencies, but it does not fetch or
install anything.

### What Colcon does

1. Discovers packages in the workspace (via `package.xml`).
2. Reads each package's `<depend>`, `<build_depend>`, `<exec_depend>`,
   `<buildtool_depend>`, `<test_depend>`, `<doc_depend>`.
3. Builds a dependency graph between workspace members.
4. Builds packages in topological order (deps first).
5. For each dep that is **not** a workspace member, Colcon assumes it is
   already available in the ament environment. If it isn't, the build fails.

### What Colcon does NOT do

- Does not install missing dependencies.
- Does not fetch source repositories.
- Does not know about rosdep, rosdistro, or apt.
- Does not distinguish between "this dep is a system library" vs "this dep
  is a ROS package I forgot to clone".

### The user's burden

Before `colcon build` works, the user must ensure every dependency exists:

1. **Source packages** — clone them into the workspace source tree.
2. **Binary packages** — install via `apt` (e.g. `ros-humble-rclcpp`) or
   `rosdep install`.
3. **System libraries** — install via `apt` (e.g. `libpcl-all-dev`, `cgal`).
4. **Custom/vendor packages** — clone from custom repos, or install from
   custom PPAs/debs.

This manual process is what rosup automates.

---

## Ament Environment

Ament is the build system and environment framework for ROS 2. The **ament
environment** is the union of all sourced overlays.

### How ament discovers installed packages

Each ament install prefix (e.g. `/opt/ros/humble`, `/opt/autoware/1.5.0`,
`./install/`) contains:

```
<prefix>/share/ament_index/resource_index/packages/<pkg_name>
```

A package is "installed" if this marker file exists in any prefix on
`AMENT_PREFIX_PATH`.

### Overlay layering (underlay → overlay)

When multiple overlays are sourced, `AMENT_PREFIX_PATH` is a colon-separated
list. Later overlays (higher priority) shadow earlier ones:

```
AMENT_PREFIX_PATH=/path/to/ws/install:/opt/autoware/1.5.0:/opt/ros/humble
```

For dependency checking, rosup scans all prefixes — a dep is "ament-installed"
if it exists in any of them.

### Typical prefix counts

| Prefix | Packages | Description |
|--------|----------|-------------|
| `/opt/ros/humble` | ~413 | Base ROS 2 distribution |
| `/opt/autoware/1.5.0` | ~453 | Autoware binary release |
| `./install/` | varies | Workspace-local build output |

---

## rosup's Resolution Categories

When rosup resolves a dependency name, it falls into exactly one of these
categories, checked in priority order:

### Category 1 — Workspace member (source package in the worktree)

**Check:** The dependency name matches the `<name>` in a `package.xml` of
another workspace member.

**Action:** None. Colcon builds it from source as part of the workspace.
These deps are excluded from the external resolver entirely.

**Example:** `autoware_utils` depending on `autoware_point_types` — both are
workspace members.

### Category 2 — User override (explicit source from rosup.toml)

**Check:** The dependency name appears in `[resolve.overrides.<dep_name>]`
in `rosup.toml`.

**Action:** Clone from the user-specified URL/branch/rev.

**Rationale for high priority:** If a user explicitly adds an override, they
want that version used — even if the package happens to be installed in ament
from a previous build or a system package. Common use cases:
- Patching a bug in an upstream package (fork with fix).
- Using a package not available in any public registry.
- Pinning to a specific commit for reproducibility.

**Example:**
```toml
[resolve.overrides.nav2_core]
git = "https://github.com/myfork/navigation2.git"
branch = "my-fix"
```

### Category 3 — Ament-installed (exists in a sourced overlay)

**Check:** The ament index marker exists at
`<prefix>/share/ament_index/resource_index/packages/<dep_name>` for any
prefix on `AMENT_PREFIX_PATH`.

**Action:** None. Already available at build time.

**Note:** This includes packages from:
- `/opt/ros/<distro>` — the base ROS install
- `/opt/autoware/<ver>` — binary Autoware release
- `./install/` — previously built workspace packages
- Any other sourced overlay

**Example:** `rclcpp`, `sensor_msgs`, `ament_cmake` — all in `/opt/ros/humble`.

### Category 4 — rosdep binary (system package installable via apt/pip)

**Check:** `rosdep resolve <dep_name>` succeeds and returns an installer +
package name.

**Action:** `rosdep install` installs the system package.

**Covers two sub-types:**
- **ROS binary packages:** `ros-humble-rclcpp`, `ros-humble-nav2-core`
  (these are apt packages built from ROS source and published to the ROS apt
  repo).
- **System libraries:** `libpcl-dev`, `libcgal-dev`, `python3-flask`
  (mapped by rosdep YAML rules to apt/pip package names).

**Example:** `libpcl-all-dev` → `apt install libpcl-dev`.

### Category 5 — rosdistro source (clone from the ROS distribution index)

**Check:** The package name appears in the rosdistro distribution cache
(e.g. `humble-cache.yaml`) with a source repository URL and branch.

**Action:** Clone the source repository, build it in the rosup dep layer
(`.rosup/src/`, `.rosup/install/`).

**Note:** A single repo may contain many packages. Cloning `navigation2`
gives `nav2_core`, `nav2_planner`, etc. rosup deduplicates by repo name.

**Example:** `nav2_core` → `https://github.com/ros-planning/navigation2.git`
@ `humble`.

### Category 6 — Unresolved

**Check:** None of the above matched.

**Action:** Report as an error. The user must manually install or add an
override.

**Common causes:**
- Vendor-specific packages not in rosdistro (`blickfeld-scanner`,
  `isaac_ros_visual_slam`).
- ROS 1 packages referenced in ROS 2 manifests (`roscpp`, `catkin`).
- Typos in `package.xml`.
- Internal packages from a private/corporate registry.

---

## Resolution Order in rosup

**Category 1 (workspace member)** is handled before the resolver runs:
`collect_dep_names` in `main.rs` filters out deps whose names match workspace
member names. The resolver never sees workspace-local deps.

The resolver in `resolver/mod.rs` then checks the remaining deps:

```
resolve_one(dep):
  1. User override?              → Category 2 (Override)
  2. Ament installed?            → Category 3 (Ament)
  3. rosdep binary? (if allowed) → Category 4 (Binary)
  4. rosdistro source?           → Category 5 (Source)
  5. None                        → Category 6 (Unresolved)
```

User overrides have the highest resolver priority: if you explicitly
configure a fork, it wins even over an ament-installed version. This covers
the case where you need a patched version of a package that is already
installed system-wide.

---

## What `rosup tree` needs

The tree command needs to classify every dependency of every workspace member
into one of the 6 categories. This requires:

1. **Package discovery** — same as `project.members()` (already implemented).
2. **Dep extraction** — parse `package.xml` for each member, preserving dep
   type (`depend`, `build_depend`, etc.) and the dep name.
3. **Classification** — for each dep name, determine which category it falls
   into. This is the same logic as `resolve_one` but with category 1 added.
4. **Display** — format the tree with category annotations.

### `rosup tree` (default)

Shows each workspace member and its direct deps, annotated with category:

```
autoware_pointcloud_preprocessor
├── autoware_utils [workspace]
├── rclcpp [ament]
├── cv_bridge [ament]
├── cgal [rosdep: libcgal-dev]
├── libpcl-all-dev [rosdep: libpcl-dev]
└── managed_transform_buffer [workspace]
```

### `rosup tree -p <pkg>`

Shows deps for a single package.

### `rosup tree -d`

Shows deps that are used by 2+ workspace members, sorted by usage count:

```
rclcpp                      312 packages  [ament]
ament_cmake_auto            298 packages  [ament]
autoware_cmake              287 packages  [ament]
autoware_utils              145 packages  [workspace]
rclcpp_components            89 packages  [ament]
```

### `rosup tree -i <dep>`

Inverted: shows which workspace members depend on a given dep:

```
rclcpp [ament]
├── autoware_pointcloud_preprocessor
├── autoware_ndt_scan_matcher
├── autoware_ekf_localizer
└── ... (312 total)
```

---

## Dependency types in package.xml

| Tag | Meaning | When needed |
|-----|---------|-------------|
| `<depend>` | build + build_export + exec | Always |
| `<build_depend>` | Needed at build time only | Compile |
| `<build_export_depend>` | Needed by downstream build consumers | Compile |
| `<buildtool_depend>` | Build tool (e.g. `ament_cmake`) | Compile |
| `<exec_depend>` | Needed at runtime only | Run |
| `<test_depend>` | Needed for testing only | Test |
| `<doc_depend>` | Needed for documentation only | Doc |

For `rosup tree`, all types are shown by default. Flags like `--build-only`,
`--exec-only`, `--test-only` could filter by type.
