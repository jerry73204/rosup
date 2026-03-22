# rosup — Claude guidance

## Project

Cargo-like package manager for ROS 2. See `docs/` for design and reference
docs, `docs/phases/` for phased work items.

## Build, check, and test

Use `just` for all routine tasks. Do **not** call `cargo` directly unless you
need flags or options not exposed by the justfile.

```
just build    # cargo build --all-targets (dev-release profile)
just check    # nightly fmt check + clippy -D warnings
just test     # cargo nextest run --no-fail-fast
just ci       # check + test
just run ...  # run the rosup CLI, e.g. just run --help
just install  # cargo install --path crates/rosup-cli
```

**Run `just ci` before marking any implementation task complete.**

### Integration test binary

`just build` uses the `dev-release` profile (`target/dev-release/`), but
integration tests in `rosup-tests` use `assert_cmd::Command::cargo_bin` which
resolves to `target/debug/`. After changing `rosup-cli` or `rosup-core`, run:

```
cargo build -p rosup-cli   # rebuilds target/debug/rosup used by integration tests
just ci                    # then run full CI
```

## Workspace layout

```
Cargo.toml              # workspace manifest only — no [package]
crates/
  rosup-core/           # library: config, package_xml, manifest, init, project
  rosup-cli/            # binary: clap CLI (bin name: rosup)
  shims/                # fake colcon/git/rosdep binaries for integration tests
  rosup-tests/          # integration test crate (spawns rosup binary)
docs/
  design/               # architecture, dependency model, resolution SSoT
  reference/            # rosup.toml and package.xml specs, CLI reference
  phases/               # phased work items with checkboxes
  testing/              # edge case scenarios from real-world testing
external/               # cloned third-party repos for study (git-ignored)
tmp/                    # temporary scripts and scratch files (git-ignored)
```

## Test fixtures

Fixture files live in `crates/rosup-core/tests/fixtures/`. Tests load them via
the shared helpers in `lib.rs`:

```rust
use crate::test_helpers::{fixture, fixture_path, copy_fixture, copy_fixture_dir};
```

- Do **not** embed XML content as inline strings in test code. Add a fixture
  file instead.
- For directory-based fixtures (workspace layouts), create them under
  `tests/fixtures/workspaces/` and use `copy_fixture_dir`.
- TOML manipulation uses `toml_edit` for format-preserving edits — no
  hand-rolled string patching.

## Exploration practices

- To explore a full source repo, clone it to `$project/external/` and study
  it locally. Use `gh api` only when peeking at specific files or using
  GitHub-specific features (issues, PRs, releases).
- If you need to run a shell script repeatedly, create a temporary script in
  `$project/tmp/` and run it rather than running the script inline in the
  Bash tool.
- Prefer `$project/tmp/` over `/tmp/` for temporary files.

## Reference projects

- `~/repos/play_launch/` — uses PyO3 to implement mocked Python API for users.
- `~/repos/colcon-cargo-ros2/` — a Colcon plugin for Cargo-based ROS 2
  packages. Shows how Colcon's plugin system works.

## Key conventions

- `package.xml` is the source of truth for ROS 2 deps — rosup reads and writes
  it, never replaces it.
- One `rosup.toml` per project at the root. `[workspace]` and `[package]` are
  mutually exclusive.
- XML manipulation in `manifest.rs` is line-level to preserve comments,
  processing instructions, and indentation.
- Errors use `thiserror` in `rosup-core` and `eyre`/`color-eyre` in `rosup-cli`.
- TOML field names use kebab-case (`ros-distro`, `source-preference`, etc.) via
  `#[serde(rename_all = "kebab-case")]` on config structs.

### rosup.toml — `[resolve]` section

- `ros-distro` resolves via: `--ros-distro` flag → `rosup.toml` → `ROS_DISTRO`
  env → interactive prompt (for `init`) or error (for other commands).
- `overlays` is a list of ament prefix paths sourced in underlay-first order
  using `bash` + `setup.bash` (not `sh` + `setup.sh` — ament's POSIX sh
  scripts have bugs with `AMENT_PREFIX_PATH` propagation).
- Overlay environments are activated in `cmd_resolve`, `cmd_build`, and
  `cmd_test` only. Non-build commands (`search`, `sync`, `add`, `exclude`,
  etc.) do not source overlays.
- `ignore-deps` suppresses specific external dep names during resolution
  (for erroneous or platform-specific deps in unmodifiable `package.xml`
  files). Does not affect workspace members.

### `[workspace] exclude`

- `exclude` stores concrete directory paths (not globs). No entry is a
  prefix of another (invariant maintained by `exclude_add`/`normalize_excludes`).
- The CLI `rosup exclude` accepts `*` in the last path segment and expands
  to concrete paths. `**` is not supported.
- `rosup exclude --pkg <name>` resolves a package name to its directory path.
- Only workspace members can be excluded — external packages cannot.

### Workspace vs package detection

- A directory is a **package** if it has `package.xml` at the root.
- A directory is a **workspace** only when explicitly declared: `--workspace`
  flag on `rosup init`, or detected by `rosup clone` when multiple `package.xml`
  files are found in the cloned repo.
- A `src/` directory alone does **not** imply workspace mode.

### Dependency model

See `docs/design/dependency-model.md`. Resolution priority:

1. Workspace member (colcon builds from source)
2. User override (`[resolve.overrides]`)
3. Ament-installed (in `AMENT_PREFIX_PATH`)
4. rosdep binary (apt/pip)
5. rosdistro source (git clone)
6. Unresolved (error)

### Build pipeline

`rosup build` runs three phases:

1. **Check deps** — `resolver.plan()` verifies all deps are resolvable. Only
   truly unresolvable deps block the build. Binary/source deps are assumed
   satisfied after `rosup resolve`.
2. **Build dep layer** — `colcon build` on `.rosup/src/` → `.rosup/install/`.
   Stale deps (packages now in the workspace source tree) are automatically
   cleaned from `.rosup/install/` before the workspace build.
3. **Build workspace** — `colcon build` on the user's packages with
   `AMENT_PREFIX_PATH` layered: `.rosup/install/` + configured overlays.

### Conditional deps (REP-149)

The `package_xml` parser evaluates `condition` attributes on dependency
elements. `$ROS_VERSION` is substituted with `2`. Simple comparisons
(`==`, `!=`) are evaluated; complex expressions with unresolved variables
are treated conservatively (dep included). ROS 1 conditional deps
(`$ROS_VERSION == 1`) are skipped.
