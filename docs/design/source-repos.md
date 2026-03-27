# Source Repos — `.repos` file and git repo integration

## Prior art

### Cargo: `[patch]` and source replacement

Cargo has two mechanisms for alternative sources:

**`[patch.crates-io]`** — override a specific dependency with a local
path or git repo. Per-dependency (like our `[resolve.overrides]`).

**`[source]` replacement** — replace an entire registry with a mirror.

**Cargo's git dependency syntax** — the convention we follow:

```toml
[dependencies]
uuid = { git = "https://github.com/uuid-rs/uuid", branch = "next" }
uuid = { git = "https://github.com/uuid-rs/uuid", tag = "1.0.0" }
uuid = { git = "https://github.com/uuid-rs/uuid", rev = "abc123" }
```

### uv (Python): `[[tool.uv.index]]`

uv registers named package indexes. A `.repos` file is analogous —
a named index of git repos.

---

## Design for rosup (implemented)

### Two source forms in `[[resolve.sources]]`

```toml
# .repos file source (multiple repos)
[[resolve.sources]]
name = "autoware"
repos = "autoware.repos"     # path relative to workspace root

# Direct git repo source (Cargo-style)
[[resolve.sources]]
name = "my-nav2-fork"
git = "https://github.com/myfork/navigation2.git"
branch = "humble"            # or tag = "1.5.0" or rev = "abc123"
```

The `repos` form reads a vcstool `.repos` YAML file containing multiple
repos. The `git` form points to a single repo using Cargo's
`git`/`branch`/`tag`/`rev` convention.

### Resolution order

```
1. Workspace member       (colcon builds from source)
2. User override          ([resolve.overrides])
3. Ament-installed        (AMENT_PREFIX_PATH)
4. rosdep binary          (apt/pip)
5. .repos / git sources   ([resolve.sources])     ← THIS FEATURE
6. rosdistro source       (ros2 distribution index)
7. Unresolved
```

### Package → repo mapping

For each source, rosup builds an index mapping package names to repo
entries. Two strategies:

1. **rosdistro URL match** — if a rosdistro package's source URL matches
   a repo entry's URL, that repo provides that package. Works for
   multi-package repos when the packages are in rosdistro.

2. **Path-based fallback** — the last segment of the repo path is treated
   as a package name. Works for single-package repos.

**Limitation:** Multi-package repos not in rosdistro (e.g. private repos)
can only be matched by the path fallback. A future improvement will clone
the repo and scan for `package.xml` files to build a complete index.

### What we DON'T do

- **No `rosup import` command** — use `vcs import` for workspace assembly.
- **No cloning of ALL repos** — only repos needed by deps are pulled.
- **No modification of `.repos` files** — rosup reads them, never writes.

### Lockfile integration

The lockfile records the origin:

```toml
[[deps]]
name = "autoware_core"
source = "git+https://github.com/autowarefoundation/autoware_core.git#abc123"
origin = "repos:autoware"
branch = "1.5.0"
```

---

## Scenarios

**Scenario 1: Autoware developer** — uses `vcs import` to clone repos
into `src/`. Those become workspace members. No `.repos` registration
needed.

**Scenario 2: Autoware user** — has their own packages, depends on
Autoware. Registers the `.repos` file as a source. rosup pulls only
the needed Autoware repos into `.rosup/src/`.

```toml
[[resolve.sources]]
name = "autoware"
repos = "autoware.repos"
```

**Scenario 3: Fork of a single dependency** — uses the `git` form to
point at a fork repo.

```toml
[[resolve.sources]]
name = "my-nav2-fork"
git = "https://github.com/myfork/navigation2.git"
tag = "1.1.20-patched"
```

---

## Comparison

| Feature | vcs import | rosup resolve with sources |
|---------|-----------|---------------------------|
| Clones | Everything in the file | Only repos needed by deps |
| Destination | `src/` (workspace members) | `.rosup/src/` (dep layer) |
| Version pin | From .repos file | From .repos file + lockfile SHA |
| User intent | "I'm developing these" | "I depend on these" |
| VCS support | git (Cargo-style `git`/`branch`/`tag`/`rev`) | git (future: hg, svn) |
