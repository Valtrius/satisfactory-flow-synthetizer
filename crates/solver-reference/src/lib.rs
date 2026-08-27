//! Deliberately simple exhaustive oracle for small exact solver instances.
//!
//! This crate does not depend on production search, pruning, propagation,
//! SCC summaries, lower bounds, or state memoization.

mod canonical;
mod profile;
mod solver;

pub use canonical::{CanonicalizedGraph, canonicalize_graph};
pub use profile::{
    ProfilePlan, ProfileTopology, enumerate_profile_plans, enumerate_profiles,
    enumerate_topologies, enumerate_topologies_with_discard,
};
pub use solver::{ReferenceError, ReferenceOptions, solve_problem, solve_reference};
