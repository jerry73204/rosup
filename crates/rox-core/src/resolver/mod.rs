pub mod ament;
pub mod rosdep;
pub mod rosdistro;
pub mod store;

use std::collections::HashMap;
use std::path::Path;

use thiserror::Error;
use tracing::{debug, info, warn};

use crate::config::{DepOverride, ResolveConfig, SourcePreference};
use rosdep::RosdepError;
use rosdistro::DistroCache;
use store::{GlobalStore, ProjectStore};

// ── public types ──────────────────────────────────────────────────────────────

/// How a dependency was (or will be) resolved.
#[derive(Debug, Clone)]
pub enum ResolutionMethod {
    /// Already present in the ament environment.
    Ament,
    /// Installed as a binary package via rosdep.
    Binary {
        installer: String,
        packages: Vec<String>,
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
    /// Cloned using a user-supplied override from `[resolve.overrides]` in `rox.toml`.
    Override {
        url: String,
        branch: Option<String>,
        rev: Option<String>,
    },
}

impl std::fmt::Display for ResolutionMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionMethod::Ament => write!(f, "ament (already installed)"),
            ResolutionMethod::Binary { packages, .. } => {
                write!(f, "binary ({})", packages.join(", "))
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
    #[error("git error running `{cmd}`: {detail}")]
    Git { cmd: String, detail: String },
    #[error("unresolved dependencies: {0:?}")]
    Unresolved(Vec<String>),
    #[error("no ROS distro configured — set ROS_DISTRO or add ros-distro to [resolve] in rox.toml")]
    NoDistro,
}

// ── Resolver ──────────────────────────────────────────────────────────────────

pub struct Resolver {
    distro: String,
    config: ResolveConfig,
    global: GlobalStore,
    project: ProjectStore,
}

impl Resolver {
    pub fn new(config: ResolveConfig, project_root: &Path) -> Result<Self, ResolverError> {
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
        })
    }

    /// Build a resolution plan without executing anything.
    pub fn plan(
        &self,
        deps: &[String],
        source_only: bool,
        force_refresh: bool,
    ) -> Result<ResolutionPlan, ResolverError> {
        let cache = DistroCache::load(&self.global.cache, &self.distro, force_refresh)?;
        let mut plan = ResolutionPlan::default();

        for dep in deps {
            match self.resolve_one(dep, &cache, source_only) {
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
    pub fn resolve(
        &self,
        deps: &[String],
        dry_run: bool,
        source_only: bool,
        force_refresh: bool,
    ) -> Result<ResolutionPlan, ResolverError> {
        let plan = self.plan(deps, source_only, force_refresh)?;

        if !plan.unresolved.is_empty() {
            return Err(ResolverError::Unresolved(plan.unresolved.clone()));
        }

        if dry_run {
            return Ok(plan);
        }

        self.execute(&plan)?;
        Ok(plan)
    }

    fn resolve_one(
        &self,
        dep: &str,
        cache: &DistroCache,
        source_only: bool,
    ) -> Option<ResolutionMethod> {
        // 1. Ament environment (always check first).
        if ament::is_installed(dep) {
            debug!("{dep}: found in ament environment");
            return Some(ResolutionMethod::Ament);
        }

        // 2. User override from rox.toml (takes priority over auto-resolution).
        if let Some(ov) = self.config.overrides.get(dep) {
            debug!("{dep}: using rox.toml override");
            return Some(override_method(ov));
        }

        let prefer_source =
            source_only || self.config.source_preference == SourcePreference::Source;
        let allow_binary =
            !source_only && self.config.source_preference != SourcePreference::Source;
        let allow_source = self.config.source_preference != SourcePreference::Binary;

        // 3. rosdep binary (unless source-only).
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

        // 4. rosdistro source pull.
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

    fn execute(&self, plan: &ResolutionPlan) -> Result<(), ResolverError> {
        let mut binary_keys: Vec<&str> = Vec::new();

        // Collect source repos, grouping by repo name.
        // Multiple packages from the same repo (e.g. sensor_msgs + std_msgs from
        // common_interfaces) produce a single bare clone and a single worktree.
        // Value: (url, branch, rev)
        let mut source_repos: HashMap<String, (String, String, Option<String>)> = HashMap::new();

        for dep in &plan.resolved {
            match &dep.method {
                ResolutionMethod::Ament => {
                    debug!("{}: already installed, skipping", dep.name);
                }
                ResolutionMethod::Binary { .. } => {
                    binary_keys.push(&dep.name);
                }
                ResolutionMethod::Source { repo, url, branch } => {
                    // First dep that references this repo wins for url/branch.
                    source_repos
                        .entry(repo.clone())
                        .or_insert_with(|| (url.clone(), branch.clone(), None));
                }
                ResolutionMethod::Override { url, branch, rev } => {
                    // Override deps each get their own repo keyed by dep name.
                    source_repos.entry(dep.name.clone()).or_insert_with(|| {
                        (
                            url.clone(),
                            branch.clone().unwrap_or_else(|| "HEAD".to_owned()),
                            rev.clone(),
                        )
                    });
                }
            }
        }

        if !source_repos.is_empty() {
            // Create project dirs only when we actually need them.
            self.project.ensure()?;

            for (repo_name, (url, branch, rev)) in &source_repos {
                info!("pulling {repo_name} from {url} @ {branch}");
                source_pull(
                    &self.global,
                    &self.project,
                    repo_name,
                    url,
                    branch,
                    rev.as_deref(),
                )?;
            }
        }

        // Install all binary deps in one rosdep call.
        if !binary_keys.is_empty() {
            rosdep::install(&binary_keys, &self.distro, false)?;
        }

        Ok(())
    }
}

// ── source pull ───────────────────────────────────────────────────────────────

/// Clone or update a source repository using a two-level bare clone + worktree model.
///
/// # Global level (`~/.rox/src/<repo>.git`)
/// A bare git clone serves as the shared object store. Git objects are
/// downloaded once and reused across all projects that need the same repo.
///
/// # Project level (`<project>/.rox/src/<repo>/`)
/// A git worktree checked out from the bare clone. Each project gets an
/// isolated working directory at the pinned branch or rev. Two projects can
/// use different branches of the same repo without conflicts.
pub fn source_pull(
    global: &GlobalStore,
    project: &ProjectStore,
    repo_name: &str,
    url: &str,
    branch: &str,
    rev: Option<&str>,
) -> Result<(), ResolverError> {
    let bare = global.bare_clone(repo_name);
    let worktree = project.worktree(repo_name);
    let target = rev.unwrap_or(branch);

    // ── Step 1: ensure bare clone exists and is up to date ──────────────────
    if bare.exists() {
        info!("fetching {repo_name}");
        git(&bare, &["fetch", "--quiet", "origin"])?;
    } else {
        info!("cloning {repo_name} from {url}");
        let bare_str = bare.display().to_string();
        git(&global.src, &["clone", "--bare", "--quiet", url, &bare_str])?;
    }

    // ── Step 2: ensure per-project worktree is at the right commit ──────────
    if worktree.exists() {
        // Worktree already exists — reset to the target ref.
        git(&worktree, &["reset", "--quiet", "--hard", target])?;
    } else {
        // Add a new detached worktree from the bare clone.
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
