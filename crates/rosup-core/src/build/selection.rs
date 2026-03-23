//! Package selection: `--packages-select`, `--packages-skip`,
//! `--packages-up-to`.
//!
//! Operates on a set of `PackageNode`s and returns the filtered subset.
//! Selection happens before the `BuildGraph` is constructed for execution,
//! so the graph only contains packages the user actually wants to build.

use std::collections::HashSet;

use super::topo::PackageNode;

/// Selection criteria for filtering workspace packages.
#[derive(Debug, Default)]
pub struct SelectionFilter {
    /// Only build these packages (by name). Empty = all.
    pub select: Vec<String>,
    /// Skip these packages (by name).
    pub skip: Vec<String>,
    /// Build these packages and all their transitive dependencies.
    /// Implies `select` for the named packages plus their dep closure.
    pub up_to: Vec<String>,
}

impl SelectionFilter {
    /// Returns true if no filters are active (build everything).
    pub fn is_empty(&self) -> bool {
        self.select.is_empty() && self.skip.is_empty() && self.up_to.is_empty()
    }
}

/// Apply selection filters to a set of packages.
///
/// Returns the filtered list preserving the input order.
pub fn apply_selection(packages: &[PackageNode], filter: &SelectionFilter) -> Vec<PackageNode> {
    if filter.is_empty() {
        return packages.to_vec();
    }

    // Build a name → node lookup for dependency traversal.
    let by_name: std::collections::HashMap<&str, &PackageNode> =
        packages.iter().map(|n| (n.name.as_str(), n)).collect();
    let all_names: HashSet<&str> = by_name.keys().copied().collect();

    // Step 1: compute the initial inclusion set.
    let mut included: HashSet<&str> = if !filter.up_to.is_empty() {
        // --packages-up-to: include named packages + transitive deps.
        let mut set = HashSet::new();
        let mut queue: Vec<&str> = filter
            .up_to
            .iter()
            .filter_map(|n| {
                if all_names.contains(n.as_str()) {
                    Some(n.as_str())
                } else {
                    None
                }
            })
            .collect();
        while let Some(name) = queue.pop() {
            if !set.insert(name) {
                continue;
            }
            if let Some(node) = by_name.get(name) {
                for dep in &node.all_deps {
                    if all_names.contains(dep.as_str()) && !set.contains(dep.as_str()) {
                        queue.push(by_name.get(dep.as_str()).map(|n| n.name.as_str()).unwrap());
                    }
                }
            }
        }
        set
    } else if !filter.select.is_empty() {
        // --packages-select: only named packages.
        filter
            .select
            .iter()
            .filter(|n| all_names.contains(n.as_str()))
            .map(|n| n.as_str())
            .collect()
    } else {
        // No positive filter — start with all.
        all_names
    };

    // Step 2: apply skip.
    for name in &filter.skip {
        included.remove(name.as_str());
    }

    // Step 3: filter preserving input order.
    packages
        .iter()
        .filter(|n| included.contains(n.name.as_str()))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn node(name: &str, deps: &[&str]) -> PackageNode {
        PackageNode {
            name: name.to_owned(),
            path: PathBuf::from(format!("src/{name}")),
            build_type: Some("ament_cmake".to_owned()),
            build_deps: deps.iter().map(|d| d.to_string()).collect(),
            all_deps: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    fn names(nodes: &[PackageNode]) -> Vec<&str> {
        nodes.iter().map(|n| n.name.as_str()).collect()
    }

    fn all_packages() -> Vec<PackageNode> {
        vec![
            node("core", &[]),
            node("msgs", &[]),
            node("utils", &["core"]),
            node("perception", &["core", "msgs", "utils"]),
            node("planning", &["core", "msgs", "utils"]),
            node("system", &["perception", "planning"]),
        ]
    }

    #[test]
    fn no_filter_returns_all() {
        let pkgs = all_packages();
        let result = apply_selection(&pkgs, &SelectionFilter::default());
        assert_eq!(names(&result), names(&pkgs));
    }

    #[test]
    fn select_specific_packages() {
        let pkgs = all_packages();
        let filter = SelectionFilter {
            select: vec!["core".into(), "msgs".into()],
            ..Default::default()
        };
        let result = apply_selection(&pkgs, &filter);
        assert_eq!(names(&result), vec!["core", "msgs"]);
    }

    #[test]
    fn skip_specific_packages() {
        let pkgs = all_packages();
        let filter = SelectionFilter {
            skip: vec!["system".into(), "planning".into()],
            ..Default::default()
        };
        let result = apply_selection(&pkgs, &filter);
        assert_eq!(names(&result), vec!["core", "msgs", "utils", "perception"]);
    }

    #[test]
    fn up_to_includes_transitive_deps() {
        let pkgs = all_packages();
        let filter = SelectionFilter {
            up_to: vec!["perception".into()],
            ..Default::default()
        };
        let result = apply_selection(&pkgs, &filter);
        // perception depends on core, msgs, utils
        let result_names = names(&result);
        assert!(result_names.contains(&"perception"));
        assert!(result_names.contains(&"core"));
        assert!(result_names.contains(&"msgs"));
        assert!(result_names.contains(&"utils"));
        // system and planning should NOT be included
        assert!(!result_names.contains(&"system"));
        assert!(!result_names.contains(&"planning"));
    }

    #[test]
    fn up_to_with_skip() {
        let pkgs = all_packages();
        let filter = SelectionFilter {
            up_to: vec!["perception".into()],
            skip: vec!["msgs".into()],
            ..Default::default()
        };
        let result = apply_selection(&pkgs, &filter);
        let result_names = names(&result);
        assert!(result_names.contains(&"perception"));
        assert!(result_names.contains(&"core"));
        assert!(result_names.contains(&"utils"));
        // msgs is skipped even though it's a dep
        assert!(!result_names.contains(&"msgs"));
    }

    #[test]
    fn select_unknown_package_is_ignored() {
        let pkgs = all_packages();
        let filter = SelectionFilter {
            select: vec!["nonexistent".into(), "core".into()],
            ..Default::default()
        };
        let result = apply_selection(&pkgs, &filter);
        assert_eq!(names(&result), vec!["core"]);
    }

    #[test]
    fn up_to_diamond_deps() {
        //   system
        //  /      \
        // perception  planning
        //  \      /
        //   utils → core
        let pkgs = all_packages();
        let filter = SelectionFilter {
            up_to: vec!["system".into()],
            ..Default::default()
        };
        let result = apply_selection(&pkgs, &filter);
        // system depends on everything
        assert_eq!(names(&result).len(), 6);
    }

    #[test]
    fn preserves_input_order() {
        let pkgs = vec![node("z_pkg", &[]), node("a_pkg", &[]), node("m_pkg", &[])];
        let filter = SelectionFilter::default();
        let result = apply_selection(&pkgs, &filter);
        // Order preserved from input, not sorted
        assert_eq!(names(&result), vec!["z_pkg", "a_pkg", "m_pkg"]);
    }
}
