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

## Workspace layout

```
Cargo.toml              # workspace manifest only — no [package]
crates/
  rosup-core/           # library: config, package_xml, manifest, init, project
  rosup-cli/            # binary: clap CLI (bin name: rosup)
  shims/                # fake colcon/git/rosdep binaries for integration tests
  rosup-tests/          # integration test crate (spawns rosup binary)
docs/
  design/               # architecture
  reference/            # rosup.toml and package.xml specs, CLI reference
  phases/               # phased work items with checkboxes
```

## Test fixtures

Fixture files live in `crates/rosup-core/tests/fixtures/`. Tests load them via
the shared helpers in `lib.rs`:

```rust
use crate::test_helpers::{fixture, fixture_path, copy_fixture};
```

Do **not** embed XML content as inline strings in test code. Add a fixture
file instead.

## Key conventions

- `package.xml` is the source of truth for ROS 2 deps — rosup reads and writes
  it, never replaces it.
- One `rosup.toml` per project at the root. `[workspace]` and `[package]` are
  mutually exclusive.
- XML manipulation in `manifest.rs` is line-level to preserve comments,
  processing instructions, and indentation.
- Errors use `thiserror` in `rosup-core` and `eyre`/`color-eyre` in `rosup-cli`.
