pub mod ament;
pub mod repos_file;
pub mod rosdep;
pub mod rosdistro;
pub mod store;

use std::collections::HashMap;
use std::path::Path;

use thiserror::Error;
use tracing::{debug, info, warn};

use crate::config::{DepOverride, ResolveConfig, SourcePreference};
use crate::git::sparse::{self, SparseError};
use rosdep::RosdepError;
use rosdistro::DistroCache;
use store::{GlobalStore, ProjectStore};

// ── public types ──────────────────────────────────────────────────────────────

/// How a dependency was (or will be) resolved.
#[derive(Debug, Clone)]
pub enum ResolutionMethod {
    /// Another package in the same workspace — built from source by colcon.
    WorkspaceMember,
    /// Already present in the ament environment.
    Ament,
    /// Installed as a binary package via rosdep.
    Binary {
        installer: String,
        packages: Vec<String>,
    },
    /// Cloned from a .repos source file registered in `[[resolve.sources]]`.
    ReposSource {
        /// Name of the source (from rosup.toml).
        source_name: String,
        url: String,
        version: String,
    },
    /// Cloned from a source repository found in rosdistro.
    ///
    /// `repo` is the rosdistro repository name, which may differ from the
    /// package name for multi-package repos (e.g. `common_interfaces` contains
    /// `sensor_msgs`, `std_msgs`, etc.).
    Source {
        repo: String,
        url: String,
        branch: String,
    },
    /// Cloned using a user-supplied override from `[resolve.overrides]` in `rosup.toml`.
    Override {
        url: String,
        branch: Option<String>,
        rev: Option<String>,
    },
}

impl std::fmt::Display for ResolutionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionMethod::WorkspaceMember => write!(f, "workspace member"),
            ResolutionMethod::Ament => write!(f, "ament (already installed)"),
            ResolutionMethod::Binary { packages, .. } => {
                write!(f, "binary ({})", packages.join(", "))
            }
            ResolutionMethod::ReposSource {
                source_name,
                url,
                version,
            } => {
                write!(f, "repos ({source_name}: {url} @ {version})")
            }
            ResolutionMethod::Source { repo, url, branch } => {
                write!(f, "source ({repo}: {url} @ {branch})")
            }
            ResolutionMethod::Override { url, branch, rev } => {
                let loc = rev.as_deref().or(branch.as_deref()).unwrap_or("HEAD");
                write!(f, "override ({url} @ {loc})")
            }
        }
    }
}

/// A resolved (or unresolvable) dependency.
#[derive(Debug)]
pub struct ResolvedDep {
    pub name: String,
    pub method: ResolutionMethod,
}

/// The full resolution plan for a set of dependencies.
#[derive(Debug, Default)]
pub struct ResolutionPlan {
    pub resolved: Vec<ResolvedDep>,
    pub unresolved: Vec<String>,
}

impl ResolutionPlan {
    pub fn is_ok(&self) -> bool {
        self.unresolved.is_empty()
    }
}

#[derive(Debug, Error)]
pub enum ResolverError {
    #[error("store error: {0}")]
    Store(#[from] store::StoreError),
    #[error("rosdistro error: {0}")]
    Rosdistro(#[from] rosdistro::RosdistroError),
    #[error("rosdep error: {0}")]
    Rosdep(#[from] rosdep::RosdepError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("repos file error: {0}")]
    ReposFile(#[from] repos_file::ReposFileError),
    #[error("git error running `{cmd}`: {detail}")]
    Git { cmd: String, detail: String },
    #[error("sparse checkout error: {0}")]
    Sparse(#[from] SparseError),
    #[error("unresolved dependencies: {0:?}")]
    Unresolved(Vec<String>),
    #[error("no ROS distro configured — add ros-distro to [resolve] in rosup.toml")]
    NoDistro,
}

// ── Resolver ──────────────────────────────────────────────────────────────────

pub struct Resolver {
    distro: String,
    config: ResolveConfig,
    global: GlobalStore,
    project: ProjectStore,
    project_root: std::path::PathBuf,
}

impl Resolver {
    pub fn new(config: ResolveConfig, project_root: &Path) -> Result<Self, ResolverError> {
        // Prefer explicit config; fall back to the sourced ROS environment.
        let distro = config
            .ros_distro
            .clone()
            .or_else(|| std::env::var("ROS_DISTRO").ok())
            .ok_or(ResolverError::NoDistro)?;

        let global = GlobalStore::open()?;
        global.ensure()?;

        let project = ProjectStore::open(project_root);
        // Project dirs are created on demand in execute(), not here.

        Ok(Self {
            distro,
            config,
            global,
            project,
            project_root: project_root.to_owned(),
        })
    }

    /// Load sources from [[resolve.sources]] and build a package → entry index.
    ///
    /// Handles both `.repos` files and direct `git` repo sources.
    /// Returns a map of package_name → (source_name, RepoEntry).
    fn load_repos_index(
        &self,
        cache: &DistroCache,
    ) -> Result<HashMap<String, (String, repos_file::RepoEntry)>, ResolverError> {
        let mut combined_index = HashMap::new();

        // Build rosdistro URL index once (shared across all sources).
        let rosdistro_urls: HashMap<String, String> = cache
            .all_packages()
            .into_iter()
            .filter_map(|pkg| cache.lookup(&pkg).map(|src| (pkg, src.url.clone())))
            .collect();

        for source in &self.config.sources {
            if let Some(repos_path_str) = &source.repos {
                // .repos file source
                let repos_path = self.project_root.join(repos_path_str);
                if !repos_path.exists() {
                    warn!(
                        "repos file not found: {} (source: {})",
                        repos_path.display(),
                        source.name
                    );
                    continue;
                }

                let repos = repos_file::ReposFile::parse(&repos_path, &source.name)?;
                let file_index = repos_file::build_package_index(
                    &repos,
                    &rosdistro_urls,
                    Some(&self.global.src),
                );
                for (pkg_name, entry) in file_index {
                    combined_index
                        .entry(pkg_name)
                        .or_insert_with(|| (source.name.clone(), entry.clone()));
                }
            } else if let Some(git_url) = &source.git {
                // Direct git repo source — resolve version from branch/tag/rev.
                let version = source
                    .rev
                    .as_deref()
                    .or(source.tag.as_deref())
                    .or(source.branch.as_deref())
                    .unwrap_or("HEAD")
                    .to_owned();

                let entry = repos_file::RepoEntry {
                    path: source.name.clone(),
                    url: git_url.clone(),
                    version,
                };

                // For a direct git repo, use the source name as the package
                // name (single-package repo heuristic). Also check rosdistro
                // for multi-package repos with matching URL.
                let mut matched = false;
                let normalized = git_url.trim_end_matches(".git");
                for (pkg_name, src_url) in &rosdistro_urls {
                    if src_url.trim_end_matches(".git") == normalized {
                        combined_index
                            .entry(pkg_name.clone())
                            .or_insert_with(|| (source.name.clone(), entry.clone()));
                        matched = true;
                    }
                }
                if !matched {
                    // Fallback: use source name as package name
                    combined_index
                        .entry(source.name.clone())
                        .or_insert_with(|| (source.name.clone(), entry));
                }
            }
            // else: neither repos nor git — skip (config validation should catch this)
        }

        Ok(combined_index)
    }

    /// Classify all dependencies into the full 6-category model.
    ///
    /// This is the single source of truth for dependency classification.
    /// Workspace member names are checked first (Category 1), then the
    /// external resolver handles categories 2–6.
    pub fn classify_all(
        &self,
        deps: &[String],
        workspace_members: &std::collections::HashSet<&str>,
        source_only: bool,
        force_refresh: bool,
    ) -> Result<ResolutionPlan, ResolverError> {
        let cache = DistroCache::load(&self.global.cache, &self.distro, force_refresh)?;
        let repos_index = self.load_repos_index(&cache)?;
        let mut plan = ResolutionPlan::default();

        for dep in deps {
            if workspace_members.contains(dep.as_str()) {
                plan.resolved.push(ResolvedDep {
                    name: dep.clone(),
                    method: ResolutionMethod::WorkspaceMember,
                });
            } else {
                match self.resolve_one(dep, &cache, &repos_index, source_only) {
                    Some(method) => plan.resolved.push(ResolvedDep {
                        name: dep.clone(),
                        method,
                    }),
                    None => plan.unresolved.push(dep.clone()),
                }
            }
        }

        Ok(plan)
    }

    /// Build a resolution plan without executing anything.
    pub fn plan(
        &self,
        deps: &[String],
        source_only: bool,
        force_refresh: bool,
    ) -> Result<ResolutionPlan, ResolverError> {
        let cache = DistroCache::load(&self.global.cache, &self.distro, force_refresh)?;
        let repos_index = self.load_repos_index(&cache)?;
        let mut plan = ResolutionPlan::default();

        for dep in deps {
            match self.resolve_one(dep, &cache, &repos_index, source_only) {
                Some(method) => plan.resolved.push(ResolvedDep {
                    name: dep.clone(),
                    method,
                }),
                None => plan.unresolved.push(dep.clone()),
            }
        }

        Ok(plan)
    }

    /// Resolve and install all deps. Returns error if any dep is unresolvable.
    ///
    /// `yes`: pass `--default-yes` to rosdep (non-interactive). When false,
    /// rosdep prompts interactively for confirmation.
    ///
    /// `shallow`: use `--depth=1` for initial bare clones (CI optimisation).
    pub fn resolve(
        &self,
        deps: &[String],
        dry_run: bool,
        source_only: bool,
        force_refresh: bool,
        yes: bool,
        shallow: bool,
    ) -> Result<ResolutionPlan, ResolverError> {
        let plan = self.plan(deps, source_only, force_refresh)?;

        if !plan.unresolved.is_empty() {
            return Err(ResolverError::Unresolved(plan.unresolved.clone()));
        }

        if dry_run {
            return Ok(plan);
        }

        self.execute(&plan, yes, shallow)?;
        Ok(plan)
    }

    fn resolve_one(
        &self,
        dep: &str,
        cache: &DistroCache,
        repos_index: &HashMap<String, (String, repos_file::RepoEntry)>,
        source_only: bool,
    ) -> Option<ResolutionMethod> {
        // 1. User override from rosup.toml — explicit user intent wins over
        //    auto-detection. This covers cases where the user wants a patched
        //    fork even though the package is installed in ament.
        if let Some(ov) = self.config.overrides.get(dep) {
            debug!("{dep}: using rosup.toml override");
            return Some(override_method(ov));
        }

        // 2. .repos / git sources (from [[resolve.sources]]).
        // Higher priority than ament — like Cargo's [patch], the user
        // explicitly registered these sources to use specific versions,
        // even if the package is already installed in an overlay.
        if let Some((source_name, entry)) = repos_index.get(dep) {
            debug!(
                "{dep}: found in .repos source '{source_name}' ({} @ {})",
                entry.url, entry.version
            );
            return Some(ResolutionMethod::ReposSource {
                source_name: source_name.to_string(),
                url: entry.url.clone(),
                version: entry.version.clone(),
            });
        }

        // 3. Ament environment (already installed in a sourced overlay).
        if ament::is_installed(dep) {
            debug!("{dep}: found in ament environment");
            return Some(ResolutionMethod::Ament);
        }

        let prefer_source =
            source_only || self.config.source_preference == SourcePreference::Source;
        let allow_binary =
            !source_only && self.config.source_preference != SourcePreference::Source;
        let allow_source = self.config.source_preference != SourcePreference::Binary;

        // 4. rosdep binary (unless source-only).
        if allow_binary {
            match rosdep::resolve(dep, &self.distro) {
                Ok(r) => {
                    debug!("{dep}: rosdep → {:?}", r.packages);
                    return Some(ResolutionMethod::Binary {
                        installer: r.installer,
                        packages: r.packages,
                    });
                }
                Err(RosdepError::Other(_)) => {
                    debug!("{dep}: not in rosdep");
                }
                Err(e) => {
                    warn!("{dep}: rosdep error: {e}");
                }
            }
        }

        // 5. rosdistro source pull.
        if (allow_source || prefer_source)
            && let Some(src) = cache.lookup(dep)
        {
            debug!("{dep}: found in rosdistro ({} @ {})", src.url, src.branch);
            return Some(ResolutionMethod::Source {
                repo: src.repo.clone(),
                url: src.url.clone(),
                branch: src.branch.clone(),
            });
        }

        None
    }

    fn execute(
        &self,
        plan: &ResolutionPlan,
        yes: bool,
        shallow: bool,
    ) -> Result<(), ResolverError> {
        let mut binary_keys: Vec<&str> = Vec::new();

        // Collect source repos, grouping by repo name.
        // Multiple packages from the same repo (e.g. sensor_msgs + std_msgs from
        // common_interfaces) produce a single bare clone and a single worktree.
        let mut source_repos: HashMap<String, SourceRepo> = HashMap::new();

        for dep in &plan.resolved {
            match &dep.method {
                ResolutionMethod::WorkspaceMember => {
                    debug!("{}: workspace member, skipping", dep.name);
                }
                ResolutionMethod::Ament => {
                    debug!("{}: already installed, skipping", dep.name);
                }
                ResolutionMethod::Binary { .. } => {
                    binary_keys.push(&dep.name);
                }
                ResolutionMethod::ReposSource {
                    source_name: _,
                    url,
                    version,
                } => {
                    // .repos source deps are cloned like rosdistro source deps.
                    // Use the dep name as the repo key.
                    source_repos
                        .entry(dep.name.clone())
                        .or_insert_with(|| SourceRepo {
                            url: url.clone(),
                            branch: version.clone(),
                            rev: None,
                            sparse_paths: Vec::new(),
                        });
                }
                ResolutionMethod::Source { repo, url, branch } => {
                    let entry = source_repos
                        .entry(repo.clone())
                        .or_insert_with(|| SourceRepo {
                            url: url.clone(),
                            branch: branch.clone(),
                            rev: None,
                            sparse_paths: Vec::new(),
                        });
                    // Multi-package repo: package lives in a subdirectory
                    // named after itself (standard ROS convention).
                    if *repo != dep.name {
                        entry.sparse_paths.push(dep.name.clone());
                    }
                }
                ResolutionMethod::Override { url, branch, rev } => {
                    // Override deps each get their own repo keyed by dep name.
                    source_repos
                        .entry(dep.name.clone())
                        .or_insert_with(|| SourceRepo {
                            url: url.clone(),
                            branch: branch.clone().unwrap_or_else(|| "HEAD".to_owned()),
                            rev: rev.clone(),
                            sparse_paths: Vec::new(),
                        });
                }
            }
        }

        if !source_repos.is_empty() {
            // Create project dirs only when we actually need them.
            self.project.ensure()?;

            for (repo_name, repo) in &source_repos {
                info!("pulling {repo_name} from {} @ {}", repo.url, repo.branch);
                source_pull(
                    &self.global,
                    &self.project,
                    repo_name,
                    &repo.url,
                    &repo.branch,
                    repo.rev.as_deref(),
                    &repo.sparse_paths,
                    shallow,
                )?;
            }
        }

        // Install all binary deps: resolve each key to system packages,
        // then call the installer directly for better control and error messages.
        if !binary_keys.is_empty() {
            let resolved = rosdep::resolve_all(&binary_keys, &self.distro)?;

            for (key, msg) in &resolved.failures {
                warn!("rosdep resolve failed for {key}: {msg}");
            }

            for (installer, packages) in &resolved.by_installer {
                for pkg in packages {
                    info!(
                        "{} → {} ({})",
                        pkg.rosdep_key, pkg.system_package, installer
                    );
                }
            }

            rosdep::install_direct(&resolved, false, yes)?;
        }

        Ok(())
    }
}

// ── source pull ───────────────────────────────────────────────────────────────

/// Grouping of resolved source deps for a single repository.
struct SourceRepo {
    url: String,
    branch: String,
    rev: Option<String>,
    /// Package subdirectories to sparse-checkout. Empty means full checkout
    /// (single-package repo or all packages needed).
    sparse_paths: Vec<String>,
}

/// Clone or update a source repository using a two-level bare clone + worktree model.
///
/// # Global level (`~/.rosup/src/<repo>.git`)
/// A bare git clone serves as the shared object store. Git objects are
/// downloaded once and reused across all projects that need the same repo.
///
/// # Project level (`<project>/.rosup/src/<repo>/`)
/// A git worktree checked out from the bare clone. Each project gets an
/// isolated working directory at the pinned branch or rev. Two projects can
/// use different branches of the same repo without conflicts.
///
/// # Sparse checkout
/// When `sparse_paths` is non-empty (multi-package repos), only the listed
/// subdirectories are checked out. This significantly reduces disk usage for
/// large repos like `common_interfaces`.
///
/// # Shallow clone
/// When `shallow` is true, the initial bare clone uses `--depth=1`. Faster
/// for CI but cannot switch branches without re-fetching.
#[allow(clippy::too_many_arguments)]
pub fn source_pull(
    global: &GlobalStore,
    project: &ProjectStore,
    repo_name: &str,
    url: &str,
    branch: &str,
    rev: Option<&str>,
    sparse_paths: &[String],
    shallow: bool,
) -> Result<(), ResolverError> {
    let bare = global.bare_clone(repo_name);
    let worktree = project.worktree(repo_name);
    let target = rev.unwrap_or(branch);
    let use_sparse = !sparse_paths.is_empty();

    // ── Step 1: ensure bare clone exists and is up to date ──────────────────
    if bare.exists() {
        info!("fetching {repo_name}");
        git(&bare, &["fetch", "--quiet", "origin"])?;
    } else {
        info!("cloning {repo_name} from {url}");
        if shallow {
            sparse::shallow_bare_clone(url, &bare)?;
        } else {
            // Always use --filter=blob:none so git fetches blobs on demand.
            // Saves bandwidth and disk on initial clone with no functional
            // downside — objects are transparently fetched when checked out.
            sparse::partial_bare_clone(url, &bare)?;
        }
    }

    // ── Step 2: ensure per-project worktree is at the right commit ──────────
    if worktree.exists() {
        // Worktree already exists — reset to the target ref.
        git(&worktree, &["reset", "--quiet", "--hard", target])?;

        // If sparse checkout is active, add any new paths.
        if use_sparse && sparse::is_sparse(&worktree) {
            let path_refs: Vec<&str> = sparse_paths.iter().map(String::as_str).collect();
            sparse::sparse_add_paths(&worktree, &path_refs)?;
        }
    } else if use_sparse {
        // Sparse checkout: add worktree with --no-checkout, configure sparse,
        // then materialise only the needed directories.
        sparse::sparse_worktree_add(&bare, &worktree, target)?;
        let path_refs: Vec<&str> = sparse_paths.iter().map(String::as_str).collect();
        sparse::sparse_init_and_checkout(&worktree, &path_refs)?;
    } else {
        // Full checkout: add a new detached worktree from the bare clone.
        let worktree_str = worktree.display().to_string();
        git(
            &bare,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                &worktree_str,
                target,
            ],
        )?;
    }

    Ok(())
}

fn git(cwd: &Path, args: &[&str]) -> Result<(), ResolverError> {
    let status = std::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .status()
        .map_err(ResolverError::Io)?;

    if !status.success() {
        return Err(ResolverError::Git {
            cmd: format!("git {}", args.join(" ")),
            detail: format!("exited with {}", status),
        });
    }
    Ok(())
}

fn override_method(ov: &DepOverride) -> ResolutionMethod {
    ResolutionMethod::Override {
        url: ov.git.clone(),
        branch: ov.branch.clone(),
        rev: ov.rev.clone(),
    }
}
