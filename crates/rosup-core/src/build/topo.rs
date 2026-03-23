//! Dependency graph and topological ordering for workspace packages.
//!
//! Implements Kahn's algorithm. The graph produces **sets** of ready
//! packages at each step — packages whose dependencies are all satisfied.
//! This naturally supports both sequential and parallel execution:
//!
//! ```text
//! while !graph.is_empty() {
//!     let ready = graph.ready();   // all independent packages
//!     spawn(ready);                // parallel: spawn all; sequential: pick one
//!     wait_any();
//!     graph.mark_done(completed);
//! }
//! ```

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum TopoError {
    #[error("dependency cycle detected involving: {}", .0.join(", "))]
    Cycle(Vec<String>),
    #[error("unknown dependency `{dep}` required by `{pkg}` (not a workspace member)")]
    UnknownDep { pkg: String, dep: String },
}

/// A package in the build graph.
#[derive(Debug, Clone)]
pub struct PackageNode {
    pub name: String,
    pub path: PathBuf,
    pub build_type: Option<String>,
    /// Build-time dependencies (from `<depend>`, `<build_depend>`,
    /// `<buildtool_depend>`).
    pub build_deps: HashSet<String>,
    /// All dependencies including runtime and test (for full topo order).
    pub all_deps: HashSet<String>,
}

/// A dependency graph over workspace packages.
///
/// Only tracks dependencies between workspace members. External deps
/// (ament, rosdep, etc.) are ignored — they're resolved separately.
#[derive(Debug)]
pub struct BuildGraph {
    nodes: HashMap<String, PackageNode>,
    /// in-degree: how many unsatisfied deps remain for each package.
    in_degree: HashMap<String, usize>,
    /// reverse adjacency: package → set of packages that depend on it.
    dependents: HashMap<String, Vec<String>>,
}

impl BuildGraph {
    /// Build a dependency graph from workspace package nodes.
    ///
    /// Only dependencies that are themselves workspace members are tracked.
    /// External deps are silently ignored (they're handled by the resolver).
    ///
    /// Returns an error if a cycle is detected.
    pub fn new(nodes: Vec<PackageNode>) -> Result<Self, TopoError> {
        let node_map: HashMap<String, PackageNode> =
            nodes.into_iter().map(|n| (n.name.clone(), n)).collect();

        let member_names: HashSet<&str> = node_map.keys().map(String::as_str).collect();

        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for (name, node) in &node_map {
            let mut count = 0usize;
            for dep in &node.all_deps {
                if member_names.contains(dep.as_str()) {
                    count += 1;
                    dependents
                        .entry(dep.clone())
                        .or_default()
                        .push(name.clone());
                }
            }
            in_degree.insert(name.clone(), count);
        }

        let graph = Self {
            nodes: node_map,
            in_degree,
            dependents,
        };

        // Validate: detect cycles by checking if a full drain is possible.
        graph.validate_no_cycles()?;

        Ok(graph)
    }

    /// Returns all packages with no unsatisfied dependencies,
    /// sorted alphabetically for determinism.
    pub fn ready(&self) -> Vec<&PackageNode> {
        let mut ready: Vec<&PackageNode> = self
            .in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .filter_map(|(name, _)| self.nodes.get(name))
            .collect();
        ready.sort_by(|a, b| a.name.cmp(&b.name));
        ready
    }

    /// Mark a package as completed. Decrements in-degree of its dependents.
    pub fn mark_done(&mut self, name: &str) {
        self.in_degree.remove(name);
        if let Some(deps) = self.dependents.remove(name) {
            for dependent in &deps {
                if let Some(deg) = self.in_degree.get_mut(dependent) {
                    *deg = deg.saturating_sub(1);
                }
            }
        }
    }

    /// Returns true if all packages have been processed.
    pub fn is_empty(&self) -> bool {
        self.in_degree.is_empty()
    }

    /// Number of remaining packages.
    pub fn remaining(&self) -> usize {
        self.in_degree.len()
    }

    /// Drain the graph sequentially, returning packages in topological order.
    /// This is equivalent to running the executor with max_workers=1.
    pub fn drain_sequential(&mut self) -> Vec<PackageNode> {
        let mut result = Vec::new();
        while !self.is_empty() {
            let ready_names: Vec<String> = self.ready().iter().map(|n| n.name.clone()).collect();
            assert!(
                !ready_names.is_empty(),
                "bug: no ready packages but graph is not empty"
            );
            for name in ready_names {
                if let Some(node) = self.nodes.remove(&name) {
                    result.push(node);
                }
                self.mark_done(&name);
            }
        }
        result
    }

    /// Validate that the graph has no cycles.
    /// Uses a temporary copy of in-degrees to simulate a full drain.
    fn validate_no_cycles(&self) -> Result<(), TopoError> {
        let mut in_degree = self.in_degree.clone();
        let mut processed = 0usize;
        let total = in_degree.len();

        loop {
            let ready: Vec<String> = in_degree
                .iter()
                .filter(|(_, deg)| **deg == 0)
                .map(|(name, _)| name.clone())
                .collect();

            if ready.is_empty() {
                break;
            }

            for name in &ready {
                in_degree.remove(name);
                if let Some(deps) = self.dependents.get(name) {
                    for dep in deps {
                        if let Some(deg) = in_degree.get_mut(dep) {
                            *deg = deg.saturating_sub(1);
                        }
                    }
                }
            }
            processed += ready.len();
        }

        if processed < total {
            let cycle_members: Vec<String> = in_degree.into_keys().collect();
            return Err(TopoError::Cycle(cycle_members));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, deps: &[&str]) -> PackageNode {
        PackageNode {
            name: name.to_owned(),
            path: PathBuf::from(format!("src/{name}")),
            build_type: Some("ament_cmake".to_owned()),
            build_deps: deps.iter().map(|d| d.to_string()).collect(),
            all_deps: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    #[test]
    fn linear_chain() {
        // A → B → C (A depends on B, B depends on C)
        let nodes = vec![node("a", &["b"]), node("b", &["c"]), node("c", &[])];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let order: Vec<String> = graph
            .drain_sequential()
            .iter()
            .map(|n| n.name.clone())
            .collect();
        assert_eq!(order, vec!["c", "b", "a"]);
    }

    #[test]
    fn diamond() {
        //   A
        //  / \
        // B   C
        //  \ /
        //   D
        let nodes = vec![
            node("a", &["b", "c"]),
            node("b", &["d"]),
            node("c", &["d"]),
            node("d", &[]),
        ];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let order: Vec<String> = graph
            .drain_sequential()
            .iter()
            .map(|n| n.name.clone())
            .collect();
        // D first, then B and C (alphabetical), then A
        assert_eq!(order, vec!["d", "b", "c", "a"]);
    }

    #[test]
    fn parallel_roots() {
        // No deps — all independent
        let nodes = vec![node("z_pkg", &[]), node("a_pkg", &[]), node("m_pkg", &[])];
        let graph = BuildGraph::new(nodes).unwrap();
        // All should be ready at once, sorted alphabetically
        let ready: Vec<&str> = graph.ready().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(ready, vec!["a_pkg", "m_pkg", "z_pkg"]);
    }

    #[test]
    fn wide_graph_all_independent() {
        let nodes: Vec<PackageNode> = (0..10).map(|i| node(&format!("pkg_{i:02}"), &[])).collect();
        let graph = BuildGraph::new(nodes).unwrap();
        assert_eq!(graph.ready().len(), 10);
    }

    #[test]
    fn cycle_detected() {
        let nodes = vec![node("a", &["b"]), node("b", &["c"]), node("c", &["a"])];
        let err = BuildGraph::new(nodes).unwrap_err();
        match err {
            TopoError::Cycle(members) => {
                assert_eq!(members.len(), 3);
                assert!(members.contains(&"a".to_owned()));
                assert!(members.contains(&"b".to_owned()));
                assert!(members.contains(&"c".to_owned()));
            }
            _ => panic!("expected Cycle error"),
        }
    }

    #[test]
    fn external_deps_ignored() {
        // "rclcpp" and "sensor_msgs" are external — not workspace members
        let nodes = vec![
            node("my_pkg", &["rclcpp", "sensor_msgs", "helper_pkg"]),
            node("helper_pkg", &["rclcpp"]),
        ];
        let mut graph = BuildGraph::new(nodes).unwrap();
        let order: Vec<String> = graph
            .drain_sequential()
            .iter()
            .map(|n| n.name.clone())
            .collect();
        // helper_pkg first (no workspace deps), then my_pkg
        assert_eq!(order, vec!["helper_pkg", "my_pkg"]);
    }

    #[test]
    fn mark_done_releases_dependents() {
        let nodes = vec![node("app", &["lib"]), node("lib", &[])];
        let mut graph = BuildGraph::new(nodes).unwrap();

        // Only lib is ready initially
        let ready: Vec<&str> = graph.ready().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(ready, vec!["lib"]);

        // After marking lib done, app becomes ready
        graph.mark_done("lib");
        let ready: Vec<&str> = graph.ready().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(ready, vec!["app"]);
    }

    #[test]
    fn complex_graph() {
        // Simulate a small Autoware-like graph:
        //   core (no deps)
        //   msgs (no deps)
        //   utils → core
        //   perception → core, msgs, utils
        //   planning → core, msgs, utils
        //   system → perception, planning
        let nodes = vec![
            node("system", &["perception", "planning"]),
            node("perception", &["core", "msgs", "utils"]),
            node("planning", &["core", "msgs", "utils"]),
            node("utils", &["core"]),
            node("msgs", &[]),
            node("core", &[]),
        ];
        let mut graph = BuildGraph::new(nodes).unwrap();

        // Step 1: core and msgs are ready (no deps)
        let ready: Vec<&str> = graph.ready().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(ready, vec!["core", "msgs"]);

        graph.mark_done("core");
        graph.mark_done("msgs");

        // Step 2: utils is ready (core done)
        let ready: Vec<&str> = graph.ready().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(ready, vec!["utils"]);

        graph.mark_done("utils");

        // Step 3: perception and planning are ready (core, msgs, utils done)
        let ready: Vec<&str> = graph.ready().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(ready, vec!["perception", "planning"]);

        graph.mark_done("perception");
        graph.mark_done("planning");

        // Step 4: system is ready
        let ready: Vec<&str> = graph.ready().iter().map(|n| n.name.as_str()).collect();
        assert_eq!(ready, vec!["system"]);

        graph.mark_done("system");
        assert!(graph.is_empty());
    }
}
