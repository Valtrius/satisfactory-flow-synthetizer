//! Exhaustive production search for one fixed physical node profile.

mod donation;
mod frontier;
#[cfg(feature = "bench-internals")]
pub(crate) mod prefix_benchmark;
pub(crate) use donation::DonationPool;
mod shared;
pub(crate) use frontier::plan_adaptive_partitions;
pub(crate) use shared::SharedStateCache;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    hash::{DefaultHasher, Hash, Hasher},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use solver_api::{
    CanonicalGraphKey, ConsumerPortRef, IncompleteReason, NodeId, NodeProfile, NodeType,
    PhysicalGraph, PhysicalLink, PhysicalNode, Problem, ProducerPortRef, Rational,
    ValidationSummary,
};
use solver_validation::{ValidationError, solve_topology, validate_solution};
use thiserror::Error;

use crate::{
    algebra::sparse::Consistency,
    canonical::{
        CanonicalFlowEndpoint, MarkedLinkCanonicalKey, PartialLink, PartialTopology, SccSummaryKey,
        StateKey, canonicalize_marked_link_cancellable,
        canonicalize_scc_summary_input_with_relabeling_cancellable,
        canonicalize_state_and_open_ports_cancellable, canonicalize_state_cancellable,
        canonicalize_witness_cancellable,
    },
    hotspot_profile,
    lower_bound::profile_impossibility,
    problem::NormalizedProblem,
    profile::{ProfileArithmeticError, ProfileLinkAccounting, profile_link_accounting},
    propagation::{
        PropagationCheckpoint, PropagationConflict, PropagationError, PropagationOutcome,
        PropagationState,
    },
    reachability::is_proven_unreachable,
    scc::{SccError, detect_affected_sccs, summarize_open_scc},
    telemetry::SearchInstrumentation,
    topology::{FlowVarId, TopologyDecision, TopologyState},
};

#[cfg(test)]
use crate::canonical::canonicalize_state;

/// A complete fixed-profile witness that passed the independent exact validator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileWitness {
    pub canonical_graph_key: CanonicalGraphKey,
    pub graph: PhysicalGraph,
    pub validation: ValidationSummary,
}

/// Additional counters local to one fixed-profile search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileSearchStats {
    pub instrumentation: SearchInstrumentation,
    pub state_cache_hits: u64,
    pub complete_topologies: u64,
    pub rejected_complete_topologies: u64,
    pub validated_cyclic_topologies: u64,
}

/// Completed status stored only in the cache for the current fixed profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StateStatus {
    /// The owning DFS frame has not yet exhausted this state.
    InProgress,
    /// A direct mathematical proof rejected this state.
    ProvenDead,
    /// Every canonical continuation was exhausted without a witness.
    Exhausted,
    /// Every canonical continuation was exhausted and this is its smallest witness key.
    SatWitness(CanonicalGraphKey),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct StateFingerprint {
    remaining_profile: NodeProfile,
    link_count: u32,
    digest: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum FingerprintOwner {
    Input(Rational),
    Output(Rational),
    Discard,
    Node(NodeType),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum FingerprintIncident {
    Incoming(FingerprintOwner, Option<Rational>),
    Outgoing(FingerprintOwner, Option<Rational>),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DeferredSnapshot {
    nodes: Vec<PhysicalNode>,
    links: Vec<PartialLink>,
    discard_count: u32,
    remaining_profile: NodeProfile,
}

impl DeferredSnapshot {
    fn from_partial(topology: PartialTopology) -> Self {
        Self {
            nodes: topology.nodes,
            links: topology.links,
            discard_count: topology.discard_count,
            remaining_profile: topology.remaining_profile,
        }
    }

    fn restore(&self, problem: &Problem) -> PartialTopology {
        PartialTopology {
            problem: problem.clone(),
            nodes: self.nodes.clone(),
            links: self.links.clone(),
            discard_count: self.discard_count,
            remaining_profile: self.remaining_profile,
        }
    }
}

#[derive(Clone, Debug)]
struct DeferredStateEntry {
    snapshot: DeferredSnapshot,
    status: StateStatus,
}

#[derive(Clone, Debug)]
enum DeferredStateBucket {
    Unique(DeferredStateEntry),
    Canonicalized,
}

#[derive(Clone, Debug)]
enum StateCacheToken {
    Exact(StateKey),
    Deferred(StateFingerprint),
}

enum StateOpenPortOrder {
    Canonical(BTreeMap<crate::topology::OpenPortRef, crate::topology::OpenPortRef>),
    Raw,
}

/// One exact SCC value consequence in canonical physical-port coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedKnownValue {
    variable: CanonicalFlowEndpoint,
    value: Rational,
}

/// One exact SCC ratio consequence in jointly canonical port coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedHomogeneousRatio {
    lhs: CanonicalFlowEndpoint,
    rhs: CanonicalFlowEndpoint,
    factor: Rational,
}

/// Relabeling-safe exact facts retained from one open-SCC analysis.
///
/// Every endpoint uses the same winning canonical labeling as `SccSummaryKey`.
/// Storing both endpoints jointly preserves ratio correlations that separate
/// marked-port orbit keys would lose.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedOpenSccSummary {
    consistency: Consistency,
    coefficient_rank: usize,
    augmented_rank: usize,
    variable_count: usize,
    known_values: Vec<CachedKnownValue>,
    homogeneous_ratios: Vec<CachedHomogeneousRatio>,
}

fn state_cache_entry_bytes(key: &StateKey, status: &StateStatus) -> u64 {
    let witness_key_bytes = match status {
        StateStatus::SatWitness(key) => key.as_bytes().len(),
        StateStatus::InProgress | StateStatus::ProvenDead | StateStatus::Exhausted => 0,
    };
    u64::try_from(
        std::mem::size_of::<StateKey>()
            .saturating_add(std::mem::size_of::<StateStatus>())
            .saturating_add(key.as_bytes().len())
            .saturating_add(witness_key_bytes),
    )
    .unwrap_or(u64::MAX)
}

fn deferred_state_entry_bytes(entry: &DeferredStateEntry) -> u64 {
    let witness_key_bytes = match &entry.status {
        StateStatus::SatWitness(key) => key.as_bytes().len(),
        StateStatus::InProgress | StateStatus::ProvenDead | StateStatus::Exhausted => 0,
    };
    let rational_payload = entry
        .snapshot
        .links
        .iter()
        .filter_map(|link| link.flow.as_ref())
        .map(rational_payload_bytes)
        .fold(0_usize, usize::saturating_add);
    u64::try_from(
        std::mem::size_of::<StateFingerprint>()
            .saturating_add(std::mem::size_of::<DeferredStateEntry>())
            .saturating_add(
                entry
                    .snapshot
                    .nodes
                    .len()
                    .saturating_mul(std::mem::size_of::<PhysicalNode>()),
            )
            .saturating_add(
                entry
                    .snapshot
                    .links
                    .len()
                    .saturating_mul(std::mem::size_of::<PartialLink>()),
            )
            .saturating_add(rational_payload)
            .saturating_add(witness_key_bytes),
    )
    .unwrap_or(u64::MAX)
}

fn state_fingerprint(topology: &PartialTopology, cancel: &AtomicBool) -> Option<StateFingerprint> {
    let node_types = topology
        .nodes
        .iter()
        .map(|node| (node.id, node.node_type))
        .collect::<BTreeMap<_, _>>();
    let mut node_incidents = topology
        .nodes
        .iter()
        .map(|node| (node.id, Vec::new()))
        .collect::<BTreeMap<NodeId, Vec<FingerprintIncident>>>();
    let mut link_descriptors = Vec::with_capacity(topology.links.len());

    for (index, link) in topology.links.iter().enumerate() {
        if index % 64 == 0 && cancel.load(Ordering::Relaxed) {
            return None;
        }
        let producer = fingerprint_producer_owner(topology, &node_types, link.producer);
        let consumer = fingerprint_consumer_owner(topology, &node_types, link.consumer);
        link_descriptors.push((producer.clone(), consumer.clone(), link.flow.clone()));
        if let ProducerPortRef::Node { node, .. } = link.producer {
            node_incidents
                .get_mut(&node)
                .expect("partial topology link producer must own a materialized node")
                .push(FingerprintIncident::Outgoing(consumer, link.flow.clone()));
        }
        if let ConsumerPortRef::Node { node, .. } = link.consumer {
            node_incidents
                .get_mut(&node)
                .expect("partial topology link consumer must own a materialized node")
                .push(FingerprintIncident::Incoming(producer, link.flow.clone()));
        }
    }
    link_descriptors.sort();
    let mut node_descriptors = topology
        .nodes
        .iter()
        .map(|node| {
            let mut incidents = node_incidents.remove(&node.id).unwrap_or_default();
            incidents.sort();
            (node.node_type, incidents)
        })
        .collect::<Vec<_>>();
    node_descriptors.sort();

    let mut hasher = DefaultHasher::new();
    topology.remaining_profile.hash(&mut hasher);
    topology.discard_count.hash(&mut hasher);
    link_descriptors.hash(&mut hasher);
    node_descriptors.hash(&mut hasher);
    Some(StateFingerprint {
        remaining_profile: topology.remaining_profile,
        link_count: u32::try_from(topology.links.len()).unwrap_or(u32::MAX),
        digest: hasher.finish(),
    })
}

fn fingerprint_producer_owner(
    topology: &PartialTopology,
    node_types: &BTreeMap<NodeId, NodeType>,
    producer: ProducerPortRef,
) -> FingerprintOwner {
    match producer {
        ProducerPortRef::Input(index) => {
            FingerprintOwner::Input(topology.problem.inputs[index.0 as usize].clone())
        }
        ProducerPortRef::Node { node, .. } => FingerprintOwner::Node(node_types[&node]),
    }
}

fn fingerprint_consumer_owner(
    topology: &PartialTopology,
    node_types: &BTreeMap<NodeId, NodeType>,
    consumer: ConsumerPortRef,
) -> FingerprintOwner {
    match consumer {
        ConsumerPortRef::Output(index) => {
            FingerprintOwner::Output(topology.problem.outputs[index.0 as usize].clone())
        }
        ConsumerPortRef::Discard(_) => FingerprintOwner::Discard,
        ConsumerPortRef::Node { node, .. } => FingerprintOwner::Node(node_types[&node]),
    }
}

fn scc_cache_entry_bytes(key: &SccSummaryKey, summary: &CachedOpenSccSummary) -> u64 {
    // This deterministic lower bound includes the minimal signed byte payload
    // of every heap-backed BigInt numerator and denominator. It intentionally
    // excludes allocator bucket/capacity overhead, which is platform- and
    // scheduling-dependent and therefore unsuitable for deterministic stats.
    let rational_payload = summary
        .known_values
        .iter()
        .map(|deduction| rational_payload_bytes(&deduction.value))
        .chain(
            summary
                .homogeneous_ratios
                .iter()
                .map(|deduction| rational_payload_bytes(&deduction.factor)),
        )
        .fold(0_usize, usize::saturating_add);
    u64::try_from(
        std::mem::size_of::<SccSummaryKey>()
            .saturating_add(std::mem::size_of::<CachedOpenSccSummary>())
            .saturating_add(key.as_bytes().len())
            .saturating_add(
                summary
                    .known_values
                    .len()
                    .saturating_mul(std::mem::size_of::<CachedKnownValue>()),
            )
            .saturating_add(
                summary
                    .homogeneous_ratios
                    .len()
                    .saturating_mul(std::mem::size_of::<CachedHomogeneousRatio>()),
            )
            .saturating_add(rational_payload),
    )
    .unwrap_or(u64::MAX)
}

fn rational_payload_bytes(value: &Rational) -> usize {
    signed_payload_bytes(value.numerator())
        .saturating_add(signed_payload_bytes(value.denominator()))
}

fn signed_payload_bytes(value: &num::BigInt) -> usize {
    let bits = value.bits();
    // A negative power of two at a byte boundary already contains its sign bit.
    // Other values need a sign bit; zero still occupies one byte.
    let sign_already_fits = bits.is_multiple_of(8)
        && value.sign() == num::bigint::Sign::Minus
        && value.trailing_zeros() == bits.checked_sub(1);
    usize::try_from(bits / 8 + u64::from(!sign_already_fits)).unwrap_or(usize::MAX)
}

/// Internal failure while exhausting a fixed profile.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ProfileSearchError {
    #[error("profile arithmetic failed: {0}")]
    ProfileArithmetic(String),
    #[error("normalized terminal count does not fit the public u32 index type")]
    TerminalCountOverflow,
    #[error("mutable topology operation failed: {0}")]
    Topology(String),
    #[error("exact propagation failed internally: {0}")]
    Propagation(String),
    #[error("exact SCC analysis failed internally: {0}")]
    Scc(String),
    #[error("a successful topology decision did not identify its appended physical link")]
    MissingAppliedLink,
    #[error("root augmentation partition is invalid for this fixed profile: {0}")]
    InvalidRootPartition(&'static str),
    #[error("a canonical state was re-entered before its owning DFS frame completed")]
    ReentrantCanonicalState,
    #[error("the complete topology violated a structural enumerator invariant: {0}")]
    InvalidCompleteTopology(Box<ValidationError>),
    #[error("the canonical witness failed the independent validation firewall: {0}")]
    ValidationFirewall(Box<ValidationError>),
    #[error(
        "validator count mismatch: expected ({expected_nodes} nodes, {expected_links} optimized links, {expected_physical_links} physical links, {expected_discard_links} discard links), got ({actual_nodes}, {actual_links}, {actual_physical_links}, {actual_discard_links})"
    )]
    ValidationCountMismatch {
        expected_nodes: u32,
        expected_links: u32,
        expected_physical_links: u32,
        expected_discard_links: u32,
        actual_nodes: u32,
        actual_links: u32,
        actual_physical_links: u32,
        actual_discard_links: u32,
    },
}

impl From<ProfileArithmeticError> for ProfileSearchError {
    fn from(error: ProfileArithmeticError) -> Self {
        Self::ProfileArithmetic(error.to_string())
    }
}

/// Proof status for one fixed node-type profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileSearchResult {
    /// The complete canonical topology quotient was exhausted.
    Exhausted {
        best_witness: Option<ProfileWitness>,
        /// Every distinct validated witness when collection was requested.
        witnesses: Vec<ProfileWitness>,
        stats: ProfileSearchStats,
    },
    /// Cancellation stopped the profile before its proof obligation completed.
    Incomplete {
        reason: IncompleteReason,
        best_witness: Option<ProfileWitness>,
        /// Distinct validated witnesses found before cancellation.
        witnesses: Vec<ProfileWitness>,
        stats: ProfileSearchStats,
    },
    /// An internal invariant or independent validation firewall failed.
    Failed {
        error: ProfileSearchError,
        stats: ProfileSearchStats,
    },
}

/// Stable identity of one independently exhaustible root-search obligation.
///
/// Ordinals are assigned after sorting by the canonical marked-child key. They
/// therefore depend only on the normalized problem and fixed profile, not on a
/// worker count, scheduling order, or insertion label.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct RootPartitionId(u32);

impl RootPartitionId {
    #[must_use]
    pub(crate) const fn ordinal(self) -> u32 {
        self.0
    }
}

/// One leaf below the fixed profile in the hierarchical proof ledger.
///
/// The decision is deliberately private. Callers may schedule or identify a
/// partition but cannot manufacture a different subtree for its proof ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RootPartition {
    id: RootPartitionId,
    stable_key: Vec<u8>,
    path: Vec<TopologyDecision>,
}

impl RootPartition {
    #[must_use]
    pub(crate) const fn id(&self) -> RootPartitionId {
        self.id
    }

    /// Borrows the canonical identity used to order root obligations.
    #[must_use]
    pub(crate) fn stable_key(&self) -> &[u8] {
        &self.stable_key
    }
}

/// Planning either yields independently searchable leaves or a profile result
/// already proved before a structural root decision exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RootPartitionPlan {
    Partitions(Vec<RootPartition>),
    Immediate(Box<ProfileSearchResult>),
}

/// Exhaustively searches one fixed physical node profile.
///
/// The input must be the exact canonical problem produced by
/// [`crate::prepare_problem`]. An `Exhausted` result is a complete SAT or UNSAT
/// proof for this profile only. `Incomplete` and `Failed` never carry an
/// exhaustion claim.
#[must_use]
pub fn search_profile(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
) -> ProfileSearchResult {
    search_profile_with_features(problem, profile, cancel, SearchFeatures::PRODUCTION)
}

/// Builds deterministic first-decision leaves for one fixed profile.
///
/// `TopologyState::legal_decisions` contains exactly one representative of
/// each marked-child orbit. Partitioning over every such representative covers
/// the complete primitive search. Canonical state memoization below each root
/// removes equivalent later construction histories.
///
/// A terminal partition represents the whole root only when the root is
/// already complete or has no accepted primitive augmentation. This keeps the
/// degenerate obligation explicit in the proof ledger without overlapping the
/// ordinary first-link leaves.
#[must_use]
#[cfg(test)]
pub(crate) fn plan_profile_root_partitions(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
) -> RootPartitionPlan {
    plan_profile_root_partitions_with_accounting(problem, profile, cancel, None)
}

pub(crate) fn plan_profile_root_partitions_with_accounting(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    accounting: Option<ProfileLinkAccounting>,
) -> RootPartitionPlan {
    let initialized = initialize_profile_search(
        problem,
        profile,
        cancel,
        SearchFeatures::PRODUCTION,
        accounting,
    );
    let InitializedSearch {
        context,
        mut state,
        propagation: _,
        started,
    } = match initialized {
        Ok(initialized) => initialized,
        Err(result) => return RootPartitionPlan::Immediate(result),
    };

    if cancel.load(Ordering::Relaxed) {
        return RootPartitionPlan::Immediate(Box::new(
            context.incomplete(IncompleteReason::Cancelled, started),
        ));
    }
    if state.is_complete() {
        return RootPartitionPlan::Partitions(vec![terminal_root_partition()]);
    }

    let Some(decisions) = state.legal_decisions_cancellable(cancel) else {
        return RootPartitionPlan::Immediate(Box::new(
            context.incomplete(IncompleteReason::Cancelled, started),
        ));
    };
    let mut partitions = Vec::with_capacity(decisions.len());
    for decision in decisions {
        if cancel.load(Ordering::Relaxed) {
            return RootPartitionPlan::Immediate(Box::new(
                context.incomplete(IncompleteReason::Cancelled, started),
            ));
        }
        let checkpoint = state.checkpoint();
        let decision_id = match state.apply_legal_decision(decision) {
            Ok(decision_id) => decision_id,
            Err(error) => {
                state.rollback(checkpoint);
                return RootPartitionPlan::Immediate(Box::new(
                    context.failed(topology_error(&error), started),
                ));
            }
        };
        let Some(link_index) = state.link_index_for(decision_id) else {
            state.rollback(checkpoint);
            return RootPartitionPlan::Immediate(Box::new(
                context.failed(ProfileSearchError::MissingAppliedLink, started),
            ));
        };
        let topology = state.partial_topology();
        let Some(marked_key) = canonicalize_marked_link_cancellable(&topology, link_index, cancel)
        else {
            state.rollback(checkpoint);
            return RootPartitionPlan::Immediate(Box::new(
                context.incomplete(IncompleteReason::Cancelled, started),
            ));
        };
        partitions.push(RootPartition {
            id: RootPartitionId(0),
            stable_key: augmentation_partition_key(&marked_key),
            path: vec![decision],
        });
        state.rollback(checkpoint);
    }

    if partitions.is_empty() {
        return RootPartitionPlan::Partitions(vec![terminal_root_partition()]);
    }
    partitions.sort_by(|left, right| left.stable_key().cmp(right.stable_key()));
    partitions.dedup_by(|left, right| left.stable_key() == right.stable_key());
    for (ordinal, partition) in partitions.iter_mut().enumerate() {
        let Ok(ordinal) = u32::try_from(ordinal) else {
            return RootPartitionPlan::Immediate(Box::new(context.failed(
                ProfileSearchError::InvalidRootPartition("partition count does not fit u32"),
                started,
            )));
        };
        partition.id = RootPartitionId(ordinal);
    }
    RootPartitionPlan::Partitions(partitions)
}

/// Exhausts exactly one canonical root-augmentation subtree.
///
/// Each invocation owns its state cache. An `Exhausted` result therefore
/// discharges this partition and no other one; `Incomplete` or `Failed` must
/// remain visible to the parent proof ledger.
#[must_use]
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn search_profile_root_partition(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    accounting: Option<ProfileLinkAccounting>,
    partition: &RootPartition,
    collect_all_witnesses: bool,
    progress: Option<&(dyn Fn(&SearchInstrumentation) + Sync)>,
) -> ProfileSearchResult {
    search_profile_root_partition_with_execution(
        problem,
        profile,
        cancel,
        accounting,
        partition,
        collect_all_witnesses,
        progress,
        SearchExecution::default(),
    )
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SearchExecution<'a> {
    pub shared: Option<&'a SharedStateCache>,
    pub donations: Option<&'a DonationPool>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn search_profile_root_partition_with_execution<'a>(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &'a AtomicBool,
    accounting: Option<ProfileLinkAccounting>,
    partition: &RootPartition,
    collect_all_witnesses: bool,
    progress: Option<&'a (dyn Fn(&SearchInstrumentation) + Sync)>,
    execution: SearchExecution<'a>,
) -> ProfileSearchResult {
    let activity = |kind| {
        crate::diagnostics::ActivitySpan::start(
            kind,
            profile.node_count(),
            accounting.map(|a| a.link_count),
            Some(profile),
            Some(partition.id().ordinal()),
            1,
        )
    };
    let search_activity = activity("root_search");
    let initialized = initialize_profile_search(
        problem,
        profile,
        cancel,
        SearchFeatures::PRODUCTION,
        accounting,
    );
    let InitializedSearch {
        mut context,
        mut state,
        mut propagation,
        started,
    } = match initialized {
        Ok(initialized) => initialized,
        Err(result) => return *result,
    };
    context.collect_all_witnesses = collect_all_witnesses;
    context.shared = execution.shared;
    context.donations = execution.donations;
    context.progress = progress;

    let result = match replay_prefix(&mut state, &mut propagation, &mut context, partition) {
        Ok(()) => search_state(&mut state, &mut propagation, &mut context),
        Err(result) => result,
    };
    drop(search_activity);
    let output = {
        let _activity = activity("root_finish");
        finish_profile_search(context, started, result)
    };
    {
        let _activity = activity("root_propagation_drop");
        drop(propagation);
    }
    {
        let _activity = activity("root_topology_drop");
        drop(state);
    }
    output
}

fn replay_prefix(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    context: &mut SearchContext<'_>,
    partition: &RootPartition,
) -> Result<(), DfsResult> {
    let mut last_link = None;
    for &decision in &partition.path {
        if context.cancel.load(Ordering::Relaxed) {
            return Err(DfsResult::Incomplete);
        }
        let id = state
            .apply_legal_decision(decision)
            .map_err(|error| DfsResult::Failed(topology_error(&error)))?;
        let link = state
            .link_index_for(id)
            .ok_or(DfsResult::Failed(ProfileSearchError::MissingAppliedLink))?;
        increment(&mut context.stats.instrumentation.raw_structural_decisions);
        prepare_applied_child(state, propagation, context, link)?;
        last_link = Some(link);
    }
    let actual = match (partition.stable_key.first(), last_link) {
        (Some(0), None) => vec![0],
        (Some(1), Some(link)) => {
            canonicalize_marked_link_cancellable(&state.partial_topology(), link, context.cancel)
                .map(|key| augmentation_partition_key(&key))
                .ok_or(DfsResult::Incomplete)?
        }
        (Some(2), Some(_)) => prefix_state_key(state, propagation.as_ref(), context.cancel)?,
        _ => {
            return Err(DfsResult::Failed(ProfileSearchError::InvalidRootPartition(
                "malformed prefix key",
            )));
        }
    };
    if actual != partition.stable_key {
        return Err(DfsResult::Failed(ProfileSearchError::InvalidRootPartition(
            "prefix does not match its key",
        )));
    }
    Ok(())
}

fn prefix_state_key(
    state: &TopologyState,
    propagation: Option<&PropagationState>,
    cancel: &AtomicBool,
) -> Result<Vec<u8>, DfsResult> {
    let snapshot = match propagation {
        Some(p) => p
            .partial_topology_with_known_link_flows(state)
            .map_err(|error| DfsResult::Failed(propagation_error(&error)))?,
        None => state.partial_topology(),
    };
    let key = canonicalize_state_cancellable(&snapshot, cancel).ok_or(DfsResult::Incomplete)?;
    let mut bytes = vec![2];
    bytes.extend_from_slice(key.as_bytes());
    Ok(bytes)
}

fn terminal_root_partition() -> RootPartition {
    RootPartition {
        id: RootPartitionId(0),
        stable_key: vec![0],
        path: Vec::new(),
    }
}

fn augmentation_partition_key(marked_key: &MarkedLinkCanonicalKey) -> Vec<u8> {
    let mut key = Vec::with_capacity(marked_key.as_bytes().len().saturating_add(1));
    key.push(1);
    key.extend_from_slice(marked_key.as_bytes());
    key
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SearchFeatures {
    exact_propagation: bool,
    profile_lower_bounds: Option<ProfileBounds>,
    reachability: bool,
    dynamic_scc: bool,
    scc_cache: SccCacheMode,
    state_cache: StateCacheMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
enum SccCacheMode {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
enum StateCacheMode {
    Deferred,
    Exact,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProfileBounds;

impl SearchFeatures {
    const PRODUCTION: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        reachability: true,
        dynamic_scc: true,
        scc_cache: SccCacheMode::Enabled,
        state_cache: StateCacheMode::Deferred,
    };

    #[cfg(test)]
    const UNOPTIMIZED: Self = Self {
        exact_propagation: false,
        profile_lower_bounds: None,
        reachability: false,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        state_cache: StateCacheMode::Deferred,
    };

    #[cfg(test)]
    const PROPAGATION_ONLY: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: None,
        reachability: false,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        state_cache: StateCacheMode::Deferred,
    };

    #[cfg(test)]
    const REACHABILITY_ONLY: Self = Self {
        exact_propagation: false,
        profile_lower_bounds: None,
        reachability: true,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        state_cache: StateCacheMode::Deferred,
    };

    #[cfg(test)]
    const WITHOUT_SCC: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        reachability: true,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        state_cache: StateCacheMode::Deferred,
    };

    #[cfg(test)]
    const WITHOUT_SCC_CACHE: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        reachability: true,
        dynamic_scc: true,
        scc_cache: SccCacheMode::Disabled,
        state_cache: StateCacheMode::Deferred,
    };

    #[cfg(test)]
    const EXACT_STATE_CACHE: Self = Self {
        state_cache: StateCacheMode::Exact,
        ..Self::PRODUCTION
    };
}

fn search_profile_with_features(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    features: SearchFeatures,
) -> ProfileSearchResult {
    let initialized = initialize_profile_search(problem, profile, cancel, features, None);
    let InitializedSearch {
        mut context,
        mut state,
        mut propagation,
        started,
    } = match initialized {
        Ok(initialized) => initialized,
        Err(result) => return *result,
    };
    let result = search_state(&mut state, &mut propagation, &mut context);
    finish_profile_search(context, started, result)
}

struct InitializedSearch<'a> {
    context: SearchContext<'a>,
    state: TopologyState,
    propagation: Option<PropagationState>,
    started: Instant,
}

fn initialize_profile_search<'a>(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &'a AtomicBool,
    features: SearchFeatures,
    expected_accounting: Option<ProfileLinkAccounting>,
) -> Result<InitializedSearch<'a>, Box<ProfileSearchResult>> {
    let started = Instant::now();
    let mut context = SearchContext::new_with_features(problem, profile, cancel, features);
    if cancel.load(Ordering::Relaxed) {
        return Err(Box::new(
            context.incomplete(IncompleteReason::Cancelled, started),
        ));
    }
    let Ok(input_count) = u32::try_from(problem.inputs.len()) else {
        return Err(Box::new(
            context.failed(ProfileSearchError::TerminalCountOverflow, started),
        ));
    };
    let Ok(output_count) = u32::try_from(problem.outputs.len()) else {
        return Err(Box::new(
            context.failed(ProfileSearchError::TerminalCountOverflow, started),
        ));
    };
    let accounting = match expected_accounting {
        Some(accounting) => accounting,
        None => match profile_link_accounting(
            profile,
            input_count,
            output_count,
            &problem.surplus,
            &problem.max_link_rate,
        ) {
            Ok(Some(accounting)) => accounting,
            Ok(None) => return Err(Box::new(context.exhausted(started))),
            Err(error) => return Err(Box::new(context.failed(error.into(), started))),
        },
    };
    context.expected_link_count = expected_accounting.map(|accounting| accounting.link_count);
    context.expected_physical_link_count = accounting.physical_link_count;
    context.expected_discard_link_count = accounting.discard_link_count;
    if features.profile_lower_bounds.is_some()
        && profile_impossibility(problem, profile, accounting).is_some()
    {
        increment(&mut context.stats.instrumentation.lower_bound_prunes);
        return Err(Box::new(context.exhausted(started)));
    }

    let state =
        match TopologyState::new_with_discards(problem, profile, accounting.discard_link_count) {
            Ok(state) => state,
            Err(error) => return Err(Box::new(context.failed(topology_error(&error), started))),
        };
    let propagation = if features.exact_propagation {
        let propagation_started = Instant::now();
        let propagation = PropagationState::new(&state, &problem.max_link_rate);
        context.record_algebra(propagation_started.elapsed());
        let propagation = match propagation {
            Ok(propagation) => propagation,
            Err(error) => {
                return Err(Box::new(context.failed(propagation_error(&error), started)));
            }
        };
        if let PropagationOutcome::Pruned(conflict) = propagation.current_outcome() {
            context.record_propagation_prune(conflict);
            return Err(Box::new(context.exhausted(started)));
        }
        Some(propagation)
    } else {
        None
    };
    Ok(InitializedSearch {
        context,
        state,
        propagation,
        started,
    })
}

fn finish_profile_search(
    context: SearchContext<'_>,
    started: Instant,
    result: DfsResult,
) -> ProfileSearchResult {
    match result {
        DfsResult::Exhausted(_) => context.exhausted(started),
        DfsResult::Incomplete => context.incomplete(IncompleteReason::Cancelled, started),
        DfsResult::Failed(error) => context.failed(error, started),
    }
}

enum DfsResult {
    Exhausted(Option<CanonicalGraphKey>),
    Incomplete,
    Failed(ProfileSearchError),
}

struct SearchContext<'a> {
    problem: Problem,
    expected_node_count: u32,
    expected_link_count: Option<u32>,
    expected_physical_link_count: u32,
    expected_discard_link_count: u32,
    cancel: &'a AtomicBool,
    cache: HashMap<StateKey, StateStatus>,
    deferred_cache: HashMap<StateFingerprint, DeferredStateBucket>,
    profile: NodeProfile,
    shared: Option<&'a SharedStateCache>,
    donations: Option<&'a DonationPool>,
    scc_cache: HashMap<SccSummaryKey, CachedOpenSccSummary>,
    owned_cache_bytes: u64,
    best_witness: Option<ProfileWitness>,
    witnesses: BTreeMap<CanonicalGraphKey, ProfileWitness>,
    collect_all_witnesses: bool,
    stats: ProfileSearchStats,
    features: SearchFeatures,
    progress: Option<&'a (dyn Fn(&SearchInstrumentation) + Sync)>,
    last_progress_at: Option<Instant>,
}

impl<'a> SearchContext<'a> {
    #[cfg(test)]
    fn new(normalized: &NormalizedProblem, profile: NodeProfile, cancel: &'a AtomicBool) -> Self {
        Self::new_with_features(normalized, profile, cancel, SearchFeatures::PRODUCTION)
    }

    fn new_with_features(
        normalized: &NormalizedProblem,
        profile: NodeProfile,
        cancel: &'a AtomicBool,
        features: SearchFeatures,
    ) -> Self {
        Self {
            problem: Problem {
                inputs: normalized.inputs.as_slice().to_vec(),
                outputs: normalized.outputs.as_slice().to_vec(),
                max_link_rate: normalized.max_link_rate.clone(),
            },
            expected_node_count: checked_node_count(profile).unwrap_or(u32::MAX),
            expected_link_count: None,
            expected_physical_link_count: 0,
            expected_discard_link_count: 0,
            cancel,
            cache: HashMap::new(),
            deferred_cache: HashMap::new(),
            profile,
            shared: None,
            donations: None,
            scc_cache: HashMap::new(),
            owned_cache_bytes: 0,
            best_witness: None,
            witnesses: BTreeMap::new(),
            collect_all_witnesses: false,
            stats: ProfileSearchStats::default(),
            features,
            progress: None,
            last_progress_at: None,
        }
    }

    fn publish_progress(&mut self) {
        if let Some(progress) = self.progress
            && self
                .last_progress_at
                .is_none_or(|last| last.elapsed() >= Duration::from_millis(50))
        {
            progress(&self.stats.instrumentation);
            self.last_progress_at = Some(Instant::now());
        }
    }

    fn record_canonicalization(&mut self, elapsed: Duration) {
        add_duration_ns(
            &mut self.stats.instrumentation.canonicalization_time_ns,
            elapsed,
        );
        self.publish_progress();
    }

    fn record_algebra(&mut self, elapsed: Duration) {
        add_duration_ns(&mut self.stats.instrumentation.algebra_time_ns, elapsed);
        self.publish_progress();
    }

    fn record_propagation_prune(&mut self, conflict: &PropagationConflict) {
        match conflict {
            PropagationConflict::SparseInconsistency
            | PropagationConflict::ExactConstraintContradiction => {
                increment(&mut self.stats.instrumentation.propagation_contradictions);
            }
            PropagationConflict::NonPositiveKnown { .. }
            | PropagationConflict::CapacityExceeded { .. }
            | PropagationConflict::NegativeRatio { .. } => {
                increment(&mut self.stats.instrumentation.capacity_prunes);
            }
        }
    }

    fn retain_witness(&mut self, candidate: ProfileWitness) {
        if let Some(shared) = self.shared {
            shared.retain(self.profile, &candidate);
        }
        if self.collect_all_witnesses {
            self.witnesses
                .entry(candidate.canonical_graph_key.clone())
                .or_insert_with(|| candidate.clone());
        }
        if self
            .best_witness
            .as_ref()
            .is_none_or(|current| candidate.canonical_graph_key < current.canonical_graph_key)
        {
            self.best_witness = Some(candidate);
        }
    }

    /// Inserts or replaces one state-cache entry while maintaining a
    /// deterministic lower-bound measurement of solver-owned cache memory.
    /// The measurement counts key byte buffers and fixed entry payloads. It
    /// deliberately excludes allocator buckets and process-global memory,
    /// whose sizes can depend on randomized hashing or unrelated threads.
    fn insert_state_status(&mut self, key: StateKey, status: StateStatus) {
        if !matches!(status, StateStatus::InProgress)
            && let Some(shared) = self.shared
        {
            shared.insert(self.expected_link_count, key.clone(), status.clone());
        }
        let old_bytes = self
            .cache
            .get(&key)
            .map_or(0, |old| state_cache_entry_bytes(&key, old));
        let new_bytes = state_cache_entry_bytes(&key, &status);
        self.cache.insert(key, status);
        self.replace_owned_cache_bytes(old_bytes, new_bytes);
    }

    fn insert_deferred_state(
        &mut self,
        fingerprint: StateFingerprint,
        snapshot: DeferredSnapshot,
        status: StateStatus,
    ) {
        let entry = DeferredStateEntry { snapshot, status };
        let new_bytes = deferred_state_entry_bytes(&entry);
        let old_bytes = self
            .deferred_cache
            .insert(fingerprint, DeferredStateBucket::Unique(entry))
            .and_then(|bucket| match bucket {
                DeferredStateBucket::Unique(entry) => Some(deferred_state_entry_bytes(&entry)),
                DeferredStateBucket::Canonicalized => None,
            })
            .unwrap_or(0);
        self.replace_owned_cache_bytes(old_bytes, new_bytes);
    }

    fn promote_deferred_state(
        &mut self,
        fingerprint: StateFingerprint,
    ) -> Option<DeferredStateEntry> {
        let previous = self
            .deferred_cache
            .insert(fingerprint, DeferredStateBucket::Canonicalized)?;
        match previous {
            DeferredStateBucket::Unique(entry) => {
                let removed = deferred_state_entry_bytes(&entry);
                self.owned_cache_bytes = self.owned_cache_bytes.saturating_sub(removed);
                Some(entry)
            }
            DeferredStateBucket::Canonicalized => None,
        }
    }

    fn update_cache_status(&mut self, token: StateCacheToken, status: StateStatus) {
        match token {
            StateCacheToken::Exact(key) => self.insert_state_status(key, status),
            StateCacheToken::Deferred(fingerprint) => {
                let Some(DeferredStateBucket::Unique(entry)) =
                    self.deferred_cache.get_mut(&fingerprint)
                else {
                    debug_assert!(false, "live deferred state was promoted unexpectedly");
                    return;
                };
                let old_bytes = deferred_state_entry_bytes(entry);
                entry.status = status;
                let new_bytes = deferred_state_entry_bytes(entry);
                self.replace_owned_cache_bytes(old_bytes, new_bytes);
            }
        }
    }

    fn remove_cache_status(&mut self, token: &StateCacheToken) {
        match token {
            StateCacheToken::Exact(key) => self.remove_state_status(key),
            StateCacheToken::Deferred(fingerprint) => {
                if let Some(DeferredStateBucket::Unique(entry)) =
                    self.deferred_cache.remove(fingerprint)
                {
                    let removed = deferred_state_entry_bytes(&entry);
                    self.owned_cache_bytes = self.owned_cache_bytes.saturating_sub(removed);
                }
            }
        }
    }

    fn state_cache_len(&self) -> usize {
        self.cache.len()
            + self
                .deferred_cache
                .values()
                .filter(|bucket| matches!(bucket, DeferredStateBucket::Unique(_)))
                .count()
    }

    fn remove_state_status(&mut self, key: &StateKey) {
        if let Some((owned_key, status)) = self.cache.remove_entry(key) {
            let removed = state_cache_entry_bytes(&owned_key, &status);
            self.owned_cache_bytes = self.owned_cache_bytes.saturating_sub(removed);
        }
    }

    fn insert_scc_summary(&mut self, key: SccSummaryKey, summary: CachedOpenSccSummary) {
        let old_bytes = self
            .scc_cache
            .get(&key)
            .map_or(0, |old| scc_cache_entry_bytes(&key, old));
        let new_bytes = scc_cache_entry_bytes(&key, &summary);
        self.scc_cache.insert(key, summary);
        self.replace_owned_cache_bytes(old_bytes, new_bytes);
    }

    fn replace_owned_cache_bytes(&mut self, old_bytes: u64, new_bytes: u64) {
        self.owned_cache_bytes = self
            .owned_cache_bytes
            .saturating_sub(old_bytes)
            .saturating_add(new_bytes);
        self.stats.instrumentation.peak_memory_bytes = self
            .stats
            .instrumentation
            .peak_memory_bytes
            .max(self.owned_cache_bytes);
    }

    fn finish_stats_and_release_caches(&mut self, started: Instant) {
        self.stats.instrumentation.wall_time_ms =
            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        {
            let _activity = crate::diagnostics::ActivitySpan::start(
                "state_cache_drop",
                self.expected_node_count,
                self.expected_link_count,
                Some(self.profile),
                None,
                1,
            );
            drop(std::mem::take(&mut self.cache));
        }
        {
            let _activity = crate::diagnostics::ActivitySpan::start(
                "deferred_state_cache_drop",
                self.expected_node_count,
                self.expected_link_count,
                Some(self.profile),
                None,
                1,
            );
            drop(std::mem::take(&mut self.deferred_cache));
        }
        {
            let _activity = crate::diagnostics::ActivitySpan::start(
                "scc_cache_drop",
                self.expected_node_count,
                self.expected_link_count,
                Some(self.profile),
                None,
                1,
            );
            drop(std::mem::take(&mut self.scc_cache));
        }
    }

    fn exhausted(mut self, started: Instant) -> ProfileSearchResult {
        self.finish_stats_and_release_caches(started);
        ProfileSearchResult::Exhausted {
            best_witness: self.best_witness,
            witnesses: self.witnesses.into_values().collect(),
            stats: self.stats,
        }
    }

    fn incomplete(mut self, reason: IncompleteReason, started: Instant) -> ProfileSearchResult {
        self.finish_stats_and_release_caches(started);
        ProfileSearchResult::Incomplete {
            reason,
            best_witness: self.best_witness,
            witnesses: self.witnesses.into_values().collect(),
            stats: self.stats,
        }
    }

    fn failed(mut self, error: ProfileSearchError, started: Instant) -> ProfileSearchResult {
        self.finish_stats_and_release_caches(started);
        ProfileSearchResult::Failed {
            error,
            stats: self.stats,
        }
    }
}

// Keep cache ownership, terminal handling, and physical decisions together so
// every exit visibly discharges or removes the in-progress state.
#[allow(clippy::too_many_lines)]
fn search_state(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    context: &mut SearchContext<'_>,
) -> DfsResult {
    context.publish_progress();
    if context.cancel.load(Ordering::Relaxed) {
        return DfsResult::Incomplete;
    }

    let snapshot_started = Instant::now();
    let snapshot = match propagation {
        Some(propagation) => match propagation.partial_topology_with_known_link_flows(state) {
            Ok(snapshot) => snapshot,
            Err(error) => return DfsResult::Failed(propagation_error(&error)),
        },
        None => state.partial_topology(),
    };
    hotspot_profile::record_snapshot(snapshot_started.elapsed());
    if context
        .expected_link_count
        .is_some_and(|expected| operator_link_count(&snapshot) > expected)
    {
        increment(&mut context.stats.instrumentation.lower_bound_prunes);
        return DfsResult::Exhausted(None);
    }
    let defer = context.features.state_cache == StateCacheMode::Deferred
        && context.shared.is_none()
        && context.donations.is_none();
    let (state_token, open_port_order, status) = if defer {
        let Some(fingerprint) = state_fingerprint(&snapshot, context.cancel) else {
            return DfsResult::Incomplete;
        };
        match context.deferred_cache.get(&fingerprint) {
            None => {
                increment(&mut context.stats.instrumentation.deferred_state_visits);
                context.insert_deferred_state(
                    fingerprint,
                    DeferredSnapshot::from_partial(snapshot),
                    StateStatus::InProgress,
                );
                (
                    StateCacheToken::Deferred(fingerprint),
                    StateOpenPortOrder::Raw,
                    None,
                )
            }
            Some(DeferredStateBucket::Unique(_)) => {
                increment(&mut context.stats.instrumentation.deferred_state_promotions);
                let prior = context
                    .promote_deferred_state(fingerprint)
                    .expect("unique deferred bucket must contain its first state");
                let prior_snapshot = prior.snapshot.restore(&context.problem);
                let Some(prior_key) = canonicalize_search_state(&prior_snapshot, context) else {
                    return DfsResult::Incomplete;
                };
                context.insert_state_status(prior_key, prior.status);
                let Some(canonical) = canonicalize_search_state_with_open_ports(&snapshot, context)
                else {
                    return DfsResult::Incomplete;
                };
                let status = state_status(context, &canonical.key);
                (
                    StateCacheToken::Exact(canonical.key),
                    StateOpenPortOrder::Canonical(canonical.open_port_coordinates),
                    status,
                )
            }
            Some(DeferredStateBucket::Canonicalized) => {
                let Some(canonical) = canonicalize_search_state_with_open_ports(&snapshot, context)
                else {
                    return DfsResult::Incomplete;
                };
                let status = state_status(context, &canonical.key);
                (
                    StateCacheToken::Exact(canonical.key),
                    StateOpenPortOrder::Canonical(canonical.open_port_coordinates),
                    status,
                )
            }
        }
    } else {
        let Some(canonical) = canonicalize_search_state_with_open_ports(&snapshot, context) else {
            return DfsResult::Incomplete;
        };
        let status = state_status(context, &canonical.key);
        (
            StateCacheToken::Exact(canonical.key),
            StateOpenPortOrder::Canonical(canonical.open_port_coordinates),
            status,
        )
    };

    // Exact keys remain authoritative whenever a cheap invariant bucket repeats.
    // A first visit uses raw traversal order but still enumerates every decision.
    // Cache equality is never inferred from the fingerprint alone.
    if let Some(status) = status {
        if let StateStatus::SatWitness(key) = &status
            && let Some(shared) = context.shared
        {
            let Some(witness) = shared.witness(context.profile, key) else {
                return DfsResult::Failed(ProfileSearchError::InvalidRootPartition(
                    "cached SAT witness missing from registry",
                ));
            };
            context.retain_witness(witness);
        }
        increment(&mut context.stats.state_cache_hits);
        increment(
            &mut context
                .stats
                .instrumentation
                .canonical_duplicates_eliminated,
        );
        return match status {
            StateStatus::InProgress => {
                DfsResult::Failed(ProfileSearchError::ReentrantCanonicalState)
            }
            StateStatus::ProvenDead | StateStatus::Exhausted => DfsResult::Exhausted(None),
            StateStatus::SatWitness(key) => DfsResult::Exhausted(Some(key)),
        };
    }

    if let StateCacheToken::Exact(key) = &state_token {
        context.insert_state_status(key.clone(), StateStatus::InProgress);
    }
    increment(&mut context.stats.instrumentation.canonical_states_retained);
    context.stats.instrumentation.peak_state_cache_size = context
        .stats
        .instrumentation
        .peak_state_cache_size
        .max(u64::try_from(context.state_cache_len()).unwrap_or(u64::MAX));

    if state.is_complete() {
        return evaluate_complete_state(state, context, state_token);
    }

    let orbit_started = Instant::now();
    let selected = match &open_port_order {
        StateOpenPortOrder::Canonical(coordinates) => {
            state.selected_dfs_open_port_cancellable(coordinates, context.cancel)
        }
        StateOpenPortOrder::Raw => state.selected_raw_dfs_open_port_cancellable(context.cancel),
    };
    let Some(open_port) = selected else {
        hotspot_profile::record_legal_decisions(orbit_started.elapsed());
        context.remove_cache_status(&state_token);
        return DfsResult::Incomplete;
    };
    let Some(open_port) = open_port else {
        hotspot_profile::record_legal_decisions(orbit_started.elapsed());
        // No open port means no future structural decision can attach a remaining
        // node or occupy a missing mandatory port. Since this state is not
        // complete, its completion set is empty.
        context.update_cache_status(state_token, StateStatus::ProvenDead);
        return DfsResult::Exhausted(None);
    };
    let Some(decisions) = ({
        let decisions = state.dfs_decisions_for_open_port_cancellable(open_port, context.cancel);
        hotspot_profile::record_legal_decisions(orbit_started.elapsed());
        decisions
    }) else {
        context.remove_cache_status(&state_token);
        return DfsResult::Incomplete;
    };

    let mut decisions = decisions;
    let attempted_transition = !decisions.is_empty();
    let joins = context
        .donations
        .map(|pool| pool.donate(state, propagation.as_ref(), context, &mut decisions))
        .unwrap_or_default();
    let mut result = DfsResult::Exhausted(None);
    for decision in decisions {
        if context.cancel.load(Ordering::Relaxed) {
            donation::fold_result(&mut result, DfsResult::Incomplete);
            break;
        }
        increment(&mut context.stats.instrumentation.raw_structural_decisions);
        context.publish_progress();
        donation::fold_result(
            &mut result,
            search_decision(state, propagation, context, decision),
        );
        if !matches!(result, DfsResult::Exhausted(_)) {
            break;
        }
    }
    if let Some(pool) = context.donations {
        pool.join(joins, context, &mut result);
    }
    match &result {
        DfsResult::Exhausted(best_key) => {
            let status = best_key.as_ref().map_or_else(
                || {
                    if attempted_transition {
                        StateStatus::Exhausted
                    } else {
                        StateStatus::ProvenDead
                    }
                },
                |key| StateStatus::SatWitness(key.clone()),
            );
            context.update_cache_status(state_token, status);
        }
        DfsResult::Incomplete | DfsResult::Failed(_) => context.remove_cache_status(&state_token),
    }
    result
}

fn canonicalize_search_state(
    snapshot: &PartialTopology,
    context: &mut SearchContext<'_>,
) -> Option<StateKey> {
    let started = Instant::now();
    let key = canonicalize_state_cancellable(snapshot, context.cancel)?;
    let elapsed = started.elapsed();
    context.record_canonicalization(elapsed);
    hotspot_profile::record_state_canonicalize(elapsed);
    Some(key)
}

fn canonicalize_search_state_with_open_ports(
    snapshot: &PartialTopology,
    context: &mut SearchContext<'_>,
) -> Option<crate::canonical::CanonicalStateAndOpenPorts> {
    let started = Instant::now();
    let canonical = canonicalize_state_and_open_ports_cancellable(snapshot, context.cancel)?;
    let elapsed = started.elapsed();
    context.record_canonicalization(elapsed);
    hotspot_profile::record_state_canonicalize(elapsed);
    Some(canonical)
}

fn state_status(context: &mut SearchContext<'_>, key: &StateKey) -> Option<StateStatus> {
    context.cache.get(key).cloned().or_else(|| {
        let hit = context.shared?.lookup(context.expected_link_count, key);
        if hit.is_some() {
            increment(&mut context.stats.instrumentation.shared_cache_hits);
        }
        hit
    })
}

fn operator_link_count(topology: &PartialTopology) -> u32 {
    u32::try_from(
        topology
            .links
            .iter()
            .filter(|link| {
                matches!(link.producer, solver_api::ProducerPortRef::Node { .. })
                    && matches!(link.consumer, solver_api::ConsumerPortRef::Node { .. })
            })
            .count(),
    )
    .unwrap_or(u32::MAX)
}

fn search_decision(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    context: &mut SearchContext<'_>,
    decision: TopologyDecision,
) -> DfsResult {
    let topology_checkpoint = state.checkpoint();
    let propagation_checkpoint = propagation.as_ref().map(PropagationState::checkpoint);
    let apply_started = Instant::now();
    let decision_id = match state.apply_legal_decision(decision) {
        Ok(decision_id) => decision_id,
        Err(error) => {
            hotspot_profile::record_apply_decision(apply_started.elapsed());
            rollback_branch(
                state,
                propagation,
                topology_checkpoint,
                propagation_checkpoint,
            );
            return DfsResult::Failed(topology_error(&error));
        }
    };
    hotspot_profile::record_apply_decision(apply_started.elapsed());
    let Some(link_index) = state.link_index_for(decision_id) else {
        rollback_branch(
            state,
            propagation,
            topology_checkpoint,
            propagation_checkpoint,
        );
        return DfsResult::Failed(ProfileSearchError::MissingAppliedLink);
    };

    let result = evaluate_applied_child(state, propagation, context, link_index);
    rollback_branch(
        state,
        propagation,
        topology_checkpoint,
        propagation_checkpoint,
    );
    result
}

fn evaluate_applied_child(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    context: &mut SearchContext<'_>,
    link_index: usize,
) -> DfsResult {
    if let Err(result) = prepare_applied_child(state, propagation, context, link_index) {
        return result;
    }
    search_state(state, propagation, context)
}

fn prepare_applied_child(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    context: &mut SearchContext<'_>,
    link_index: usize,
) -> Result<(), DfsResult> {
    if let Some(propagation) = propagation.as_mut() {
        let propagation_started = Instant::now();
        let outcome = propagation.synchronize_after_topology_mutation(state);
        let elapsed = propagation_started.elapsed();
        context.record_algebra(elapsed);
        hotspot_profile::record_propagation_sync(elapsed);
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => return Err(DfsResult::Failed(propagation_error(&error))),
        };
        if let PropagationOutcome::Pruned(conflict) = outcome {
            context.record_propagation_prune(&conflict);
            return Err(DfsResult::Exhausted(None));
        }
    }

    if context.features.dynamic_scc {
        match analyze_affected_dynamic_sccs(state, propagation.as_mut(), context, Some(link_index))
        {
            Ok(DynamicSccVerdict::Open) => {}
            Ok(DynamicSccVerdict::ProvenDead) => return Err(DfsResult::Exhausted(None)),
            Ok(DynamicSccVerdict::Cancelled) => return Err(DfsResult::Incomplete),
            Err(error) => return Err(DfsResult::Failed(error)),
        }
    }

    // Full analysis always returns Open while profile nodes remain unmaterialized.
    if context.features.reachability && state.remaining_profile().is_empty() {
        let reachability_started = Instant::now();
        let dead = is_proven_unreachable(state);
        hotspot_profile::record_reachability(reachability_started.elapsed());
        if dead {
            increment(&mut context.stats.instrumentation.lower_bound_prunes);
            return Err(DfsResult::Exhausted(None));
        }
    }

    // Canonical state memoization below removes equivalent construction
    // histories directly.
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DynamicSccVerdict {
    Open,
    ProvenDead,
    Cancelled,
}

fn analyze_affected_dynamic_sccs(
    state: &TopologyState,
    propagation: Option<&mut PropagationState>,
    context: &mut SearchContext<'_>,
    link_index: Option<usize>,
) -> Result<DynamicSccVerdict, ProfileSearchError> {
    analyze_affected_dynamic_sccs_inner(state, propagation, context, link_index)
}

fn analyze_affected_dynamic_sccs_inner(
    state: &TopologyState,
    mut propagation: Option<&mut PropagationState>,
    context: &mut SearchContext<'_>,
    link_index: Option<usize>,
) -> Result<DynamicSccVerdict, ProfileSearchError> {
    let mut affected_link = link_index;
    loop {
        let known = propagated_known_values(state, propagation.as_deref())?;
        let affected =
            detect_affected_sccs(state, affected_link).map_err(|error| scc_error(&error))?;
        let mut added_fact = false;
        for region in affected.affected_regions().filter(|region| region.cyclic) {
            let canonical_started = Instant::now();
            let Some(coordinates) =
                canonical_scc_summary_coordinates(state, &region.nodes, &known, context.cancel)?
            else {
                return Ok(DynamicSccVerdict::Cancelled);
            };
            let elapsed = canonical_started.elapsed();
            context.record_canonicalization(elapsed);
            hotspot_profile::record_scc_canonicalize(elapsed);

            // Soundness: the key canonically encodes the full partial-state
            // equations, selected region, and every supplied exact known port
            // value. Deductions are stored in the *same winning joint port
            // labeling*, so a hit translates exact logical consequences across
            // an isomorphism rather than reusing topology-local identifiers.
            let cached = if context.features.scc_cache == SccCacheMode::Enabled
                && context.scc_cache.contains_key(&coordinates.key)
            {
                increment(&mut context.stats.instrumentation.scc_cache_hits);
                context
                    .scc_cache
                    .get(&coordinates.key)
                    .expect("the cache key was just observed")
                    .clone()
            } else {
                increment(&mut context.stats.instrumentation.scc_solves);
                let algebra_started = Instant::now();
                let summary =
                    summarize_open_scc(state, region, &known).map_err(|error| scc_error(&error));
                let elapsed = algebra_started.elapsed();
                context.record_algebra(elapsed);
                hotspot_profile::record_scc_algebra(elapsed);
                let summary = summary?;
                let cached = cache_open_scc_summary(&summary.algebra, &coordinates)?;
                if context.features.scc_cache == SccCacheMode::Enabled {
                    context.insert_scc_summary(coordinates.key.clone(), cached.clone());
                }
                cached
            };

            // Every equation in an open summary belongs to an already
            // materialized node or physical link. Future decisions can add
            // equations but cannot remove these, so an exact inconsistency is
            // persistent. Rank deficiency remains descriptive: later boundary
            // links may make a larger graph-level SCC unique.
            if cached.consistency == Consistency::Inconsistent {
                increment(&mut context.stats.instrumentation.propagation_contradictions);
                return Ok(DynamicSccVerdict::ProvenDead);
            }

            let Some(propagation) = propagation.as_deref_mut() else {
                continue;
            };
            let (known_values, ratios) = materialize_cached_scc_deductions(&cached, &coordinates)?;
            let propagation_started = Instant::now();
            let update = propagation
                .promote_open_scc_deductions(&known_values, &ratios)
                .map_err(|error| propagation_error(&error))?;
            let elapsed = propagation_started.elapsed();
            context.record_algebra(elapsed);
            hotspot_profile::record_propagation_sync(elapsed);
            if let PropagationOutcome::Pruned(conflict) = update.outcome {
                context.record_propagation_prune(&conflict);
                return Ok(DynamicSccVerdict::ProvenDead);
            }
            if update.changed {
                added_fact = true;
                break;
            }
        }

        // An SCC consequence can make another SCC value exact or expose a
        // physical capacity contradiction. Restarting with the enlarged known
        // map computes the least fixed point of ordinary propagation and every
        // affected open-SCC summary before branching.
        if !added_fact {
            return Ok(DynamicSccVerdict::Open);
        }
        // The structural mutation initially limits rebuilding to its changed
        // region. Once a derived fact crosses an SCC boundary, any existing
        // cycle can become algebraically affected, so the remaining fixed-point
        // passes conservatively revisit the full current partition.
        affected_link = None;
    }
}

struct SccSummaryCoordinates {
    key: SccSummaryKey,
    raw_to_canonical: BTreeMap<FlowVarId, CanonicalFlowEndpoint>,
    canonical_to_raw: BTreeMap<CanonicalFlowEndpoint, FlowVarId>,
}

fn canonical_scc_summary_coordinates(
    state: &TopologyState,
    region_nodes: &[solver_api::NodeId],
    known: &BTreeMap<FlowVarId, Rational>,
    cancel: &AtomicBool,
) -> Result<Option<SccSummaryCoordinates>, ProfileSearchError> {
    let known_producers = state
        .producer_ports()
        .iter()
        .filter_map(|(&reference, port)| {
            known
                .get(&port.flow_var)
                .cloned()
                .map(|value| (reference, value))
        })
        .collect::<BTreeMap<_, _>>();
    let known_consumers = state
        .consumer_ports()
        .iter()
        .filter_map(|(&reference, port)| {
            known
                .get(&port.flow_var)
                .cloned()
                .map(|value| (reference, value))
        })
        .collect::<BTreeMap<_, _>>();
    let region_nodes = region_nodes.iter().copied().collect::<BTreeSet<_>>();
    let Some(canonical) = canonicalize_scc_summary_input_with_relabeling_cancellable(
        &state.partial_topology(),
        &region_nodes,
        &known_producers,
        &known_consumers,
        cancel,
    ) else {
        return Ok(None);
    };
    let mut raw_to_canonical = BTreeMap::new();
    let mut canonical_to_raw = BTreeMap::new();
    for (&reference, port) in state.producer_ports() {
        let canonical_reference = canonical
            .producer_relabeling
            .get(&reference)
            .copied()
            .ok_or(ProfileSearchError::Scc(
                "canonical SCC labeling omitted a declared producer port".to_owned(),
            ))?;
        insert_scc_coordinate(
            &mut raw_to_canonical,
            &mut canonical_to_raw,
            port.flow_var,
            CanonicalFlowEndpoint::Producer(canonical_reference),
        )?;
    }
    for (&reference, port) in state.consumer_ports() {
        let canonical_reference = canonical
            .consumer_relabeling
            .get(&reference)
            .copied()
            .ok_or(ProfileSearchError::Scc(
                "canonical SCC labeling omitted a declared consumer port".to_owned(),
            ))?;
        insert_scc_coordinate(
            &mut raw_to_canonical,
            &mut canonical_to_raw,
            port.flow_var,
            CanonicalFlowEndpoint::Consumer(canonical_reference),
        )?;
    }
    Ok(Some(SccSummaryCoordinates {
        key: canonical.key,
        raw_to_canonical,
        canonical_to_raw,
    }))
}

fn insert_scc_coordinate(
    raw_to_canonical: &mut BTreeMap<FlowVarId, CanonicalFlowEndpoint>,
    canonical_to_raw: &mut BTreeMap<CanonicalFlowEndpoint, FlowVarId>,
    raw: FlowVarId,
    canonical: CanonicalFlowEndpoint,
) -> Result<(), ProfileSearchError> {
    if raw_to_canonical.insert(raw, canonical).is_some()
        || canonical_to_raw.insert(canonical, raw).is_some()
    {
        return Err(ProfileSearchError::Scc(
            "canonical SCC port relabeling was not bijective".to_owned(),
        ));
    }
    Ok(())
}

fn cache_open_scc_summary(
    analysis: &crate::algebra::sparse::SparseAnalysis,
    coordinates: &SccSummaryCoordinates,
) -> Result<CachedOpenSccSummary, ProfileSearchError> {
    let mut known_values = analysis
        .known_values
        .iter()
        .map(|deduction| {
            Ok(CachedKnownValue {
                variable: canonical_scc_variable(coordinates, deduction.variable)?,
                value: Rational::from(deduction.value.clone()),
            })
        })
        .collect::<Result<Vec<_>, ProfileSearchError>>()?;
    known_values
        .sort_by(|left, right| (&left.variable, &left.value).cmp(&(&right.variable, &right.value)));
    let mut homogeneous_ratios = analysis
        .homogeneous_ratios
        .iter()
        .map(|deduction| {
            Ok(CachedHomogeneousRatio {
                lhs: canonical_scc_variable(coordinates, deduction.lhs)?,
                rhs: canonical_scc_variable(coordinates, deduction.rhs)?,
                factor: Rational::from(deduction.factor.clone()),
            })
        })
        .collect::<Result<Vec<_>, ProfileSearchError>>()?;
    homogeneous_ratios.sort_by(|left, right| {
        (&left.lhs, &left.rhs, &left.factor).cmp(&(&right.lhs, &right.rhs, &right.factor))
    });
    Ok(CachedOpenSccSummary {
        consistency: analysis.consistency,
        coefficient_rank: analysis.coefficient_rank,
        augmented_rank: analysis.augmented_rank,
        variable_count: analysis.variable_count,
        known_values,
        homogeneous_ratios,
    })
}

fn canonical_scc_variable(
    coordinates: &SccSummaryCoordinates,
    variable: FlowVarId,
) -> Result<CanonicalFlowEndpoint, ProfileSearchError> {
    coordinates
        .raw_to_canonical
        .get(&variable)
        .copied()
        .ok_or_else(|| {
            ProfileSearchError::Scc(
                "open-SCC deduction referred to an undeclared flow variable".to_owned(),
            )
        })
}

type MaterializedKnownValues = Vec<(FlowVarId, Rational)>;
type MaterializedRatios = Vec<(FlowVarId, FlowVarId, Rational)>;

fn materialize_cached_scc_deductions(
    cached: &CachedOpenSccSummary,
    coordinates: &SccSummaryCoordinates,
) -> Result<(MaterializedKnownValues, MaterializedRatios), ProfileSearchError> {
    let known_values = cached
        .known_values
        .iter()
        .map(|deduction| {
            Ok((
                raw_scc_variable(coordinates, deduction.variable)?,
                deduction.value.clone(),
            ))
        })
        .collect::<Result<Vec<_>, ProfileSearchError>>()?;
    let ratios = cached
        .homogeneous_ratios
        .iter()
        .map(|deduction| {
            Ok((
                raw_scc_variable(coordinates, deduction.lhs)?,
                raw_scc_variable(coordinates, deduction.rhs)?,
                deduction.factor.clone(),
            ))
        })
        .collect::<Result<Vec<_>, ProfileSearchError>>()?;
    Ok((known_values, ratios))
}

fn raw_scc_variable(
    coordinates: &SccSummaryCoordinates,
    variable: CanonicalFlowEndpoint,
) -> Result<FlowVarId, ProfileSearchError> {
    coordinates
        .canonical_to_raw
        .get(&variable)
        .copied()
        .ok_or_else(|| {
            ProfileSearchError::Scc(
                "cached SCC deduction has no port in the isomorphic state".to_owned(),
            )
        })
}

fn propagated_known_values(
    state: &TopologyState,
    propagation: Option<&PropagationState>,
) -> Result<BTreeMap<FlowVarId, Rational>, ProfileSearchError> {
    let mut known = BTreeMap::new();
    let Some(propagation) = propagation else {
        return Ok(known);
    };
    for port in state
        .producer_ports()
        .values()
        .chain(state.consumer_ports().values())
    {
        if let Some(value) = propagation
            .known_value(port.flow_var)
            .map_err(|error| propagation_error(&error))?
        {
            known.insert(port.flow_var, value);
        }
    }
    Ok(known)
}

fn rollback_branch(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    topology_checkpoint: crate::topology::Checkpoint,
    propagation_checkpoint: Option<PropagationCheckpoint>,
) {
    if let Some(checkpoint) = propagation_checkpoint {
        propagation
            .as_mut()
            .expect("a propagation checkpoint has a paired propagation state")
            .rollback(checkpoint);
    }
    state.rollback(topology_checkpoint);
}

// Keep solving, exact-L rejection, canonicalization and the validation firewall
// in their execution order so their different failure semantics remain visible.
#[allow(clippy::too_many_lines)]
fn evaluate_complete_state(
    state: &TopologyState,
    context: &mut SearchContext<'_>,
    state_token: StateCacheToken,
) -> DfsResult {
    let complete_started = Instant::now();
    increment(&mut context.stats.complete_topologies);
    let partial = state.partial_topology();
    let topology = PhysicalGraph {
        nodes: partial.nodes,
        links: partial
            .links
            .into_iter()
            .map(|link| PhysicalLink {
                producer: link.producer,
                consumer: link.consumer,
                flow: Rational::from(0),
            })
            .collect(),
    };

    let algebra_started = Instant::now();
    let solved = solve_topology(&context.problem, &topology);
    context.record_algebra(algebra_started.elapsed());
    let solved = match solved {
        Ok(solved) => solved,
        Err(error) if is_candidate_rejection(&error) => {
            increment(&mut context.stats.rejected_complete_topologies);
            context.update_cache_status(state_token, StateStatus::ProvenDead);
            hotspot_profile::record_evaluate_complete(complete_started.elapsed());
            return DfsResult::Exhausted(None);
        }
        Err(error) => {
            context.remove_cache_status(&state_token);
            hotspot_profile::record_evaluate_complete(complete_started.elapsed());
            return DfsResult::Failed(ProfileSearchError::InvalidCompleteTopology(Box::new(error)));
        }
    };

    // Witness labeling only renames endpoints. Reject an out-of-group topology
    // before exhaustive byte minimization; accepted witnesses still pass the
    // independent validator and all accounting checks below.
    if context.expected_link_count.is_some_and(|expected| {
        solved
            .links
            .iter()
            .filter(|link| {
                matches!(link.producer, solver_api::ProducerPortRef::Node { .. })
                    && matches!(link.consumer, solver_api::ConsumerPortRef::Node { .. })
            })
            .count()
            != expected as usize
    }) {
        increment(&mut context.stats.rejected_complete_topologies);
        context.update_cache_status(state_token, StateStatus::ProvenDead);
        hotspot_profile::record_evaluate_complete(complete_started.elapsed());
        return DfsResult::Exhausted(None);
    }

    let canonical_started = Instant::now();
    let Some(canonical) =
        canonicalize_witness_cancellable(&context.problem, &solved, context.cancel)
    else {
        context.remove_cache_status(&state_token);
        hotspot_profile::record_evaluate_complete(complete_started.elapsed());
        return DfsResult::Incomplete;
    };
    context.record_canonicalization(canonical_started.elapsed());
    let validation_started = Instant::now();
    let validation_activity = crate::diagnostics::ActivitySpan::start(
        "witness_validate",
        context.expected_node_count,
        context.expected_link_count,
        Some(context.profile),
        None,
        1,
    );
    let validation = validate_solution(&context.problem, &canonical.graph);
    drop(validation_activity);
    context.record_algebra(validation_started.elapsed());
    let validation = match validation {
        Ok(validation) => validation,
        Err(error) => {
            context.remove_cache_status(&state_token);
            hotspot_profile::record_evaluate_complete(complete_started.elapsed());
            return DfsResult::Failed(ProfileSearchError::ValidationFirewall(Box::new(error)));
        }
    };
    if validation.cyclic_scc_count != 0 {
        increment(&mut context.stats.validated_cyclic_topologies);
    }
    if context
        .expected_link_count
        .is_some_and(|expected| validation.link_count != expected)
    {
        context.update_cache_status(state_token, StateStatus::ProvenDead);
        hotspot_profile::record_evaluate_complete(complete_started.elapsed());
        return DfsResult::Exhausted(None);
    }
    if validation.node_count != context.expected_node_count
        || validation.physical_link_count != context.expected_physical_link_count
        || validation.discard_link_count != context.expected_discard_link_count
    {
        context.remove_cache_status(&state_token);
        hotspot_profile::record_evaluate_complete(complete_started.elapsed());
        return DfsResult::Failed(ProfileSearchError::ValidationCountMismatch {
            expected_nodes: context.expected_node_count,
            expected_links: context.expected_link_count.unwrap_or(validation.link_count),
            expected_physical_links: context.expected_physical_link_count,
            expected_discard_links: context.expected_discard_link_count,
            actual_nodes: validation.node_count,
            actual_links: validation.link_count,
            actual_physical_links: validation.physical_link_count,
            actual_discard_links: validation.discard_link_count,
        });
    }

    let key = canonical.key;
    context.retain_witness(ProfileWitness {
        canonical_graph_key: key.clone(),
        graph: canonical.graph,
        validation,
    });
    context.update_cache_status(state_token, StateStatus::SatWitness(key.clone()));
    hotspot_profile::record_evaluate_complete(complete_started.elapsed());
    DfsResult::Exhausted(Some(key))
}

fn is_candidate_rejection(error: &ValidationError) -> bool {
    matches!(
        error,
        ValidationError::NodeUnreachableFromInput { .. }
            | ValidationError::NodeCannotReachOutput { .. }
            | ValidationError::InconsistentSteadyState
            | ValidationError::NonUniqueSteadyState { .. }
            | ValidationError::InconsistentCyclicScc { .. }
            | ValidationError::NonUniqueCyclicScc { .. }
            | ValidationError::IncorrectOutputRate { .. }
            | ValidationError::NonPositiveResolvedFlow { .. }
            | ValidationError::ResolvedFlowAboveCapacity { .. }
    )
}

fn checked_node_count(profile: NodeProfile) -> Option<u32> {
    profile
        .splitter2
        .checked_add(profile.splitter3)?
        .checked_add(profile.merger2)?
        .checked_add(profile.merger3)
}

fn topology_error(error: &impl ToString) -> ProfileSearchError {
    ProfileSearchError::Topology(error.to_string())
}

fn propagation_error(error: &PropagationError) -> ProfileSearchError {
    ProfileSearchError::Propagation(error.to_string())
}

fn scc_error(error: &SccError) -> ProfileSearchError {
    ProfileSearchError::Scc(error.to_string())
}

fn retain_smallest_key(
    target: &mut Option<CanonicalGraphKey>,
    candidate: Option<CanonicalGraphKey>,
) {
    if let Some(candidate) = candidate
        && target.as_ref().is_none_or(|current| candidate < *current)
    {
        *target = Some(candidate);
    }
}

fn increment(counter: &mut u64) {
    *counter = counter.saturating_add(1);
}

fn add_duration_ns(counter: &mut u64, duration: Duration) {
    *counter = counter.saturating_add(u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX));
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        ops::ControlFlow,
        sync::Mutex,
    };

    use solver_api::{
        ConsumerPortRef, InputTerminalIndex, NodeId, NodeType, OutputTerminalIndex, PhysicalNode,
        ProducerPortRef, SolveResult,
    };
    use solver_reference::{
        ReferenceOptions, canonicalize_graph as reference_canonicalize,
        enumerate_profiles as enumerate_reference_profiles,
        enumerate_topologies_with_discard as enumerate_reference_topologies, solve_reference,
    };

    use super::*;
    use crate::{
        Preparation,
        canonical::{PartialLink, PartialTopology},
        prepare_problem,
    };

    fn problem(inputs: &[&str], outputs: &[&str], capacity: &str) -> Problem {
        Problem {
            inputs: inputs.iter().map(|rate| rate.parse().unwrap()).collect(),
            outputs: outputs.iter().map(|rate| rate.parse().unwrap()).collect(),
            max_link_rate: capacity.parse().unwrap(),
        }
    }

    fn normalized(problem: &Problem) -> NormalizedProblem {
        match prepare_problem(problem).unwrap() {
            Preparation::Prepared(problem) => problem,
            Preparation::GloballyUnsat(proof) => panic!("unexpected global proof: {proof:?}"),
        }
    }

    const fn profile(splitter2: u32, splitter3: u32, merger2: u32, merger3: u32) -> NodeProfile {
        NodeProfile {
            splitter2,
            splitter3,
            merger2,
            merger3,
        }
    }

    fn exhaustive_witness(result: ProfileSearchResult) -> Option<ProfileWitness> {
        exhausted_result(result).0
    }

    fn exhausted_result(
        result: ProfileSearchResult,
    ) -> (Option<ProfileWitness>, ProfileSearchStats) {
        match result {
            ProfileSearchResult::Exhausted {
                best_witness,
                stats,
                ..
            } => (best_witness, stats),
            other => panic!("expected exhaustive profile result, got {other:?}"),
        }
    }

    fn search(problem: &Problem, profile: NodeProfile) -> ProfileSearchResult {
        search_profile(&normalized(problem), profile, &AtomicBool::new(false))
    }

    fn root_partitions(problem: &NormalizedProblem, profile: NodeProfile) -> Vec<RootPartition> {
        match plan_profile_root_partitions(problem, profile, &AtomicBool::new(false)) {
            RootPartitionPlan::Partitions(partitions) => partitions,
            RootPartitionPlan::Immediate(result) => {
                panic!("expected structural root partitions, got {result:?}")
            }
        }
    }

    #[test]
    #[ignore = "manual hard-case root-partition benchmark"]
    fn benchmark_hard_case_root_partition_planning() {
        let problem = normalized(&Problem {
            inputs: vec![Rational::from(216)],
            outputs: vec![Rational::from(66), Rational::from(150)],
            max_link_rate: Rational::from(1_200),
        });
        let profile = NodeProfile {
            splitter2: 2,
            splitter3: 2,
            merger2: 1,
            merger3: 2,
        };
        let cancel = AtomicBool::new(false);
        let mut initialized =
            initialize_profile_search(&problem, profile, &cancel, SearchFeatures::PRODUCTION, None)
                .unwrap();
        eprintln!(
            "hard-case root legal_decisions={}",
            initialized.state.legal_decisions().len()
        );
        let started = Instant::now();
        let partitions = root_partitions(&problem, profile);
        eprintln!(
            "hard-case root partitions={} elapsed={:?}",
            partitions.len(),
            started.elapsed()
        );
    }

    fn run_root_partition_schedule(
        problem: &NormalizedProblem,
        profile: NodeProfile,
        partitions: &[RootPartition],
        schedule: &[usize],
        worker_count: usize,
    ) -> BTreeMap<RootPartitionId, Option<ProfileWitness>> {
        assert!(worker_count > 0);
        let results = Mutex::new(Vec::new());
        let cancel = AtomicBool::new(false);
        std::thread::scope(|scope| {
            for worker in 0..worker_count {
                let assigned = schedule
                    .iter()
                    .copied()
                    .enumerate()
                    .filter_map(|(position, index)| {
                        (position % worker_count == worker).then_some(index)
                    })
                    .collect::<Vec<_>>();
                let results = &results;
                let cancel = &cancel;
                scope.spawn(move || {
                    for index in assigned {
                        let partition = &partitions[index];
                        let result = search_profile_root_partition(
                            problem, profile, cancel, None, partition, false, None,
                        );
                        let witness = match result {
                            ProfileSearchResult::Exhausted { best_witness, .. } => best_witness,
                            other => panic!("partition must exhaust in test: {other:?}"),
                        };
                        results.lock().unwrap().push((partition.id(), witness));
                    }
                });
            }
        });
        results.into_inner().unwrap().into_iter().collect()
    }

    fn merged_partition_witness(
        outcomes: &BTreeMap<RootPartitionId, Option<ProfileWitness>>,
    ) -> Option<ProfileWitness> {
        outcomes
            .values()
            .filter_map(Clone::clone)
            .min_by(|left, right| left.canonical_graph_key.cmp(&right.canonical_graph_key))
    }

    fn reference_fixed_profile(problem: &Problem, profile: NodeProfile) -> Option<ProfileWitness> {
        let input_count = u32::try_from(problem.inputs.len()).unwrap();
        let output_count = u32::try_from(problem.outputs.len()).unwrap();
        let surplus = problem.surplus().unwrap();
        let accounting = profile_link_accounting(
            profile,
            input_count,
            output_count,
            &surplus,
            &problem.max_link_rate,
        )
        .unwrap()?;
        let mut seen = BTreeSet::new();
        let mut best = None;
        let exhausted = enumerate_reference_topologies(
            profile,
            input_count,
            output_count,
            accounting.discard_link_count,
            |topology| {
                let graph = PhysicalGraph {
                    nodes: topology.nodes,
                    links: topology
                        .links
                        .into_iter()
                        .map(|(producer, consumer)| PhysicalLink {
                            producer,
                            consumer,
                            flow: Rational::from(0),
                        })
                        .collect(),
                };
                let structural = reference_canonicalize(problem, &graph);
                if !seen.insert(structural.key) {
                    return ControlFlow::Continue(());
                }
                let solved = match solve_topology(problem, &structural.graph) {
                    Ok(solved) => solved,
                    Err(error) if is_candidate_rejection(&error) => {
                        return ControlFlow::Continue(());
                    }
                    Err(error) => panic!("reference topology invariant failed: {error}"),
                };
                let solved = reference_canonicalize(problem, &solved);
                let validation = validate_solution(problem, &solved.graph).unwrap();
                let candidate = ProfileWitness {
                    canonical_graph_key: solved.key,
                    graph: solved.graph,
                    validation,
                };
                if best.as_ref().is_none_or(|current: &ProfileWitness| {
                    candidate.canonical_graph_key < current.canonical_graph_key
                }) {
                    best = Some(candidate);
                }
                ControlFlow::Continue(())
            },
        );
        assert_eq!(exhausted, ControlFlow::Continue(()));
        best
    }

    #[test]
    fn direct_profile_is_exhausted_and_validated() {
        let problem = problem(&["60"], &["60"], "60");
        let witness = exhaustive_witness(search(&problem, NodeProfile::default())).unwrap();
        assert_eq!(witness.validation.node_count, 0);
        assert_eq!(witness.validation.link_count, 0);
        assert_eq!(witness.validation.discard_link_count, 0);
        assert_eq!(witness.validation.physical_link_count, 1);
        validate_solution(&normalized_problem(&problem), &witness.graph).unwrap();
    }

    #[test]
    fn canonical_root_partition_union_matches_unpartitioned_and_reference_search() {
        let caller_problem = problem(&["2"], &["1", "1"], "2");
        let normalized = normalized(&caller_problem);
        let fixed_profile = profile(1, 0, 0, 0);
        let partitions = root_partitions(&normalized, fixed_profile);
        assert!(
            partitions.len() > 1,
            "the fixture must exercise more than one root obligation"
        );
        assert!(
            partitions
                .windows(2)
                .all(|pair| pair[0].stable_key() < pair[1].stable_key())
        );
        assert_eq!(
            partitions
                .iter()
                .map(|partition| partition.id().ordinal())
                .collect::<Vec<_>>(),
            (0..u32::try_from(partitions.len()).unwrap()).collect::<Vec<_>>()
        );

        let schedule = (0..partitions.len()).collect::<Vec<_>>();
        let outcomes =
            run_root_partition_schedule(&normalized, fixed_profile, &partitions, &schedule, 1);
        assert_eq!(outcomes.len(), partitions.len());
        let partitioned = merged_partition_witness(&outcomes);
        let unpartitioned = exhaustive_witness(search_profile(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
        ));
        let reference =
            reference_fixed_profile(&normalized_problem(&caller_problem), fixed_profile);
        assert_eq!(partitioned, unpartitioned);
        assert_eq!(partitioned, reference);
    }

    #[test]
    fn root_partition_ids_and_merged_witness_ignore_worker_order() {
        let caller_problem = problem(&["2"], &["1", "1"], "2");
        let normalized = normalized(&caller_problem);
        let fixed_profile = profile(1, 0, 0, 0);
        let first_plan = root_partitions(&normalized, fixed_profile);
        let second_plan = root_partitions(&normalized, fixed_profile);
        assert_eq!(first_plan, second_plan);

        let forward = (0..first_plan.len()).collect::<Vec<_>>();
        let reverse = forward.iter().copied().rev().collect::<Vec<_>>();
        let single_worker =
            run_root_partition_schedule(&normalized, fixed_profile, &first_plan, &forward, 1);
        let two_workers =
            run_root_partition_schedule(&normalized, fixed_profile, &first_plan, &reverse, 2);
        let four_workers =
            run_root_partition_schedule(&normalized, fixed_profile, &first_plan, &forward, 4);
        assert_eq!(single_worker, two_workers);
        assert_eq!(single_worker, four_workers);
        assert_eq!(
            merged_partition_witness(&single_worker),
            exhaustive_witness(search_profile(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
            ))
        );
    }

    #[test]
    fn minimum_capacity_discard_partition_matches_the_reference() {
        let caller_problem = problem(&["1", "1", "1"], &["1"], "1");
        let normalized = normalized(&caller_problem);
        let canonical_problem = normalized_problem(&caller_problem);
        let fixed_profile = NodeProfile::default();
        let reference = reference_fixed_profile(&canonical_problem, fixed_profile);
        let optimized = exhaustive_witness(search_profile_with_features(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
            SearchFeatures::PRODUCTION,
        ));
        let unoptimized = exhaustive_witness(search_profile_with_features(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
            SearchFeatures::UNOPTIMIZED,
        ));
        assert_eq!(optimized, reference);
        assert_eq!(unoptimized, reference);
        let witness = optimized.unwrap();
        assert_eq!(witness.validation.link_count, 0);
        assert_eq!(witness.validation.discard_link_count, 2);
        assert_eq!(witness.validation.physical_link_count, 3);
        assert!(witness.graph.links.iter().all(|link| link.flow == 1.into()));
    }

    #[test]
    fn splitter_and_merger_profiles_are_found_exactly() {
        let split = problem(&["120"], &["60", "60"], "120");
        let split_witness = exhaustive_witness(search(&split, profile(1, 0, 0, 0))).unwrap();
        assert_eq!(split_witness.validation.node_count, 1);
        assert_eq!(split_witness.graph.nodes[0].node_type, NodeType::Splitter2);

        let merge = problem(&["30", "90"], &["120"], "120");
        let merge_witness = exhaustive_witness(search(&merge, profile(0, 0, 1, 0))).unwrap();
        assert_eq!(merge_witness.validation.node_count, 1);
        assert_eq!(merge_witness.graph.nodes[0].node_type, NodeType::Merger2);
    }

    #[test]
    fn splitter2_mrv_search_matches_reference() {
        // Compare the complete MRV construction path with the independent oracle.
        let problem = problem(&["2"], &["1", "1"], "2");
        let profile = profile(1, 0, 0, 0);
        let production = exhaustive_witness(search(&problem, profile));
        let reference = reference_fixed_profile(&normalized_problem(&problem), profile);
        assert_eq!(production, reference);
        assert!(production.is_some());
    }

    #[test]
    fn cyclic_profile_is_exhausted_without_special_cycle_pruning() {
        let problem = problem(&["1"], &["1"], "2");
        let result = search(&problem, profile(1, 0, 1, 0));
        let ProfileSearchResult::Exhausted {
            best_witness: Some(witness),
            stats,
            ..
        } = result
        else {
            panic!("feedback-capable profile must be exhaustively satisfiable");
        };
        assert!(witness.validation.cyclic_scc_count <= 1);
        assert!(stats.validated_cyclic_topologies > 0);
        validate_solution(&normalized_problem(&problem), &witness.graph).unwrap();
    }

    #[test]
    fn dynamic_scc_and_its_cache_on_off_match_reference_for_cyclic_profiles() {
        let fixed_profile = profile(1, 0, 1, 0);
        let cases = [problem(&["1"], &["1"], "2"), problem(&["1"], &["1"], "1")];
        let mut observed_scc_solves = 0_u64;

        for caller_problem in cases {
            let normalized = normalized(&caller_problem);
            let canonical_problem = normalized_problem(&caller_problem);
            let reference = reference_fixed_profile(&canonical_problem, fixed_profile);
            let (with_scc, with_stats) = exhausted_result(search_profile_with_features(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                SearchFeatures::PRODUCTION,
            ));
            let (without_scc, without_stats) = exhausted_result(search_profile_with_features(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                SearchFeatures::WITHOUT_SCC,
            ));
            let (without_cache, without_cache_stats) =
                exhausted_result(search_profile_with_features(
                    &normalized,
                    fixed_profile,
                    &AtomicBool::new(false),
                    SearchFeatures::WITHOUT_SCC_CACHE,
                ));

            assert_eq!(with_scc, reference, "SCC enabled: {canonical_problem:?}");
            assert_eq!(
                without_cache, reference,
                "SCC cache disabled: {canonical_problem:?}"
            );
            assert_eq!(
                without_scc, reference,
                "SCC disabled: {canonical_problem:?}"
            );
            assert_eq!(without_stats.instrumentation.scc_solves, 0);
            assert_eq!(without_stats.instrumentation.scc_cache_hits, 0);
            assert_eq!(without_cache_stats.instrumentation.scc_cache_hits, 0);
            observed_scc_solves =
                observed_scc_solves.saturating_add(with_stats.instrumentation.scc_solves);
        }

        assert!(
            observed_scc_solves > 0,
            "the feedback-capable matrix must exercise dynamic SCC algebra"
        );
    }

    #[test]
    fn consistent_singular_open_scc_remains_a_live_branch() {
        let caller_problem = problem(&["1"], &["1"], "10");
        let normalized = normalized(&caller_problem);
        let partial = PartialTopology {
            discard_count: 0,
            problem: normalized_problem(&caller_problem),
            nodes: vec![
                PhysicalNode {
                    id: NodeId(0),
                    node_type: NodeType::Splitter2,
                },
                PhysicalNode {
                    id: NodeId(1),
                    node_type: NodeType::Merger2,
                },
            ],
            links: vec![
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(1),
                        port: 0,
                    },
                    flow: None,
                },
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(1),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    flow: None,
                },
            ],
            remaining_profile: NodeProfile::default(),
        };
        let topology = TopologyState::from_partial_topology(&partial).unwrap();
        let mut propagation = PropagationState::new(&topology, &normalized.max_link_rate).unwrap();
        let known = propagated_known_values(&topology, Some(&propagation)).unwrap();
        let affected = detect_affected_sccs(&topology, Some(1)).unwrap();
        let region = affected
            .affected_regions()
            .find(|region| region.cyclic)
            .unwrap();
        let summary = summarize_open_scc(&topology, region, &known).unwrap();
        assert_eq!(summary.algebra.consistency, Consistency::Consistent);
        assert!(!summary.algebra.is_unique());

        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(&normalized, profile(1, 0, 1, 0), &cancel);
        let verdict =
            analyze_affected_dynamic_sccs(&topology, Some(&mut propagation), &mut context, Some(1))
                .unwrap();
        assert_eq!(verdict, DynamicSccVerdict::Open);
        assert_eq!(context.stats.instrumentation.scc_solves, 1);
        let repeated =
            analyze_affected_dynamic_sccs(&topology, Some(&mut propagation), &mut context, Some(1))
                .unwrap();
        assert_eq!(repeated, verdict);
        assert_eq!(context.stats.instrumentation.scc_solves, 1);
        assert_eq!(context.stats.instrumentation.scc_cache_hits, 1);
    }

    fn singular_cycle_fixture(
        problem: Problem,
        splitter: u32,
        merger: u32,
        splitter_feedback_port: u8,
        merger_feedback_port: u8,
    ) -> TopologyState {
        let mut nodes = vec![
            PhysicalNode {
                id: NodeId(splitter),
                node_type: NodeType::Splitter2,
            },
            PhysicalNode {
                id: NodeId(merger),
                node_type: NodeType::Merger2,
            },
        ];
        nodes.sort_by_key(|node| node.id);
        TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem,
            nodes,
            links: vec![
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(splitter),
                        port: splitter_feedback_port,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(merger),
                        port: merger_feedback_port,
                    },
                    flow: None,
                },
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(merger),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(splitter),
                        port: 0,
                    },
                    flow: None,
                },
            ],
            remaining_profile: NodeProfile::default(),
        })
        .unwrap()
    }

    #[test]
    fn scc_cache_hit_transports_ratio_deductions_across_an_isomorphism() {
        let caller_problem = problem(&["1"], &["1"], "10");
        let normalized = normalized(&caller_problem);
        let left = singular_cycle_fixture(normalized_problem(&caller_problem), 0, 1, 0, 0);
        let right = singular_cycle_fixture(normalized_problem(&caller_problem), 1, 0, 1, 1);
        let mut left_propagation = PropagationState::new(&left, &normalized.max_link_rate).unwrap();
        let mut right_propagation =
            PropagationState::new(&right, &normalized.max_link_rate).unwrap();
        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(&normalized, profile(1, 0, 1, 0), &cancel);

        assert_eq!(
            analyze_affected_dynamic_sccs(
                &left,
                Some(&mut left_propagation),
                &mut context,
                Some(1),
            )
            .unwrap(),
            DynamicSccVerdict::Open
        );
        let cached = context
            .scc_cache
            .values()
            .next()
            .expect("the first isomorph installs one SCC summary");
        assert!(
            !cached.homogeneous_ratios.is_empty(),
            "the fixture must cache a real exact SCC deduction"
        );
        assert_eq!(context.stats.instrumentation.scc_solves, 1);

        assert_eq!(
            analyze_affected_dynamic_sccs(
                &right,
                Some(&mut right_propagation),
                &mut context,
                Some(1),
            )
            .unwrap(),
            DynamicSccVerdict::Open
        );
        assert_eq!(
            context.stats.instrumentation.scc_solves, 1,
            "an isomorphic relabeling must reuse the exact canonical payload"
        );
        assert_eq!(context.stats.instrumentation.scc_cache_hits, 1);
    }

    #[test]
    fn scc_cache_key_tracks_speculation_and_returns_after_topology_rollback() {
        let caller_problem = problem(&["1"], &["1"], "10");
        let normalized = normalized(&caller_problem);
        let partial = PartialTopology {
            discard_count: 0,
            problem: normalized_problem(&caller_problem),
            nodes: vec![
                PhysicalNode {
                    id: NodeId(0),
                    node_type: NodeType::Splitter2,
                },
                PhysicalNode {
                    id: NodeId(1),
                    node_type: NodeType::Merger2,
                },
            ],
            links: vec![
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(1),
                        port: 0,
                    },
                    flow: None,
                },
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(1),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    flow: None,
                },
            ],
            remaining_profile: NodeProfile::default(),
        };
        let mut topology = TopologyState::from_partial_topology(&partial).unwrap();
        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(&normalized, profile(1, 0, 1, 0), &cancel);

        assert_eq!(
            analyze_affected_dynamic_sccs(&topology, None, &mut context, Some(1)).unwrap(),
            DynamicSccVerdict::Open
        );
        assert_eq!(context.stats.instrumentation.scc_solves, 1);

        let checkpoint = topology.checkpoint();
        let decision = topology
            .legal_decisions()
            .into_iter()
            .find(|decision| {
                matches!(
                    decision.producer,
                    crate::topology::ProducerChoice::Existing(ProducerPortRef::Node { .. })
                ) || matches!(
                    decision.consumer,
                    crate::topology::ConsumerChoice::Existing(ConsumerPortRef::Node { .. })
                )
            })
            .expect("a cyclic open subsystem has a node-adjacent continuation");
        let decision_id = topology.apply(decision).unwrap();
        let speculative_link = topology.link_index_for(decision_id).unwrap();
        let _ =
            analyze_affected_dynamic_sccs(&topology, None, &mut context, Some(speculative_link))
                .unwrap();
        assert_eq!(
            context.stats.instrumentation.scc_solves, 2,
            "a changed physical state must not reuse the earlier summary"
        );
        assert_eq!(context.stats.instrumentation.scc_cache_hits, 0);

        topology.rollback(checkpoint);
        assert_eq!(
            analyze_affected_dynamic_sccs(&topology, None, &mut context, Some(1)).unwrap(),
            DynamicSccVerdict::Open
        );
        assert_eq!(context.stats.instrumentation.scc_solves, 2);
        assert_eq!(context.stats.instrumentation.scc_cache_hits, 1);
    }

    #[test]
    fn pre_cancelled_profile_is_incomplete() {
        let problem = normalized(&problem(&["1"], &["1"], "2"));
        let result = search_profile(&problem, profile(1, 0, 1, 0), &AtomicBool::new(true));
        assert!(matches!(
            result,
            ProfileSearchResult::Incomplete {
                reason: IncompleteReason::Cancelled,
                ..
            }
        ));
    }

    #[test]
    fn production_matches_reference_for_tiny_fixed_profiles() {
        let cases = [
            (problem(&["1"], &["1"], "2"), profile(0, 0, 0, 0)),
            (problem(&["2"], &["1", "1"], "2"), profile(1, 0, 0, 0)),
            (problem(&["3"], &["1", "1", "1"], "3"), profile(0, 1, 0, 0)),
            (problem(&["1", "2"], &["3"], "3"), profile(0, 0, 1, 0)),
            (problem(&["1", "1", "1"], &["3"], "3"), profile(0, 0, 0, 1)),
            (problem(&["1"], &["1"], "2"), profile(1, 0, 1, 0)),
        ];

        for (problem, profile) in cases {
            let normalized = normalized(&problem);
            let canonical_problem = normalized_problem(&problem);
            let production = exhaustive_witness(search_profile(
                &normalized,
                profile,
                &AtomicBool::new(false),
            ));
            let reference = reference_fixed_profile(&canonical_problem, profile);
            assert_eq!(production, reference, "fixed profile {profile:?}");
        }
    }

    #[test]
    fn every_pruning_layer_matches_reference_on_an_exhaustive_tiny_matrix() {
        let feature_sets = [
            ("none", SearchFeatures::UNOPTIMIZED),
            ("propagation", SearchFeatures::PROPAGATION_ONLY),
            ("reachability", SearchFeatures::REACHABILITY_ONLY),
            ("all", SearchFeatures::PRODUCTION),
        ];

        // Exhaust every positive ordered terminal-rate partition through total
        // two and every balanced zero- or one-node profile. Comparing each
        // pruning layer independently makes any lost completion visible against
        // the deliberately simple labeled reference enumerator.
        for total in 1..=2 {
            let sides = positive_ordered_partitions(total, 2);
            for inputs in &sides {
                for outputs in &sides {
                    let caller_problem = integer_problem(inputs, outputs, total);
                    let normalized = normalized(&caller_problem);
                    let canonical_problem = normalized_problem(&caller_problem);
                    for node_count in 0..=1 {
                        let profiles = enumerate_reference_profiles(
                            node_count,
                            u32::try_from(inputs.len()).unwrap(),
                            u32::try_from(outputs.len()).unwrap(),
                        );
                        for profile in profiles {
                            let reference = reference_fixed_profile(&canonical_problem, profile);
                            for (label, features) in feature_sets {
                                let production = exhaustive_witness(search_profile_with_features(
                                    &normalized,
                                    profile,
                                    &AtomicBool::new(false),
                                    features,
                                ));
                                assert_eq!(
                                    production, reference,
                                    "feature set {label}, profile {profile:?}, problem {canonical_problem:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn deferred_and_exact_state_caches_return_the_same_fixed_profile_witnesses() {
        let cases = [
            (problem(&["2"], &["1", "1"], "2"), profile(1, 0, 0, 0)),
            (problem(&["1", "2"], &["3"], "3"), profile(0, 0, 1, 0)),
            (problem(&["1"], &["1"], "2"), profile(1, 0, 1, 0)),
        ];
        for (caller_problem, profile) in cases {
            let normalized = normalized(&caller_problem);
            let deferred = exhaustive_witness(search_profile_with_features(
                &normalized,
                profile,
                &AtomicBool::new(false),
                SearchFeatures::PRODUCTION,
            ));
            let exact = exhaustive_witness(search_profile_with_features(
                &normalized,
                profile,
                &AtomicBool::new(false),
                SearchFeatures::EXACT_STATE_CACHE,
            ));
            assert_eq!(deferred, exact, "fixed profile {profile:?}");
        }
    }

    #[test]
    fn arithmetic_depth_bounds_prune_only_reference_unsat_profiles() {
        let cases = [
            (problem(&["5"], &["1"], "5"), profile(1, 0, 0, 0)),
            (problem(&["2", "3", "5"], &["4"], "6"), profile(0, 0, 1, 0)),
        ];
        for (caller_problem, fixed_profile) in cases {
            let normalized = normalized(&caller_problem);
            let reference =
                reference_fixed_profile(&normalized_problem(&caller_problem), fixed_profile);
            let (bounded, stats) = exhausted_result(search_profile_with_features(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                SearchFeatures::PRODUCTION,
            ));
            let unbounded = exhaustive_witness(search_profile_with_features(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                SearchFeatures::UNOPTIMIZED,
            ));
            assert_eq!(bounded, reference);
            assert_eq!(unbounded, reference);
            assert!(reference.is_none());
            assert!(stats.instrumentation.lower_bound_prunes > 0);
        }
    }

    #[test]
    fn exact_capacity_and_positivity_prunes_preserve_reference_outcomes() {
        // The feedback completion carries two units on its central link, so
        // capacity one is an exact impossibility proof.
        let caller_problem = problem(&["1"], &["1"], "1");
        let capacity_normalized = normalized(&caller_problem);
        let fixed_profile = profile(1, 0, 1, 0);
        let canonical_problem = normalized_problem(&caller_problem);
        let reference = reference_fixed_profile(&canonical_problem, fixed_profile);
        let (optimized_witness, optimized_stats) = exhausted_result(search_profile_with_features(
            &capacity_normalized,
            fixed_profile,
            &AtomicBool::new(false),
            SearchFeatures::PRODUCTION,
        ));
        let (unoptimized_witness, _) = exhausted_result(search_profile_with_features(
            &capacity_normalized,
            fixed_profile,
            &AtomicBool::new(false),
            SearchFeatures::UNOPTIMIZED,
        ));

        assert_eq!(optimized_witness, reference);
        assert_eq!(unoptimized_witness, reference);
        assert!(optimized_stats.instrumentation.capacity_prunes > 0);

        // A closed two-splitter region has only the exact zero solution. This
        // prefix is intentionally constructed directly: lazy connected search
        // cannot materialize a detached source-free region, while the search
        // instrumentation must still classify the algebra proof as a strict
        // positivity/capacity prune rather than an internal failure.
        let positivity_problem = problem(&["1"], &["1"], "10");
        let positivity_normalized = normalized(&positivity_problem);
        let topology = TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem: normalized_problem(&positivity_problem),
            nodes: vec![
                PhysicalNode {
                    id: NodeId(0),
                    node_type: NodeType::Splitter2,
                },
                PhysicalNode {
                    id: NodeId(1),
                    node_type: NodeType::Splitter2,
                },
            ],
            links: vec![
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(1),
                        port: 0,
                    },
                    flow: None,
                },
                PartialLink {
                    producer: ProducerPortRef::Node {
                        node: NodeId(1),
                        port: 0,
                    },
                    consumer: ConsumerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                    flow: None,
                },
            ],
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let propagation =
            PropagationState::new(&topology, &positivity_normalized.max_link_rate).unwrap();
        let PropagationOutcome::Pruned(conflict @ PropagationConflict::NonPositiveKnown { .. }) =
            propagation.current_outcome()
        else {
            panic!("closed splitter cycle must prove an exact nonpositive flow");
        };
        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(&positivity_normalized, profile(2, 0, 0, 0), &cancel);
        context.record_propagation_prune(conflict);
        assert_eq!(context.stats.instrumentation.capacity_prunes, 1);
    }

    #[test]
    fn exact_pruning_reduces_a_proven_search_without_changing_its_result() {
        let caller_problem = problem(&["1"], &["1"], "1");
        let normalized = normalized(&caller_problem);
        let profile = profile(1, 0, 1, 0);
        let (optimized_witness, optimized_stats) = exhausted_result(search_profile_with_features(
            &normalized,
            profile,
            &AtomicBool::new(false),
            SearchFeatures::PRODUCTION,
        ));
        let (unoptimized_witness, unoptimized_stats) =
            exhausted_result(search_profile_with_features(
                &normalized,
                profile,
                &AtomicBool::new(false),
                SearchFeatures::UNOPTIMIZED,
            ));

        assert_eq!(optimized_witness, unoptimized_witness);
        assert!(
            optimized_stats.instrumentation.raw_structural_decisions
                < unoptimized_stats.instrumentation.raw_structural_decisions,
            "optimized search must discharge this exact impossible profile earlier"
        );
    }

    #[test]
    fn paired_random_mutation_and_rollback_matches_fresh_propagation() {
        let caller_problem = problem(&["1"], &["1"], "2");
        let normalized = normalized(&caller_problem);
        let profile = profile(1, 0, 1, 0);

        for seed in 0_u64..16 {
            let mut topology = TopologyState::new(&normalized, profile).unwrap();
            let mut propagation =
                PropagationState::new(&topology, &normalized.max_link_rate).unwrap();
            let root_topology = topology.clone();
            let root_propagation = propagation.clone();
            let mut frames = Vec::new();
            let mut random = seed.wrapping_add(1);

            for _ in 0..8 {
                let decisions = topology.legal_decisions();
                if decisions.is_empty() {
                    break;
                }
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let index =
                    usize::try_from(random % u64::try_from(decisions.len()).unwrap()).unwrap();
                frames.push((
                    topology.checkpoint(),
                    propagation.checkpoint(),
                    topology.clone(),
                    propagation.clone(),
                ));
                topology.apply(decisions[index]).unwrap();
                let incremental_outcome = propagation
                    .synchronize_after_topology_mutation(&topology)
                    .unwrap();
                let rebuilt = PropagationState::new(&topology, &normalized.max_link_rate).unwrap();
                assert_propagation_semantics_eq(&topology, &propagation, &rebuilt);
                if incremental_outcome.is_pruned() {
                    break;
                }
            }

            while let Some((
                topology_checkpoint,
                propagation_checkpoint,
                old_topology,
                old_propagation,
            )) = frames.pop()
            {
                propagation.rollback(propagation_checkpoint);
                topology.rollback(topology_checkpoint);
                assert_eq!(topology, old_topology, "seed {seed}, topology rollback");
                assert_eq!(
                    propagation, old_propagation,
                    "seed {seed}, propagation rollback"
                );
                let rebuilt = PropagationState::new(&topology, &normalized.max_link_rate).unwrap();
                assert_propagation_semantics_eq(&topology, &propagation, &rebuilt);
            }
            assert_eq!(topology, root_topology);
            assert_eq!(propagation, root_propagation);
        }
    }

    #[test]
    fn production_matches_reference_for_all_tiny_rate_partitions_and_profiles() {
        // This is an exhaustive finite matrix, not sampled property data. It
        // covers every positive ordered rate partition through total four with
        // at most three terminals, and every balanced zero- or one-node profile.
        for total in 1..=4 {
            let sides = positive_ordered_partitions(total, 3);
            for inputs in &sides {
                for outputs in &sides {
                    let problem = integer_problem(inputs, outputs, total);
                    for node_count in 0..=1 {
                        let profiles = enumerate_reference_profiles(
                            node_count,
                            u32::try_from(inputs.len()).unwrap(),
                            u32::try_from(outputs.len()).unwrap(),
                        );
                        for profile in profiles {
                            assert_matches_reference(&problem, profile);
                        }
                    }
                }
            }
        }

        // Splitter2+Merger2 is the smallest profile with both acyclic and
        // feedback completions. Exhaust all one- and two-terminal partitions
        // through total three against the independent fixed-profile oracle.
        let profile = profile(1, 0, 1, 0);
        for total in 1..=3 {
            let sides = positive_ordered_partitions(total, 2);
            for inputs in &sides {
                for outputs in &sides {
                    if inputs.len() == outputs.len() {
                        assert_matches_reference(&integer_problem(inputs, outputs, total), profile);
                    }
                }
            }
        }
    }

    #[test]
    fn canonical_mrv_keeps_relabelled_state_cache_outcomes_equivalent() {
        let caller_problem = problem(&["4"], &["2", "1", "1"], "4");
        let normalized = normalized(&caller_problem);
        let profile = profile(2, 0, 0, 0);
        let left_partial =
            two_splitter_partial(&normalized_problem(&caller_problem), [0, 1], false);
        let right_partial =
            two_splitter_partial(&normalized_problem(&caller_problem), [1, 0], true);
        assert_eq!(
            canonicalize_state(&left_partial),
            canonicalize_state(&right_partial)
        );
        let cancel = AtomicBool::new(false);
        assert_eq!(
            state_fingerprint(&left_partial, &cancel),
            state_fingerprint(&right_partial, &cancel)
        );

        let mut left = TopologyState::from_partial_topology(&left_partial).unwrap();
        let mut right = TopologyState::from_partial_topology(&right_partial).unwrap();
        let left_orbit = left.selected_open_orbit().unwrap();
        let right_orbit = right.selected_open_orbit().unwrap();
        assert_eq!(left_orbit.canonical_key, right_orbit.canonical_key);
        let left_children = left.ordered_decision_child_keys();
        let right_children = right.ordered_decision_child_keys();
        assert_eq!(left_children, right_children);
        assert_eq!(left_children.len(), 2);
        assert!(left_children.windows(2).all(|pair| pair[0] < pair[1]));

        let left_outcome = partial_search_outcome(&normalized, profile, &left_partial);
        let right_outcome = partial_search_outcome(&normalized, profile, &right_partial);
        assert_eq!(left_outcome, right_outcome);

        let mut context = SearchContext::new(&normalized, profile, &cancel);
        let accounting = profile_link_accounting(
            profile,
            1,
            3,
            &normalized.surplus,
            &normalized.max_link_rate,
        )
        .unwrap()
        .unwrap();
        context.expected_link_count = Some(accounting.link_count);
        context.expected_physical_link_count = accounting.physical_link_count;
        context.expected_discard_link_count = accounting.discard_link_count;
        let mut left_propagation =
            Some(PropagationState::new(&left, &normalized.max_link_rate).unwrap());
        let mut right_propagation =
            Some(PropagationState::new(&right, &normalized.max_link_rate).unwrap());
        let first = exhausted_key(search_state(&mut left, &mut left_propagation, &mut context));
        let hits_before = context.stats.state_cache_hits;
        let second = exhausted_key(search_state(
            &mut right,
            &mut right_propagation,
            &mut context,
        ));
        assert_eq!(first, second);
        assert_eq!(context.stats.state_cache_hits, hits_before + 1);
        assert!(context.stats.instrumentation.peak_memory_bytes > 0);
    }

    #[test]
    fn signed_payload_size_matches_allocating_oracle() {
        let check = |value: num::BigInt| {
            assert_eq!(
                super::signed_payload_bytes(&value),
                value.to_signed_bytes_le().len()
            );
        };
        for value in i16::MIN..=i16::MAX {
            check(num::BigInt::from(value));
        }
        for bit in 0..=4096 {
            let power = num::BigInt::from(1_u8) << bit;
            for offset in -1..=1 {
                let value: num::BigInt = &power + offset;
                check(value.clone());
                check(-value);
            }
        }
        let mut seed = 902_045_u64;
        for length in [1, 8, 16, 32, 128, 1024] {
            for _ in 0..32 {
                let bytes: Vec<_> = (0..length)
                    .map(|_| {
                        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
                        seed.to_le_bytes()[4]
                    })
                    .collect();
                check(num::BigInt::from_signed_bytes_le(&bytes));
            }
        }
    }

    #[test]
    fn deterministic_owned_cache_memory_peak_is_nonzero_and_monotone() {
        let caller_problem = problem(&["1"], &["1"], "2");
        let normalized = normalized(&caller_problem);
        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(&normalized, NodeProfile::default(), &cancel);
        let state = TopologyState::new(&normalized, NodeProfile::default()).unwrap();
        let key = canonicalize_state(&state.partial_topology());

        context.insert_state_status(key.clone(), StateStatus::InProgress);
        let after_insert = context.stats.instrumentation.peak_memory_bytes;
        assert!(after_insert > 0);
        assert_eq!(
            context.owned_cache_bytes,
            state_cache_entry_bytes(&key, &StateStatus::InProgress)
        );

        let witness = StateStatus::SatWitness(CanonicalGraphKey::from_bytes(vec![7; 32]));
        context.insert_state_status(key.clone(), witness);
        let after_replace = context.stats.instrumentation.peak_memory_bytes;
        assert!(after_replace >= after_insert);
        assert!(context.owned_cache_bytes >= after_insert);

        context.remove_state_status(&key);
        assert_eq!(context.owned_cache_bytes, 0);
        assert_eq!(
            context.stats.instrumentation.peak_memory_bytes, after_replace,
            "releasing a speculative entry must not reduce the reported peak"
        );
    }

    #[test]
    fn one_node_results_also_match_the_reference_solver_optimum() {
        for problem in [
            problem(&["2"], &["1", "1"], "2"),
            problem(&["1", "2"], &["3"], "3"),
        ] {
            let result = solve_reference(
                &problem,
                &ReferenceOptions { max_nodes: 1 },
                &AtomicBool::new(false),
            )
            .unwrap();
            let SolveResult::Optimal(reference) = result else {
                panic!("reference must prove the one-node optimum");
            };
            let profile = if problem.inputs.len() == 1 {
                profile(1, 0, 0, 0)
            } else {
                profile(0, 0, 1, 0)
            };
            let production = exhaustive_witness(search(&problem, profile)).unwrap();
            assert_eq!(
                production.canonical_graph_key,
                reference.canonical_graph_key
            );
            assert_eq!(production.graph, reference.graph);
        }
    }

    fn normalized_problem(problem: &Problem) -> Problem {
        let normalized = normalized(problem);
        Problem {
            inputs: normalized.inputs.as_slice().to_vec(),
            outputs: normalized.outputs.as_slice().to_vec(),
            max_link_rate: normalized.max_link_rate,
        }
    }

    fn integer_problem(inputs: &[u32], outputs: &[u32], capacity: u32) -> Problem {
        Problem {
            inputs: inputs.iter().copied().map(Rational::from).collect(),
            outputs: outputs.iter().copied().map(Rational::from).collect(),
            max_link_rate: Rational::from(capacity),
        }
    }

    fn assert_matches_reference(problem: &Problem, profile: NodeProfile) {
        let normalized = normalized(problem);
        let canonical_problem = normalized_problem(problem);
        let production = exhaustive_witness(search_profile(
            &normalized,
            profile,
            &AtomicBool::new(false),
        ));
        let reference = reference_fixed_profile(&canonical_problem, profile);
        assert_eq!(
            production, reference,
            "fixed profile {profile:?} for {canonical_problem:?}"
        );
    }

    fn positive_ordered_partitions(total: u32, max_parts: usize) -> Vec<Vec<u32>> {
        fn append(
            remaining: u32,
            parts_left: usize,
            prefix: &mut Vec<u32>,
            result: &mut Vec<Vec<u32>>,
        ) {
            if parts_left == 1 {
                if remaining != 0 {
                    prefix.push(remaining);
                    result.push(prefix.clone());
                    prefix.pop();
                }
                return;
            }
            for next in 1..remaining {
                prefix.push(next);
                append(remaining - next, parts_left - 1, prefix, result);
                prefix.pop();
            }
        }

        let mut result = Vec::new();
        for parts in 1..=max_parts.min(usize::try_from(total).unwrap()) {
            append(total, parts, &mut Vec::new(), &mut result);
        }
        result
    }

    fn two_splitter_partial(
        problem: &Problem,
        node_by_role: [u32; 2],
        reverse_links: bool,
    ) -> PartialTopology {
        let nodes = (0..2)
            .map(|id| PhysicalNode {
                id: NodeId(id),
                node_type: NodeType::Splitter2,
            })
            .collect();
        let root = NodeId(node_by_role[0]);
        let child = NodeId(node_by_role[1]);
        let mut links = vec![
            PartialLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                consumer: ConsumerPortRef::Node {
                    node: root,
                    port: 0,
                },
                flow: None,
            },
            PartialLink {
                producer: ProducerPortRef::Node {
                    node: root,
                    port: 1,
                },
                consumer: ConsumerPortRef::Node {
                    node: child,
                    port: 0,
                },
                flow: None,
            },
            PartialLink {
                producer: ProducerPortRef::Node {
                    node: child,
                    port: 1,
                },
                consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                flow: None,
            },
        ];
        if reverse_links {
            links.reverse();
        }
        PartialTopology {
            discard_count: 0,
            problem: problem.clone(),
            nodes,
            links,
            remaining_profile: NodeProfile::default(),
        }
    }

    #[test]
    fn complete_exact_link_guard_preserves_witnesses_and_cancellation() {
        let caller = problem(&["4"], &["2", "1", "1"], "1200");
        let problem = normalized(&caller);
        let fixed = profile(2, 0, 0, 0);
        let mut partial = two_splitter_partial(&normalized_problem(&caller), [0, 1], false);
        partial.links.extend([
            PartialLink {
                producer: ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 0,
                },
                consumer: ConsumerPortRef::Output(OutputTerminalIndex(2)),
                flow: None,
            },
            PartialLink {
                producer: ProducerPortRef::Node {
                    node: NodeId(1),
                    port: 0,
                },
                consumer: ConsumerPortRef::Output(OutputTerminalIndex(1)),
                flow: None,
            },
        ]);
        let state = TopologyState::from_partial_topology(&partial).unwrap();
        let key = canonicalize_state(&partial);
        for expected in [None, Some(1), Some(2)] {
            for cancelled in [false, true] {
                let cancel = AtomicBool::new(cancelled);
                let mut context = SearchContext::new(&problem, fixed, &cancel);
                context.expected_link_count = expected;
                context.expected_physical_link_count = 5;
                context.expected_discard_link_count = 0;
                let result = evaluate_complete_state(
                    &state,
                    &mut context,
                    StateCacheToken::Exact(key.clone()),
                );
                if expected == Some(2) {
                    assert!(matches!(result, DfsResult::Exhausted(None)));
                    assert!(context.best_witness.is_none());
                    assert_eq!(context.stats.rejected_complete_topologies, 1);
                } else if cancelled {
                    assert!(matches!(result, DfsResult::Incomplete));
                    assert!(context.best_witness.is_none());
                } else {
                    assert!(matches!(result, DfsResult::Exhausted(Some(_))));
                    assert_eq!(context.best_witness.unwrap().validation.link_count, 1);
                }
            }
        }
    }

    fn partial_search_outcome(
        problem: &NormalizedProblem,
        profile: NodeProfile,
        partial: &PartialTopology,
    ) -> (Option<CanonicalGraphKey>, Option<CanonicalGraphKey>) {
        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(problem, profile, &cancel);
        let inputs = u32::try_from(problem.inputs.len()).unwrap();
        let outputs = u32::try_from(problem.outputs.len()).unwrap();
        let accounting = profile_link_accounting(
            profile,
            inputs,
            outputs,
            &problem.surplus,
            &problem.max_link_rate,
        )
        .unwrap()
        .unwrap();
        context.expected_link_count = Some(accounting.link_count);
        context.expected_physical_link_count = accounting.physical_link_count;
        context.expected_discard_link_count = accounting.discard_link_count;
        let mut state = TopologyState::from_partial_topology(partial).unwrap();
        let mut propagation = Some(PropagationState::new(&state, &problem.max_link_rate).unwrap());
        let subtree = exhausted_key(search_state(&mut state, &mut propagation, &mut context));
        let witness = context
            .best_witness
            .map(|witness| witness.canonical_graph_key);
        (subtree, witness)
    }

    fn assert_propagation_semantics_eq(
        topology: &TopologyState,
        left: &PropagationState,
        right: &PropagationState,
    ) {
        assert_eq!(left.current_outcome(), right.current_outcome());
        for variable in topology
            .producer_ports()
            .values()
            .chain(topology.consumer_ports().values())
            .map(|port| port.flow_var)
        {
            assert_eq!(
                left.known_value(variable).unwrap(),
                right.known_value(variable).unwrap()
            );
        }
        assert_eq!(
            left.partial_topology_with_known_link_flows(topology)
                .unwrap(),
            right
                .partial_topology_with_known_link_flows(topology)
                .unwrap()
        );
    }

    fn exhausted_key(result: DfsResult) -> Option<CanonicalGraphKey> {
        match result {
            DfsResult::Exhausted(key) => key,
            DfsResult::Incomplete => panic!("uncancelled partial search became incomplete"),
            DfsResult::Failed(error) => panic!("partial search failed: {error}"),
        }
    }
    #[test]
    fn shared_completed_hits_preserve_each_partition_sat_witness() {
        let normalized = normalized(&problem(&["2", "3"], &["1", "4"], "5"));
        let fixed = profile(1, 0, 1, 0);
        let cancel = AtomicBool::new(false);
        let shared = SharedStateCache::default();
        let partitions = root_partitions(&normalized, fixed);
        let mut expected = Vec::new();
        for partition in &partitions {
            let result = search_profile_root_partition_with_execution(
                &normalized,
                fixed,
                &cancel,
                None,
                partition,
                true,
                None,
                SearchExecution {
                    shared: Some(&shared),
                    donations: None,
                },
            );
            expected.push(exhausted_result(result).0);
        }
        assert!(!shared.witnesses(fixed).is_empty());
        let mut hits = 0;
        for (partition, expected) in partitions.iter().zip(expected) {
            let result = search_profile_root_partition_with_execution(
                &normalized,
                fixed,
                &cancel,
                None,
                partition,
                true,
                None,
                SearchExecution {
                    shared: Some(&shared),
                    donations: None,
                },
            );
            let (actual, stats) = exhausted_result(result);
            assert_eq!(actual, expected);
            hits += stats.instrumentation.shared_cache_hits;
        }
        assert!(hits > 0);
    }

    #[test]
    fn shared_completed_proofs_are_isolated_by_exact_l() {
        let normalized = normalized(&problem(&["2"], &["1", "1"], "2"));
        let state = TopologyState::new(&normalized, profile(1, 0, 0, 0)).unwrap();
        let key = canonicalize_state(&state.partial_topology());
        let shared = SharedStateCache::default();
        shared.insert(Some(1), key.clone(), StateStatus::ProvenDead);
        assert_eq!(shared.lookup(Some(1), &key), Some(StateStatus::ProvenDead));
        assert_eq!(shared.lookup(Some(2), &key), None);
        assert_eq!(shared.lookup(None, &key), None);
    }

    #[test]
    fn donated_child_panic_returns_failure_without_publishing_parent_proof() {
        let normalized = normalized(&problem(&["2", "3"], &["1", "4"], "5"));
        let fixed = profile(1, 0, 1, 0);
        let cancel = AtomicBool::new(false);
        let shared = SharedStateCache::default();
        let pool = DonationPool::new(1);
        pool.panic_next_job.store(true, Ordering::Relaxed);
        let mut failed = false;
        for partition in root_partitions(&normalized, fixed) {
            let result = search_profile_root_partition_with_execution(
                &normalized,
                fixed,
                &cancel,
                None,
                &partition,
                true,
                None,
                SearchExecution {
                    shared: Some(&shared),
                    donations: Some(&pool),
                },
            );
            failed |= matches!(result, ProfileSearchResult::Failed { .. });
        }
        assert!(failed);
        assert!(!pool.panic_next_job.load(Ordering::Relaxed));
    }
}
