# Phase 12 — Source Repos

Add `.repos` file and direct git repo support as dependency sources.
See `docs/design/source-repos.md` for background.

---

## 12.1 Config: `[[resolve.sources]]` — DONE

**File:** `crates/rosup-core/src/config.rs`

- [x] `SourceRepo` struct with `name`, `repos` (Option), `git` (Option),
  `branch`, `tag`, `rev` fields.
- [x] `sources: Vec<SourceRepo>` in `ResolveConfig`.
- [x] Cargo-style git syntax: `git` + `branch`/`tag`/`rev`.
- [x] `.repos` file path via `repos` field.
- [x] `repos` and `git` are mutually exclusive.

---

## 12.2 `.repos` file parser — DONE

**File:** `crates/rosup-core/src/resolver/repos_file.rs`

- [x] Parse vcstool `.repos` YAML format.
- [x] `RepoEntry` struct: path, url, version.
- [x] `ReposFile::parse(path, name)` and `parse_str(yaml, name, path)`.
- [x] Repos sorted by path for determinism.
- [x] 6 unit tests: parse, sort, version types, index building, URL
  normalization.

---

## 12.3 Package → repo index — DONE

**File:** `crates/rosup-core/src/resolver/repos_file.rs`

Three strategies for mapping package names to repo entries:

- [x] **Strategy 1: git scan** — clone the repo (bare, `--filter=blob:none`)
  into `~/.rosup/src/`, then `git ls-tree -r --name-only HEAD` to list
  all `package.xml` files, then `git show HEAD:<path>` to extract `<name>`.
  No worktree needed. Works for all repos including multi-package and
  private repos.
- [x] **Strategy 2: rosdistro URL match** — if a rosdistro package's
  source URL matches a repo entry's URL. No clone needed.
- [x] **Strategy 3: path-based fallback** — last path segment = package
  name. No clone needed.
- [x] `resolve_git_ref()` — resolves version string (tag/branch/SHA) to
  a git ref, trying the version as-is then `origin/<version>`.
- [x] `extract_package_name()` — lightweight `<name>` extraction from XML
  without full parser.
- [x] `scan_repo_packages()` — orchestrates clone + ls-tree + show.
- [x] Bare clones use `--filter=blob:none` for minimal disk usage.
- [x] Existing bare clones are reused (fetch instead of re-clone).

### Acceptance criteria

- [x] `autoware_pointcloud_preprocessor` (inside `autoware_universe`
  repo, 100+ packages) resolves from `autoware.repos`.
- [x] `tier4_localization_msgs` (inside `tier4_autoware_msgs` repo)
  resolves from `autoware.repos`.
- [x] AutoSDV workspace: 0 unresolved warnings with Autoware `.repos`
  as source (previously 7 unresolved).

---

## 12.4 Resolution integration — DONE

**File:** `crates/rosup-core/src/resolver/mod.rs`

- [x] `ResolutionMethod::ReposSource` variant with `source_name`, `url`,
  `version`.
- [x] `load_repos_index(cache)` — loads all `.repos` files and git
  sources, builds combined package index.
- [x] `resolve_one()` checks `.repos` sources at step 4 (after rosdep,
  before rosdistro).
- [x] `execute()` clones `.repos` source deps like rosdistro source deps.
- [x] `format_method()` displays `repos (<name>): <url> @ <version>`.

---

## 12.5 Direct git repo sources — DONE

**File:** `crates/rosup-core/src/resolver/mod.rs`

- [x] `load_repos_index()` handles `git` + `branch`/`tag`/`rev` sources.
- [x] Resolves version from `rev` → `tag` → `branch` → `"HEAD"`.
- [x] For direct git repos: rosdistro URL match for multi-package,
  source name as package name fallback.

---

## 12.6 Integration tests — DONE

**File:** `crates/rosup-tests/tests/repos_source.rs`

- [x] `resolve_dry_run_shows_repos_source` — .repos file source resolves.
- [x] `resolve_dry_run_without_repos_source_uses_rosdistro` — no source
  configured, falls back to rosdistro.
- [x] `missing_repos_file_warns_but_resolves` — missing file doesn't
  fail, just warns.
- [x] `resolve_with_git_source` — direct git repo source resolves
  (using `FAKE_ROSDEP_PACKAGES` to bypass rosdep).

---

## 12.7 CLI commands (TODO)

**File:** `crates/rosup-cli/src/main.rs`

- [ ] `rosup source add <file_or_url> --name <name>` — register a
  `.repos` file or git URL as a source in `rosup.toml`.
- [ ] `rosup source add --git <url> --branch <b> --name <name>` —
  register a direct git repo source.
- [ ] `rosup source list` — list registered sources.
- [ ] `rosup source remove <name>` — remove a source by name.

### Acceptance criteria

- [ ] `rosup source add autoware.repos --name autoware` adds the entry
  to `rosup.toml`.
- [ ] `rosup source list` shows registered sources.

---

## 12.8 Documentation (TODO)

- [ ] Update `docs/reference/rosup-toml.md` with `[[resolve.sources]]`
  section.
- [ ] Update `docs/reference/cli.md` with `rosup source` commands.
- [ ] Update `README.md` with source repos example.

---

## Summary

| Item | Status | Tests |
|------|--------|-------|
| 12.1 Config | DONE | — |
| 12.2 .repos parser | DONE | 6 unit |
| 12.3 Package index + git scan | DONE | 3 unit |
| 12.4 Resolution integration | DONE | — |
| 12.5 Direct git sources | DONE | — |
| 12.6 Integration tests | DONE | 4 integration |
| 12.7 CLI commands | TODO | — |
| 12.8 Documentation | TODO | — |
