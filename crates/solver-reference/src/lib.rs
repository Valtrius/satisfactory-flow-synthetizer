//! Deliberately simple exhaustive oracle for small exact solver instances.
//!
//! This crate does not depend on production search, pruning, propagation,
//! components, SCC summaries, lower bounds, no-goods, or state memoization.

mod canonical;
mod component;
mod profile;
mod solver;

pub use canonical::{CanonicalizedGraph, canonicalize_graph};
pub use component::{
    COMPONENT_MANIFEST_VERSION, CanonicalComponentTopology, ComponentCell,
    ComponentCellEnumeration, ComponentCellManifest, ComponentEnumerationBounds,
    ComponentEnumerationProgress, ComponentInternalLink, ComponentOracleError, component_cells,
    enumerate_component_cell,
};
pub use profile::{
    ProfilePlan, ProfileTopology, enumerate_profile_plans, enumerate_profiles,
    enumerate_topologies, enumerate_topologies_with_discard,
};
pub use solver::{ReferenceError, ReferenceOptions, solve_reference};
