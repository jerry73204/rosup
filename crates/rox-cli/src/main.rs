use clap::{Parser, Subcommand};
use color_eyre::eyre::Result;

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
        Command::Init { .. } => todo!("rox init"),
        Command::Clone { .. } => todo!("rox clone"),
        Command::Build { .. } => todo!("rox build"),
        Command::Test { .. } => todo!("rox test"),
        Command::Add { .. } => todo!("rox add"),
        Command::Remove { .. } => todo!("rox remove"),
        Command::Search { .. } => todo!("rox search"),
        Command::Run { .. } => todo!("rox run"),
        Command::Launch { .. } => todo!("rox launch"),
        Command::Resolve { .. } => todo!("rox resolve"),
        Command::Clean { .. } => todo!("rox clean"),
    }
}
