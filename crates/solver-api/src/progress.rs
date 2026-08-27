use serde::{Deserialize, Serialize};

use crate::{BestKnownSolution, NodeProfile};

/// Current high-level phase of a live solve or component prewarm operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolvePhase {
    /// Parsing and exact global normalization.
    Normalizing,
    /// Finite global validity, conservation, and capacity proofs.
    GlobalChecks,
    /// Computing the proven starting node lower bound.
    ComputingLowerBound,
    /// Exhausting profiles in lexicographic `(node_count, link_count)` order.
    Searching,
    /// Flattening and independently validating a candidate witness.
    ValidatingWitness,
    /// Exhaustively generating bounded reusable components.
    PrewarmingComponents,
}

/// Hierarchical finite proof obligation currently assigned to a worker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofObligation {
    /// Node count currently being proved SAT or UNSAT.
    pub node_count: u32,
    /// Current fixed link group, if profile enumeration has reached that level.
    pub link_count: Option<u32>,
    /// Current fixed node-type profile, if assigned.
    pub profile: Option<NodeProfile>,
    /// Deterministic canonical root-partition number, if assigned.
    pub root_partition: Option<u32>,
}

/// Monotone counters and timing measurements for search diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchInstrumentation {
    /// Structural edge or component decisions attempted.
    pub raw_structural_decisions: u64,
    /// Canonical states retained for search.
    pub canonical_states_retained: u64,
    /// Isomorphic or equivalent states rejected.
    pub canonical_duplicates_eliminated: u64,
    /// Contradictions found by exact propagation.
    pub propagation_contradictions: u64,
    /// Branches killed by exact positivity or capacity proofs.
    pub capacity_prunes: u64,
    /// Branches killed by proven lower bounds.
    pub lower_bound_prunes: u64,
    /// Stored no-goods that matched a state.
    pub no_good_hits: u64,
    /// Exact SCC algebra solves performed.
    pub scc_solves: u64,
    /// Reusable SCC summaries found in cache.
    pub scc_cache_hits: u64,
    /// Component macro applications used.
    pub component_hits: u64,
    /// Synchronous component optimizations requested by live search.
    pub component_optimizations: u64,
    /// Component database lookups attempted.
    pub component_db_lookups: u64,
    /// Component database lookups that found an applicable record.
    pub component_db_hits: u64,
    /// Nanoseconds spent canonicalizing topology and algebra.
    pub canonicalization_time_ns: u64,
    /// Nanoseconds spent in exact algebra.
    pub algebra_time_ns: u64,
    /// Largest number of entries in a live state cache.
    pub peak_state_cache_size: u64,
    /// Peak deterministic solver-owned canonical state/SCC cache payload in bytes.
    ///
    /// Implementations count owned key buffers and fixed cache values, but not
    /// allocator buckets or process-global memory whose size depends on runtime
    /// hashing, worker scheduling, or unrelated allocations.
    pub peak_memory_bytes: u64,
    /// End-to-end wall-clock duration in milliseconds.
    pub wall_time_ms: u64,
}

impl SearchInstrumentation {
    /// Returns the exact database hit ratio as `(hits, lookups)` without floating point.
    #[must_use]
    pub const fn component_db_hit_ratio(&self) -> (u64, u64) {
        (self.component_db_hits, self.component_db_lookups)
    }
}

/// A serializable snapshot of deterministic proof progress.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolverProgress {
    /// Current high-level solver phase.
    pub phase: SolvePhase,
    /// Current hierarchical proof obligation, when search is active.
    pub obligation: Option<ProofObligation>,
    /// Profiles conclusively completed in the current equal-link group.
    pub completed_profiles: u32,
    /// Total profiles in the current equal-link group, when known.
    pub total_profiles: Option<u32>,
    /// Current search counters and timings.
    pub instrumentation: SearchInstrumentation,
}

/// Live solver notification delivered to an application or service adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SolverEvent {
    /// Updated proof progress.
    Progress(SolverProgress),
    /// A newly improved, independently validated upper-bound witness.
    Incumbent(BestKnownSolution),
    /// A distinct validated layout found during complete minimum-N enumeration.
    SolutionFound(BestKnownSolution),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrumentation_reports_an_exact_hit_ratio() {
        let counters = SearchInstrumentation {
            component_db_lookups: 3,
            component_db_hits: 1,
            ..SearchInstrumentation::default()
        };
        assert_eq!(counters.component_db_hit_ratio(), (1, 3));
    }
}
