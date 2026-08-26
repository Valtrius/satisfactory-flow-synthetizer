use serde::{Deserialize, Serialize};

use crate::{CanonicalGraphKey, InputTerminalIndex, OutputTerminalIndex, PhysicalGraph, Rational};

/// Successful checks performed by the independent concrete graph validator.
///
/// The presence of this value records validation success. Validation failures are errors and do
/// not produce a summary.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationSummary {
    /// Validator/schema version that performed the checks.
    pub validator_version: u32,
    /// Number of physical splitters and mergers checked.
    pub node_count: u32,
    /// Number of operator-to-operator belts checked; excludes terminal stubs and discard lines.
    pub link_count: u32,
    /// Total number of positive, capacity-constrained physical links checked.
    pub physical_link_count: u32,
    /// Number of anonymous surplus-discard links checked.
    pub discard_link_count: u32,
    /// Number of cyclic strongly connected components checked for a unique exact flow.
    pub cyclic_scc_count: u32,
}

/// Compact accounting for the finite search obligations completed by a solve.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofSummary {
    /// Schema version of this proof summary.
    pub proof_version: u32,
    /// Proven lower bound used to start node-count iteration.
    pub initial_node_lower_bound: u32,
    /// Greatest consecutive node count fully exhausted, if any.
    pub node_counts_exhausted_through: Option<u32>,
    /// Equal-link groups completely exhausted.
    pub link_groups_exhausted: u64,
    /// Fixed node-type profiles completely exhausted.
    pub profiles_exhausted: u64,
    /// Canonical root partitions completely exhausted.
    pub root_partitions_exhausted: u64,
}

/// A feasible witness that passed independent exact validation.
///
/// This type deliberately carries no optimality proof. In particular, a `best_known` witness in
/// an [`IncompleteResult`] is only an upper bound.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BestKnownSolution {
    /// Physical splitter/merger count of this feasible witness.
    pub node_count: u32,
    /// Operator-to-operator belt count; excludes terminal stubs and discard lines.
    pub link_count: u32,
    /// Total physical link count, including discard lines.
    pub physical_link_count: u32,
    /// Anonymous discard-link count, excluded from `link_count`.
    pub discard_link_count: u32,
    /// Canonical identity and deterministic ordering key.
    pub canonical_graph_key: CanonicalGraphKey,
    /// Fully flattened physical graph.
    pub graph: PhysicalGraph,
    /// Independent exact validation record.
    pub validation: ValidationSummary,
}

/// A minimum-node and independently validated physical solution.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimalSolution {
    /// Proven minimum physical splitter/merger count.
    pub node_count: u32,
    /// Operator-to-operator belt count of the selected minimum-node witness.
    pub link_count: u32,
    /// Total physical link count of the selected witness, including discard lines.
    pub physical_link_count: u32,
    /// Anonymous discard-link count, excluded from optimization.
    pub discard_link_count: u32,
    /// Deterministic canonical identity of the selected minimum-node witness.
    pub canonical_graph_key: CanonicalGraphKey,
    /// Fully flattened physical graph.
    pub graph: PhysicalGraph,
    /// Completed hierarchical proof accounting.
    pub proof: ProofSummary,
    /// Independent exact validation record.
    pub validation: ValidationSummary,
}

/// Identifies an external terminal in a finite global capacity proof.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "direction", content = "index", rename_all = "snake_case")]
pub enum ExternalTerminal {
    /// An input terminal.
    Input(InputTerminalIndex),
    /// An output terminal.
    Output(OutputTerminalIndex),
}

/// A finite theorem proving that no network can satisfy the problem at any node count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum GlobalUnsatReason {
    /// Exact requested output exceeds total external input.
    InsufficientInput {
        /// Sum of all input terminal rates.
        total_input: Rational,
        /// Sum of all output terminal rates.
        total_output: Rational,
    },
    /// An external terminal alone exceeds the mandatory link capacity.
    ExternalRateExceedsCapacity {
        /// Terminal whose sole physical link would exceed capacity.
        terminal: ExternalTerminal,
        /// Exact terminal rate.
        rate: Rational,
        /// Inclusive physical link capacity.
        max_link_rate: Rational,
    },
}

/// Structured finite proof that the entire problem is unsatisfiable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalUnsatProof {
    /// Finite global contradiction.
    pub reason: GlobalUnsatReason,
    /// Proof accounting available when the contradiction was found.
    pub proof: ProofSummary,
}

/// Why a finite run ended without completing the current optimality proof.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum IncompleteReason {
    /// The caller requested cancellation.
    Cancelled,
    /// A caller-supplied deadline expired.
    DeadlineReached,
    /// A configured finite search resource limit was reached.
    ResourceLimit {
        /// Stable, user-facing description of the reached limit.
        detail: String,
    },
    /// A proof worker could not finish its assigned partition.
    WorkerFailed {
        /// Stable diagnostic text suitable for logs and UI display.
        detail: String,
    },
}

/// A run that ended without proving an optimal solution or global impossibility.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompleteResult {
    /// Reason the proof stopped.
    pub reason: IncompleteReason,
    /// Optional independently validated upper bound. It is not an optimal solution.
    pub best_known: Option<BestKnownSolution>,
    /// Finite proof obligations completed before the run stopped.
    pub proof: ProofSummary,
}

/// The mathematical outcome of an exact solve.
///
/// Invalid input and internal failures belong in the surrounding `Result<_, SolverError>` and
/// are not represented as mathematical outcomes here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum SolveResult {
    /// A complete lexicographic optimality proof and validated witness.
    Optimal(OptimalSolution),
    /// A finite proof that no solution exists at any node count.
    GloballyUnsat(GlobalUnsatProof),
    /// A stopped proof, possibly with a validated but non-optimal incumbent.
    Incomplete(IncompleteResult),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConsumerPortRef, ProducerPortRef};

    fn validated_direct_link() -> BestKnownSolution {
        BestKnownSolution {
            node_count: 0,
            link_count: 1,
            physical_link_count: 1,
            discard_link_count: 0,
            canonical_graph_key: CanonicalGraphKey::from_bytes(vec![0]),
            graph: PhysicalGraph {
                nodes: Vec::new(),
                links: vec![crate::PhysicalLink {
                    producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                    consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                    flow: "1/3".parse().unwrap(),
                }],
            },
            validation: ValidationSummary {
                validator_version: 1,
                node_count: 0,
                link_count: 1,
                physical_link_count: 1,
                discard_link_count: 0,
                cyclic_scc_count: 0,
            },
        }
    }

    #[test]
    fn incomplete_incumbent_is_serialized_as_best_known_not_optimal() {
        let outcome = SolveResult::Incomplete(IncompleteResult {
            reason: IncompleteReason::Cancelled,
            best_known: Some(validated_direct_link()),
            proof: ProofSummary::default(),
        });
        let json = serde_json::to_value(outcome).unwrap();
        assert_eq!(json["kind"], "incomplete");
        assert!(json["result"]["bestKnown"].is_object());
        assert!(json["result"]["bestKnown"].get("proof").is_none());
        assert_eq!(json["result"]["bestKnown"]["linkCount"], 1);
        assert_eq!(json["result"]["bestKnown"]["physicalLinkCount"], 1);
        assert_eq!(json["result"]["bestKnown"]["discardLinkCount"], 0);
    }

    #[test]
    fn global_unsat_reason_keeps_exact_rates_as_strings() {
        let outcome = SolveResult::GloballyUnsat(GlobalUnsatProof {
            reason: GlobalUnsatReason::InsufficientInput {
                total_input: "1/3".parse().unwrap(),
                total_output: "1/2".parse().unwrap(),
            },
            proof: ProofSummary::default(),
        });
        let json = serde_json::to_value(outcome).unwrap();
        assert_eq!(json["result"]["reason"]["kind"], "insufficient_input");
        assert_eq!(json["result"]["reason"]["totalInput"], "1/3");
        assert_eq!(json["result"]["reason"]["totalOutput"], "1/2");
    }
}
