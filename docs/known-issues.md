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

## KI-007 — `rosup add -p` / `rosup remove -p` broken in auto-discovery workspaces

**Symptom:** `rosup add rclcpp -p <pkg>` and `rosup remove rclcpp -p <pkg>`
always fail with "could not find package.xml for `<pkg>`" when the workspace
uses auto-discovery (no explicit `members` list).

**Root cause:** `find_member_xml()` in `main.rs` iterates
`ws.members.iter().flatten()`, which yields nothing when `members` is `None`.

**Impact:** High. Any auto-discovery workspace (the recommended default) cannot
use the `-p` flag for `add` or `remove`.

**Workaround:** `cd` into the target package directory and run `rosup add` /
`rosup remove` without `-p`.

**Planned fix:** `find_member_xml()` should call `colcon_scan` or
`project.members()` when `members` is `None`, the same way `discover_members`
does in `project.rs`.

---

## KI-008 — Invalid distro name produces a misleading "invalid gzip" error

**Symptom:** `rosup search --distro nonexistent_distro` produces:

```
failed to decompress cache: invalid gzip header
```

**Root cause:** `fetch_and_store()` in `rosdistro.rs` downloads from the
rosdistro GitHub URL without checking the HTTP status code. A 404 response
returns HTML, which is then fed to `GzDecoder`, causing a gzip error.

**Impact:** Low. Confusing error message; does not corrupt state.

**Workaround:** Use a valid distro name.

**Planned fix:** Check the HTTP status code before decompressing. Return a
clear error like "distro `<name>` not found in rosdistro index".

---

## KI-009 — Overlay validation is fatal for all commands

**Symptom:** If any configured overlay path lacks `setup.sh` (e.g. the
overlay is not installed on this machine), **all** rosup commands fail —
including `search`, `sync`, `add`, and other commands that do not need
overlays.

**Root cause:** `apply_overlays_from_cwd()` runs eagerly at startup for every
command and calls `build_env()`, which returns a hard error for missing
`setup.sh`.

**Impact:** Medium. A `rosup.toml` committed to a shared repo with
`overlays = ["/opt/autoware/1.5.0"]` breaks rosup for anyone who has not
installed the Autoware binary packages.

**Workaround:** Remove or comment out the overlay entry.

**Planned fix:** Either make overlay errors non-fatal for commands that don't
need them, or downgrade to a warning.

---

## KI-010 — `rosup init` prompts before checking if `rosup.toml` exists

**Symptom:** Running `rosup init` (without `--ros-distro`) interactively
prompts for the ROS distribution, then reports "`rosup.toml` already exists".
The user typed a value for nothing.

**Impact:** Low. Minor UX annoyance.

**Workaround:** Always pass `--ros-distro` or set `ROS_DISTRO`.

**Planned fix:** Move the existence check before the distro prompt in
`cmd_init`.

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

## KI-012 — `search` and `clone` do not fall back to `ROS_DISTRO` env

**Symptom:** `ROS_DISTRO=humble rosup search nav` fails with "no ROS distro
specified" even though the environment variable is set. `init` and `resolve`
do fall back to `ROS_DISTRO`.

**Impact:** Low. Inconsistent UX across commands.

**Workaround:** Pass `--distro humble` explicitly, or add `ros-distro` to
`rosup.toml`.

**Planned fix:** Add `ROS_DISTRO` env fallback to `resolve_distro()`,
matching the behavior of the resolver.
