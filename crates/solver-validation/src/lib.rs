//! Independent exact validation for fully materialized physical witnesses.
//!
//! This crate intentionally does not depend on production search, propagation,
//! SCC summaries, component records, or memoized flows. It reconstructs every
//! physical port and solves the complete steady-state system from scratch.

mod algebra;
mod validator;

pub use algebra::{LinearSolveError, solve_fraction_free};
pub use validator::{ValidationError, solve_topology, validate_solution};

#[cfg(feature = "identity")]
mod collection;
#[cfg(feature = "identity")]
mod identity;
#[cfg(feature = "identity")]
pub use collection::SolutionCollector;
#[cfg(feature = "identity")]
pub use identity::{canonical_layout, layout_key, normalize_outcome_identity};
