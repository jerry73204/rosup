# Known Issues

## KI-001 — Workspace member list drifts in large workspaces

**Symptom:** In workspaces with many packages (e.g. Autoware with 450+
packages), the `members` list in `rosup.toml` must be maintained by hand.
Adding, removing, or moving packages requires manually editing the manifest.
Running `rosup init --workspace --force` regenerates the list but discards
any manual customisations in the file.

**Impact:** High for large Colcon workspaces.

**Workaround:** Re-run `rosup init --workspace --force` after structural
changes, then manually re-apply any customisations.

**Planned fix:** `rosup sync` command — see `docs/features/workspace-rescan.md`.

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

**Symptom:** `rosup clone` clones the repository into the current working
directory with no option to specify a destination. If the current directory is
a workspace root, the clone lands at the top level rather than under `src/`.

**Impact:** Low. The user must either `cd src/` first or move the directory
afterwards.

**Workaround:** Run `rosup clone` from inside `src/`, or move the cloned
directory manually and run `rosup sync` (once available).

**Planned fix:** Add `--destination <dir>` flag to `rosup clone`.

---

## KI-006 — `rosup run` and `rosup launch` are not yet implemented

**Symptom:** The `run` and `launch` subcommands are defined in the CLI but
return an error immediately. They cannot be used.

**Impact:** High for users who want a unified workflow. Currently `ros2 run`
and `ros2 launch` must be used directly after sourcing the workspace manually.

**Workaround:** Source `install/local_setup.bash` and `.rosup/install/local_setup.bash`
manually, then use `ros2 run` / `ros2 launch`.

**Planned fix:** Phase 6 — see `docs/phases/6-run-and-launch.md`.
