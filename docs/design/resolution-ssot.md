# Resolution SSoT — Current State and Proposed Refactor

## Problem

Dependency resolution logic is split across two crates and three code paths:

1. **Category 1 (workspace member)** — `collect_dep_names()` in `main.rs`
   strips out workspace-member deps before passing them to the resolver.
   The resolver never sees these deps.

2. **Categories 2–6 (override → ament → rosdep → rosdistro → unresolved)** —
   `Resolver::resolve_one()` in `resolver/mod.rs` handles external deps.

3. **Bare package.xml fallback** — `cmd_resolve()` has a special path that
   reads a standalone `package.xml` without `rosup.toml`, using `ROS_DISTRO`
   env directly. This path bypasses `collect_dep_names()` (no workspace
   member filtering, since there is no workspace).

### Where resolution is invoked

| Command | Dep collection | Resolver | Notes |
|---------|---------------|----------|-------|
| `resolve` | custom (bare-pkg.xml fallback) | `plan()` or `resolve()` | Two code paths |
| `build` | `collect_dep_names()` | `resolve()` | Standard path |
| `test` | `collect_dep_names()` | `resolve()` | Same as build |
| `tree` (planned) | needs full classification | needs all 6 categories | Not yet implemented |

### Issues

- **No single function** classifies a dep into one of the 6 categories.
  `collect_dep_names` handles category 1 by filtering, and `resolve_one`
  handles categories 2–6. The tree command needs both in a unified view.

- **`collect_dep_names` is in `main.rs`** (the CLI crate), not in
  `rosup-core`. It can't be reused by the tree command if tree is
  implemented in core.

- **Three callers** create `Resolver::new()` + `collect_dep_names()`
  independently. A change to the resolution workflow requires updating all
  three.

## Proposed Refactor (for `rosup tree` implementation)

### 1. Move workspace-member filtering into the resolver

Add a `classify` method to `Resolver` that accepts a dep name and a set of
workspace member names, and returns the full category:

```rust
pub enum DepCategory {
    WorkspaceMember,
    Override { url: String, branch: Option<String>, rev: Option<String> },
    Ament,
    Binary { installer: String, packages: Vec<String> },
    Source { repo: String, url: String, branch: String },
    Unresolved,
}

impl Resolver {
    pub fn classify(
        &self,
        dep: &str,
        workspace_members: &HashSet<&str>,
        cache: &DistroCache,
        source_only: bool,
    ) -> DepCategory { ... }
}
```

### 2. Move `collect_dep_names` into `rosup-core`

Either into `project.rs` (as a method on `Project`) or into the resolver
module. This makes it available to both CLI commands and future library
consumers.

### 3. Unify the three resolution call sites

Create a helper that encapsulates the common pattern:

```rust
fn resolve_project_deps(
    project: &Project,
    dry_run: bool,
    source_only: bool,
    refresh: bool,
) -> Result<ResolutionPlan> { ... }
```

`cmd_resolve`, `cmd_build`, and `cmd_test` would all call this, with
`cmd_resolve` adding the bare-package.xml fallback on top.
