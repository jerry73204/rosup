# Feature: Shell Completion

## Problem

rosup has many subcommands and flags. Without shell completion, users must
remember exact flag names and rely on `--help`. Completion also enables
workspace-aware suggestions such as completing package names from the current
project's member list.

## Proposed UX

### Generation

```bash
# Bash
rosup completions bash >> ~/.bash_completion.d/rosup

# Zsh
rosup completions zsh > ~/.zfunc/_rosup

# Fish
rosup completions fish > ~/.config/fish/completions/rosup.fish
```

All three major shells are supported from a single command.

### Verification

```bash
$ rosup completions --list
bash
zsh
fish
```

## Static Completions (Phase 1)

Generate static completion scripts covering all commands and flags. These do
not require a live workspace and are sufficient for the majority of use cases.

Clap provides `clap_complete` as an out-of-the-box completion generator.
Adding it is a small change:

```toml
# rosup-cli/Cargo.toml
[dependencies]
clap_complete = "4"
```

```rust
// main.rs
Command::Completions { shell } => {
    clap_complete::generate(shell, &mut Cli::command(), "rosup", &mut std::io::stdout());
}
```

`clap_complete` natively supports `bash`, `zsh`, `fish`, `elvish`, and
`powershell`. The `Shell` enum from `clap_complete` can be used directly as
the clap argument type.

## Dynamic Completions (Phase 2)

Extend completions to be workspace-aware: complete package names, node names,
and launch file names from the current project.

### Package name completion (`-p`, `--package`)

When the cursor is after `-p` or `--package`, complete with names from the
current workspace member list.

### Node name completion (`rosup run <pkg> <node>`)

Complete the second positional argument with nodes discovered from
`rosup list nodes -p <pkg> -q`.

### Launch file completion (`rosup launch <pkg> <file>`)

Complete launch file names from `rosup list launches -p <pkg> -q`.

Dynamic completions require a shell-specific mechanism:

| Shell | Mechanism                                         |
|-------|---------------------------------------------------|
| Bash  | `complete -C rosup rosup` (command completion)   |
| Zsh   | `_arguments` with `(( $+commands[rosup] ))` guard |
| Fish  | `complete --command rosup --arguments "(rosup ...)"`|

The `rosup completions` output for dynamic shells will call back into rosup
with a special hidden subcommand (e.g. `rosup __complete`) that reads the
partial command line and emits candidates.

## Installation Helpers

### Bash

```bash
# ~/.bashrc or ~/.bash_completion.d/rosup
source <(rosup completions bash)
```

### Zsh

```zsh
# Add to fpath before compinit
fpath=(~/.zfunc $fpath)
autoload -Uz compinit && compinit
```

Then run:

```zsh
rosup completions zsh > ~/.zfunc/_rosup
```

### Fish

Fish auto-loads completions from `~/.config/fish/completions/`:

```fish
rosup completions fish > ~/.config/fish/completions/rosup.fish
```

## Implementation Notes

- Phase 1 is straightforward: add `clap_complete` and a `Command::Completions
  { shell: clap_complete::Shell }` variant.
- Phase 2 requires `rosup list` (see `features/list.md`) to be implemented
  first, as it provides the data source for dynamic candidates.
- The hidden `__complete` subcommand should be excluded from `--help` output
  (`#[clap(hide = true)]`).
- Shell completion scripts should be checked in under `completions/` in the
  repository and regenerated as part of the release workflow so users can
  install them without running `rosup` first (e.g. via a package manager).
