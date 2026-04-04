//! Dependency-aware parallel build executor.
//!
//! Schedules build tasks respecting the dependency graph, with configurable
//! parallelism and error handling. Uses std threads (no async runtime needed).
//!
//! ```text
//! [1/10] Building autoware_cmake...
//! [2/10] Building autoware_lint_common...
//! [3/10] Building autoware_utils_math... (2 jobs active)
//! ...
//! Summary: 10 packages finished [4.2s] (3 cached)
//! ```

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::topo::{BuildGraph, PackageNode};

/// What to do when a build fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnError {
    /// Stop immediately: no new jobs are spawned, wait for running jobs,
    /// then return.
    Stop,
    /// Skip the failed package's downstream dependents but continue
    /// building independent packages.
    SkipDownstream,
}

/// Result of building a single package.
#[derive(Debug)]
pub enum BuildResult {
    Success {
        name: String,
        duration: Duration,
    },
    Cached {
        name: String,
    },
    Failed {
        name: String,
        error: String,
        duration: Duration,
    },
    Skipped {
        name: String,
        reason: String,
    },
}

impl BuildResult {
    pub fn name(&self) -> &str {
        match self {
            Self::Success { name, .. }
            | Self::Cached { name }
            | Self::Failed { name, .. }
            | Self::Skipped { name, .. } => name,
        }
    }

    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Failed { .. })
    }
}

/// Summary of a build execution.
#[derive(Debug)]
pub struct BuildSummary {
    pub results: Vec<BuildResult>,
    pub total_duration: Duration,
}

impl BuildSummary {
    pub fn succeeded(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r, BuildResult::Success { .. }))
            .count()
    }

    pub fn cached(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r, BuildResult::Cached { .. }))
            .count()
    }

    pub fn failed(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r, BuildResult::Failed { .. }))
            .count()
    }

    pub fn skipped(&self) -> usize {
        self.results
            .iter()
            .filter(|r| matches!(r, BuildResult::Skipped { .. }))
            .count()
    }

    pub fn has_failures(&self) -> bool {
        self.results.iter().any(BuildResult::is_failure)
    }
}

/// Return value from the user's build function.
pub enum BuildOutcome {
    /// Package was built successfully.
    Built,
    /// Package was skipped (fingerprint unchanged).
    Cached,
}

/// Message sent from worker threads back to the executor.
struct WorkerResult {
    name: String,
    outcome: Result<BuildOutcome, String>,
    duration: Duration,
}

/// Parallel build executor.
///
/// Dispatches build tasks respecting the dependency graph. Uses a thread
/// pool bounded by `max_workers`.
pub struct Executor {
    pub max_workers: usize,
    pub on_error: OnError,
}

impl Executor {
    pub fn new(max_workers: usize, on_error: OnError) -> Self {
        Self {
            max_workers,
            on_error,
        }
    }

    /// Execute builds for all packages in the graph.
    ///
    /// `build_fn` is called for each package. It should return
    /// `Ok(BuildOutcome::Built)` on success, `Ok(BuildOutcome::Cached)` if
    /// the package was skipped (fingerprint match), or `Err(message)` on
    /// failure.
    ///
    /// `progress_fn` is called for status updates (package starting, done,
    /// failed). Called from the main thread.
    pub fn execute<F, P>(
        &self,
        graph: &mut BuildGraph,
        build_fn: F,
        mut progress_fn: P,
    ) -> BuildSummary
    where
        F: Fn(&PackageNode) -> Result<BuildOutcome, String> + Send + Sync,
        P: FnMut(&ProgressEvent),
    {
        let start = Instant::now();
        let total_packages = graph.remaining();
        let mut results: Vec<BuildResult> = Vec::new();
        let mut completed = 0usize;
        let mut active_jobs = 0usize;
        let mut stop_spawning = false;

        let (tx, rx) = mpsc::channel::<WorkerResult>();
        let build_fn = &build_fn;

        // We use std::thread::scope so threads can borrow build_fn
        std::thread::scope(|scope| {
            loop {
                // Spawn ready packages up to max_workers
                if !stop_spawning {
                    // Collect ready packages (names + cloned nodes) before
                    // mutating the graph, to satisfy the borrow checker.
                    let ready: Vec<PackageNode> = graph
                        .ready()
                        .iter()
                        .take(self.max_workers - active_jobs)
                        .map(|p| (*p).clone())
                        .collect();

                    for node in ready {
                        let name = node.name.clone();
                        let tx = tx.clone();

                        // Mark dispatched so ready() won't return it again
                        graph.mark_dispatched(&name);

                        active_jobs += 1;
                        completed += 1;

                        progress_fn(&ProgressEvent::Starting {
                            name: &name,
                            index: completed,
                            total: total_packages,
                            active: active_jobs,
                        });

                        scope.spawn(move || {
                            let t = Instant::now();
                            let outcome = build_fn(&node);
                            let _ = tx.send(WorkerResult {
                                name: node.name.clone(),
                                outcome,
                                duration: t.elapsed(),
                            });
                        });
                    }
                }

                // If no active jobs and nothing ready, we're done
                if active_jobs == 0 && graph.ready().is_empty() {
                    break;
                }

                // If no active jobs but there are ready packages, loop back
                // to spawn them
                if active_jobs == 0 {
                    continue;
                }

                // Wait for any completion
                let Ok(result) = rx.recv() else {
                    break;
                };
                active_jobs -= 1;

                match result.outcome {
                    Ok(BuildOutcome::Built) => {
                        graph.mark_done(&result.name);
                        progress_fn(&ProgressEvent::Finished {
                            name: &result.name,
                            cached: false,
                            duration: result.duration,
                        });
                        results.push(BuildResult::Success {
                            name: result.name,
                            duration: result.duration,
                        });
                    }
                    Ok(BuildOutcome::Cached) => {
                        graph.mark_done(&result.name);
                        progress_fn(&ProgressEvent::Finished {
                            name: &result.name,
                            cached: true,
                            duration: result.duration,
                        });
                        results.push(BuildResult::Cached { name: result.name });
                    }
                    Err(err) => {
                        progress_fn(&ProgressEvent::Failed {
                            name: &result.name,
                            error: &err,
                        });

                        results.push(BuildResult::Failed {
                            name: result.name.clone(),
                            error: err,
                            duration: result.duration,
                        });

                        match self.on_error {
                            OnError::Stop => {
                                stop_spawning = true;
                                // Drain remaining active jobs
                            }
                            OnError::SkipDownstream => {
                                let skipped = graph.mark_failed(&result.name);
                                for skipped_name in skipped {
                                    results.push(BuildResult::Skipped {
                                        name: skipped_name.clone(),
                                        reason: format!("dependency {} failed", result.name),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        });

        BuildSummary {
            results,
            total_duration: start.elapsed(),
        }
    }
}

/// Progress events emitted during execution.
#[derive(Debug)]
pub enum ProgressEvent<'a> {
    Starting {
        name: &'a str,
        index: usize,
        total: usize,
        active: usize,
    },
    Finished {
        name: &'a str,
        cached: bool,
        duration: Duration,
    },
    Failed {
        name: &'a str,
        error: &'a str,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn node(name: &str, deps: &[&str]) -> PackageNode {
        PackageNode {
            name: name.to_owned(),
            path: PathBuf::from(format!("src/{name}")),
            build_type: Some("ament_cmake".to_owned()),
            build_deps: deps.iter().map(|d| d.to_string()).collect(),
            all_deps: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    fn no_progress(_: &ProgressEvent) {}

    // ── Basic execution ─────────────────────────────────────────────────

    #[test]
    fn executes_all_packages() {
        let nodes = vec![node("a", &["b"]), node("b", &["c"]), node("c", &[])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(4, OnError::Stop);

        let summary = executor.execute(&mut graph, |_pkg| Ok(BuildOutcome::Built), no_progress);

        assert_eq!(summary.succeeded(), 3);
        assert_eq!(summary.failed(), 0);
        assert!(!summary.has_failures());
    }

    #[test]
    fn respects_dependency_order() {
        let nodes = vec![node("a", &["b"]), node("b", &["c"]), node("c", &[])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(1, OnError::Stop); // sequential

        let order = Mutex::new(Vec::new());
        let summary = executor.execute(
            &mut graph,
            |pkg| {
                order.lock().unwrap().push(pkg.name.clone());
                Ok(BuildOutcome::Built)
            },
            no_progress,
        );

        let order = order.into_inner().unwrap();
        assert_eq!(order, vec!["c", "b", "a"]);
        assert_eq!(summary.succeeded(), 3);
    }

    #[test]
    fn handles_independent_packages() {
        let nodes = vec![node("a", &[]), node("b", &[]), node("c", &[])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(4, OnError::Stop);

        let summary = executor.execute(&mut graph, |_pkg| Ok(BuildOutcome::Built), no_progress);

        assert_eq!(summary.succeeded(), 3);
    }

    // ── Cached packages ─────────────────────────────────────────────────

    #[test]
    fn tracks_cached_packages() {
        let nodes = vec![node("a", &[]), node("b", &[])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(4, OnError::Stop);

        let summary = executor.execute(
            &mut graph,
            |pkg| {
                if pkg.name == "a" {
                    Ok(BuildOutcome::Cached)
                } else {
                    Ok(BuildOutcome::Built)
                }
            },
            no_progress,
        );

        assert_eq!(summary.cached(), 1);
        assert_eq!(summary.succeeded(), 1);
    }

    // ── Error handling: Stop ────────────────────────────────────────────

    #[test]
    fn stop_on_error_prevents_new_jobs() {
        // c (no deps) → b → a
        // c fails → b and a should not be built
        let nodes = vec![node("a", &["b"]), node("b", &["c"]), node("c", &[])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(1, OnError::Stop);

        let built = Mutex::new(Vec::new());
        let summary = executor.execute(
            &mut graph,
            |pkg| {
                if pkg.name == "c" {
                    Err("compile error".into())
                } else {
                    built.lock().unwrap().push(pkg.name.clone());
                    Ok(BuildOutcome::Built)
                }
            },
            no_progress,
        );

        assert_eq!(summary.failed(), 1);
        assert!(built.into_inner().unwrap().is_empty());
        assert!(summary.has_failures());
    }

    // ── Error handling: SkipDownstream ───────────────────────────────────

    #[test]
    fn skip_downstream_continues_independent() {
        //   a (no deps) — fails
        //   b (no deps) — should still build
        //   c → a       — should be skipped
        let nodes = vec![node("a", &[]), node("b", &[]), node("c", &["a"])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(1, OnError::SkipDownstream);

        let summary = executor.execute(
            &mut graph,
            |pkg| {
                if pkg.name == "a" {
                    Err("failed".into())
                } else {
                    Ok(BuildOutcome::Built)
                }
            },
            no_progress,
        );

        assert_eq!(summary.failed(), 1);
        assert_eq!(summary.succeeded(), 1); // b
        assert_eq!(summary.skipped(), 1); // c
        // Check the skipped reason
        let skipped = summary
            .results
            .iter()
            .find(|r| matches!(r, BuildResult::Skipped { .. }))
            .unwrap();
        assert_eq!(skipped.name(), "c");
    }

    #[test]
    fn skip_downstream_cascades() {
        // a fails → b skipped → c skipped
        let nodes = vec![node("a", &[]), node("b", &["a"]), node("c", &["b"])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(1, OnError::SkipDownstream);

        let summary = executor.execute(
            &mut graph,
            |pkg| {
                if pkg.name == "a" {
                    Err("broken".into())
                } else {
                    Ok(BuildOutcome::Built)
                }
            },
            no_progress,
        );

        assert_eq!(summary.failed(), 1);
        assert_eq!(summary.skipped(), 2); // b and c
    }

    // ── Parallelism ─────────────────────────────────────────────────────

    #[test]
    fn parallel_execution_uses_multiple_workers() {
        let nodes: Vec<PackageNode> = (0..8).map(|i| node(&format!("pkg_{i}"), &[])).collect();
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(4, OnError::Stop);

        let peak = AtomicUsize::new(0);
        let active = AtomicUsize::new(0);

        let summary = executor.execute(
            &mut graph,
            |_pkg| {
                let cur = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(cur, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(20));
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(BuildOutcome::Built)
            },
            no_progress,
        );

        assert_eq!(summary.succeeded(), 8);
        // With 4 workers and 8 independent packages, peak should be >= 2
        assert!(
            peak.load(Ordering::SeqCst) >= 2,
            "should use multiple workers"
        );
    }

    // ── Progress reporting ──────────────────────────────────────────────

    #[test]
    fn progress_events_emitted() {
        let nodes = vec![node("a", &[]), node("b", &[])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(1, OnError::Stop);

        let events = Mutex::new(Vec::new());
        executor.execute(
            &mut graph,
            |_pkg| Ok(BuildOutcome::Built),
            |event| {
                let desc = match event {
                    ProgressEvent::Starting { name, .. } => format!("start:{name}"),
                    ProgressEvent::Finished { name, .. } => format!("done:{name}"),
                    ProgressEvent::Failed { name, .. } => format!("fail:{name}"),
                };
                events.lock().unwrap().push(desc);
            },
        );

        let events = events.into_inner().unwrap();
        // Should have start+done for both packages
        assert_eq!(events.len(), 4);
        assert!(events.iter().any(|e| e == "start:a"));
        assert!(events.iter().any(|e| e == "done:a"));
        assert!(events.iter().any(|e| e == "start:b"));
        assert!(events.iter().any(|e| e == "done:b"));
    }

    // ── Diamond graph ───────────────────────────────────────────────────

    #[test]
    fn diamond_graph_executes_correctly() {
        let nodes = vec![
            node("a", &["b", "c"]),
            node("b", &["d"]),
            node("c", &["d"]),
            node("d", &[]),
        ];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(2, OnError::Stop);

        let built = Mutex::new(Vec::new());
        let summary = executor.execute(
            &mut graph,
            |pkg| {
                built.lock().unwrap().push(pkg.name.clone());
                Ok(BuildOutcome::Built)
            },
            no_progress,
        );

        assert_eq!(summary.succeeded(), 4);
        let built = built.into_inner().unwrap();
        // d must come before b and c; b and c must come before a
        let pos = |name: &str| built.iter().position(|n| n == name).unwrap();
        assert!(pos("d") < pos("b"));
        assert!(pos("d") < pos("c"));
        assert!(pos("b") < pos("a"));
        assert!(pos("c") < pos("a"));
    }

    // ── Empty graph ─────────────────────────────────────────────────────

    #[test]
    fn empty_graph() {
        let mut graph = BuildGraph::new(vec![]).unwrap();
        let executor = Executor::new(4, OnError::Stop);

        let summary = executor.execute(&mut graph, |_| Ok(BuildOutcome::Built), no_progress);

        assert_eq!(summary.results.len(), 0);
        assert!(!summary.has_failures());
    }

    // ── BuildSummary ────────────────────────────────────────────────────

    #[test]
    fn summary_counts() {
        let nodes = vec![
            node("ok", &[]),
            node("cached", &[]),
            node("bad", &[]),
            node("child", &["bad"]),
        ];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let executor = Executor::new(1, OnError::SkipDownstream);

        let summary = executor.execute(
            &mut graph,
            |pkg| match pkg.name.as_str() {
                "cached" => Ok(BuildOutcome::Cached),
                "bad" => Err("oops".into()),
                _ => Ok(BuildOutcome::Built),
            },
            no_progress,
        );

        assert_eq!(summary.succeeded(), 1); // ok
        assert_eq!(summary.cached(), 1); // cached
        assert_eq!(summary.failed(), 1); // bad
        assert_eq!(summary.skipped(), 1); // child
    }
}
