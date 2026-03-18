pub mod ament;
pub mod rosdep;
pub mod rosdistro;
pub mod store;

use std::path::Path;

use thiserror::Error;
use tracing::{debug, info, warn};

use crate::config::{DepOverride, ResolveConfig, SourcePreference};
use rosdep::RosdepError;
use rosdistro::DistroCache;
use store::RoxDir;

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
    /// Cloned from a source repository.
    Source { url: String, branch: String },
    /// Cloned using a user-supplied override from `rox.toml`.
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
            ResolutionMethod::Source { url, branch } => {
                write!(f, "source ({url} @ {branch})")
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
    rox_dir: RoxDir,
}

impl Resolver {
    pub fn new(config: ResolveConfig) -> Result<Self, ResolverError> {
        let distro = config
            .ros_distro
            .clone()
            .or_else(|| std::env::var("ROS_DISTRO").ok())
            .ok_or(ResolverError::NoDistro)?;

        let rox_dir = RoxDir::open()?;
        rox_dir.ensure()?;

        Ok(Self {
            distro,
            config,
            rox_dir,
        })
    }

    /// Build a resolution plan without executing anything.
    pub fn plan(
        &self,
        deps: &[String],
        source_only: bool,
        force_refresh: bool,
    ) -> Result<ResolutionPlan, ResolverError> {
        let cache = DistroCache::load(&self.rox_dir.cache, &self.distro, force_refresh)?;
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
                url: src.url.clone(),
                branch: src.branch.clone(),
            });
        }

        None
    }

    fn execute(&self, plan: &ResolutionPlan) -> Result<(), ResolverError> {
        let mut binary_keys: Vec<&str> = Vec::new();

        for dep in &plan.resolved {
            match &dep.method {
                ResolutionMethod::Ament => {
                    debug!("{}: already installed, skipping", dep.name);
                }
                ResolutionMethod::Binary { packages, .. } => {
                    info!("{}: will install via rosdep", dep.name);
                    binary_keys.push(&dep.name);
                    let _ = packages;
                }
                ResolutionMethod::Source { url, branch } => {
                    info!("{}: cloning from {url} @ {branch}", dep.name);
                    source_pull(&self.rox_dir.src, &dep.name, url, branch, None)?;
                }
                ResolutionMethod::Override { url, branch, rev } => {
                    info!("{}: cloning override from {url}", dep.name);
                    source_pull(
                        &self.rox_dir.src,
                        &dep.name,
                        url,
                        branch.as_deref().unwrap_or("HEAD"),
                        rev.as_deref(),
                    )?;
                }
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

/// Clone or update a source package into `src_dir/<name>`.
pub fn source_pull(
    src_dir: &Path,
    name: &str,
    url: &str,
    branch: &str,
    rev: Option<&str>,
) -> Result<(), ResolverError> {
    let dest = src_dir.join(name);

    if dest.join(".git").exists() {
        // Already cloned — fetch and reset.
        info!("updating {name}");
        git(&dest, &["fetch", "--quiet", "origin"])?;
        let target = rev.unwrap_or(branch);
        git(&dest, &["checkout", "--quiet", target])?;
        if rev.is_none() {
            git(
                &dest,
                &["reset", "--quiet", "--hard", &format!("origin/{branch}")],
            )?;
        }
    } else {
        // Fresh clone.
        info!("cloning {name} from {url}");
        let mut args = vec!["clone", "--quiet", "--branch", branch, url];
        let dest_str = dest.display().to_string();
        args.push(&dest_str);
        git(src_dir, &args)?;

        if let Some(rev) = rev {
            git(&dest, &["checkout", "--quiet", rev])?;
        }
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
