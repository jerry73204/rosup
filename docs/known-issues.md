# Known Issues

## ~~KI-001 — Workspace member list drifts in large workspaces~~ CLOSED

**Fixed in Phase 8.** `members` is now optional in `[workspace]`. When omitted,
rosup uses Colcon's auto-discovery rules. When an explicit list is desired, use
`rosup init --workspace --lock` to generate it or `rosup sync` to keep it
up to date.

See `docs/reference/rosup-toml.md` and `docs/reference/cli.md`.

---

## KI-002 — No pre-build package/node introspection

**Symptom:** There is no way to list the packages, executable nodes, or launch
files in a workspace without building it first. Users must either build the
workspace or manually inspect `CMakeLists.txt` and `setup.py` files.

**Impact:** Medium. Affects discoverability, especially in large workspaces.

**Workaround:** Inspect source files manually or use `colcon list`.

**Planned fix:** `rosup list pkgs|nodes|launches` — see
`docs/features/list.md`.

---

## KI-003 — No shell completion

**Symptom:** Tab completion is not available for rosup commands, subcommands,
or flags in Bash, Zsh, or Fish.

**Impact:** Medium. Reduces usability compared to other CLI tools.

**Workaround:** None.

**Planned fix:** `rosup completions <shell>` — see
`docs/features/shell-completion.md`.

---

## KI-004 — No `--manifest-path` flag

**Symptom:** rosup must be run from within a project directory (or a
subdirectory of one). There is no way to point rosup at a `rosup.toml` in an
arbitrary location, unlike `cargo --manifest-path`.

The upward directory search also lacks filesystem boundary detection, so rosup
may inadvertently pick up a `rosup.toml` from an unrelated parent directory.

**Impact:** Low for typical use; medium for scripting and CI.

**Workaround:** `cd` to the project root before running rosup commands.

**Planned fix:** `--manifest-path` global flag and filesystem boundary
detection — see `docs/features/manifest-discovery.md`.

---

## KI-005 — `rosup clone` always clones into `$CWD`

Superseded by KI-011. See Phase 9.6 for the `--destination` flag plan.

---

## KI-006 — `rosup run` and `rosup launch` are not yet implemented

**Symptom:** The `run` and `launch` subcommands are defined in the CLI but
return an error immediately. They cannot be used.

**Impact:** High for users who want a unified workflow. Currently `ros2 run`
and `ros2 launch` must be used directly after sourcing the workspace manually.

**Workaround:** Source `install/local_setup.bash` and `.rosup/install/local_setup.bash`
manually, then use `ros2 run` / `ros2 launch`.

**Planned fix:** Phase 6 — see `docs/phases/6-run-and-launch.md`.

---

## ~~KI-007 — `rosup add -p` / `rosup remove -p` broken in auto-discovery workspaces~~ CLOSED

**Fixed in Phase 9.1.** `find_member_xml()` now uses `colcon_scan` to
discover packages in auto-discovery mode, matching the behavior of
`discover_members` in `project.rs`.

---

## ~~KI-008 — Invalid distro name produces a misleading "invalid gzip" error~~ CLOSED

**Fixed in Phase 9.2.** `fetch_and_store()` now checks the HTTP status code
before decompressing. A 404 produces:
`distro "nonexistent" not found in rosdistro index (HTTP 404)`.

---

## ~~KI-009 — Overlay validation is fatal for all commands~~ CLOSED

**Fixed in Phase 9.3.** Overlay activation moved out of startup into
`cmd_build` and `cmd_test` only. Non-build commands (`search`, `sync`, `add`,
`remove`, `clean`, `resolve`, `init`, `clone`) no longer trigger overlay
validation.

---

## ~~KI-010 — `rosup init` prompts before checking if `rosup.toml` exists~~ CLOSED

**Fixed in Phase 9.4.** The existence check now runs before the distro
prompt in `cmd_init`.

---

## KI-011 — `rosup clone` clones into `$CWD`, no `--destination` flag

**Symptom:** `rosup clone` always clones into the current working directory.
There is no way to specify a different destination. Users who want to clone
into `src/` (a common Colcon convention, but not a strict rule) must `cd`
there first.

**Impact:** Low. Users must `cd` to the desired parent directory first.

**Workaround:** `cd src/ && rosup clone <pkg> --distro <d>`.

**Planned fix:** Add `--destination <dir>` flag to `rosup clone`
(see Phase 9.6). Supersedes KI-005.

---

## ~~KI-012 — `search` and `clone` do not fall back to `ROS_DISTRO` env~~ CLOSED

**Fixed in Phase 9.5.** `resolve_distro()` now falls back to `ROS_DISTRO`
env after checking `--distro` and `rosup.toml`. All commands now have
consistent distro resolution.
