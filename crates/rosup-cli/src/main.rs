use std::collections::HashMap;

use clap::{Parser, Subcommand};
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
        ResolutionMethod, Resolver,
        rosdistro::DistroCache,
        store::{GlobalStore, ProjectStore},
    },
};

#[derive(Parser)]
#[command(name = "rosup", about = "A Cargo-like package manager for ROS 2")]
#[command(version, propagate_version = true)]
struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

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

    // Best-effort: load overlays from the nearest rosup.toml and apply them to
    // the current process env before dispatching any subcommand. This makes
    // ROS_DISTRO, AMENT_PREFIX_PATH, etc. available to all commands — not just
    // build/test — so search, clone, and resolve also see the right distro
    // without requiring ros-distro in the config.
    let overlay_env = apply_overlays_from_cwd()?;

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
        Command::Resolve {
            dry_run,
            source_only,
            refresh,
        } => cmd_resolve(dry_run, source_only, refresh),
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
            &overlay_env,
        ),
        Command::Test {
            packages,
            retest_until_pass,
            no_resolve,
        } => cmd_test(packages, retest_until_pass, no_resolve, &overlay_env),
        Command::Clean {
            all,
            deps,
            src,
            packages,
        } => cmd_clean(all, deps, src, packages),
        Command::Clone {
            package_name,
            distro,
        } => cmd_clone(package_name, distro),
        Command::Search {
            query,
            distro,
            limit,
        } => cmd_search(query, distro, limit),
        Command::Run { .. } => todo!("rosup run"),
        Command::Launch { .. } => todo!("rosup launch"),
    }
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
        let install_setup = project.root.join("install").join("setup.sh");
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
    let ros_distro = match ros_distro {
        Some(d) => Some(d),
        None => match std::env::var("ROS_DISTRO") {
            Ok(d) if !d.is_empty() => Some(d),
            _ => Some(prompt_ros_distro()?),
        },
    };
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
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

    let excludes: Vec<glob::Pattern> = ws
        .exclude
        .iter()
        .map(|p| glob::Pattern::new(p))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    let current: Vec<String> =
        init::colcon_scan(&project.root, &excludes).map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

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

fn cmd_resolve(dry_run: bool, source_only: bool, refresh: bool) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;

    // Try to load a project manifest; fall back to a bare package.xml.
    let (dep_names, resolve_config, project_root) = match Project::load_from(&cwd) {
        Ok(project) => {
            let deps = collect_dep_names(&project)?;
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
            // No rosup.toml — this is the one place where ROS_DISTRO env is
            // accepted as a source of truth (the user must have a ROS environment
            // sourced to be working with a bare package.xml).
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
    let plan = resolver
        .resolve(&dep_names, dry_run, source_only, refresh)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    for dep in &plan.resolved {
        let action = match &dep.method {
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
        };
        let prefix = if dry_run { "would resolve" } else { "resolved" };
        println!("{prefix} {}: {action}", dep.name);
    }

    if !plan.unresolved.is_empty() {
        for name in &plan.unresolved {
            eprintln!("error: could not resolve `{name}`");
        }
        color_eyre::eyre::bail!("unresolved dependencies");
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
    overlay_env: &HashMap<String, String>,
) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd)?;
    let project_store = ProjectStore::open(&project.root);

    // Phase 0: warn if base ROS environment is not sourced.
    builder::check_ros_env(
        project.config.resolve.ros_distro.as_deref(),
        &overlay::ament_prefix_path(overlay_env),
    );

    // Phase 1: resolve deps and clone source packages.
    if !no_resolve {
        let dep_names = collect_dep_names(&project)?;
        if !dep_names.is_empty() {
            let resolver = Resolver::new(project.config.resolve.clone(), &project.root)?;
            resolver
                .resolve(&dep_names, false, false, false)
                .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        }
    }

    // Phase 2: build the dep layer from source-pulled worktrees.
    builder::build_dep_layer(&project.root, &project_store, rebuild_deps, overlay_env)
        .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    // Phase 3: build the user workspace.
    let opts = BuildOptions {
        packages: &packages,
        include_deps,
        cmake_args: &cmake_args,
        release,
        debug,
    };
    builder::build_workspace(
        &project.root,
        &project_store,
        &project.config.build,
        &opts,
        overlay_env,
    )
    .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;

    Ok(())
}

// ── test ──────────────────────────────────────────────────────────────────────

fn cmd_test(
    packages: Vec<String>,
    retest_until_pass: Option<usize>,
    no_resolve: bool,
    overlay_env: &HashMap<String, String>,
) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let project = Project::load_from(&cwd)?;
    let project_store = ProjectStore::open(&project.root);

    builder::check_ros_env(
        project.config.resolve.ros_distro.as_deref(),
        &overlay::ament_prefix_path(overlay_env),
    );

    if !no_resolve {
        let dep_names = collect_dep_names(&project)?;
        if !dep_names.is_empty() {
            let resolver = Resolver::new(project.config.resolve.clone(), &project.root)?;
            resolver
                .resolve(&dep_names, false, false, false)
                .map_err(|e| color_eyre::eyre::eyre!("{e}"))?;
        }
    }

    let opts = TestOptions {
        packages: &packages,
        retest_until_pass,
    };
    builder::test_workspace(
        &project.root,
        &project_store,
        &project.config.test,
        &opts,
        overlay_env,
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

fn cmd_clone(package_name: String, distro: Option<String>) -> Result<()> {
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

    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let dest = cwd.join(&src.repo);

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
fn collect_dep_names(project: &Project) -> Result<Vec<String>> {
    let manifests = if project.config.workspace.is_some() {
        project.members()?
    } else {
        let pkg_xml = project.root.join("package.xml");
        if pkg_xml.exists() {
            vec![
                rosup_core::package_xml::parse_file(&pkg_xml)
                    .map_err(|e| color_eyre::eyre::eyre!("{e}"))?,
            ]
        } else {
            Vec::new()
        }
    };

    // In workspace mode, deps that are names of workspace members are resolved
    // by colcon building the workspace — don't pass them to the external resolver.
    let member_names: std::collections::HashSet<&str> =
        manifests.iter().map(|m| m.name.as_str()).collect();

    let mut dep_names: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for pkg in &manifests {
        for dep in pkg.deps.all() {
            if member_names.contains(dep) {
                continue;
            }
            if seen.insert(dep.to_owned()) {
                dep_names.push(dep.to_owned());
            }
        }
    }
    Ok(dep_names)
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

/// Resolve the ROS distro for commands that accept `--distro`: CLI flag →
/// ros-distro in rosup.toml → error.
///
/// `ROS_DISTRO` from the environment is intentionally not consulted: the distro
/// must be explicit either on the command line or in the project manifest.
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
    color_eyre::eyre::bail!(
        "no ROS distro specified — pass --distro or add ros-distro to [resolve] in rosup.toml"
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
    let excludes: Vec<glob::Pattern> = ws
        .exclude
        .iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();

    let discovered =
        rosup_core::init::colcon_scan(root, &excludes).map_err(|e| eyre::eyre!("{e}"))?;
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
