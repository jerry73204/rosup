# Phase 1 — Foundation

Establish the Cargo workspace, CLI skeleton, core data structures, and dev
tooling.

## Work Items

### 1.1 Project Structure

- [x] Create a multi-crate Cargo workspace
  - `rosup-cli` — binary crate, CLI entry point (clap)
  - `rosup-core` — library crate, config models, parsers, project discovery
- [x] Add a `dev-release` Cargo profile (inherits `release`, enables debug)
- [x] Create a `justfile` with recipes:
  - `default`: `just --list`
  - `build`: `cargo build --all-targets`
  - `clean`: `cargo clean`
  - `format`: `cargo +nightly fmt`
  - `check`: format + clippy
  - `test`: `cargo nextest run --no-fail-fast`
  - `ci`: check + test
  - `setup`: install dev deps (nextest, nightly toolchain)

### 1.2 Dependencies

- [x] `clap` with derive — CLI framework
- [x] `serde` + `toml` — rosup.toml parsing
- [x] `quick-xml` + `serde` — package.xml parsing
- [x] `eyre` / `color-eyre` — error reporting
- [x] `thiserror` — typed errors in rosup-core
- [x] `tracing` + `tracing-subscriber` — structured logging
- [x] `glob` — workspace member pattern matching

### 1.3 rosup-core

- [x] Define `rosup.toml` data model (`RoxConfig`, `Package`, `Workspace`, `Resolve`, `Build`, `Test`)
- [x] Implement `rosup.toml` parser with serde deserialization
- [x] Detect project mode: `[workspace]` vs `[package]` (mutually exclusive)
- [x] Implement project root discovery (walk up directories for `rosup.toml`)
- [x] Implement `package.xml` format 3 parser: name, version, deps by type
- [x] Implement workspace member discovery from glob patterns + exclude
- [x] Set up `~/.rosup/cache/` directory on first run (rosdistro YAML store)

### 1.4 rosup-cli

- [x] Wire up clap subcommands: `init`, `build`, `test`, `add`, `remove`, `search`, `run`, `launch`, `resolve`, `clean`, `clone`
- [x] Initialize `color-eyre` and `tracing-subscriber` in main
- [x] Stub handlers that print "not yet implemented" for each command

## Acceptance Criteria

- [x] `just build` compiles the entire workspace without warnings
- [x] `just check` passes format and clippy checks
- [x] `just test` runs and passes all unit tests
- [x] `rosup --help` prints all subcommands
- [x] `rosup.toml` with `[package]` parses correctly; rejects simultaneous `[workspace]`
- [x] `rosup.toml` with `[workspace]` discovers members by scanning for `package.xml`
- [x] `package.xml` format 3 parses into structured dep lists
- [ ] Running `rosup` outside a project prints a helpful error pointing to `rosup init`
- [x] Unit tests cover config parsing, project discovery, and package.xml parsing
