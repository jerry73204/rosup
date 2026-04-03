//! Native build engine — replacement for colcon delegation.
//!
//! This module implements the build pipeline directly in Rust:
//! topological ordering, package selection, build task dispatch,
//! and parallel execution.

pub mod ament_index;
pub mod environment;
pub mod post_install;
pub mod selection;
pub mod task;
pub mod topo;
