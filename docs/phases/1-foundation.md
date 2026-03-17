# Phase 1 — Foundation

Establish the Cargo workspace, CLI skeleton, core data structures, and dev
tooling.

## Work Items

### Project Structure

- [ ] Create a multi-crate Cargo workspace
  - `rox-cli` — binary crate, CLI entry point (clap)
  - `rox-core` — library crate, config models, parsers, project discovery
- [ ] Add a `dev-release` Cargo profile (inherits `release`, enables debug)
- [ ] Create a `justfile` with recipes:
  - `default`: `just --list`
  - `build`: `cargo build --all-targets`
  - `clean`: `cargo clean`
  - `format`: `cargo +nightly fmt`
  - `check`: format + clippy
  - `test`: `cargo nextest run --no-fail-fast`
  - `ci`: check + test
  - `setup`: install dev deps (nextest, nightly toolchain)

### Dependencies

- [ ] `clap` with derive — CLI framework
- [ ] `serde` + `toml` — rox.toml parsing
- [ ] `quick-xml` + `serde` — package.xml parsing
- [ ] `eyre` / `color-eyre` — error reporting
- [ ] `thiserror` — typed errors in rox-core
- [ ] `tracing` + `tracing-subscriber` — structured logging
- [ ] `glob` — workspace member pattern matching

### rox-core

- [ ] Define `rox.toml` data model (`RoxConfig`, `Package`, `Workspace`, `Resolve`, `Build`, `Test`)
- [ ] Implement `rox.toml` parser with serde deserialization
- [ ] Detect project mode: `[workspace]` vs `[package]` (mutually exclusive)
- [ ] Implement project root discovery (walk up directories for `rox.toml`)
- [ ] Implement `package.xml` format 3 parser: name, version, deps by type
- [ ] Implement workspace member discovery from glob patterns + exclude
- [ ] Set up `~/.rox/` directory structure on first run (`cache/`, `src/`)

### rox-cli

- [ ] Wire up clap subcommands: `init`, `build`, `test`, `add`, `remove`, `search`, `run`, `launch`, `resolve`, `clean`, `clone`
- [ ] Initialize `color-eyre` and `tracing-subscriber` in main
- [ ] Stub handlers that print "not yet implemented" for each command

## Acceptance Criteria

- [ ] `just build` compiles the entire workspace without warnings
- [ ] `just check` passes format and clippy checks
- [ ] `just test` runs and passes all unit tests
- [ ] `rox --help` prints all subcommands
- [ ] `rox.toml` with `[package]` parses correctly; rejects simultaneous `[workspace]`
- [ ] `rox.toml` with `[workspace]` discovers members by scanning for `package.xml`
- [ ] `package.xml` format 3 parses into structured dep lists
- [ ] Running `rox` outside a project prints a helpful error pointing to `rox init`
- [ ] Unit tests cover config parsing, project discovery, and package.xml parsing
