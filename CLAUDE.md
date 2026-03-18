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
- TOML field names use kebab-case (`ros-distro`, `source-preference`, etc.) via
  `#[serde(rename_all = "kebab-case")]` on config structs.

### rosup.toml — `[resolve]` section

- `ros-distro` is **mandatory** for all resolver operations. There is no
  `ROS_DISTRO` env fallback (except the bare-`package.xml` path in `cmd_resolve`).
- `overlays` is a list of ament prefix paths sourced in underlay-first order.
  `rosup new` automatically populates it with `/opt/ros/<distro>`.
- Overlay environments are activated once in `main()` via `apply_overlays_from_cwd()`
  and applied to the current process with `std::env::set_var`.

### Workspace vs package detection

- A directory is a **package** if it has `package.xml` at the root.
- A directory is a **workspace** only when explicitly declared: `--workspace`
  flag on `rosup init`, or detected by `rosup clone` when multiple `package.xml`
  files are found in the cloned repo.
- A `src/` directory alone does **not** imply workspace mode.
