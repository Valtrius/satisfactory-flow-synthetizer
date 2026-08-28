//! Native Custom search diagnostics, projected into common progress at the solver boundary.

use solver_api::{Diagnostic, NodeProfile};

/// Hierarchical finite proof obligation currently assigned to a worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]

pub struct ProofObligation {
    /// Node count currently being proved SAT or UNSAT.
    pub node_count: u32,
    /// Current exact operator-to-operator link group, if reached.
    pub link_count: Option<u32>,
    /// Current fixed node-type profile, if assigned.
    pub profile: Option<NodeProfile>,
    /// Deterministic canonical root-partition number, if assigned.
    pub root_partition: Option<u32>,
}

/// Monotone counters and timing measurements for search diagnostics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]

pub struct SearchInstrumentation {
    /// Sibling subtrees published to the bounded work queue.
    pub donated_tasks: u64,
    /// Completed proofs borrowed from another search context.
    pub shared_cache_hits: u64,
    /// Largest group-owned completed-state payload, counted once per group.
    pub shared_cache_bytes: u64,
    /// Static root obligations dispatched.
    pub root_partitions: u64,
    /// Elapsed time planning the static frontier.
    pub partition_planning_ns: u64,
    /// Structural edge decisions attempted.
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
    /// Exact SCC algebra solves performed.
    pub scc_solves: u64,
    /// Reusable SCC summaries found in cache.
    pub scc_cache_hits: u64,
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
    #[must_use]
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        vec![
            Diagnostic::counter("custom.donated_tasks", "Donated tasks", self.donated_tasks),
            Diagnostic::counter(
                "custom.shared_cache_hits",
                "Shared completed hits",
                self.shared_cache_hits,
            ),
            Diagnostic::counter(
                "custom.shared_cache_bytes",
                "Shared state bytes",
                self.shared_cache_bytes,
            ),
            Diagnostic::counter(
                "custom.root_partitions",
                "Root partitions",
                self.root_partitions,
            ),
            Diagnostic::counter(
                "custom.partition_planning_ns",
                "Partition planning ns",
                self.partition_planning_ns,
            ),
            Diagnostic::counter(
                "custom.raw_structural_decisions",
                "Structural decisions",
                self.raw_structural_decisions,
            ),
            Diagnostic::counter(
                "custom.canonical_states_retained",
                "Canonical states retained",
                self.canonical_states_retained,
            ),
            Diagnostic::counter(
                "custom.canonical_duplicates_eliminated",
                "Canonical duplicates",
                self.canonical_duplicates_eliminated,
            ),
            Diagnostic::counter(
                "custom.propagation_contradictions",
                "Propagation contradictions",
                self.propagation_contradictions,
            ),
            Diagnostic::counter(
                "custom.capacity_prunes",
                "Capacity prunes",
                self.capacity_prunes,
            ),
            Diagnostic::counter(
                "custom.lower_bound_prunes",
                "Lower-bound prunes",
                self.lower_bound_prunes,
            ),
            Diagnostic::counter("custom.scc_solves", "SCC solves", self.scc_solves),
            Diagnostic::counter(
                "custom.scc_cache_hits",
                "SCC cache hits",
                self.scc_cache_hits,
            ),
            Diagnostic::counter(
                "custom.canonicalization_time_ns",
                "Canonicalization ns",
                self.canonicalization_time_ns,
            ),
            Diagnostic::counter("custom.algebra_time_ns", "Algebra ns", self.algebra_time_ns),
            Diagnostic::counter(
                "custom.peak_state_cache_size",
                "Peak cache entries",
                self.peak_state_cache_size,
            ),
            Diagnostic::counter(
                "custom.peak_memory_bytes",
                "Peak cache bytes",
                self.peak_memory_bytes,
            ),
        ]
    }
}
