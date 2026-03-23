//! Native build engine — replacement for colcon delegation.
//!
//! This module implements the build pipeline directly in Rust:
//! topological ordering, package selection, build task dispatch,
//! and parallel execution.

pub mod topo;
