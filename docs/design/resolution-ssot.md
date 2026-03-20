# Resolution SSoT — Implemented

## Architecture

Dependency resolution is structured as a single model with clear layers:

### Layer 1: `Project` (rosup-core)

`Project` provides two methods for dependency collection:

- **`external_deps()`** — returns `(dep_names, member_names)` where
  `dep_names` excludes workspace members. Used by `resolve`, `build`, `test`
  (commands that install external deps).

- **`all_dep_names()`** — returns all dep names including workspace members.
  Used by `tree` (commands that need the full picture).

### Layer 2: `Resolver` (rosup-core)

The resolver classifies external deps into categories 2–6:

- **`classify_all(deps, workspace_members, ...)`** — the full 6-category
  classifier. Checks workspace membership first (Category 1), then runs
  `resolve_one` for categories 2–6. Returns a `ResolutionPlan` with every
  dep classified. Used by `tree`.

- **`plan(deps, ...)`** — builds a plan for external deps only (categories
  2–6). Used by `resolve --dry-run`.

- **`resolve(deps, ...)`** — builds a plan and executes it (installs binary
  packages, clones source repos). Used by `build` and `test`.

### Layer 3: CLI (rosup-cli)

Each command picks the appropriate combination:

| Command             | Dep collection            | Resolver method           |
|---------------------|---------------------------|---------------------------|
| `resolve --dry-run` | `project.external_deps()` | `resolver.plan()`         |
| `resolve`           | `project.external_deps()` | `resolver.resolve()`      |
| `build` / `test`    | `project.external_deps()` | `resolver.resolve()`      |
| `tree` (planned)    | `project.all_dep_names()` | `resolver.classify_all()` |

### Resolution priority order

```
1. Workspace member  — another package in the same workspace (colcon builds it)
2. User override     — explicit fork/pin in [resolve.overrides]
3. Ament installed   — already in AMENT_PREFIX_PATH
4. rosdep binary     — installable via apt/pip
5. rosdistro source  — cloneable from the ROS index
6. Unresolved        — nothing matched
```

User overrides beat ament because the user explicitly wants a different
version (e.g. patched fork) even if the package is already installed.

### Display formatting

`format_method()` in `main.rs` converts a `ResolutionMethod` to a
human-readable string. Used by `cmd_resolve` for both dry-run and non-dry-run
output. Will also be used by `tree`.
