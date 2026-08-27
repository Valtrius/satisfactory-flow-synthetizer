//! Explicit hierarchical proof accounting for lexicographic search.
//!
//! The ledger is deliberately stricter than a collection of counters. Every
//! expected `N -> L -> profile -> root partition` leaf is registered before it
//! can be completed. An UNSAT fold is available only when every registered
//! child is explicitly UNSAT. Missing, incomplete, or failed leaves therefore
//! cannot be mistaken for exhaustion.

use std::collections::{BTreeMap, BTreeSet};

use solver_api::{IncompleteReason, NodeProfile};
use thiserror::Error;

use crate::search::{ProfileSearchError, ProfileSearchResult, ProfileSearchStats, ProfileWitness};

/// Stable ordinal within one fixed profile's canonical root partition plan.
pub(crate) type RootPartitionId = u32;

/// Result stored at one proof-ledger leaf.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PartitionResult {
    Sat {
        best_witness: ProfileWitness,
        exhaustion: ProfileSearchStats,
    },
    Unsat {
        exhaustion: ProfileSearchStats,
    },
    Incomplete {
        reason: IncompleteReason,
        best_witness: Option<ProfileWitness>,
        progress: ProfileSearchStats,
    },
    Failed {
        error: PartitionFailure,
        progress: ProfileSearchStats,
    },
}

/// Internal reason a leaf could not return a mathematical proof status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PartitionFailure {
    Search(ProfileSearchError),
    WorkerPanicked,
}

/// Folded fixed-profile status, including operational worker loss which is not
/// itself a [`ProfileSearchResult`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FoldedProfileResult {
    Search(Box<ProfileSearchResult>),
    WorkerPanicked,
}

impl From<ProfileSearchResult> for PartitionResult {
    fn from(result: ProfileSearchResult) -> Self {
        match result {
            ProfileSearchResult::Exhausted {
                best_witness: Some(best_witness),
                witnesses: _,
                stats,
            } => Self::Sat {
                best_witness,
                exhaustion: stats,
            },
            ProfileSearchResult::Exhausted {
                best_witness: None,
                witnesses: _,
                stats,
            } => Self::Unsat { exhaustion: stats },
            ProfileSearchResult::Incomplete {
                reason,
                best_witness,
                witnesses: _,
                stats,
            } => Self::Incomplete {
                reason,
                best_witness,
                progress: stats,
            },
            ProfileSearchResult::Failed { error, stats } => Self::Failed {
                error: PartitionFailure::Search(error),
                progress: stats,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ParentProofStatus {
    Pending,
    Sat { best_witness: ProfileWitness },
    Unsat,
    Incomplete { reason: IncompleteReason },
    Failed { error: PartitionFailure },
}

#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub(crate) enum ProofLedgerError {
    #[error("proof ledger already registered node {node_count}")]
    DuplicateNode { node_count: u32 },
    #[error("proof ledger has no registered node {node_count}")]
    UnknownNode { node_count: u32 },
    #[error("proof ledger already registered node {node_count}, link group {link_count}")]
    DuplicateLinkGroup { node_count: u32, link_count: u32 },
    #[error(
        "proof ledger has no registered node {node_count}, link group {link_count}, profile {profile:?}"
    )]
    UnknownProfile {
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
    },
    #[error(
        "proof ledger already registered root partitions for node {node_count}, link group {link_count}, profile {profile:?}"
    )]
    DuplicatePartitionPlan {
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
    },
    #[error(
        "proof ledger root partition plan is empty for node {node_count}, link group {link_count}, profile {profile:?}"
    )]
    EmptyPartitionPlan {
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
    },
    #[error(
        "proof ledger has no root partition {partition} for node {node_count}, link group {link_count}, profile {profile:?}"
    )]
    UnknownPartition {
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
        partition: RootPartitionId,
    },
    #[error(
        "proof ledger root partition {partition} was completed twice for node {node_count}, link group {link_count}, profile {profile:?}"
    )]
    DuplicatePartitionResult {
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
        partition: RootPartitionId,
    },
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ProofLedger {
    nodes: BTreeMap<u32, NodeLedger>,
}

#[derive(Clone, Debug, Default)]
struct NodeLedger {
    link_groups: BTreeMap<u32, LinkGroupLedger>,
}

#[derive(Clone, Debug)]
struct LinkGroupLedger {
    profiles: BTreeMap<NodeProfile, Option<ProfileLedger>>,
}

#[derive(Clone, Debug)]
struct ProfileLedger {
    expected: BTreeSet<RootPartitionId>,
    results: BTreeMap<RootPartitionId, PartitionResult>,
}

impl ProofLedger {
    pub(crate) fn begin_node(&mut self, node_count: u32) -> Result<(), ProofLedgerError> {
        if self.nodes.contains_key(&node_count) {
            return Err(ProofLedgerError::DuplicateNode { node_count });
        }
        self.nodes.insert(node_count, NodeLedger::default());
        Ok(())
    }

    pub(crate) fn register_link_group(
        &mut self,
        node_count: u32,
        link_count: u32,
        profiles: impl IntoIterator<Item = NodeProfile>,
    ) -> Result<(), ProofLedgerError> {
        let node = self
            .nodes
            .get_mut(&node_count)
            .ok_or(ProofLedgerError::UnknownNode { node_count })?;
        if node.link_groups.contains_key(&link_count) {
            return Err(ProofLedgerError::DuplicateLinkGroup {
                node_count,
                link_count,
            });
        }
        node.link_groups.insert(
            link_count,
            LinkGroupLedger {
                profiles: profiles
                    .into_iter()
                    .map(|profile| (profile, None))
                    .collect(),
            },
        );
        Ok(())
    }

    pub(crate) fn register_profile_partitions(
        &mut self,
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
        partitions: impl IntoIterator<Item = RootPartitionId>,
    ) -> Result<(), ProofLedgerError> {
        let slot = self.profile_slot_mut(node_count, link_count, profile)?;
        if slot.is_some() {
            return Err(ProofLedgerError::DuplicatePartitionPlan {
                node_count,
                link_count,
                profile,
            });
        }
        let expected = partitions.into_iter().collect::<BTreeSet<_>>();
        if expected.is_empty() {
            return Err(ProofLedgerError::EmptyPartitionPlan {
                node_count,
                link_count,
                profile,
            });
        }
        *slot = Some(ProfileLedger {
            expected,
            results: BTreeMap::new(),
        });
        Ok(())
    }

    pub(crate) fn record_partition(
        &mut self,
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
        partition: RootPartitionId,
        result: PartitionResult,
    ) -> Result<(), ProofLedgerError> {
        let ledger = self
            .profile_slot_mut(node_count, link_count, profile)?
            .as_mut()
            .ok_or(ProofLedgerError::UnknownPartition {
                node_count,
                link_count,
                profile,
                partition,
            })?;
        if !ledger.expected.contains(&partition) {
            return Err(ProofLedgerError::UnknownPartition {
                node_count,
                link_count,
                profile,
                partition,
            });
        }
        if ledger.results.insert(partition, result).is_some() {
            return Err(ProofLedgerError::DuplicatePartitionResult {
                node_count,
                link_count,
                profile,
                partition,
            });
        }
        Ok(())
    }

    pub(crate) fn fold_profile(
        &self,
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
    ) -> Result<Option<FoldedProfileResult>, ProofLedgerError> {
        let ledger = self.profile_ledger(node_count, link_count, profile)?;
        if ledger.results.len() != ledger.expected.len() {
            return Ok(None);
        }

        let mut stats = ProfileSearchStats::default();
        let mut best_witness = None;
        let mut first_incomplete = None;
        let mut first_failure = None;
        for partition in &ledger.expected {
            let result = ledger
                .results
                .get(partition)
                .expect("equal expected/result lengths imply every registered partition exists");
            match result {
                PartitionResult::Sat {
                    best_witness: witness,
                    exhaustion,
                } => {
                    merge_stats(&mut stats, exhaustion);
                    retain_witness(&mut best_witness, witness.clone());
                }
                PartitionResult::Unsat { exhaustion } => merge_stats(&mut stats, exhaustion),
                PartitionResult::Incomplete {
                    reason,
                    best_witness: witness,
                    progress,
                } => {
                    merge_stats(&mut stats, progress);
                    if let Some(witness) = witness {
                        retain_witness(&mut best_witness, witness.clone());
                    }
                    first_incomplete.get_or_insert_with(|| reason.clone());
                }
                PartitionResult::Failed { error, progress } => {
                    merge_stats(&mut stats, progress);
                    first_failure.get_or_insert_with(|| error.clone());
                }
            }
        }

        if let Some(error) = first_failure {
            return Ok(Some(match error {
                PartitionFailure::Search(error) => {
                    FoldedProfileResult::Search(Box::new(ProfileSearchResult::Failed {
                        error,
                        stats,
                    }))
                }
                PartitionFailure::WorkerPanicked => FoldedProfileResult::WorkerPanicked,
            }));
        }
        if let Some(reason) = first_incomplete {
            return Ok(Some(FoldedProfileResult::Search(Box::new(
                ProfileSearchResult::Incomplete {
                    reason,
                    best_witness,
                    witnesses: Vec::new(),
                    stats,
                },
            ))));
        }
        Ok(Some(FoldedProfileResult::Search(Box::new(
            ProfileSearchResult::Exhausted {
                best_witness,
                witnesses: Vec::new(),
                stats,
            },
        ))))
    }

    pub(crate) fn link_group_status(&self, node_count: u32, link_count: u32) -> ParentProofStatus {
        let Some(group) = self
            .nodes
            .get(&node_count)
            .and_then(|node| node.link_groups.get(&link_count))
        else {
            return ParentProofStatus::Pending;
        };
        fold_profiles(group, |profile| {
            self.fold_profile(node_count, link_count, profile)
                .ok()
                .flatten()
        })
    }

    pub(crate) fn node_status(&self, node_count: u32) -> ParentProofStatus {
        let Some(node) = self.nodes.get(&node_count) else {
            return ParentProofStatus::Pending;
        };
        for link_count in node.link_groups.keys().copied() {
            match self.link_group_status(node_count, link_count) {
                ParentProofStatus::Unsat => {}
                status => return status,
            }
        }
        ParentProofStatus::Unsat
    }

    fn profile_slot_mut(
        &mut self,
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
    ) -> Result<&mut Option<ProfileLedger>, ProofLedgerError> {
        self.nodes
            .get_mut(&node_count)
            .and_then(|node| node.link_groups.get_mut(&link_count))
            .and_then(|group| group.profiles.get_mut(&profile))
            .ok_or(ProofLedgerError::UnknownProfile {
                node_count,
                link_count,
                profile,
            })
    }

    fn profile_ledger(
        &self,
        node_count: u32,
        link_count: u32,
        profile: NodeProfile,
    ) -> Result<&ProfileLedger, ProofLedgerError> {
        self.nodes
            .get(&node_count)
            .and_then(|node| node.link_groups.get(&link_count))
            .and_then(|group| group.profiles.get(&profile))
            .and_then(Option::as_ref)
            .ok_or(ProofLedgerError::UnknownProfile {
                node_count,
                link_count,
                profile,
            })
    }
}

fn fold_profiles(
    group: &LinkGroupLedger,
    mut result: impl FnMut(NodeProfile) -> Option<FoldedProfileResult>,
) -> ParentProofStatus {
    let mut best_witness = None;
    let mut pending = false;
    let mut incomplete = None;
    let mut failed = None;
    for profile in group.profiles.keys().copied() {
        match result(profile) {
            Some(FoldedProfileResult::Search(result)) => match *result {
                ProfileSearchResult::Exhausted {
                    best_witness: witness,
                    ..
                } => {
                    if let Some(witness) = witness {
                        retain_witness(&mut best_witness, witness);
                    }
                }
                ProfileSearchResult::Incomplete { reason, .. } => {
                    incomplete.get_or_insert(reason);
                }
                ProfileSearchResult::Failed { error, .. } => {
                    failed.get_or_insert(PartitionFailure::Search(error));
                }
            },
            Some(FoldedProfileResult::WorkerPanicked) => {
                failed.get_or_insert(PartitionFailure::WorkerPanicked);
            }
            None => pending = true,
        }
    }
    if let Some(error) = failed {
        ParentProofStatus::Failed { error }
    } else if let Some(reason) = incomplete {
        ParentProofStatus::Incomplete { reason }
    } else if pending {
        ParentProofStatus::Pending
    } else if let Some(best_witness) = best_witness {
        ParentProofStatus::Sat { best_witness }
    } else {
        ParentProofStatus::Unsat
    }
}

fn retain_witness(target: &mut Option<ProfileWitness>, candidate: ProfileWitness) {
    if target
        .as_ref()
        .is_none_or(|current| candidate.canonical_graph_key < current.canonical_graph_key)
    {
        *target = Some(candidate);
    }
}

fn merge_stats(total: &mut ProfileSearchStats, child: &ProfileSearchStats) {
    let target = &mut total.instrumentation;
    let source = &child.instrumentation;
    target.raw_structural_decisions = target
        .raw_structural_decisions
        .saturating_add(source.raw_structural_decisions);
    target.canonical_states_retained = target
        .canonical_states_retained
        .saturating_add(source.canonical_states_retained);
    target.canonical_duplicates_eliminated = target
        .canonical_duplicates_eliminated
        .saturating_add(source.canonical_duplicates_eliminated);
    target.propagation_contradictions = target
        .propagation_contradictions
        .saturating_add(source.propagation_contradictions);
    target.capacity_prunes = target
        .capacity_prunes
        .saturating_add(source.capacity_prunes);
    target.lower_bound_prunes = target
        .lower_bound_prunes
        .saturating_add(source.lower_bound_prunes);
    target.scc_solves = target.scc_solves.saturating_add(source.scc_solves);
    target.scc_cache_hits = target.scc_cache_hits.saturating_add(source.scc_cache_hits);
    target.canonicalization_time_ns = target
        .canonicalization_time_ns
        .saturating_add(source.canonicalization_time_ns);
    target.algebra_time_ns = target
        .algebra_time_ns
        .saturating_add(source.algebra_time_ns);
    target.peak_state_cache_size = target
        .peak_state_cache_size
        .max(source.peak_state_cache_size);
    target.peak_memory_bytes = target.peak_memory_bytes.max(source.peak_memory_bytes);
    target.wall_time_ms = target.wall_time_ms.max(source.wall_time_ms);
    total.state_cache_hits = total
        .state_cache_hits
        .saturating_add(child.state_cache_hits);
    total.complete_topologies = total
        .complete_topologies
        .saturating_add(child.complete_topologies);
    total.rejected_complete_topologies = total
        .rejected_complete_topologies
        .saturating_add(child.rejected_complete_topologies);
    total.validated_cyclic_topologies = total
        .validated_cyclic_topologies
        .saturating_add(child.validated_cyclic_topologies);
}

#[cfg(test)]
mod tests {
    use solver_api::{CanonicalGraphKey, PhysicalGraph, ValidationSummary};

    use super::*;

    fn profile(splitter2: u32, merger2: u32) -> NodeProfile {
        NodeProfile {
            splitter2,
            splitter3: 0,
            merger2,
            merger3: 0,
        }
    }

    fn witness(key: u8) -> ProfileWitness {
        ProfileWitness {
            canonical_graph_key: CanonicalGraphKey::from_bytes(vec![key]),
            graph: PhysicalGraph {
                nodes: Vec::new(),
                links: Vec::new(),
            },
            validation: ValidationSummary {
                validator_version: 1,
                node_count: 0,
                link_count: 0,
                physical_link_count: 0,
                discard_link_count: 0,
                cyclic_scc_count: 0,
            },
        }
    }

    fn ledger_with_two_profiles() -> ProofLedger {
        let mut ledger = ProofLedger::default();
        ledger.begin_node(2).unwrap();
        ledger
            .register_link_group(2, 5, [profile(1, 1), profile(0, 2)])
            .unwrap();
        ledger
            .register_profile_partitions(2, 5, profile(1, 1), [0, 1])
            .unwrap();
        ledger
            .register_profile_partitions(2, 5, profile(0, 2), [0])
            .unwrap();
        ledger
    }

    #[test]
    fn unsat_parent_requires_every_registered_leaf_to_be_unsat() {
        let mut ledger = ledger_with_two_profiles();
        ledger
            .record_partition(
                2,
                5,
                profile(1, 1),
                0,
                PartitionResult::Unsat {
                    exhaustion: ProfileSearchStats::default(),
                },
            )
            .unwrap();
        assert_eq!(ledger.link_group_status(2, 5), ParentProofStatus::Pending);
        ledger
            .record_partition(
                2,
                5,
                profile(1, 1),
                1,
                PartitionResult::Unsat {
                    exhaustion: ProfileSearchStats::default(),
                },
            )
            .unwrap();
        assert_eq!(ledger.link_group_status(2, 5), ParentProofStatus::Pending);
        ledger
            .record_partition(
                2,
                5,
                profile(0, 2),
                0,
                PartitionResult::Unsat {
                    exhaustion: ProfileSearchStats::default(),
                },
            )
            .unwrap();
        assert_eq!(ledger.link_group_status(2, 5), ParentProofStatus::Unsat);
    }

    #[test]
    fn incomplete_leaf_prevents_sat_or_unsat_parent() {
        let mut ledger = ledger_with_two_profiles();
        for (fixed_profile, partition) in [(profile(1, 1), 0), (profile(0, 2), 0)] {
            ledger
                .record_partition(
                    2,
                    5,
                    fixed_profile,
                    partition,
                    PartitionResult::Unsat {
                        exhaustion: ProfileSearchStats::default(),
                    },
                )
                .unwrap();
        }
        ledger
            .record_partition(
                2,
                5,
                profile(1, 1),
                1,
                PartitionResult::Incomplete {
                    reason: IncompleteReason::Cancelled,
                    best_witness: None,
                    progress: ProfileSearchStats::default(),
                },
            )
            .unwrap();
        assert_eq!(
            ledger.link_group_status(2, 5),
            ParentProofStatus::Incomplete {
                reason: IncompleteReason::Cancelled
            }
        );
    }

    #[test]
    fn incomplete_profile_retains_the_best_witness_without_becoming_sat() {
        let mut ledger = ledger_with_two_profiles();
        ledger
            .record_partition(
                2,
                5,
                profile(1, 1),
                0,
                PartitionResult::Sat {
                    best_witness: witness(9),
                    exhaustion: ProfileSearchStats::default(),
                },
            )
            .unwrap();
        ledger
            .record_partition(
                2,
                5,
                profile(1, 1),
                1,
                PartitionResult::Incomplete {
                    reason: IncompleteReason::Cancelled,
                    best_witness: Some(witness(2)),
                    progress: ProfileSearchStats::default(),
                },
            )
            .unwrap();

        let Some(FoldedProfileResult::Search(result)) =
            ledger.fold_profile(2, 5, profile(1, 1)).unwrap()
        else {
            panic!("the fully recorded profile must fold");
        };
        let ProfileSearchResult::Incomplete {
            reason,
            best_witness: Some(best_witness),
            ..
        } = *result
        else {
            panic!("an incomplete leaf must prevent a SAT proof");
        };
        assert_eq!(reason, IncompleteReason::Cancelled);
        assert_eq!(
            best_witness.canonical_graph_key,
            CanonicalGraphKey::from_bytes(vec![2])
        );
        assert_eq!(
            ledger.link_group_status(2, 5),
            ParentProofStatus::Incomplete {
                reason: IncompleteReason::Cancelled
            }
        );
    }

    #[test]
    fn failed_leaf_prevents_parent_proof_even_when_every_sibling_is_unsat() {
        let mut ledger = ledger_with_two_profiles();
        for (fixed_profile, partition) in [(profile(1, 1), 0), (profile(0, 2), 0)] {
            ledger
                .record_partition(
                    2,
                    5,
                    fixed_profile,
                    partition,
                    PartitionResult::Unsat {
                        exhaustion: ProfileSearchStats::default(),
                    },
                )
                .unwrap();
        }
        ledger
            .record_partition(
                2,
                5,
                profile(1, 1),
                1,
                PartitionResult::Failed {
                    error: PartitionFailure::Search(ProfileSearchError::MissingAppliedLink),
                    progress: ProfileSearchStats::default(),
                },
            )
            .unwrap();
        assert_eq!(
            ledger.link_group_status(2, 5),
            ParentProofStatus::Failed {
                error: PartitionFailure::Search(ProfileSearchError::MissingAppliedLink)
            }
        );
    }

    #[test]
    fn complete_sat_group_selects_the_smallest_witness_key() {
        let mut ledger = ledger_with_two_profiles();
        for (fixed_profile, partition, key) in [
            (profile(1, 1), 0, Some(9)),
            (profile(1, 1), 1, Some(2)),
            (profile(0, 2), 0, None),
        ] {
            let result = key.map_or_else(
                || PartitionResult::Unsat {
                    exhaustion: ProfileSearchStats::default(),
                },
                |key| PartitionResult::Sat {
                    best_witness: witness(key),
                    exhaustion: ProfileSearchStats::default(),
                },
            );
            ledger
                .record_partition(2, 5, fixed_profile, partition, result)
                .unwrap();
        }
        let ParentProofStatus::Sat { best_witness } = ledger.link_group_status(2, 5) else {
            panic!("all leaves completed and at least one was SAT");
        };
        assert_eq!(
            best_witness.canonical_graph_key,
            CanonicalGraphKey::from_bytes(vec![2])
        );
    }

    #[test]
    fn node_fold_stops_at_the_first_non_unsat_link_group() {
        let mut ledger = ProofLedger::default();
        ledger.begin_node(0).unwrap();
        ledger.register_link_group(0, 1, [profile(0, 0)]).unwrap();
        ledger
            .register_profile_partitions(0, 1, profile(0, 0), [0])
            .unwrap();
        ledger.register_link_group(0, 2, [profile(1, 1)]).unwrap();
        ledger
            .register_profile_partitions(0, 2, profile(1, 1), [0])
            .unwrap();
        ledger
            .record_partition(
                0,
                1,
                profile(0, 0),
                0,
                PartitionResult::Unsat {
                    exhaustion: ProfileSearchStats::default(),
                },
            )
            .unwrap();
        assert_eq!(ledger.node_status(0), ParentProofStatus::Pending);
        ledger
            .record_partition(
                0,
                2,
                profile(1, 1),
                0,
                PartitionResult::Sat {
                    best_witness: witness(1),
                    exhaustion: ProfileSearchStats::default(),
                },
            )
            .unwrap();
        assert!(matches!(
            ledger.node_status(0),
            ParentProofStatus::Sat { .. }
        ));
    }
}
