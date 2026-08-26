//! Exhaustive production search for one fixed physical node profile.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use solver_api::{
    CanonicalGraphKey, IncompleteReason, NodeProfile, PhysicalGraph, PhysicalLink, Problem,
    Rational, SearchInstrumentation, ValidationSummary,
};
use solver_validation::{ValidationError, solve_topology, validate_solution};
use thiserror::Error;

use crate::{
    algebra::sparse::Consistency,
    canonical::{
        CanonicalFlowEndpoint, ConstructibilityCache, MarkedLinkCanonicalKey, PartialTopology,
        SccSummaryKey, StateKey, canonicalize_local_decision_core_cancellable,
        canonicalize_marked_link_cancellable,
        canonicalize_scc_summary_input_with_relabeling_cancellable, canonicalize_state_cancellable,
        canonicalize_structural_decision_core_cancellable, canonicalize_witness_cancellable,
    },
    component_search::{ComponentResolver, discover_components},
    components::Component,
    hotspot_profile,
    lower_bound::profile_impossibility,
    no_good::{
        CanonicalNoGoodKey, ConflictCore, ConflictRule, NoGoodScope, SolveLocalNoGoods,
        StructuralNoGoodStore,
    },
    problem::NormalizedProblem,
    profile::{ProfileArithmeticError, ProfileLinkAccounting, profile_link_accounting},
    propagation::{
        PropagationCheckpoint, PropagationConflict, PropagationError, PropagationOutcome,
        PropagationState,
    },
    reachability::{ReachabilityVerdict, analyze_reachability},
    scc::{SccError, detect_affected_sccs, summarize_open_scc},
    topology::{
        AppliedComponentBatch, ComponentAttachment, FlowVarId, TopologyDecision, TopologyState,
    },
};

#[cfg(test)]
use crate::canonical::canonicalize_state;

/// Maximum induced decision subsets canonicalized during one no-good lookup.
///
/// Reaching this limit returns "no hit" and ordinary exhaustive search
/// continues. It can therefore reduce acceleration but can never remove a
/// completion or manufacture a proof.
const MAX_NO_GOOD_SUBSET_PROJECTIONS: usize = 8_192;

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
    value
        .numerator()
        .to_signed_bytes_le()
        .len()
        .saturating_add(value.denominator().to_signed_bytes_le().len())
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
    first_decision: Option<TopologyDecision>,
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
    search_profile_with_features(
        problem,
        profile,
        cancel,
        &[],
        SearchFeatures::WITHOUT_COMPONENTS,
    )
}

/// Exhaustively searches one fixed profile with optional certified component
/// macro transitions.
///
/// Components only add batch construction paths. Ordinary physical decisions
/// remain enabled for every state, so an empty, cold, disabled, or incomplete
/// catalog cannot affect completeness or the exact optimum.
#[must_use]
pub fn search_profile_with_components(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    components: &[Component],
) -> ProfileSearchResult {
    search_profile_with_features(
        problem,
        profile,
        cancel,
        components,
        SearchFeatures::OPTIMIZED,
    )
}

/// Exhaustively searches one fixed profile with live component discovery.
///
/// The resolver may add proof-complete macro construction paths. Provider,
/// repository, optimization, or recursive-work failures are treated as misses;
/// the primitive physical transition loop remains the complete proof path.
#[must_use]
pub fn search_profile_with_component_resolver(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    components: &[Component],
    resolver: &dyn ComponentResolver,
) -> ProfileSearchResult {
    let structural_no_goods = Arc::new(StructuralNoGoodStore::default());
    search_profile_with_component_resolver_and_no_goods(
        problem,
        profile,
        cancel,
        components,
        resolver,
        structural_no_goods,
    )
}

/// Searches one profile while sharing only problem-independent structural
/// no-goods with sibling profile obligations.
#[must_use]
pub(crate) fn search_profile_with_component_resolver_and_no_goods(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    components: &[Component],
    resolver: &dyn ComponentResolver,
    structural_no_goods: Arc<StructuralNoGoodStore>,
) -> ProfileSearchResult {
    search_profile_with_features_and_resolver(
        problem,
        profile,
        cancel,
        components,
        SearchFeatures::OPTIMIZED,
        Some(resolver),
        structural_no_goods,
    )
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
    plan_profile_root_partitions_with_cache(
        problem,
        profile,
        cancel,
        None,
        &ConstructibilityCache::default(),
    )
}

pub(crate) fn plan_profile_root_partitions_with_cache(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    accounting: Option<ProfileLinkAccounting>,
    _constructibility_cache: &ConstructibilityCache,
) -> RootPartitionPlan {
    let initialized = initialize_profile_search(
        problem,
        profile,
        cancel,
        &[],
        SearchFeatures::WITHOUT_COMPONENTS,
        None,
        Arc::new(StructuralNoGoodStore::default()),
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
            first_decision: Some(decision),
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
/// Each invocation owns its state cache and solve-local no-goods. Only proven
/// problem-independent structural no-goods may be shared with sibling leaves.
/// An `Exhausted` result therefore discharges this partition and no other one;
/// `Incomplete` or `Failed` must remain visible to the parent proof ledger.
#[must_use]
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn search_profile_root_partition_with_component_resolver_and_no_goods(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    components: &[Component],
    resolver: &dyn ComponentResolver,
    structural_no_goods: Arc<StructuralNoGoodStore>,
    partition: &RootPartition,
) -> ProfileSearchResult {
    search_profile_root_partition_with_caches(
        problem,
        profile,
        cancel,
        components,
        resolver,
        structural_no_goods,
        Arc::new(ConstructibilityCache::default()),
        None,
        partition,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn search_profile_root_partition_with_caches(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    components: &[Component],
    resolver: &dyn ComponentResolver,
    structural_no_goods: Arc<StructuralNoGoodStore>,
    constructibility_cache: Arc<ConstructibilityCache>,
    accounting: Option<ProfileLinkAccounting>,
    partition: &RootPartition,
    collect_all_witnesses: bool,
) -> ProfileSearchResult {
    let initialized = initialize_profile_search(
        problem,
        profile,
        cancel,
        components,
        SearchFeatures::PRODUCTION,
        Some(resolver),
        structural_no_goods,
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
    context.constructibility_cache = constructibility_cache;
    context.collect_all_witnesses = collect_all_witnesses;

    let result = match partition.first_decision {
        None => {
            if partition.stable_key == terminal_root_partition().stable_key {
                search_state(&mut state, &mut propagation, &mut context)
            } else {
                DfsResult::Failed(ProfileSearchError::InvalidRootPartition(
                    "terminal partition key is malformed",
                ))
            }
        }
        Some(decision) => {
            if cancel.load(Ordering::Relaxed) {
                return context.incomplete(IncompleteReason::Cancelled, started);
            }
            let Ok(decision_id) = state.apply_legal_decision(decision) else {
                return context.failed(
                    ProfileSearchError::InvalidRootPartition(
                        "first decision is not legal at this root",
                    ),
                    started,
                );
            };
            let Some(link_index) = state.link_index_for(decision_id) else {
                return context.failed(ProfileSearchError::MissingAppliedLink, started);
            };
            let topology = state.partial_topology();
            let Some(actual_key) =
                canonicalize_marked_link_cancellable(&topology, link_index, cancel)
                    .map(|key| augmentation_partition_key(&key))
            else {
                return context.incomplete(IncompleteReason::Cancelled, started);
            };
            if actual_key == partition.stable_key {
                increment(&mut context.stats.instrumentation.raw_structural_decisions);
                evaluate_applied_child(&mut state, &mut propagation, &mut context, link_index)
            } else {
                DfsResult::Failed(ProfileSearchError::InvalidRootPartition(
                    "first decision does not match its partition key",
                ))
            }
        }
    };
    finish_profile_search(context, started, result)
}

fn terminal_root_partition() -> RootPartition {
    RootPartition {
        id: RootPartitionId(0),
        stable_key: vec![0],
        first_decision: None,
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
    no_goods: NoGoodMode,
    reachability: bool,
    dynamic_scc: bool,
    scc_cache: SccCacheMode,
    component_macros: ComponentMacroMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComponentMacroMode {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NoGoodMode {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(not(test), allow(dead_code))]
enum SccCacheMode {
    Disabled,
    Enabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProfileBounds;

impl SearchFeatures {
    const OPTIMIZED: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        no_goods: NoGoodMode::Enabled,
        reachability: true,
        dynamic_scc: true,
        scc_cache: SccCacheMode::Enabled,
        component_macros: ComponentMacroMode::Enabled,
    };

    const PRODUCTION: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        no_goods: NoGoodMode::Disabled,
        reachability: true,
        dynamic_scc: true,
        scc_cache: SccCacheMode::Enabled,
        component_macros: ComponentMacroMode::Disabled,
    };

    #[cfg(test)]
    const UNOPTIMIZED: Self = Self {
        exact_propagation: false,
        profile_lower_bounds: None,
        no_goods: NoGoodMode::Disabled,
        reachability: false,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        component_macros: ComponentMacroMode::Disabled,
    };

    #[cfg(test)]
    const PROPAGATION_ONLY: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: None,
        no_goods: NoGoodMode::Disabled,
        reachability: false,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        component_macros: ComponentMacroMode::Disabled,
    };

    #[cfg(test)]
    const REACHABILITY_ONLY: Self = Self {
        exact_propagation: false,
        profile_lower_bounds: None,
        no_goods: NoGoodMode::Disabled,
        reachability: true,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        component_macros: ComponentMacroMode::Disabled,
    };

    #[cfg(test)]
    const NO_GOODS_ONLY: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: None,
        no_goods: NoGoodMode::Enabled,
        reachability: false,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        component_macros: ComponentMacroMode::Disabled,
    };

    #[cfg(test)]
    const WITHOUT_SCC: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        no_goods: NoGoodMode::Enabled,
        reachability: true,
        dynamic_scc: false,
        scc_cache: SccCacheMode::Disabled,
        component_macros: ComponentMacroMode::Disabled,
    };

    #[cfg(test)]
    const WITHOUT_NO_GOODS: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        no_goods: NoGoodMode::Disabled,
        reachability: true,
        dynamic_scc: true,
        scc_cache: SccCacheMode::Enabled,
        component_macros: ComponentMacroMode::Disabled,
    };

    #[cfg(test)]
    const WITHOUT_SCC_CACHE: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        no_goods: NoGoodMode::Enabled,
        reachability: true,
        dynamic_scc: true,
        scc_cache: SccCacheMode::Disabled,
        component_macros: ComponentMacroMode::Disabled,
    };

    const WITHOUT_COMPONENTS: Self = Self {
        exact_propagation: true,
        profile_lower_bounds: Some(ProfileBounds),
        no_goods: NoGoodMode::Enabled,
        reachability: true,
        dynamic_scc: true,
        scc_cache: SccCacheMode::Enabled,
        component_macros: ComponentMacroMode::Disabled,
    };
}

fn search_profile_with_features(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    components: &[Component],
    features: SearchFeatures,
) -> ProfileSearchResult {
    search_profile_with_features_and_resolver(
        problem,
        profile,
        cancel,
        components,
        features,
        None,
        Arc::new(StructuralNoGoodStore::default()),
    )
}

fn search_profile_with_features_and_resolver(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
    components: &[Component],
    features: SearchFeatures,
    resolver: Option<&dyn ComponentResolver>,
    structural_no_goods: Arc<StructuralNoGoodStore>,
) -> ProfileSearchResult {
    let initialized = initialize_profile_search(
        problem,
        profile,
        cancel,
        components,
        features,
        resolver,
        structural_no_goods,
        None,
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
    let result = search_state(&mut state, &mut propagation, &mut context);
    finish_profile_search(context, started, result)
}

struct InitializedSearch<'a> {
    context: SearchContext<'a>,
    state: TopologyState,
    propagation: Option<PropagationState>,
    started: Instant,
}

#[allow(clippy::too_many_arguments)]
fn initialize_profile_search<'a>(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &'a AtomicBool,
    components: &[Component],
    features: SearchFeatures,
    resolver: Option<&'a dyn ComponentResolver>,
    structural_no_goods: Arc<StructuralNoGoodStore>,
    expected_accounting: Option<ProfileLinkAccounting>,
) -> Result<InitializedSearch<'a>, Box<ProfileSearchResult>> {
    let started = Instant::now();
    let mut context = SearchContext::new_with_resolver(
        problem,
        profile,
        cancel,
        components,
        features,
        resolver,
        structural_no_goods,
    );
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
    scc_cache: HashMap<SccSummaryKey, CachedOpenSccSummary>,
    owned_cache_bytes: u64,
    local_no_goods: SolveLocalNoGoods,
    structural_no_goods: Arc<StructuralNoGoodStore>,
    constructibility_cache: Arc<ConstructibilityCache>,
    best_witness: Option<ProfileWitness>,
    witnesses: BTreeMap<CanonicalGraphKey, ProfileWitness>,
    collect_all_witnesses: bool,
    stats: ProfileSearchStats,
    features: SearchFeatures,
    components: Vec<Component>,
    component_resolver: Option<&'a dyn ComponentResolver>,
}

impl<'a> SearchContext<'a> {
    #[cfg(test)]
    fn new(normalized: &NormalizedProblem, profile: NodeProfile, cancel: &'a AtomicBool) -> Self {
        Self::new_with_features(normalized, profile, cancel, &[], SearchFeatures::OPTIMIZED)
    }

    #[cfg(test)]
    fn new_with_features(
        normalized: &NormalizedProblem,
        profile: NodeProfile,
        cancel: &'a AtomicBool,
        components: &[Component],
        features: SearchFeatures,
    ) -> Self {
        Self::new_with_resolver(
            normalized,
            profile,
            cancel,
            components,
            features,
            None,
            Arc::new(StructuralNoGoodStore::default()),
        )
    }

    fn new_with_resolver(
        normalized: &NormalizedProblem,
        profile: NodeProfile,
        cancel: &'a AtomicBool,
        components: &[Component],
        features: SearchFeatures,
        component_resolver: Option<&'a dyn ComponentResolver>,
        structural_no_goods: Arc<StructuralNoGoodStore>,
    ) -> Self {
        let mut components = components.to_vec();
        if let Some(resolver) = component_resolver {
            components.extend(resolver.ordered_components());
        }
        canonicalize_component_catalog(&mut components);
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
            scc_cache: HashMap::new(),
            owned_cache_bytes: 0,
            local_no_goods: SolveLocalNoGoods::default(),
            structural_no_goods,
            constructibility_cache: Arc::new(ConstructibilityCache::default()),
            best_witness: None,
            witnesses: BTreeMap::new(),
            collect_all_witnesses: false,
            stats: ProfileSearchStats::default(),
            features,
            components,
            component_resolver,
        }
    }

    fn record_canonicalization(&mut self, elapsed: Duration) {
        add_duration_ns(
            &mut self.stats.instrumentation.canonicalization_time_ns,
            elapsed,
        );
    }

    fn record_algebra(&mut self, elapsed: Duration) {
        add_duration_ns(&mut self.stats.instrumentation.algebra_time_ns, elapsed);
    }

    fn record_propagation_prune(&mut self, conflict: &PropagationConflict) {
        match conflict {
            PropagationConflict::SparseInconsistency
            | PropagationConflict::ExactConstraintContradiction => {
                increment(&mut self.stats.instrumentation.propagation_contradictions);
            }
            PropagationConflict::NonPositiveKnown { .. }
            | PropagationConflict::CapacityExceeded { .. }
            | PropagationConflict::NegativeRatio { .. }
            | PropagationConflict::ExactBoundViolation { .. } => {
                increment(&mut self.stats.instrumentation.capacity_prunes);
            }
        }
    }

    fn retain_witness(&mut self, candidate: ProfileWitness) {
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
        let old_bytes = self
            .cache
            .get(&key)
            .map_or(0, |old| state_cache_entry_bytes(&key, old));
        let new_bytes = state_cache_entry_bytes(&key, &status);
        self.cache.insert(key, status);
        self.replace_owned_cache_bytes(old_bytes, new_bytes);
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

    fn finish_stats(&mut self, started: Instant) {
        self.stats.instrumentation.wall_time_ms =
            u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    }

    fn exhausted(mut self, started: Instant) -> ProfileSearchResult {
        self.finish_stats(started);
        ProfileSearchResult::Exhausted {
            best_witness: self.best_witness,
            witnesses: self.witnesses.into_values().collect(),
            stats: self.stats,
        }
    }

    fn incomplete(mut self, reason: IncompleteReason, started: Instant) -> ProfileSearchResult {
        self.finish_stats(started);
        ProfileSearchResult::Incomplete {
            reason,
            best_witness: self.best_witness,
            witnesses: self.witnesses.into_values().collect(),
            stats: self.stats,
        }
    }

    fn failed(mut self, error: ProfileSearchError, started: Instant) -> ProfileSearchResult {
        self.finish_stats(started);
        ProfileSearchResult::Failed {
            error,
            stats: self.stats,
        }
    }
}

fn canonicalize_component_catalog(components: &mut Vec<Component>) {
    components.sort_by(|left, right| left.canonical_key().cmp(right.canonical_key()));
    components.dedup_by(|left, right| left.canonical_key() == right.canonical_key());
}

fn discover_live_components(
    state: &TopologyState,
    propagation: Option<&PropagationState>,
    context: &mut SearchContext<'_>,
) {
    if context.features.component_macros != ComponentMacroMode::Enabled {
        return;
    }
    let Some(resolver) = context.component_resolver else {
        return;
    };

    // Other profiles may have completed work since this context was created.
    // Adding their certified physical macros cannot change this state's
    // completion set because every primitive link transition remains below.
    context.components.extend(resolver.ordered_components());
    let Some(propagation) = propagation else {
        canonicalize_component_catalog(&mut context.components);
        return;
    };

    for discovered in discover_components(state, propagation) {
        if context.cancel.load(Ordering::Relaxed) {
            return;
        }
        let resolution =
            resolver.resolve(&discovered.component, &discovered.request, context.cancel);
        context.stats.instrumentation.component_db_lookups = context
            .stats
            .instrumentation
            .component_db_lookups
            .saturating_add(resolution.source_lookups);
        context.stats.instrumentation.component_db_hits = context
            .stats
            .instrumentation
            .component_db_hits
            .saturating_add(resolution.source_hits);
        context.stats.instrumentation.component_optimizations = context
            .stats
            .instrumentation
            .component_optimizations
            .saturating_add(resolution.optimizations);
        if let Some(component) = resolution.component {
            context.components.push(component);
        }
    }
    context.components.extend(resolver.ordered_components());
    canonicalize_component_catalog(&mut context.components);
}

fn learn_local_no_good(
    state: &TopologyState,
    propagation: Option<&PropagationState>,
    context: &mut SearchContext<'_>,
    rule: ConflictRule,
) -> Result<(), ProfileSearchError> {
    if context.features.no_goods == NoGoodMode::Disabled {
        return Ok(());
    }
    let snapshot = match propagation {
        Some(propagation) => propagation
            .partial_topology_with_known_link_flows(state)
            .map_err(|error| propagation_error(&error))?,
        None => state.partial_topology(),
    };
    let core = propagation
        .and_then(PropagationState::conflict_provenance)
        .map_or_else(
            || {
                // Dynamic-SCC contradictions currently arrive from the SCC
                // analyzer rather than propagation's fact store. Its broad
                // active-decision proof remains sound and solve-local.
                ConflictCore::from_active_decisions(
                    NoGoodScope::SolveLocal,
                    rule,
                    state.links().iter().map(|link| link.decision_id),
                )
            },
            |provenance| {
                // Exact propagation and capacity conflicts retain their proof
                // parents as facts are derived. Recovering the core from that
                // DAG avoids inventing an unrelated parent set at prune time.
                ConflictCore::from_provenance(NoGoodScope::SolveLocal, rule, provenance)
            },
        );
    let canonical_started = Instant::now();
    let key = canonical_no_good_key(
        state,
        &snapshot,
        &core,
        NoGoodScope::SolveLocal,
        context.cancel,
    );
    let elapsed = canonical_started.elapsed();
    context.record_canonicalization(elapsed);
    hotspot_profile::record_learn_no_good(elapsed);
    let Some(key) = key else {
        // A stale/malformed provenance reference disables this optional
        // optimization. It is never converted into an impossibility proof.
        // Cancellation also skips learning; the search frame already observes
        // the same flag on the next interruptible seam.
        return Ok(());
    };
    context.local_no_goods.learn(key, core);
    Ok(())
}

fn learn_structural_no_good(
    state: &TopologyState,
    context: &mut SearchContext<'_>,
    rule: ConflictRule,
) {
    if context.features.no_goods == NoGoodMode::Disabled {
        return;
    }
    let core = ConflictCore::from_active_decisions(
        NoGoodScope::GlobalStructural,
        rule,
        state.links().iter().map(|link| link.decision_id),
    );
    let snapshot = state.partial_topology();
    let canonical_started = Instant::now();
    let key = canonical_no_good_key(
        state,
        &snapshot,
        &core,
        NoGoodScope::GlobalStructural,
        context.cancel,
    );
    let elapsed = canonical_started.elapsed();
    context.record_canonicalization(elapsed);
    hotspot_profile::record_learn_no_good(elapsed);
    let Some(key) = key else {
        return;
    };
    context.structural_no_goods.learn(key, core);
}

fn propagation_conflict_rule(conflict: &PropagationConflict) -> ConflictRule {
    match conflict {
        PropagationConflict::NonPositiveKnown { .. }
        | PropagationConflict::CapacityExceeded { .. }
        | PropagationConflict::NegativeRatio { .. }
        | PropagationConflict::ExactBoundViolation { .. } => ConflictRule::ExactCapacity,
        PropagationConflict::SparseInconsistency
        | PropagationConflict::ExactConstraintContradiction => ConflictRule::ExactPropagation,
    }
}

fn matches_structural_no_good(
    state: &TopologyState,
    context: &mut SearchContext<'_>,
) -> Option<bool> {
    if context.features.no_goods == NoGoodMode::Disabled || context.structural_no_goods.is_empty() {
        return Some(false);
    }
    let snapshot = state.partial_topology();
    let started = Instant::now();
    let counts = context
        .structural_no_goods
        .decision_counts_through(snapshot.links.len());
    let matched = matches_canonical_no_good(
        &snapshot,
        &counts,
        NoGoodScope::GlobalStructural,
        context.cancel,
        |key| context.structural_no_goods.contains(key),
    )?;
    let elapsed = started.elapsed();
    context.record_canonicalization(elapsed);
    hotspot_profile::record_no_good_match(elapsed);
    if matched {
        increment(&mut context.stats.instrumentation.no_good_hits);
        Some(true)
    } else {
        Some(false)
    }
}

fn matches_local_no_good(
    state: &TopologyState,
    propagation: Option<&PropagationState>,
    context: &mut SearchContext<'_>,
) -> Result<Option<bool>, ProfileSearchError> {
    if context.features.no_goods == NoGoodMode::Disabled || context.local_no_goods.is_empty() {
        return Ok(Some(false));
    }
    let snapshot = match propagation {
        Some(propagation) => propagation
            .partial_topology_with_known_link_flows(state)
            .map_err(|error| propagation_error(&error))?,
        None => state.partial_topology(),
    };
    let started = Instant::now();
    let counts = context
        .local_no_goods
        .decision_counts_through(snapshot.links.len());
    let matched = matches_canonical_no_good(
        &snapshot,
        &counts,
        NoGoodScope::SolveLocal,
        context.cancel,
        |key| context.local_no_goods.contains(key),
    );
    let elapsed = started.elapsed();
    context.record_canonicalization(elapsed);
    hotspot_profile::record_no_good_match(elapsed);
    let Some(matched) = matched else {
        return Ok(None);
    };
    if matched {
        increment(&mut context.stats.instrumentation.no_good_hits);
        Ok(Some(true))
    } else {
        Ok(Some(false))
    }
}

fn canonical_no_good_key(
    state: &TopologyState,
    snapshot: &PartialTopology,
    core: &ConflictCore,
    scope: NoGoodScope,
    cancel: &AtomicBool,
) -> Option<CanonicalNoGoodKey> {
    let selected_links = core
        .decisions
        .iter()
        .map(|&decision| state.link_index_for(decision))
        .collect::<Option<Vec<_>>>()?;
    canonical_no_good_subset_key(snapshot, &selected_links, scope, cancel)?
}

/// Tests whether the current decisions contain an isomorphic proven core.
///
/// # Soundness
///
/// A solve-local core cites only its exact problem/profile scope axioms and the
/// selected link decisions. Adding later decisions adds link equalities, node
/// equations, and exact bounds; it never removes an equation or a consequence
/// that established inconsistency, nonpositivity, or a capacity violation.
/// Structural connectivity cores are learned only from the reachability
/// analyzer's potential-boundary over-approximation, so even arbitrary future
/// attachments cannot revive them. Port-completion cores have no legal future
/// transition. Thus an isomorphic selected subset is a valid monotone reuse of
/// the retained proof. Exact rates/capacity distinguish local keys, while the
/// structural projection erases only semantics its proof scope forbids.
///
/// The subset budget is one-sided: exhaustion reports no match and leaves the
/// ordinary exhaustive branch live.
fn matches_canonical_no_good(
    snapshot: &PartialTopology,
    decision_counts: &[usize],
    scope: NoGoodScope,
    cancel: &AtomicBool,
    mut contains: impl FnMut(&CanonicalNoGoodKey) -> bool,
) -> Option<bool> {
    let link_count = snapshot.links.len();
    let mut examined = 0_usize;

    // Preserve the old full-state fast path before considering proper subsets.
    if decision_counts.binary_search(&link_count).is_ok() {
        let all_links = (0..link_count).collect::<Vec<_>>();
        examined = 1;
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if canonical_no_good_subset_key(snapshot, &all_links, scope, cancel)?
            .as_ref()
            .is_some_and(&mut contains)
        {
            return Some(true);
        }
    }

    for &decision_count in decision_counts {
        if decision_count >= link_count {
            continue;
        }
        let mut subset = (0..decision_count).collect::<Vec<_>>();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            if examined >= MAX_NO_GOOD_SUBSET_PROJECTIONS {
                return Some(false);
            }
            examined += 1;
            if canonical_no_good_subset_key(snapshot, &subset, scope, cancel)?
                .as_ref()
                .is_some_and(&mut contains)
            {
                return Some(true);
            }
            if decision_count == 0 || !advance_combination(&mut subset, link_count) {
                break;
            }
        }
    }
    Some(false)
}

// The outer option reports cancellation; the inner option reports that the
// selected links do not form a valid core in the requested proof scope.
#[allow(clippy::option_option)]
fn canonical_no_good_subset_key(
    snapshot: &PartialTopology,
    selected_links: &[usize],
    scope: NoGoodScope,
    cancel: &AtomicBool,
) -> Option<Option<CanonicalNoGoodKey>> {
    let state = match scope {
        NoGoodScope::SolveLocal => {
            match canonicalize_local_decision_core_cancellable(snapshot, selected_links, cancel) {
                Some(state) => state,
                None if cancel.load(Ordering::Relaxed) => return None,
                None => return Some(None),
            }
        }
        NoGoodScope::GlobalStructural => {
            match canonicalize_structural_decision_core_cancellable(
                snapshot,
                selected_links,
                cancel,
            ) {
                Some(state) => state,
                None if cancel.load(Ordering::Relaxed) => return None,
                None => return Some(None),
            }
        }
    };
    Some(CanonicalNoGoodKey::new(selected_links.len(), state))
}

fn advance_combination(combination: &mut [usize], universe: usize) -> bool {
    let size = combination.len();
    let Some(pivot) = (0..size)
        .rev()
        .find(|&index| combination[index] < universe - size + index)
    else {
        return false;
    };
    combination[pivot] += 1;
    for index in pivot + 1..size {
        combination[index] = combination[index - 1] + 1;
    }
    true
}

// Keeping cache ownership, terminal handling, macro-first acceleration, and
// primitive fallback in one frame makes the exhaustive-proof lifecycle
// auditable; extracting any one loop would obscure when InProgress is removed.
#[allow(clippy::too_many_lines)]
fn search_state(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    context: &mut SearchContext<'_>,
) -> DfsResult {
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
    let canonical_started = Instant::now();
    let Some(state_key) = canonicalize_state_cancellable(&snapshot, context.cancel) else {
        return DfsResult::Incomplete;
    };
    let elapsed = canonical_started.elapsed();
    context.record_canonicalization(elapsed);
    hotspot_profile::record_state_canonicalize(elapsed);

    if context.features.no_goods == NoGoodMode::Enabled {
        if !context.local_no_goods.is_empty() {
            let started = Instant::now();
            let counts = context
                .local_no_goods
                .decision_counts_through(snapshot.links.len());
            let matched = matches_canonical_no_good(
                &snapshot,
                &counts,
                NoGoodScope::SolveLocal,
                context.cancel,
                |key| context.local_no_goods.contains(key),
            );
            let elapsed = started.elapsed();
            context.record_canonicalization(elapsed);
            hotspot_profile::record_no_good_match(elapsed);
            match matched {
                None => return DfsResult::Incomplete,
                Some(true) => {
                    increment(&mut context.stats.instrumentation.no_good_hits);
                    return DfsResult::Exhausted(None);
                }
                Some(false) => {}
            }
        }
        if !context.structural_no_goods.is_empty() {
            let structural_snapshot = state.partial_topology();
            let started = Instant::now();
            let counts = context
                .structural_no_goods
                .decision_counts_through(structural_snapshot.links.len());
            let matched = matches_canonical_no_good(
                &structural_snapshot,
                &counts,
                NoGoodScope::GlobalStructural,
                context.cancel,
                |key| context.structural_no_goods.contains(key),
            );
            let elapsed = started.elapsed();
            context.record_canonicalization(elapsed);
            hotspot_profile::record_no_good_match(elapsed);
            match matched {
                None => return DfsResult::Incomplete,
                Some(true) => {
                    increment(&mut context.stats.instrumentation.no_good_hits);
                    return DfsResult::Exhausted(None);
                }
                Some(false) => {}
            }
        }
    }

    // A StateKey includes the complete colored partial incidence graph, exact
    // external semantics, open ports, and remaining node inventory. MRV's final
    // tie-break individualizes the open port canonically, and legal decisions are
    // ordered and deduplicated by their marked-child canonical keys. Thus equal
    // states traverse the same child equivalence classes and have identical legal
    // completion sets. Completed statuses may be reused as equivalence proofs.
    // InProgress remains distinct because its owner has not discharged that
    // obligation yet.
    if let Some(status) = context.cache.get(&state_key).cloned() {
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

    context.insert_state_status(state_key.clone(), StateStatus::InProgress);
    increment(&mut context.stats.instrumentation.canonical_states_retained);
    context.stats.instrumentation.peak_state_cache_size = context
        .stats
        .instrumentation
        .peak_state_cache_size
        .max(u64::try_from(context.cache.len()).unwrap_or(u64::MAX));

    discover_live_components(state, propagation.as_ref(), context);

    if state.is_complete() {
        return evaluate_complete_state(state, context, state_key);
    }

    let orbit_started = Instant::now();
    let Some(orbit) = state.selected_open_orbit_cancellable(context.cancel) else {
        hotspot_profile::record_legal_decisions(orbit_started.elapsed());
        context.remove_state_status(&state_key);
        return DfsResult::Incomplete;
    };
    let Some(orbit) = orbit else {
        hotspot_profile::record_legal_decisions(orbit_started.elapsed());
        // No open port means no future structural decision can attach a remaining
        // node or occupy a missing mandatory port. Since this state is not
        // complete, its completion set is empty.
        learn_structural_no_good(state, context, ConflictRule::PortCompletionImpossibility);
        context.insert_state_status(state_key, StateStatus::ProvenDead);
        return DfsResult::Exhausted(None);
    };
    let Some(decisions) = ({
        let decisions = state.legal_decisions_for_orbit_cancellable(&orbit, context.cancel);
        hotspot_profile::record_legal_decisions(orbit_started.elapsed());
        decisions
    }) else {
        context.remove_state_status(&state_key);
        return DfsResult::Incomplete;
    };

    let mut best_key = None;
    let mut attempted_transition = false;
    let mut macro_child_keys = BTreeSet::new();
    if context.features.component_macros == ComponentMacroMode::Enabled {
        let components = context.components.clone();
        for component in &components {
            if component.domain().is_proven_empty()
                || !profile_contains(state.remaining_profile(), component.profile())
            {
                continue;
            }
            for attachment in TopologyState::component_attachments_for_orbit(component, &orbit) {
                attempted_transition = true;
                if context.cancel.load(Ordering::Relaxed) {
                    context.remove_state_status(&state_key);
                    return DfsResult::Incomplete;
                }
                increment(&mut context.stats.instrumentation.raw_structural_decisions);
                match search_component(
                    state,
                    propagation,
                    context,
                    component,
                    attachment,
                    &mut macro_child_keys,
                ) {
                    DfsResult::Exhausted(child_key) => {
                        retain_smallest_key(&mut best_key, child_key);
                    }
                    DfsResult::Incomplete => {
                        context.remove_state_status(&state_key);
                        return DfsResult::Incomplete;
                    }
                    DfsResult::Failed(error) => {
                        context.remove_state_status(&state_key);
                        return DfsResult::Failed(error);
                    }
                }
            }
        }
    }

    // Macro children run first so a catalog can accelerate discovery and seed
    // cache entries. Every primitive decision is nevertheless exhausted below;
    // component availability is never a completeness dependency.
    for decision in decisions {
        attempted_transition = true;
        if context.cancel.load(Ordering::Relaxed) {
            context.remove_state_status(&state_key);
            return DfsResult::Incomplete;
        }
        increment(&mut context.stats.instrumentation.raw_structural_decisions);
        match search_decision(state, propagation, context, decision) {
            DfsResult::Exhausted(child_key) => retain_smallest_key(&mut best_key, child_key),
            DfsResult::Incomplete => {
                context.remove_state_status(&state_key);
                return DfsResult::Incomplete;
            }
            DfsResult::Failed(error) => {
                context.remove_state_status(&state_key);
                return DfsResult::Failed(error);
            }
        }
    }

    if !attempted_transition {
        context.insert_state_status(state_key, StateStatus::ProvenDead);
        return DfsResult::Exhausted(None);
    }

    let status = best_key.as_ref().map_or(StateStatus::Exhausted, |key| {
        StateStatus::SatWitness(key.clone())
    });
    context.insert_state_status(state_key, status);
    DfsResult::Exhausted(best_key)
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

fn search_component(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    context: &mut SearchContext<'_>,
    component: &Component,
    attachment: ComponentAttachment,
    sibling_keys: &mut BTreeSet<StateKey>,
) -> DfsResult {
    let topology_checkpoint = state.checkpoint();
    let propagation_checkpoint = propagation.as_ref().map(PropagationState::checkpoint);
    let apply_started = Instant::now();
    let batch = match state.apply_component(component, attachment) {
        Ok(batch) => batch,
        Err(error) => {
            hotspot_profile::record_component_apply(apply_started.elapsed());
            rollback_branch(
                state,
                propagation,
                topology_checkpoint,
                propagation_checkpoint,
            );
            return DfsResult::Failed(topology_error(&error));
        }
    };
    hotspot_profile::record_component_apply(apply_started.elapsed());
    increment(&mut context.stats.instrumentation.component_hits);

    let result =
        evaluate_component_child(state, propagation, context, component, &batch, sibling_keys);
    rollback_branch(
        state,
        propagation,
        topology_checkpoint,
        propagation_checkpoint,
    );
    result
}

fn evaluate_component_child(
    state: &mut TopologyState,
    propagation: &mut Option<PropagationState>,
    context: &mut SearchContext<'_>,
    component: &Component,
    batch: &AppliedComponentBatch,
    sibling_keys: &mut BTreeSet<StateKey>,
) -> DfsResult {
    match matches_structural_no_good(state, context) {
        None => return DfsResult::Incomplete,
        Some(true) => return DfsResult::Exhausted(None),
        Some(false) => {}
    }
    if let Some(propagation) = propagation.as_mut() {
        let propagation_started = Instant::now();
        let outcome = propagation.synchronize_after_component_batch(state, component, batch);
        let elapsed = propagation_started.elapsed();
        context.record_algebra(elapsed);
        hotspot_profile::record_propagation_sync(elapsed);
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => return DfsResult::Failed(propagation_error(&error)),
        };
        match matches_local_no_good(state, Some(propagation), context) {
            Ok(None) => return DfsResult::Incomplete,
            Ok(Some(true)) => return DfsResult::Exhausted(None),
            Ok(Some(false)) => {}
            Err(error) => return DfsResult::Failed(error),
        }
        if let PropagationOutcome::Pruned(conflict) = outcome {
            context.record_propagation_prune(&conflict);
            if let Err(error) = learn_local_no_good(
                state,
                Some(propagation),
                context,
                propagation_conflict_rule(&conflict),
            ) {
                return DfsResult::Failed(error);
            }
            return DfsResult::Exhausted(None);
        }
    }

    if context.features.dynamic_scc {
        match analyze_affected_dynamic_sccs(state, propagation.as_mut(), context, None) {
            Ok(DynamicSccVerdict::Open) => {}
            Ok(DynamicSccVerdict::ProvenDead) => return DfsResult::Exhausted(None),
            Ok(DynamicSccVerdict::Cancelled) => return DfsResult::Incomplete,
            Err(error) => return DfsResult::Failed(error),
        }
    }
    if context.features.reachability {
        let reachability_started = Instant::now();
        let reachability = analyze_reachability(state);
        hotspot_profile::record_reachability(reachability_started.elapsed());
        if matches!(reachability.verdict, ReachabilityVerdict::ProvenDead(_)) {
            increment(&mut context.stats.instrumentation.lower_bound_prunes);
            learn_structural_no_good(state, context, ConflictRule::ConnectivityImpossibility);
            return DfsResult::Exhausted(None);
        }
    }

    // A multi-link macro has no single canonical removable augmentation. Its
    // physical child is instead quotiented directly by the complete canonical
    // StateKey. Primitive transitions remain the unconditional construction
    // path and retain the single-link canonical-parent proof.
    let snapshot = match propagation {
        Some(propagation) => match propagation.partial_topology_with_known_link_flows(state) {
            Ok(snapshot) => snapshot,
            Err(error) => return DfsResult::Failed(propagation_error(&error)),
        },
        None => state.partial_topology(),
    };
    let canonical_started = Instant::now();
    let Some(child_key) = canonicalize_state_cancellable(&snapshot, context.cancel) else {
        return DfsResult::Incomplete;
    };
    let elapsed = canonical_started.elapsed();
    context.record_canonicalization(elapsed);
    hotspot_profile::record_state_canonicalize(elapsed);
    if !sibling_keys.insert(child_key) {
        increment(
            &mut context
                .stats
                .instrumentation
                .canonical_duplicates_eliminated,
        );
        return DfsResult::Exhausted(None);
    }
    search_state(state, propagation, context)
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
    match matches_structural_no_good(state, context) {
        None => return DfsResult::Incomplete,
        Some(true) => return DfsResult::Exhausted(None),
        Some(false) => {}
    }
    if let Some(propagation) = propagation.as_mut() {
        let propagation_started = Instant::now();
        let outcome = propagation.synchronize_after_topology_mutation(state);
        let elapsed = propagation_started.elapsed();
        context.record_algebra(elapsed);
        hotspot_profile::record_propagation_sync(elapsed);
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => return DfsResult::Failed(propagation_error(&error)),
        };
        match matches_local_no_good(state, Some(propagation), context) {
            Ok(None) => return DfsResult::Incomplete,
            Ok(Some(true)) => return DfsResult::Exhausted(None),
            Ok(Some(false)) => {}
            Err(error) => return DfsResult::Failed(error),
        }
        if let PropagationOutcome::Pruned(conflict) = outcome {
            context.record_propagation_prune(&conflict);
            if let Err(error) = learn_local_no_good(
                state,
                Some(propagation),
                context,
                propagation_conflict_rule(&conflict),
            ) {
                return DfsResult::Failed(error);
            }
            return DfsResult::Exhausted(None);
        }
    }

    if context.features.dynamic_scc {
        match analyze_affected_dynamic_sccs(state, propagation.as_mut(), context, Some(link_index))
        {
            Ok(DynamicSccVerdict::Open) => {}
            Ok(DynamicSccVerdict::ProvenDead) => return DfsResult::Exhausted(None),
            Ok(DynamicSccVerdict::Cancelled) => return DfsResult::Incomplete,
            Err(error) => return DfsResult::Failed(error),
        }
    }

    if context.features.reachability {
        let reachability_started = Instant::now();
        let reachability = analyze_reachability(state);
        hotspot_profile::record_reachability(reachability_started.elapsed());
        if matches!(reachability.verdict, ReachabilityVerdict::ProvenDead(_)) {
            increment(&mut context.stats.instrumentation.lower_bound_prunes);
            learn_structural_no_good(state, context, ConflictRule::ConnectivityImpossibility);
            return DfsResult::Exhausted(None);
        }
    }

    // Canonical state memoization below removes equivalent construction
    // histories directly. The former recursive canonical-last gate attempted
    // to reject those histories before memoization, but proving that a reverse
    // parent was itself constructible required dozens of full graph canons for
    // every applied link.
    search_state(state, propagation, context)
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
                learn_local_no_good(
                    state,
                    propagation.as_deref(),
                    context,
                    ConflictRule::DynamicSccInconsistency,
                )?;
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
                learn_local_no_good(
                    state,
                    Some(propagation),
                    context,
                    propagation_conflict_rule(&conflict),
                )?;
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

fn evaluate_complete_state(
    state: &TopologyState,
    context: &mut SearchContext<'_>,
    state_key: StateKey,
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
            context.insert_state_status(state_key, StateStatus::ProvenDead);
            hotspot_profile::record_evaluate_complete(complete_started.elapsed());
            return DfsResult::Exhausted(None);
        }
        Err(error) => {
            context.remove_state_status(&state_key);
            hotspot_profile::record_evaluate_complete(complete_started.elapsed());
            return DfsResult::Failed(ProfileSearchError::InvalidCompleteTopology(Box::new(error)));
        }
    };

    let canonical_started = Instant::now();
    let Some(canonical) =
        canonicalize_witness_cancellable(&context.problem, &solved, context.cancel)
    else {
        context.remove_state_status(&state_key);
        hotspot_profile::record_evaluate_complete(complete_started.elapsed());
        return DfsResult::Incomplete;
    };
    context.record_canonicalization(canonical_started.elapsed());
    let validation_started = Instant::now();
    let validation = validate_solution(&context.problem, &canonical.graph);
    context.record_algebra(validation_started.elapsed());
    let validation = match validation {
        Ok(validation) => validation,
        Err(error) => {
            context.remove_state_status(&state_key);
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
        context.insert_state_status(state_key, StateStatus::ProvenDead);
        hotspot_profile::record_evaluate_complete(complete_started.elapsed());
        return DfsResult::Exhausted(None);
    }
    if validation.node_count != context.expected_node_count
        || validation.physical_link_count != context.expected_physical_link_count
        || validation.discard_link_count != context.expected_discard_link_count
    {
        context.remove_state_status(&state_key);
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
    context.insert_state_status(state_key, StateStatus::SatWitness(key.clone()));
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

fn profile_contains(remaining: crate::topology::RemainingProfile, required: NodeProfile) -> bool {
    remaining.splitter2 >= required.splitter2
        && remaining.splitter3 >= required.splitter3
        && remaining.merger2 >= required.merger2
        && remaining.merger3 >= required.merger3
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
        component_application::ComponentApplicationRequest,
        component_search::{ApplicationComponentRuntime, ComponentResolution, EmptyComponentStore},
        prepare_problem,
        scc::{
            DeclaredBoundaryInput, DeclaredBoundaryOutput, FrozenSubsystemAnalysis,
            FrozenSubsystemDeclaration, analyze_frozen_subsystem,
        },
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

    fn partial_link(producer: ProducerPortRef, consumer: ConsumerPortRef) -> PartialLink {
        PartialLink {
            producer,
            consumer,
            flow: None,
        }
    }

    const fn splitter_output(node: u32) -> ProducerPortRef {
        ProducerPortRef::Node {
            node: NodeId(node),
            port: 0,
        }
    }

    const fn splitter_input(node: u32) -> ConsumerPortRef {
        ConsumerPortRef::Node {
            node: NodeId(node),
            port: 0,
        }
    }

    fn splitter_pair_partial(problem: &Problem, links: Vec<PartialLink>) -> PartialTopology {
        PartialTopology {
            discard_count: 0,
            problem: normalized_problem(problem),
            nodes: (0..2)
                .map(|id| PhysicalNode {
                    id: NodeId(id),
                    node_type: NodeType::Splitter2,
                })
                .collect(),
            links,
            remaining_profile: NodeProfile::default(),
        }
    }

    fn splitter_component(reverse_outputs: bool) -> Component {
        let topology = TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem: problem(&["2"], &["1", "1"], "10"),
            nodes: vec![PhysicalNode {
                id: NodeId(0),
                node_type: NodeType::Splitter2,
            }],
            links: Vec::new(),
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        let mut boundary_outputs = vec![
            DeclaredBoundaryOutput {
                port: ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 0,
                },
            },
            DeclaredBoundaryOutput {
                port: ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 1,
                },
            },
        ];
        if reverse_outputs {
            boundary_outputs.reverse();
        }
        let analysis = analyze_frozen_subsystem(
            &topology,
            &FrozenSubsystemDeclaration {
                nodes: vec![NodeId(0)],
                boundary_inputs: vec![DeclaredBoundaryInput {
                    port: ConsumerPortRef::Node {
                        node: NodeId(0),
                        port: 0,
                    },
                }],
                boundary_outputs,
            },
        )
        .unwrap();
        let FrozenSubsystemAnalysis::Symbolic(frozen) = analysis else {
            panic!("splitter must have a symbolic frozen contract");
        };
        Component::from_frozen(*frozen).unwrap()
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

    #[derive(Clone, Copy, Debug, Default)]
    struct NoComponents;

    impl ComponentResolver for NoComponents {
        fn ordered_components(&self) -> Vec<Component> {
            Vec::new()
        }

        fn resolve(
            &self,
            _candidate: &Component,
            _request: &ComponentApplicationRequest,
            _cancel: &AtomicBool,
        ) -> ComponentResolution {
            ComponentResolution::default()
        }
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
        let mut initialized = initialize_profile_search(
            &problem,
            profile,
            &cancel,
            &[],
            SearchFeatures::WITHOUT_COMPONENTS,
            None,
            Arc::new(StructuralNoGoodStore::default()),
            None,
        )
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
        let resolver = NoComponents;
        let structural_no_goods = Arc::new(StructuralNoGoodStore::default());
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
                let structural_no_goods = Arc::clone(&structural_no_goods);
                let resolver = &resolver;
                let cancel = &cancel;
                scope.spawn(move || {
                    for index in assigned {
                        let partition = &partitions[index];
                        let result =
                            search_profile_root_partition_with_component_resolver_and_no_goods(
                                problem,
                                profile,
                                cancel,
                                &[],
                                resolver,
                                Arc::clone(&structural_no_goods),
                                partition,
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
            &[],
            SearchFeatures::OPTIMIZED,
        ));
        let unoptimized = exhaustive_witness(search_profile_with_features(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
            &[],
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
    fn component_macro_on_off_and_reference_agree_with_surplus_discard() {
        let caller_problem = problem(&["1", "2"], &["1", "1"], "2");
        let normalized = normalized(&caller_problem);
        let canonical_problem = normalized_problem(&caller_problem);
        let fixed_profile = profile(1, 0, 0, 0);
        let component = splitter_component(false);
        let reference = reference_fixed_profile(&canonical_problem, fixed_profile);
        let (enabled, enabled_stats) = exhausted_result(search_profile_with_components(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
            &[component],
        ));
        let disabled = exhaustive_witness(search_profile(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
        ));
        assert_eq!(enabled, reference);
        assert_eq!(disabled, reference);
        assert!(enabled_stats.instrumentation.component_hits > 0);
        let witness = enabled.unwrap();
        assert_eq!(witness.validation.link_count, 0);
        assert_eq!(witness.validation.discard_link_count, 1);
        assert_eq!(witness.validation.physical_link_count, 4);
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
    fn splitter2_mrv_path_survives_the_admissible_canonical_last_gate() {
        // MRV first attaches a splitter output to an external output. The later
        // mandatory input-to-splitter link is canonical only among admissible
        // reverse transitions, not among every physical link in the child.
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
                &[],
                SearchFeatures::OPTIMIZED,
            ));
            let (without_scc, without_stats) = exhausted_result(search_profile_with_features(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                &[],
                SearchFeatures::WITHOUT_SCC,
            ));
            let (without_cache, without_cache_stats) =
                exhausted_result(search_profile_with_features(
                    &normalized,
                    fixed_profile,
                    &AtomicBool::new(false),
                    &[],
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
    fn component_macros_on_off_and_catalog_order_match_reference() {
        let caller_problem = problem(&["2"], &["1", "1"], "2");
        let normalized = normalized(&caller_problem);
        let fixed_profile = profile(1, 0, 0, 0);
        let reference =
            reference_fixed_profile(&normalized_problem(&caller_problem), fixed_profile);
        let left = splitter_component(false);
        let right = splitter_component(true);
        assert_eq!(left.canonical_key(), right.canonical_key());

        let catalog_a = vec![left.clone(), right.clone()];
        let catalog_b = vec![right, left];
        let (enabled_a, stats_a) = exhausted_result(search_profile_with_components(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
            &catalog_a,
        ));
        let (enabled_b, stats_b) = exhausted_result(search_profile_with_components(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
            &catalog_b,
        ));
        let (disabled, disabled_stats) = exhausted_result(search_profile_with_features(
            &normalized,
            fixed_profile,
            &AtomicBool::new(false),
            &catalog_a,
            SearchFeatures::WITHOUT_COMPONENTS,
        ));

        assert_eq!(enabled_a, reference);
        assert_eq!(enabled_b, reference);
        assert_eq!(disabled, reference);
        assert!(stats_a.instrumentation.component_hits > 0);
        assert_eq!(
            stats_a.instrumentation.component_hits,
            stats_b.instrumentation.component_hits
        );
        assert_eq!(
            stats_a.instrumentation.raw_structural_decisions,
            stats_b.instrumentation.raw_structural_decisions
        );
        assert_eq!(disabled_stats.instrumentation.component_hits, 0);
        let witness = enabled_a.expect("splitter profile must be satisfiable");
        validate_solution(&normalized_problem(&caller_problem), &witness.graph).unwrap();
    }

    #[test]
    fn live_discovery_on_off_and_empty_sources_match_the_reference() {
        let cases = [
            (problem(&["2"], &["1", "1"], "2"), profile(1, 0, 0, 0)),
            (problem(&["1", "1"], &["2"], "2"), profile(0, 0, 1, 0)),
            (problem(&["2"], &["1"], "2"), profile(1, 0, 0, 0)),
        ];
        let mut observed_optimizations = 0_u64;
        for (caller_problem, fixed_profile) in cases {
            let normalized = normalized(&caller_problem);
            let reference =
                reference_fixed_profile(&normalized_problem(&caller_problem), fixed_profile);
            let store = EmptyComponentStore;
            let runtime = ApplicationComponentRuntime::new(&store, &store);
            let (live, stats) = exhausted_result(search_profile_with_component_resolver(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                &[],
                &runtime,
            ));
            let disabled = exhaustive_witness(search_profile(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
            ));
            assert_eq!(live, reference);
            assert_eq!(disabled, reference);
            observed_optimizations = observed_optimizations
                .saturating_add(stats.instrumentation.component_optimizations);
        }
        assert!(observed_optimizations > 0);
    }

    #[test]
    fn every_pruning_layer_matches_reference_on_an_exhaustive_tiny_matrix() {
        let feature_sets = [
            ("none", SearchFeatures::UNOPTIMIZED),
            ("propagation", SearchFeatures::PROPAGATION_ONLY),
            ("propagation + no-goods", SearchFeatures::NO_GOODS_ONLY),
            ("reachability", SearchFeatures::REACHABILITY_ONLY),
            ("all", SearchFeatures::OPTIMIZED),
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
                                    &[],
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
    fn proof_carrying_no_goods_preserve_reference_results() {
        let cases = [
            (problem(&["1"], &["1"], "1"), profile(1, 0, 1, 0)),
            (problem(&["2", "3"], &["1", "4"], "5"), profile(1, 0, 1, 0)),
            (problem(&["2"], &["1"], "2"), profile(1, 0, 0, 0)),
            (problem(&["3"], &["1", "2"], "3"), profile(1, 0, 0, 0)),
        ];
        for (caller_problem, fixed_profile) in cases {
            let normalized = normalized(&caller_problem);
            let canonical_problem = normalized_problem(&caller_problem);
            let reference = reference_fixed_profile(&canonical_problem, fixed_profile);
            let (learned, _) = exhausted_result(search_profile_with_features(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                &[],
                SearchFeatures::WITHOUT_COMPONENTS,
            ));
            let (disabled, _) = exhausted_result(search_profile_with_features(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                &[],
                SearchFeatures::WITHOUT_NO_GOODS,
            ));
            assert_eq!(learned, reference);
            assert_eq!(disabled, reference);
        }
    }

    #[test]
    fn propagation_no_good_keeps_the_retained_conflict_proof() {
        let caller_problem = problem(&["1"], &["1"], "10");
        let normalized = normalized(&caller_problem);
        let state = TopologyState::from_partial_topology(&PartialTopology {
            discard_count: 0,
            problem: normalized_problem(&caller_problem),
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
        let propagation = PropagationState::new(&state, &normalized.max_link_rate).unwrap();
        let PropagationOutcome::Pruned(conflict) = propagation.current_outcome() else {
            panic!("closed splitter cycle must violate strict positivity");
        };
        let retained = propagation
            .conflict_provenance()
            .expect("propagation conflict has retained provenance");
        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(&normalized, profile(2, 0, 0, 0), &cancel);

        learn_local_no_good(
            &state,
            Some(&propagation),
            &mut context,
            propagation_conflict_rule(conflict),
        )
        .unwrap();
        let snapshot = propagation
            .partial_topology_with_known_link_flows(&state)
            .unwrap();
        let expected_core = ConflictCore::from_provenance(
            NoGoodScope::SolveLocal,
            propagation_conflict_rule(conflict),
            retained.clone(),
        );
        let key = canonical_no_good_key(
            &state,
            &snapshot,
            &expected_core,
            NoGoodScope::SolveLocal,
            &cancel,
        )
        .unwrap();
        let learned = context
            .local_no_goods
            .get(&key)
            .expect("the canonical conflict core was learned");
        assert_eq!(learned.core.provenance, retained);
        assert_eq!(learned.core.decisions, retained.decisions());
        assert_eq!(
            matches_local_no_good(&state, Some(&propagation), &mut context).unwrap(),
            Some(true)
        );
    }

    #[test]
    fn canonical_core_no_good_prunes_a_proper_superset_but_not_a_sibling() {
        let caller_problem = problem(&["1"], &["1"], "10");
        let normalized = normalized(&caller_problem);
        let fixed_profile = profile(2, 0, 0, 0);
        let cycle = splitter_pair_partial(
            &caller_problem,
            vec![
                partial_link(splitter_output(0), splitter_input(1)),
                partial_link(splitter_output(1), splitter_input(0)),
            ],
        );
        let cycle_state = TopologyState::from_partial_topology(&cycle).unwrap();
        let cycle_propagation =
            PropagationState::new(&cycle_state, &normalized.max_link_rate).unwrap();
        let PropagationOutcome::Pruned(conflict) = cycle_propagation.current_outcome() else {
            panic!("the selected core must be an exact zero-flow splitter cycle");
        };
        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(&normalized, fixed_profile, &cancel);
        learn_local_no_good(
            &cycle_state,
            Some(&cycle_propagation),
            &mut context,
            propagation_conflict_rule(conflict),
        )
        .unwrap();

        let mut superset = cycle.clone();
        superset.links.insert(
            0,
            partial_link(
                ProducerPortRef::Input(InputTerminalIndex(0)),
                ConsumerPortRef::Output(OutputTerminalIndex(0)),
            ),
        );
        let mut superset_state = TopologyState::from_partial_topology(&superset).unwrap();
        let mut superset_propagation =
            Some(PropagationState::new(&superset_state, &normalized.max_link_rate).unwrap());
        assert!(matches!(
            search_state(&mut superset_state, &mut superset_propagation, &mut context),
            DfsResult::Exhausted(None)
        ));
        assert_eq!(context.stats.instrumentation.no_good_hits, 1);

        let mut disabled_state = TopologyState::from_partial_topology(&superset).unwrap();
        let mut disabled_propagation =
            Some(PropagationState::new(&disabled_state, &normalized.max_link_rate).unwrap());
        let mut disabled_context = SearchContext::new_with_features(
            &normalized,
            fixed_profile,
            &cancel,
            &[],
            SearchFeatures::PROPAGATION_ONLY,
        );
        assert!(matches!(
            search_state(
                &mut disabled_state,
                &mut disabled_propagation,
                &mut disabled_context,
            ),
            DfsResult::Exhausted(None)
        ));
        assert_eq!(disabled_context.stats.instrumentation.no_good_hits, 0);

        let sibling = splitter_pair_partial(
            &caller_problem,
            vec![
                partial_link(
                    ProducerPortRef::Input(InputTerminalIndex(0)),
                    splitter_input(0),
                ),
                partial_link(splitter_output(0), splitter_input(1)),
                partial_link(
                    splitter_output(1),
                    ConsumerPortRef::Output(OutputTerminalIndex(0)),
                ),
            ],
        );
        let sibling_state = TopologyState::from_partial_topology(&sibling).unwrap();
        let sibling_propagation =
            PropagationState::new(&sibling_state, &normalized.max_link_rate).unwrap();
        assert_eq!(
            matches_local_no_good(&sibling_state, Some(&sibling_propagation), &mut context)
                .unwrap(),
            Some(false)
        );
        assert_eq!(context.stats.instrumentation.no_good_hits, 1);
    }

    #[test]
    fn zero_decision_core_is_explicit_and_core_identity_survives_rollback() {
        let caller_problem = problem(&["2"], &["1", "1"], "2");
        let normalized = normalized(&caller_problem);
        let fixed_profile = profile(1, 0, 0, 0);
        let mut state = TopologyState::new(&normalized, fixed_profile).unwrap();
        let root_snapshot = state.partial_topology();
        let cancel = AtomicBool::new(false);
        let empty_key =
            canonical_no_good_subset_key(&root_snapshot, &[], NoGoodScope::SolveLocal, &cancel)
                .unwrap()
                .unwrap();
        assert_eq!(
            matches_canonical_no_good(
                &root_snapshot,
                &[0],
                NoGoodScope::SolveLocal,
                &cancel,
                |candidate| candidate == &empty_key,
            ),
            Some(true)
        );

        let checkpoint = state.checkpoint();
        let decision = state
            .legal_decisions()
            .into_iter()
            .find(|decision| {
                matches!(
                    decision.producer,
                    crate::topology::ProducerChoice::NewNode { .. }
                ) || matches!(
                    decision.consumer,
                    crate::topology::ConsumerChoice::NewNode { .. }
                )
            })
            .expect("the input frontier can materialize the splitter");
        let first_decision = state.apply(decision).unwrap();
        let first_snapshot = state.partial_topology();
        let first_core = ConflictCore::from_active_decisions(
            NoGoodScope::SolveLocal,
            ConflictRule::ExactPropagation,
            [first_decision],
        );
        let first_key = canonical_no_good_key(
            &state,
            &first_snapshot,
            &first_core,
            NoGoodScope::SolveLocal,
            &cancel,
        )
        .unwrap();

        state.rollback(checkpoint);
        assert!(
            canonical_no_good_key(
                &state,
                &state.partial_topology(),
                &first_core,
                NoGoodScope::SolveLocal,
                &cancel,
            )
            .is_none()
        );
        let replayed_decision = state.apply(decision).unwrap();
        assert_eq!(replayed_decision, first_decision);
        let replayed_core = ConflictCore::from_active_decisions(
            NoGoodScope::SolveLocal,
            ConflictRule::ExactPropagation,
            [replayed_decision],
        );
        assert_eq!(
            canonical_no_good_key(
                &state,
                &state.partial_topology(),
                &replayed_core,
                NoGoodScope::SolveLocal,
                &cancel,
            ),
            Some(first_key)
        );
    }

    #[test]
    fn a_proven_structural_conflict_is_learned_and_hit_canonically() {
        let caller_problem = problem(&["1"], &["1"], "1");
        let normalized = normalized(&caller_problem);
        let state = TopologyState::from_partial_topology(&PartialTopology {
            problem: normalized_problem(&caller_problem),
            nodes: vec![
                PhysicalNode {
                    id: NodeId(0),
                    node_type: NodeType::Merger2,
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
            discard_count: 0,
            remaining_profile: NodeProfile::default(),
        })
        .unwrap();
        assert!(matches!(
            analyze_reachability(&state).verdict,
            ReachabilityVerdict::ProvenDead(_)
        ));
        let cancel = AtomicBool::new(false);
        let mut context = SearchContext::new(&normalized, profile(0, 0, 2, 0), &cancel);
        learn_structural_no_good(
            &state,
            &mut context,
            ConflictRule::ConnectivityImpossibility,
        );
        assert_eq!(context.structural_no_goods.len(), 1);
        assert_eq!(matches_structural_no_good(&state, &mut context), Some(true));
        assert_eq!(context.stats.instrumentation.no_good_hits, 1);
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
                &[],
                SearchFeatures::WITHOUT_COMPONENTS,
            ));
            let unbounded = exhaustive_witness(search_profile_with_features(
                &normalized,
                fixed_profile,
                &AtomicBool::new(false),
                &[],
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
            &[],
            SearchFeatures::OPTIMIZED,
        ));
        let (unoptimized_witness, _) = exhausted_result(search_profile_with_features(
            &capacity_normalized,
            fixed_profile,
            &AtomicBool::new(false),
            &[],
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
            &[],
            SearchFeatures::OPTIMIZED,
        ));
        let (unoptimized_witness, unoptimized_stats) =
            exhausted_result(search_profile_with_features(
                &normalized,
                profile,
                &AtomicBool::new(false),
                &[],
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

        let cancel = AtomicBool::new(false);
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
}
