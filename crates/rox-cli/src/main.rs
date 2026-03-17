use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr};
use rox_core::{
    init::{self, InitMode},
    manifest::{self, DepTag},
    project::Project,
};

#[derive(Parser)]
#[command(name = "rox", about = "A Cargo-like package manager for ROS 2")]
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
    /// Initialize a rox.toml for an existing package or workspace
    Init {
        /// Force workspace mode
        #[arg(long)]
        workspace: bool,
        /// Overwrite an existing rox.toml
        #[arg(long)]
        force: bool,
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
        /// Skip dependency resolution
        #[arg(long)]
        no_resolve: bool,
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
        /// Force source pull for all deps
        #[arg(long)]
        source_only: bool,
    },
    /// Remove build artifacts
    Clean {
        /// Also remove install/
        #[arg(long)]
        all: bool,
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

    match cli.command {
        Command::Init { workspace, force } => cmd_init(workspace, force),
        Command::Add {
            dep_name,
            package,
            build,
            exec,
            test,
            dev,
        } => cmd_add(dep_name, package, build, exec, test, dev),
        Command::Remove { dep_name, package } => cmd_remove(dep_name, package),
        Command::Clone { .. } => todo!("rox clone"),
        Command::Build { .. } => todo!("rox build"),
        Command::Test { .. } => todo!("rox test"),
        Command::Search { .. } => todo!("rox search"),
        Command::Run { .. } => todo!("rox run"),
        Command::Launch { .. } => todo!("rox launch"),
        Command::Resolve { .. } => todo!("rox resolve"),
        Command::Clean { .. } => todo!("rox clean"),
    }
}

fn cmd_init(force_workspace: bool, force: bool) -> Result<()> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;
    let result = init::init(&cwd, force_workspace, force)?;
    let mode_label = match result.mode {
        InitMode::Package => "package",
        InitMode::Workspace => "workspace",
    };
    println!(
        "Initialized {} manifest at {}",
        mode_label,
        result.rox_toml_path.display()
    );
    Ok(())
}

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

/// Resolve which package.xml to operate on.
///
/// In workspace mode with `-p pkg_name`, find the member package by name.
/// Otherwise, look for package.xml in the current directory.
fn resolve_package_xml(package: Option<String>) -> Result<std::path::PathBuf> {
    let cwd = std::env::current_dir().wrap_err("failed to get current directory")?;

    if let Some(pkg_name) = package {
        // Walk up to find project root, then locate member.
        let project = Project::load_from(&cwd)?;
        let members = project.members()?;
        let member = members
            .into_iter()
            .find(|m| m.name == pkg_name)
            .ok_or_else(|| eyre::eyre!("package `{pkg_name}` not found in workspace"))?;
        // Re-derive path from glob results — find the dir containing package.xml.
        let pkg_xml = find_member_xml(&project.root, &member.name)?;
        return Ok(pkg_xml);
    }

    // No -p flag: use package.xml in cwd.
    let pkg_xml = cwd.join("package.xml");
    if !pkg_xml.exists() {
        eyre::bail!(
            "no package.xml found in {}. Use -p <name> to specify a package.",
            cwd.display()
        );
    }
    Ok(pkg_xml)
}

/// Find the package.xml path for a named member within a project root by
/// scanning the workspace member directories.
fn find_member_xml(root: &std::path::Path, name: &str) -> Result<std::path::PathBuf> {
    let project = Project::load_at(root)?;
    if let Some(ws) = &project.config.workspace {
        for pattern in &ws.members {
            let abs = root.join(pattern).display().to_string();
            for entry in glob::glob(&abs).wrap_err("invalid glob pattern")? {
                let entry: std::path::PathBuf = entry.wrap_err("glob error")?;
                let pkg_xml = entry.join("package.xml");
                if pkg_xml.exists() {
                    let manifest = rox_core::package_xml::parse_file(&pkg_xml)?;
                    if manifest.name == name {
                        return Ok(pkg_xml);
                    }
                }
            }
        }
    }
    eyre::bail!("could not find package.xml for `{name}`")
}
