use std::collections::HashMap;

use clap::{CommandFactory, Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr};
use rosup_core::{
    builder::{self, BuildOptions, CleanOptions, TestOptions},
    config::ResolveConfig,
    init::{self, InitMode},
    manifest::{self, DepTag},
    new::{self, BuildType, NewOptions},
    overlay,
    project::Project,
    resolver::{
        Resolver,
        rosdistro::DistroCache,
        store::{GlobalStore, ProjectStore},
    },
    runner,
};

#[derive(Parser)]
#[command(name = "rosup", about = "A Cargo-like package manager for ROS 2")]
#[command(version, propagate_version = true)]
struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Path to rosup.toml; bypasses the upward directory search
    #[arg(long, global = true)]
    manifest_path: Option<std::path::PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new ROS 2 package from a template
    New {
        /// Package name (also the new directory name)
        name: String,
        /// ROS distribution (e.g. humble, jazzy); prompted interactively if omitted
        #[arg(long)]
        ros_distro: Option<String>,
        /// Build system (ament_cmake or ament_python); prompted interactively if omitted
        #[arg(long)]
        build_type: Option<BuildType>,
        /// Add a <depend> entry (repeatable)
        #[arg(long = "dep")]
        deps: Vec<String>,
        /// Generate a starter node with this name
        #[arg(long)]
        node: Option<String>,
        /// Create the package in this directory [default: current dir]
        #[arg(long)]
        destination: Option<std::path::PathBuf>,
    },
    /// Initialize a rosup.toml for an existing package or workspace
    Init {
        /// Force workspace mode
        #[arg(long)]
        workspace: bool,
        /// Write an explicit locked member list (only with --workspace)
        #[arg(long)]
        lock: bool,
        /// ROS distribution (e.g. humble, jazzy); auto-detected from ROS_DISTRO
        /// env var or prompted interactively if not provided
        #[arg(long)]
        ros_distro: Option<String>,
        /// Overwrite an existing rosup.toml
        #[arg(long)]
        force: bool,
    },
    /// Sync the member list in rosup.toml with the current workspace state
    Sync {
        /// Print what would change; do not write
        #[arg(long)]
        dry_run: bool,
        /// Exit non-zero if the member list is out of date (CI mode)
        #[arg(long)]
        check: bool,
        /// Convert auto-discovery workspace to an explicit member list
        #[arg(long)]
        lock: bool,
    },
    /// Clone a package from the ROS index
    Clone {
        package_name: String,
        #[arg(long)]
        distro: Option<String>,
        /// Clone into this directory [default: current directory]
        #[arg(long)]
        destination: Option<std::path::PathBuf>,
    },
    /// Build packages
    Build {
        /// Build only the specified packages
        #[arg(short, long, num_args = 1..)]
        packages: Vec<String>,
        /// Also build dependencies of selected packages
        #[arg(long)]
        deps: bool,
        /// Additional CMake arguments
        #[arg(long, num_args = 1..)]
        cmake_args: Vec<String>,
        /// Skip dependency resolution and dep layer build
        #[arg(long)]
        no_resolve: bool,
        /// Force rebuild of the dep layer from source
        #[arg(long)]
        rebuild_deps: bool,
        /// Shorthand for -DCMAKE_BUILD_TYPE=Release
        #[arg(long)]
        release: bool,
        /// Shorthand for -DCMAKE_BUILD_TYPE=Debug
        #[arg(long)]
        debug: bool,
    },
    /// Run tests
    Test {
        /// Test only the specified packages
        #[arg(short, long, num_args = 1..)]
        packages: Vec<String>,
        /// Retry failed tests up to N times
        #[arg(long)]
        retest_until_pass: Option<usize>,
        /// Skip dependency resolution
        #[arg(long)]
        no_resolve: bool,
    },
    /// Add a dependency to package.xml
    Add {
        dep_name: String,
        /// Target package (workspace mode)
        #[arg(short, long)]
        package: Option<String>,
        /// Add as <build_depend> only
        #[arg(long, conflicts_with_all = ["exec", "test", "dev"])]
        build: bool,
        /// Add as <exec_depend> only
        #[arg(long, conflicts_with_all = ["build", "test", "dev"])]
        exec: bool,
        /// Add as <test_depend> only
        #[arg(long, conflicts_with_all = ["build", "exec", "dev"])]
        test: bool,
        /// Add as <build_depend> + <test_depend>
        #[arg(long, conflicts_with_all = ["build", "exec", "test"])]
        dev: bool,
    },
    /// Remove a dependency from package.xml
    Remove {
        dep_name: String,
        /// Target package (workspace mode)
        #[arg(short, long)]
        package: Option<String>,
    },
    /// Add a path glob or package name to the workspace exclude list
    Exclude {
        /// Path glob to exclude (e.g. "src/localization/autoware_isaac_*")
        pattern: Option<String>,
        /// Exclude a package by name (looks up its directory path)
        #[arg(long, conflicts_with = "pattern")]
        pkg: Option<String>,
        /// List current exclude patterns
        #[arg(long)]
        list: bool,
    },
    /// Re-include a package or path in the workspace (remove from exclude list)
    Include {
        /// Path to remove from the exclude list
        pattern: Option<String>,
        /// Re-include by package name
        #[arg(long, conflicts_with = "pattern")]
        pkg: Option<String>,
    },
    /// Search the ROS package index
    Search {
        query: String,
        #[arg(long)]
        distro: Option<String>,
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Run a ROS node
    Run {
        package_name: String,
        executable_name: String,
        /// Arguments passed to the node
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Launch a ROS launch file
    Launch {
        package_name: String,
        launch_file: String,
        /// Arguments passed to ros2 launch
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Resolve dependencies without building
    Resolve {
        /// Show what would be installed without doing it
        #[arg(long)]
        dry_run: bool,
        /// Force source pull for all deps (skip rosdep)
        #[arg(long)]
        source_only: bool,
        /// Force re-download of the rosdistro cache
        #[arg(long)]
        refresh: bool,
        /// Assume yes to all prompts (non-interactive)
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Generate shell completions
    Completions {
        /// Shell to generate completions for (bash, zsh, fish, elvish, powershell)
        shell: clap_complete::Shell,
        /// Write the completion script to the shell's conventional directory
        #[arg(long, conflicts_with = "print")]
        install: bool,
        /// Print raw completion script to stdout (for custom piping)
        #[arg(long, conflicts_with = "install")]
        print: bool,
    },
    /// Remove build artifacts
    Clean {
        /// Also remove install/ (user workspace)
        #[arg(long)]
        all: bool,
        /// Remove .rosup/build/ and .rosup/install/ (dep layer artifacts)
        #[arg(long)]
        deps: bool,
        /// Also remove .rosup/src/ worktrees (implies --deps)
        #[arg(long, requires = "deps")]
        src: bool,
        /// Clean only the specified packages
        #[arg(short, long, num_args = 1..)]
        packages: Vec<String>,
    },
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = Cli::parse();

    let log_level = match cli.verbose {
        0 => tracing::Level::WARN,
        1 => tracing::Level::INFO,
        2 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    };
    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    // If --manifest-path is given, change to its parent directory so all
    // commands that call Project::load_from(&cwd) find the right project.
    if let Some(ref manifest) = cli.manifest_path {
        let root = manifest.parent().ok_or_else(|| {
            color_eyre::eyre::eyre!("invalid --manifest-path: {}", manifest.display())
        })?;
        std::env::set_current_dir(root)
            .wrap_err_with(|| format!("failed to change directory to {}", root.display()))?;
    }

    match cli.command {
        Command::New {
            name,
            ros_distro,
            build_type,
            deps,
            node,
            destination,
        } => cmd_new(name, ros_distro, build_type, deps, node, destination),
        Command::Init {
            workspace,
            lock,
            ros_distro,
            force,
        } => cmd_init(workspace, lock, ros_distro, force),
        Command::Sync {
            dry_run,
            check,
            lock,
        } => cmd_sync(dry_run, check, lock),
        Command::Add {
            dep_name,
            package,
            build,
            exec,
            test,
            dev,
        } => cmd_add(dep_name, package, build, exec, test, dev),
        Command::Remove { dep_name, package } => cmd_remove(dep_name, package),
        Command::Exclude { pattern, pkg, list } => cmd_exclude(pattern, pkg, list),
        Command::Include { pattern, pkg } => cmd_include(pattern, pkg),
        Command::Resolve {
            dry_run,
            source_only,
            refresh,
            yes,
        } => cmd_resolve(dry_run, source_only, refresh, yes),
        Command::Build {
            packages,
            deps,
            cmake_args,
            no_resolve,
            rebuild_deps,
            release,
            debug,
        } => cmd_build(
            packages,
            deps,
            cmake_args,
            no_resolve,
            rebuild_deps,
            release,
            debug,
        ),
        Command::Test {
            packages,
            retest_until_pass,
            no_resolve,
        } => cmd_test(packages, retest_until_pass, no_resolve),
        Command::Clean {
            all,
            deps,
            src,
            packages,
        } => cmd_clean(all, deps, src, packages),
        Command::Clone {
            package_name,
            distro,
            destination,
        } => cmd_clone(package_name, distro, destination),
        Command::Search {
            query,
            distro,
            limit,
        } => cmd_search(query, distro, limit),
        Command::Completions {
            shell,
            install,
            print,
        } => cmd_completions(shell, install, print),
        Command::Run {
            package_name,
            executable_name,
            args,
        } => cmd_run(package_name, executable_name, args),
        Command::Launch {
            package_name,
            launch_file,
            args,
        } => cmd_launch(package_name, launch_file, args),
    }
}

// ── completions ───────────────────────────────────────────────────────────────

fn cmd_completions(shell: clap_complete::Shell, install: bool, print: bool) -> Result<()> {
    use clap_complete::Shell::*;

    if print {
        clap_complete::generate(shell, &mut Cli::command(), "rosup", &mut std::io::stdout());
        return Ok(());
    }

    if install {
        let mut buf = Vec::new();
        clap_complete::generate(shell, &mut Cli::command(), "rosup", &mut buf);

        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .wrap_err("$HOME is not set")?;

        let dest = match shell {
            Bash => home.join(".local/share/bash-completion/completions/rosup"),
            Zsh => home.join(".zfunc/_rosup"),
            Fish => home.join(".config/fish/completions/rosup.fish"),
            _ => {
                color_eyre::eyre::bail!(
                    "--install is not supported for {shell}; use --print and redirect manually"
                );
            }
        };

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .wrap_err_with(|| format!("failed to create {}", parent.display()))?;
        }
        std::fs::write(&dest, &buf)
            .wrap_err_with(|| format!("failed to write {}", dest.display()))?;

        println!("Wrote completions to {}", dest.display());

        // Print any remaining activation steps.
        match shell {
            Zsh => {
                println!();
                println!("Make sure your ~/.zshrc contains (before compinit):");
                println!("  fpath=(~/.zfunc $fpath)");
                println!("  autoload -Uz compinit && compinit");
            }
            Bash => {
                println!();
                println!("Completions will be loaded automatically in new shells.");
                println!("To activate now:  source {}", dest.display());
            }
            _ => {}
        }

        return Ok(());
    }

    // Default: print installation instructions.
    let instructions = match shell {
        Bash => {
            "\
Install completions automatically:
  rosup completions bash --install

Or manually:
  rosup completions bash --print > ~/.local/share/bash-completion/completions/rosup

To source directly in ~/.bashrc:
  source <(rosup completions bash --print)"
        }
        Zsh => {
            "\
Install completions automatically:
  rosup completions zsh --install

Or manually:
  rosup completions zsh --print > ~/.zfunc/_rosup

Make sure your ~/.zshrc contains (before compinit):
  fpath=(~/.zfunc $fpath)
  autoload -Uz compinit && compinit"
        }
        Fish => {
            "\
Install completions automatically:
  rosup completions fish --install

Or manually:
  rosup completions fish --print > ~/.config/fish/completions/rosup.fish"
        }
        Elvish => {
            "\
Generate and save completions:
  rosup completions elvish --print > ~/.config/elvish/lib/completions/rosup.elv

Then add to ~/.config/elvish/rc.elv:
  use completions/rosup"
        }
        PowerShell => {
            "\
Add to your PowerShell profile ($PROFILE):
  rosup completions powershell --print >> $PROFILE"
        }
        _ => "Unknown shell; use --print to output raw completions.",
    };
    println!("{instructions}");
    Ok(())
}

// ── overlay ───────────────────────────────────────────────────────────────────

/// Try to load the nearest `rosup.toml` and source its configured overlays,
/// applying the resulting environment to the current process.
///
/// This runs once at startup so every subcommand (search, clone, resolve, …)
/// automatically sees the correct `ROS_DISTRO`, `AMENT_PREFIX_PATH`, etc.
/// without requiring `ros-distro` to be set explicitly in the config.
///
/// Fails hard if overlays are configured but a setup script is missing or
/// fails.  Silently returns an empty map when there is no project in the CWD
/// or the project has no overlays.
fn apply_overlays_from_cwd() -> Result<HashMap<String, String>> {
    let Ok(cwd) = std::env::current_dir() else {
        return Ok(HashMap::new());
    };
    let Ok(project) = Project::load_from(&cwd) else {
        return Ok(HashMap::new());
    };

    // Use configured overlays; if none are listed, auto-source the workspace's
    // own Colcon install/ directory when it exists (standard colcon layout).
    let overlays = if project.config.resolve.overlays.is_empty() {
        let install_setup = project.root.join("install").join("setup.bash");
        if install_setup.exists() {
            vec![project.root.join("install")]
        } else {
            vec![]
        }
    } else {
        project.config.resolve.overlays.clone()
    };

    let env = overlay::build_env(&overlays).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    for (key, val) in &env {
        // Safety: single-threaded at this point, no concurrent env reads.
        unsafe { std::env::set_var(key, val) };
    }

    Ok(env)
}

// ── new ───────────────────────────────────────────────────────────────────────

fn prompt_ros_distro() -> Result<String> {
    use std::io::Write;
    print!("ROS distro (e.g. humble, jazzy): ");
    std::io::stdout()
        .flush()
        .wrap_err("failed to flush stdout")?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .wrap_err("failed to read input")?;
    let distro = input.trim().to_owned();
    if distro.is_empty() {
        color_eyre::eyre::bail!("ROS distro must not be empty");
    }
    Ok(distro)
}

fn prompt_build_type() -> Result<BuildType> {
    use std::io::Write;
    println!("Select build type:");
    println!("  1) ament_cmake   (C++ / CMake)");
    println!("  2) ament_python  (pure Python)");
    print!("Build type [1]: ");
    std::io::stdout()
        .flush()
        .wrap_err("failed to flush stdout")?;
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .wrap_err("failed to read input")?;
    match input.trim() {
        "" | "1" | "ament_cmake" => Ok(BuildType::AmentCmake),
        "2" | "ament_python" => Ok(BuildType::AmentPython),
        other => color_eyre::eyre::bail!(
            "unknown build type `{other}`; expected 1, 2, ament_cmake, or ament_python"
        ),
    }
}

fn cmd_new(
    name: String,
    ros_distro: Option<String>,
    build_type: Option<BuildType>,
    deps: Vec<String>,
    node: Option<String>,
    destination: Option<std::path::PathBuf>,
) -> Result<()> {
    let ros_distro = match ros_distro {
        Some(d) => d,
        None => prompt_ros_distro()?,
    };
    let build_type = match build_type {
        Some(bt) => bt,
        None => prompt_build_type()?,
    };
    let destination = match destination {
        Some(d) => d,
        None => std::env::current_dir().wrap_err("failed to get current directory")?,
    };
    let opts = NewOptions {
        name: &name,
        ros_distro: &ros_distro,
        build_type,
        deps: &deps,
        node_name: node.as_deref(),
        destination: &destination,
    };
    let pkg_dir = new::scaffold(&opts).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    println!(
        "Created {} package `{name}` at {}",
        build_type,
        pkg_dir.display()
    );
    Ok(())
}

// ── init ──────────────────────────────────────────────────────────────────────

fn cmd_init(
    force_workspace: bool,
    lock: bool,
    ros_distro: Option<String>,
    force: bool,
) -> Result<()> {
    if lock && !force_workspace {
        eprintln!("warning: --lock has no effect without --workspace");
    }
    // Check existence before prompting so users don't type a distro for nothing.
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    if cwd.join("rosup.toml").exists() && !force {
        color_eyre::eyre::bail!(
            "rosup.toml already exists at {}. Use --force to overwrite.",
            cwd.join("rosup.toml").display()
        );
    }
    let ros_distro = match ros_distro {
        Some(d) => Some(d),
        None => match std::env::var("ROS_DISTRO") {
            Ok(d) if !d.is_empty() => Some(d),
            _ => Some(prompt_ros_distro()?),
        },
    };
    let result = init::init(&cwd, force_workspace, lock, ros_distro.as_deref(), force)?;
    let mode_label = match result.mode {
        InitMode::Package => "package",
        InitMode::Workspace => "workspace (auto-discovery)",
        InitMode::WorkspaceLocked => "workspace (locked member list)",
    };
    println!(
        "Initialized {} manifest at {}",
        mode_label,
        result.rosup_toml_path.display()
    );
    Ok(())
}

// ── sync ──────────────────────────────────────────────────────────────────────

fn cmd_sync(dry_run: bool, check: bool, lock: bool) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    let ws = project
        .config
        .workspace
        .as_ref()
        .ok_or_else(|| color_eyre::eyre::eyre!("`rosup sync` requires a workspace project"))?;

    // If auto-discovery and no --lock, nothing to do.
    if ws.members.is_none() && !lock {
        println!(
            "Workspace is in auto-discovery mode. Use --lock to write an explicit member list."
        );
        return Ok(());
    }

    let current: Vec<String> = init::colcon_scan(&project.root, &ws.exclude)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    let existing: Vec<String> = ws.members.clone().unwrap_or_default();

    let added: Vec<&str> = current
        .iter()
        .filter(|m| !existing.contains(m))
        .map(String::as_str)
        .collect();
    let removed: Vec<&str> = existing
        .iter()
        .filter(|m| !current.contains(m))
        .map(String::as_str)
        .collect();

    if check {
        if added.is_empty() && removed.is_empty() {
            println!("Member list is up to date.");
            return Ok(());
        }
        for m in &added {
            println!("+ {m}");
        }
        for m in &removed {
            println!("- {m}");
        }
        color_eyre::eyre::bail!("member list is out of date");
    }

    if dry_run {
        if added.is_empty() && removed.is_empty() {
            println!("Member list is up to date.");
        } else {
            for m in &added {
                println!("+ {m}");
            }
            for m in &removed {
                println!("- {m}");
            }
        }
        return Ok(());
    }

    if added.is_empty() && removed.is_empty() && !lock {
        println!("Member list is already up to date.");
        return Ok(());
    }

    // Write the updated member list back.
    let toml_path = project.root.join("rosup.toml");
    let toml_text = std::fs::read_to_string(&toml_path).wrap_err("failed to read rosup.toml")?;
    let patched = init::patch_members(&toml_text, Some(&current));
    std::fs::write(&toml_path, &patched).wrap_err("failed to write rosup.toml")?;
    println!("Updated {} member(s) in rosup.toml.", current.len());
    Ok(())
}

// ── exclude / include ─────────────────────────────────────────────────────────

/// Resolve a --pkg name to its relative directory path within the workspace.
fn resolve_pkg_to_path(root: &std::path::Path, pkg_name: &str) -> Result<String> {
    let all_members = init::colcon_scan(root, &[]).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    for rel in &all_members {
        let pkg_xml = root.join(rel).join("package.xml");
        if pkg_xml.exists()
            && let Ok(manifest) = rosup_core::package_xml::parse_file(&pkg_xml)
            && manifest.name == pkg_name
        {
            return Ok(rel.clone());
        }
    }
    color_eyre::eyre::bail!(
        "package `{pkg_name}` not found in workspace source tree.\n\
         Only workspace member packages can be excluded."
    )
}

/// Expand a CLI pattern (may contain `*` in the last path segment) to
/// concrete directory paths. `**` is not supported.
fn expand_exclude_pattern(root: &std::path::Path, pattern: &str) -> Result<Vec<String>> {
    if !pattern.contains('*') {
        return Ok(vec![pattern.to_owned()]);
    }
    if pattern.contains("**") {
        color_eyre::eyre::bail!(
            "`**` glob is not supported in exclude patterns.\n\
             Use `*` in the last path segment (e.g. `src/isaac_*`),\n\
             or exclude directories individually."
        );
    }
    // Use glob to expand, then return relative paths that are directories.
    let abs_pattern = root.join(pattern).display().to_string();
    let mut paths = Vec::new();
    for entry in glob::glob(&abs_pattern).wrap_err("invalid glob pattern")? {
        let entry = entry.wrap_err("glob error")?;
        if entry.is_dir() {
            let rel = entry
                .strip_prefix(root)
                .unwrap_or(&entry)
                .display()
                .to_string();
            // Skip hidden directories.
            if rel.starts_with('.') || rel.contains("/.") {
                continue;
            }
            paths.push(rel);
        }
    }
    if paths.is_empty() {
        color_eyre::eyre::bail!("pattern `{pattern}` matched no directories");
    }
    Ok(paths)
}

/// Normalize the exclude list: remove entries that are redundant because
/// a broader entry already covers them. This handles manually edited
/// rosup.toml files where the user wrote overlapping entries.
fn normalize_excludes(excludes: &mut Vec<String>) {
    excludes.sort();
    let mut i = 0;
    while i < excludes.len() {
        let mut j = i + 1;
        while j < excludes.len() {
            if excludes[j].starts_with(&format!("{}/", excludes[i])) {
                excludes.remove(j);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

/// Add paths to the exclude set, maintaining the prefix invariant:
/// no entry is a prefix of another.
fn exclude_add(excludes: &mut Vec<String>, new_paths: &[String]) {
    normalize_excludes(excludes);
    for path in new_paths {
        // If already covered by an existing broader entry, skip.
        if excludes
            .iter()
            .any(|ex| path == ex || path.starts_with(&format!("{ex}/")))
        {
            continue;
        }
        // Remove existing entries that the new path subsumes.
        excludes.retain(|ex| !ex.starts_with(&format!("{path}/")));
        excludes.push(path.clone());
    }
    excludes.sort();
}

/// Remove a path from the exclude set. If the path is covered by a broader
/// entry, split that entry into sibling packages.
fn exclude_remove(
    excludes: &mut Vec<String>,
    target: &str,
    workspace_root: &std::path::Path,
) -> Result<()> {
    normalize_excludes(excludes);

    // Exact match: just remove.
    if let Some(pos) = excludes.iter().position(|e| e == target) {
        excludes.remove(pos);
        return Ok(());
    }

    // Find the covering entry (a prefix of the target).
    let covering = excludes
        .iter()
        .position(|ex| target.starts_with(&format!("{ex}/")));

    let Some(covering_idx) = covering else {
        color_eyre::eyre::bail!("`{target}` is not excluded (no matching entry in exclude list)");
    };

    let covering_path = excludes.remove(covering_idx);

    // Scan the covering directory for all packages, re-add all except target.
    let covering_dir = workspace_root.join(&covering_path);
    let siblings =
        init::colcon_scan(&covering_dir, &[]).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    for sibling_rel in &siblings {
        let full_rel = format!("{covering_path}/{sibling_rel}");
        if full_rel != target {
            excludes.push(full_rel);
        }
    }
    excludes.sort();
    Ok(())
}

fn cmd_exclude(pattern: Option<String>, pkg: Option<String>, list: bool) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    let ws = project
        .config
        .workspace
        .as_ref()
        .ok_or_else(|| color_eyre::eyre::eyre!("`exclude` requires a workspace project"))?;

    if list || (pattern.is_none() && pkg.is_none()) {
        if ws.exclude.is_empty() {
            println!("No exclude patterns configured.");
        } else {
            for pat in &ws.exclude {
                println!("{pat}");
            }
        }
        return Ok(());
    }

    // Resolve to concrete paths.
    let new_paths = if let Some(pkg_name) = pkg {
        vec![resolve_pkg_to_path(&project.root, &pkg_name)?]
    } else {
        let pat = pattern.unwrap();
        // Guard: if the pattern has no '/' it's likely a package name, not a path.
        if !pat.contains('/') {
            let all_members = init::colcon_scan(&project.root, &[])
                .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
            for rel in &all_members {
                let pkg_xml = project.root.join(rel).join("package.xml");
                if pkg_xml.exists()
                    && let Ok(manifest) = rosup_core::package_xml::parse_file(&pkg_xml)
                    && manifest.name == pat
                {
                    eprintln!(
                        "warning: `{pat}` looks like a package name, not a directory path.\n\
                         hint: rosup exclude --pkg {pat}\n\
                         hint: edit rosup.toml to fix the exclude list"
                    );
                    break;
                }
            }
        }
        expand_exclude_pattern(&project.root, &pat)?
    };

    let mut excludes = ws.exclude.clone();
    let before_len = excludes.len();
    exclude_add(&mut excludes, &new_paths);

    if excludes.len() == before_len && excludes == ws.exclude {
        println!("Already excluded.");
        return Ok(());
    }

    let toml_path = project.root.join("rosup.toml");
    let toml_text = std::fs::read_to_string(&toml_path).wrap_err("failed to read rosup.toml")?;
    let patched = init::patch_exclude(&toml_text, &excludes);
    std::fs::write(&toml_path, &patched).wrap_err("failed to write rosup.toml")?;
    for p in &new_paths {
        println!("Excluded {p}");
    }
    Ok(())
}

fn cmd_include(pattern: Option<String>, pkg: Option<String>) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    let ws = project
        .config
        .workspace
        .as_ref()
        .ok_or_else(|| color_eyre::eyre::eyre!("`include` requires a workspace project"))?;

    if pattern.is_none() && pkg.is_none() {
        color_eyre::eyre::bail!("provide a path or --pkg <name> to re-include");
    }

    let target = if let Some(pkg_name) = pkg {
        resolve_pkg_to_path(&project.root, &pkg_name)?
    } else {
        pattern.unwrap()
    };

    let mut excludes = ws.exclude.clone();
    exclude_remove(&mut excludes, &target, &project.root)?;

    let toml_path = project.root.join("rosup.toml");
    let toml_text = std::fs::read_to_string(&toml_path).wrap_err("failed to read rosup.toml")?;
    let patched = init::patch_exclude(&toml_text, &excludes);
    std::fs::write(&toml_path, &patched).wrap_err("failed to write rosup.toml")?;
    println!("Included {target}");
    Ok(())
}

// ── add / remove ──────────────────────────────────────────────────────────────

fn cmd_add(
    dep: String,
    package: Option<String>,
    build: bool,
    exec: bool,
    test: bool,
    dev: bool,
) -> Result<()> {
    let pkg_xml = resolve_package_xml(package)?;
    let tag = if build {
        DepTag::BuildDepend
    } else if exec {
        DepTag::ExecDepend
    } else if test {
        DepTag::TestDepend
    } else if dev {
        DepTag::Dev
    } else {
        DepTag::Depend
    };

    let inserted = manifest::add_dep(&pkg_xml, &dep, tag)?;
    for line in &inserted {
        println!("Added {line} to {}", pkg_xml.display());
    }
    Ok(())
}

fn cmd_remove(dep: String, package: Option<String>) -> Result<()> {
    let pkg_xml = resolve_package_xml(package)?;
    let removed = manifest::remove_dep(&pkg_xml, &dep)?;
    if removed.is_empty() {
        eprintln!("warning: `{dep}` not found in {}", pkg_xml.display());
    } else {
        for line in &removed {
            println!("Removed {line} from {}", pkg_xml.display());
        }
    }
    Ok(())
}

// ── resolve ───────────────────────────────────────────────────────────────────

fn cmd_resolve(dry_run: bool, source_only: bool, refresh: bool, yes: bool) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;

    // Resolve needs overlays sourced so ament checks and rosdep work correctly.
    apply_overlays_from_cwd()?;

    // Try to load a project manifest; fall back to a bare package.xml.
    let (dep_names, resolve_config, project_root) = match Project::load_from(&cwd) {
        Ok(project) => {
            let (deps, _members) = project
                .external_deps()
                .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
            (deps, project.config.resolve.clone(), project.root.clone())
        }
        Err(_) => {
            let pkg_xml = cwd.join("package.xml");
            if !pkg_xml.exists() {
                color_eyre::eyre::bail!(
                    "no rosup.toml or package.xml found — run `rosup init` or `rosup new`"
                );
            }
            let manifest = rosup_core::package_xml::parse_file(&pkg_xml)
                .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
            let deps = manifest.deps.all().into_iter().map(str::to_owned).collect();
            let resolve_config = ResolveConfig {
                ros_distro: std::env::var("ROS_DISTRO").ok(),
                ..ResolveConfig::default()
            };
            (deps, resolve_config, cwd.clone())
        }
    };

    if dep_names.is_empty() {
        println!("No dependencies to resolve.");
        return Ok(());
    }

    let resolver = Resolver::new(resolve_config, &project_root)?;

    if dry_run {
        // For --dry-run, build the plan and show everything: resolved deps
        // first, then unresolved. Exit non-zero if any are unresolved, but
        // always show the full picture.
        let plan = resolver
            .plan(&dep_names, source_only, refresh)
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

        for dep in &plan.resolved {
            println!("would resolve {}: {}", dep.name, format_method(&dep.method));
        }

        if !plan.unresolved.is_empty() {
            eprintln!();
            for name in &plan.unresolved {
                eprintln!("warning: could not resolve `{name}`");
            }
            color_eyre::eyre::bail!(
                "{} dependency(ies) could not be resolved",
                plan.unresolved.len()
            );
        }
    } else {
        // Non-dry-run: resolve and install. Abort on unresolved deps.
        let plan = resolver
            .resolve(&dep_names, false, source_only, refresh, yes)
            .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

        for dep in &plan.resolved {
            println!("resolved {}: {}", dep.name, format_method(&dep.method));
        }
    }

    Ok(())
}

// ── build ─────────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn cmd_build(
    packages: Vec<String>,
    include_deps: bool,
    cmake_args: Vec<String>,
    no_resolve: bool,
    rebuild_deps: bool,
    release: bool,
    debug: bool,
) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd)?;
    let project_store = ProjectStore::open(&project.root);

    let overlay_env = apply_overlays_from_cwd()?;

    // Phase 0: warn if base ROS environment is not sourced.
    builder::check_ros_env(
        project.config.resolve.ros_distro.as_deref(),
        &overlay::ament_prefix_path(&overlay_env),
    );

    // Phase 1: check that external deps are resolved.
    if !no_resolve {
        check_deps_resolved(&project)?;
    }

    // Phase 2: build the dep layer from source-pulled worktrees.
    builder::build_dep_layer(&project.root, &project_store, rebuild_deps, &overlay_env)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    // Phase 2.5: remove stale deps from .rosup/install/ that are now workspace members.
    let member_names: Vec<String> = project
        .members()
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?
        .iter()
        .map(|m| m.name.clone())
        .collect();
    builder::clean_stale_deps(&project_store, &member_names)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    // Phase 3: build the user workspace.
    let exclude_packages = project
        .excluded_package_names()
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    let opts = BuildOptions {
        packages: &packages,
        include_deps,
        cmake_args: &cmake_args,
        release,
        debug,
        exclude_packages: &exclude_packages,
    };
    builder::build_workspace(
        &project.root,
        &project_store,
        &project.config.build,
        &opts,
        &overlay_env,
    )
    .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    Ok(())
}

// ── test ──────────────────────────────────────────────────────────────────────

fn cmd_test(
    packages: Vec<String>,
    retest_until_pass: Option<usize>,
    no_resolve: bool,
) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd)?;
    let project_store = ProjectStore::open(&project.root);

    let overlay_env = apply_overlays_from_cwd()?;

    builder::check_ros_env(
        project.config.resolve.ros_distro.as_deref(),
        &overlay::ament_prefix_path(&overlay_env),
    );

    if !no_resolve {
        check_deps_resolved(&project)?;
    }

    let exclude_packages = project
        .excluded_package_names()
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    let opts = TestOptions {
        packages: &packages,
        retest_until_pass,
        exclude_packages: &exclude_packages,
    };
    builder::test_workspace(
        &project.root,
        &project_store,
        &project.config.test,
        &opts,
        &overlay_env,
    )
    .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    Ok(())
}

// ── clean ─────────────────────────────────────────────────────────────────────

fn cmd_clean(all: bool, deps: bool, src: bool, packages: Vec<String>) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd)?;
    let project_store = ProjectStore::open(&project.root);

    let opts = CleanOptions {
        all,
        deps,
        src,
        packages: &packages,
    };
    builder::clean(&project.root, &project_store, &opts)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    Ok(())
}

// ── run / launch ──────────────────────────────────────────────────────────────

fn cmd_run(package_name: String, executable_name: String, args: Vec<String>) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd)?;
    let project_store = ProjectStore::open(&project.root);
    let overlay_env = apply_overlays_from_cwd()?;

    runner::run(
        &project.root,
        &project_store,
        &overlay_env,
        &package_name,
        &executable_name,
        &args,
    )
    .map_err(|e| color_eyre::eyre::eyre!("{e}"))
}

fn cmd_launch(package_name: String, launch_file: String, args: Vec<String>) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd)?;
    let project_store = ProjectStore::open(&project.root);
    let overlay_env = apply_overlays_from_cwd()?;

    runner::launch(
        &project.root,
        &project_store,
        &overlay_env,
        &package_name,
        &launch_file,
        &args,
    )
    .map_err(|e| color_eyre::eyre::eyre!("{e}"))
}

// ── search ────────────────────────────────────────────────────────────────────

fn cmd_search(query: String, distro: Option<String>, limit: usize) -> Result<()> {
    let distro = resolve_distro(distro)?;
    let global = GlobalStore::open().map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    global
        .ensure()
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    let cache = DistroCache::load(&global.cache, &distro, false)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    let results = cache.search(&query, limit);
    if results.is_empty() {
        println!("No packages found matching `{query}`.");
        return Ok(());
    }

    // Align columns: compute max name width.
    let name_w = results.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    let repo_w = results.iter().map(|(_, s)| s.repo.len()).max().unwrap_or(0);

    for (name, src) in &results {
        println!(
            "{:<name_w$}  {:<repo_w$}  {} @ {}",
            name,
            src.repo,
            src.url,
            src.branch,
            name_w = name_w,
            repo_w = repo_w,
        );
    }
    Ok(())
}

// ── clone ─────────────────────────────────────────────────────────────────────

fn cmd_clone(
    package_name: String,
    distro: Option<String>,
    destination: Option<std::path::PathBuf>,
) -> Result<()> {
    let distro = resolve_distro(distro)?;
    let global = GlobalStore::open().map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    global
        .ensure()
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    let cache = DistroCache::load(&global.cache, &distro, false)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    let src = cache.lookup(&package_name).ok_or_else(|| {
        color_eyre::eyre::eyre!(
            "package `{package_name}` not found in rosdistro cache for {distro}"
        )
    })?;

    let parent = match &destination {
        Some(d) => d.clone(),
        None => std::env::current_dir().wrap_err("failed to get current directory")?,
    };
    let dest = parent.join(&src.repo);

    println!("Cloning {} @ {} → {}", src.url, src.branch, dest.display());

    let status = std::process::Command::new("git")
        .arg("clone")
        .arg("--branch")
        .arg(&src.branch)
        .arg(&src.url)
        .arg(&dest)
        .status()
        .wrap_err("failed to run git clone")?;

    if !status.success() {
        color_eyre::eyre::bail!(
            "git clone failed with exit code {}",
            status.code().unwrap_or(1)
        );
    }

    // Count package.xml files to choose init mode.
    let pkg_xml_count = count_package_xmls(&dest);
    let force_workspace = pkg_xml_count > 1;

    init::init(&dest, force_workspace, false, None, false)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    println!("Cloned `{}` repository to {}", src.repo, dest.display());
    Ok(())
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Collect all unique dependency names across all workspace members.
///
/// In package mode, reads the single `package.xml` at the project root.
/// In workspace mode, aggregates deps from all discovered member packages,
/// excluding deps whose names match workspace member packages (those are
/// built from source by colcon as part of the same workspace).
/// Check that all external deps can be resolved. Only truly unresolvable
/// deps block the build — deps that resolve to Binary, Source, or Override
/// are assumed to be satisfied after `rosup resolve`.
fn check_deps_resolved(project: &Project) -> Result<()> {
    let (dep_names, _members) = project
        .external_deps()
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
    if dep_names.is_empty() {
        return Ok(());
    }

    let resolver = Resolver::new(project.config.resolve.clone(), &project.root)?;
    let plan = resolver
        .plan(&dep_names, false, false)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    if plan.unresolved.is_empty() {
        return Ok(());
    }

    for name in &plan.unresolved {
        eprintln!("unresolved: {name}");
    }
    color_eyre::eyre::bail!(
        "{} dependency(ies) cannot be resolved. Fix them before building.\n\
         hint: rosup resolve --dry-run    # see the full resolution plan\n\
         hint: rosup exclude --pkg <name> # exclude the package that needs them\n\
         hint: add to [resolve] ignore-deps in rosup.toml to suppress",
        plan.unresolved.len()
    );
}

fn format_method(method: &rosup_core::resolver::ResolutionMethod) -> String {
    use rosup_core::resolver::ResolutionMethod;
    match method {
        ResolutionMethod::WorkspaceMember => "workspace member".to_owned(),
        ResolutionMethod::Ament => "already installed".to_owned(),
        ResolutionMethod::Binary { packages, .. } => {
            format!("binary: {}", packages.join(", "))
        }
        ResolutionMethod::Source { repo, url, branch } => {
            format!("source: {repo} ({url} @ {branch})")
        }
        ResolutionMethod::Override { url, branch, rev } => {
            let loc = rev.as_deref().or(branch.as_deref()).unwrap_or("HEAD");
            format!("override: {url} @ {loc}")
        }
    }
}

/// Resolve which package.xml to operate on.
///
/// In workspace mode with `-p pkg_name`, find the member package by name.
/// Otherwise, look for package.xml in the current directory.
fn resolve_package_xml(package: Option<String>) -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;

    if let Some(pkg_name) = package {
        let project = Project::load_from(&cwd)?;
        let members = project.members()?;
        let member = members
            .into_iter()
            .find(|m| m.name == pkg_name)
            .ok_or_else(|| eyre::eyre!("package `{pkg_name}` not found in workspace"))?;
        let pkg_xml = find_member_xml(&project.root, &member.name)?;
        return Ok(pkg_xml);
    }

    let pkg_xml = cwd.join("package.xml");
    if !pkg_xml.exists() {
        eyre::bail!(
            "no package.xml found in {}. Use -p <name> to specify a package.",
            cwd.display()
        );
    }
    Ok(pkg_xml)
}

/// Resolve the ROS distro for commands that accept `--distro`:
/// CLI flag → ros-distro in rosup.toml → `ROS_DISTRO` env → error.
fn resolve_distro(distro: Option<String>) -> Result<String> {
    if let Some(d) = distro {
        return Ok(d);
    }
    if let Ok(cwd) = std::env::current_dir()
        && let Ok(project) = Project::load_from(&cwd)
        && let Some(d) = project.config.resolve.ros_distro
    {
        return Ok(d);
    }
    if let Ok(d) = std::env::var("ROS_DISTRO")
        && !d.is_empty()
    {
        return Ok(d);
    }
    color_eyre::eyre::bail!(
        "no ROS distro specified — pass --distro, set ROS_DISTRO, or add ros-distro to [resolve] in rosup.toml"
    )
}

/// Count package.xml files under a directory (non-recursive depth-1 scan of
/// the tree, up to a reasonable limit).
fn count_package_xmls(dir: &std::path::Path) -> usize {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join("package.xml").exists() {
                count += 1;
            }
        } else if path.file_name().is_some_and(|n| n == "package.xml") {
            count += 1;
        }
    }
    count
}

/// Find the package.xml path for a named member within a project root.
///
/// Uses `project.members()` which handles both auto-discovery (`members`
/// absent) and glob mode (`members` present). The discovered members are
/// then matched by name to find the corresponding `package.xml` path.
fn find_member_xml(root: &std::path::Path, name: &str) -> Result<std::path::PathBuf> {
    let project = Project::load_at(root)?;
    if project.config.workspace.is_none() {
        eyre::bail!("not a workspace project");
    }

    // project.members() handles both auto-discovery and glob mode.
    // It returns parsed manifests, but we need the file path. Re-scan to
    // locate the directory by package name.
    let ws = project.config.workspace.as_ref().unwrap();

    let discovered =
        rosup_core::init::colcon_scan(root, &ws.exclude).map_err(|e| eyre::eyre!("{e}"))?;
    for rel in &discovered {
        let pkg_xml = root.join(rel).join("package.xml");
        if pkg_xml.exists() {
            let manifest = rosup_core::package_xml::parse_file(&pkg_xml)?;
            if manifest.name == name {
                return Ok(pkg_xml);
            }
        }
    }

    eyre::bail!("could not find package.xml for `{name}`")
}
