//! Canonical, proof-carrying no-goods.
//!
//! Learned entries are optional accelerators. A conflict core deliberately may
//! contain every active decision: that broad core is still sound, and the plan
//! does not require minimum cores. Keys are canonical induced identities of the
//! proof's decision set. A hit therefore proves that the current state contains
//! an isomorphic core already shown impossible; provenance remains attached to
//! every stored key.

use std::{
    collections::BTreeMap,
    sync::{Mutex, MutexGuard},
};

use crate::{canonical::StateKey, topology::DecisionId};

/// Semantic lifetime of a learned contradiction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NoGoodScope {
    /// May use exact rates, capacity, profile, and external bindings.
    SolveLocal,
    /// Depends only on physical structure and remaining inventory.
    GlobalStructural,
}

/// Exact theorem used to reject the state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConflictRule {
    ExactPropagation,
    ExactCapacity,
    DynamicSccInconsistency,
    ConnectivityImpossibility,
    PortCompletionImpossibility,
}

/// Stable compact provenance-node index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProofNodeId(pub u32);

/// One node in a compact exact-fact provenance DAG.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ProvenanceNode {
    /// Problem/profile axioms allowed by the declared scope.
    ScopeAxiom(NoGoodScope),
    /// One active structural link decision.
    Decision(DecisionId),
    /// Exact derived fact or contradiction with sorted parent references.
    Derived {
        rule: ConflictRule,
        parents: Vec<ProofNodeId>,
    },
}

/// Self-contained compact proof graph for one learned conflict.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProvenanceDag {
    pub nodes: Vec<ProvenanceNode>,
    pub root: ProofNodeId,
}

impl ProvenanceDag {
    /// Returns the structural decisions reachable from this proof root.
    #[must_use]
    pub fn decisions(&self) -> Vec<DecisionId> {
        let mut decisions = self
            .nodes
            .iter()
            .filter_map(|node| match node {
                ProvenanceNode::Decision(decision) => Some(*decision),
                ProvenanceNode::ScopeAxiom(_) | ProvenanceNode::Derived { .. } => None,
            })
            .collect::<Vec<_>>();
        decisions.sort_unstable();
        decisions.dedup();
        decisions
    }
}

/// Append-only, interned proof storage shared by all facts in one branch.
///
/// Parents always precede children. A checkpoint is therefore one vector
/// length, and rollback restores both node identity and interning exactly.
/// Individual fact maps keep only a [`ProofNodeId`] into this recorder, so a
/// common algebra derivation is stored once even when it proves many values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProvenanceRecorder {
    nodes: Vec<ProvenanceNode>,
    interned: BTreeMap<ProvenanceNode, ProofNodeId>,
    scope_root: ProofNodeId,
}

impl ProvenanceRecorder {
    #[must_use]
    pub(crate) fn new(scope: NoGoodScope) -> Self {
        let scope_root = ProofNodeId(0);
        let axiom = ProvenanceNode::ScopeAxiom(scope);
        Self {
            nodes: vec![axiom.clone()],
            interned: BTreeMap::from([(axiom, scope_root)]),
            scope_root,
        }
    }

    #[must_use]
    pub(crate) const fn scope_root(&self) -> ProofNodeId {
        self.scope_root
    }

    #[must_use]
    pub(crate) fn checkpoint(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn rollback(&mut self, checkpoint: usize) {
        assert!(checkpoint > usize::try_from(self.scope_root.0).expect("u32 fits usize"));
        assert!(checkpoint <= self.nodes.len());
        self.nodes.truncate(checkpoint);
        self.interned
            .retain(|_, id| usize::try_from(id.0).is_ok_and(|index| index < checkpoint));
    }

    pub(crate) fn decision(&mut self, decision: DecisionId) -> ProofNodeId {
        self.intern(ProvenanceNode::Decision(decision))
    }

    pub(crate) fn derived(
        &mut self,
        rule: ConflictRule,
        parents: impl IntoIterator<Item = ProofNodeId>,
    ) -> ProofNodeId {
        let mut parents = parents.into_iter().collect::<Vec<_>>();
        parents.sort_unstable();
        parents.dedup();
        debug_assert!(parents.iter().all(|parent| {
            usize::try_from(parent.0).is_ok_and(|index| index < self.nodes.len())
        }));
        self.intern(ProvenanceNode::Derived { rule, parents })
    }

    #[must_use]
    pub(crate) fn extract(&self, root: ProofNodeId) -> ProvenanceDag {
        let root_index = usize::try_from(root.0).expect("u32 proof node fits usize");
        assert!(root_index < self.nodes.len());

        let mut reachable = vec![false; self.nodes.len()];
        let mut pending = vec![root];
        while let Some(node) = pending.pop() {
            let index = usize::try_from(node.0).expect("u32 proof node fits usize");
            if std::mem::replace(&mut reachable[index], true) {
                continue;
            }
            if let ProvenanceNode::Derived { parents, .. } = &self.nodes[index] {
                pending.extend(parents.iter().copied());
            }
        }

        let mut remap = BTreeMap::new();
        let mut nodes = Vec::new();
        for (old_index, node) in self.nodes.iter().enumerate() {
            if !reachable[old_index] {
                continue;
            }
            let new_id = ProofNodeId(
                u32::try_from(nodes.len()).expect("proof node count originated from u32 ids"),
            );
            remap.insert(
                ProofNodeId(u32::try_from(old_index).expect("proof node index fits u32")),
                new_id,
            );
            let remapped = match node {
                ProvenanceNode::Derived { rule, parents } => ProvenanceNode::Derived {
                    rule: *rule,
                    parents: parents
                        .iter()
                        .map(|parent| {
                            remap
                                .get(parent)
                                .copied()
                                .expect("proof parents precede their child")
                        })
                        .collect(),
                },
                leaf => leaf.clone(),
            };
            nodes.push(remapped);
        }
        ProvenanceDag {
            nodes,
            root: remap[&root],
        }
    }

    fn intern(&mut self, node: ProvenanceNode) -> ProofNodeId {
        if let Some(id) = self.interned.get(&node) {
            return *id;
        }
        let id =
            ProofNodeId(u32::try_from(self.nodes.len()).expect("proof node count must fit u32"));
        self.nodes.push(node.clone());
        self.interned.insert(node, id);
        id
    }
}

/// Canonical broad conflict core retained with a no-good.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConflictCore {
    pub scope: NoGoodScope,
    pub rule: ConflictRule,
    pub decisions: Vec<DecisionId>,
    pub provenance: ProvenanceDag,
}

impl ConflictCore {
    /// Constructs a stable broad core from every active decision.
    ///
    /// The scope axiom represents exact fixed problem/profile semantics for a
    /// local conflict, or only structural invariants for a structural conflict.
    /// Retaining extra decisions can reduce reuse but cannot create a false hit.
    ///
    /// # Panics
    ///
    /// Panics only if the active decision collection has more than `u32::MAX`
    /// entries. [`crate::topology::TopologyState`] rejects physical link counts
    /// above that bound before provenance construction.
    #[must_use]
    pub fn from_active_decisions(
        scope: NoGoodScope,
        rule: ConflictRule,
        decisions: impl IntoIterator<Item = DecisionId>,
    ) -> Self {
        let mut decisions = decisions.into_iter().collect::<Vec<_>>();
        decisions.sort_unstable();
        decisions.dedup();
        let mut nodes = vec![ProvenanceNode::ScopeAxiom(scope)];
        nodes.extend(decisions.iter().copied().map(ProvenanceNode::Decision));
        let parents = (0..nodes.len())
            .map(|index| {
                ProofNodeId(
                    u32::try_from(index).expect("active physical decision count must fit u32"),
                )
            })
            .collect();
        let root = ProofNodeId(
            u32::try_from(nodes.len()).expect("active physical decision count must fit u32"),
        );
        nodes.push(ProvenanceNode::Derived { rule, parents });
        Self {
            scope,
            rule,
            decisions,
            provenance: ProvenanceDag { nodes, root },
        }
    }

    /// Builds a core from the exact proof retained by propagation.
    ///
    /// Unlike [`Self::from_active_decisions`], this path never guesses parents
    /// when the contradiction is observed. It traverses the already retained
    /// fact proof and recovers precisely its decision leaves. The retained set
    /// may still be broad because a sound algebra fact may deliberately cite a
    /// broad row set.
    #[must_use]
    pub(crate) fn from_provenance(
        scope: NoGoodScope,
        rule: ConflictRule,
        provenance: ProvenanceDag,
    ) -> Self {
        debug_assert!(
            provenance
                .nodes
                .iter()
                .any(|node| matches!(node, ProvenanceNode::ScopeAxiom(found) if *found == scope))
        );
        let decisions = provenance.decisions();
        Self {
            scope,
            rule,
            decisions,
            provenance,
        }
    }
}

/// One proof-carrying canonical no-good.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LearnedNoGood {
    pub core: ConflictCore,
}

/// Canonical identity of the decisions in one conflict core.
///
/// `decision_count` is stored explicitly so lookup can enumerate only current
/// link subsets of relevant cardinalities. `state` is the canonical induced
/// core projection, never the incidental full state where the proof was found.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CanonicalNoGoodKey {
    decision_count: u32,
    state: StateKey,
}

impl CanonicalNoGoodKey {
    /// Builds a key when the core cardinality fits the stable count type.
    #[must_use]
    pub fn new(decision_count: usize, state: StateKey) -> Option<Self> {
        Some(Self {
            decision_count: u32::try_from(decision_count).ok()?,
            state,
        })
    }

    /// Number of distinct structural decisions represented by the key.
    #[must_use]
    pub const fn decision_count(&self) -> u32 {
        self.decision_count
    }

    /// Canonical induced-state identity of the represented decisions.
    #[must_use]
    pub const fn state(&self) -> &StateKey {
        &self.state
    }
}

/// Per-profile exact-rate/capacity decision-core no-goods.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SolveLocalNoGoods {
    entries: BTreeMap<CanonicalNoGoodKey, LearnedNoGood>,
}

impl SolveLocalNoGoods {
    #[must_use]
    pub fn contains(&self, key: &CanonicalNoGoodKey) -> bool {
        self.entries.contains_key(key)
    }

    pub fn learn(&mut self, key: CanonicalNoGoodKey, core: ConflictCore) {
        debug_assert_eq!(core.scope, NoGoodScope::SolveLocal);
        debug_assert_eq!(
            usize::try_from(key.decision_count()).ok(),
            Some(core.decisions.len())
        );
        retain_deterministic(&mut self.entries, key, LearnedNoGood { core });
    }

    #[cfg(test)]
    pub(crate) fn get(&self, key: &CanonicalNoGoodKey) -> Option<&LearnedNoGood> {
        self.entries.get(key)
    }

    /// Sorted core cardinalities that can fit in a current state of `maximum` links.
    #[must_use]
    pub fn decision_counts_through(&self, maximum: usize) -> Vec<usize> {
        decision_counts_through(&self.entries, maximum)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Thread-safe problem-independent structural decision cores shared by proof workers.
#[derive(Debug, Default)]
pub struct StructuralNoGoodStore {
    entries: Mutex<BTreeMap<CanonicalNoGoodKey, LearnedNoGood>>,
}

impl StructuralNoGoodStore {
    #[must_use]
    pub fn contains(&self, key: &CanonicalNoGoodKey) -> bool {
        lock_unpoisoned(&self.entries).contains_key(key)
    }

    pub fn learn(&self, key: CanonicalNoGoodKey, core: ConflictCore) {
        debug_assert_eq!(core.scope, NoGoodScope::GlobalStructural);
        debug_assert_eq!(
            usize::try_from(key.decision_count()).ok(),
            Some(core.decisions.len())
        );
        retain_deterministic(
            &mut lock_unpoisoned(&self.entries),
            key,
            LearnedNoGood { core },
        );
    }

    #[must_use]
    pub fn len(&self) -> usize {
        lock_unpoisoned(&self.entries).len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        lock_unpoisoned(&self.entries).is_empty()
    }

    /// Sorted core cardinalities that can fit in a current state of `maximum` links.
    #[must_use]
    pub fn decision_counts_through(&self, maximum: usize) -> Vec<usize> {
        decision_counts_through(&lock_unpoisoned(&self.entries), maximum)
    }
}

fn retain_deterministic(
    entries: &mut BTreeMap<CanonicalNoGoodKey, LearnedNoGood>,
    key: CanonicalNoGoodKey,
    candidate: LearnedNoGood,
) {
    entries
        .entry(key)
        .and_modify(|stored| {
            if candidate < *stored {
                *stored = candidate.clone();
            }
        })
        .or_insert(candidate);
}

fn decision_counts_through(
    entries: &BTreeMap<CanonicalNoGoodKey, LearnedNoGood>,
    maximum: usize,
) -> Vec<usize> {
    let mut counts = entries
        .keys()
        .filter_map(|key| usize::try_from(key.decision_count()).ok())
        .filter(|&count| count <= maximum)
        .collect::<Vec<_>>();
    counts.sort_unstable();
    counts.dedup();
    counts
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use solver_api::{NodeProfile, Problem, Rational};

    use super::*;
    use crate::canonical::{PartialTopology, canonicalize_state};

    fn key(decision_count: usize) -> CanonicalNoGoodKey {
        let state = canonicalize_state(&PartialTopology {
            problem: Problem {
                inputs: vec![Rational::one()],
                outputs: vec![Rational::one()],
                max_link_rate: Rational::one(),
            },
            nodes: Vec::new(),
            links: Vec::new(),
            discard_count: 0,
            remaining_profile: NodeProfile::default(),
        });
        CanonicalNoGoodKey::new(decision_count, state).unwrap()
    }

    #[test]
    fn broad_conflict_core_is_sorted_deduplicated_and_stable() {
        let left = ConflictCore::from_active_decisions(
            NoGoodScope::SolveLocal,
            ConflictRule::ExactPropagation,
            [DecisionId(9), DecisionId(2), DecisionId(9)],
        );
        let right = ConflictCore::from_active_decisions(
            NoGoodScope::SolveLocal,
            ConflictRule::ExactPropagation,
            [DecisionId(2), DecisionId(9)],
        );
        assert_eq!(left, right);
        assert_eq!(left.decisions, vec![DecisionId(2), DecisionId(9)]);
        assert_eq!(
            left.provenance.nodes.last(),
            Some(&ProvenanceNode::Derived {
                rule: ConflictRule::ExactPropagation,
                parents: vec![ProofNodeId(0), ProofNodeId(1), ProofNodeId(2)],
            })
        );
    }

    #[test]
    fn local_and_structural_stores_keep_their_proof_scopes_separate() {
        let mut local = SolveLocalNoGoods::default();
        local.learn(
            key(0),
            ConflictCore::from_active_decisions(
                NoGoodScope::SolveLocal,
                ConflictRule::ExactCapacity,
                [],
            ),
        );
        let structural = StructuralNoGoodStore::default();
        assert!(local.contains(&key(0)));
        assert!(!structural.contains(&key(0)));
        structural.learn(
            key(0),
            ConflictCore::from_active_decisions(
                NoGoodScope::GlobalStructural,
                ConflictRule::ConnectivityImpossibility,
                [],
            ),
        );
        assert!(structural.contains(&key(0)));
    }

    #[test]
    fn concurrent_structural_learning_retains_the_same_minimum_proof() {
        let store = Arc::new(StructuralNoGoodStore::default());
        let handles = [9_u64, 2, 7, 2]
            .into_iter()
            .map(|decision| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    store.learn(
                        key(1),
                        ConflictCore::from_active_decisions(
                            NoGoodScope::GlobalStructural,
                            ConflictRule::PortCompletionImpossibility,
                            [DecisionId(decision)],
                        ),
                    );
                })
            })
            .collect::<Vec<_>>();
        for handle in handles {
            handle.join().unwrap();
        }
        let entries = lock_unpoisoned(&store.entries);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries.values().next().unwrap().core.decisions,
            vec![DecisionId(2)]
        );
    }

    #[test]
    fn recorder_extracts_only_the_reachable_parent_chain() {
        let mut recorder = ProvenanceRecorder::new(NoGoodScope::SolveLocal);
        let relevant = recorder.decision(DecisionId(3));
        let _unrelated = recorder.decision(DecisionId(8));
        let fact = recorder.derived(
            ConflictRule::ExactPropagation,
            [recorder.scope_root(), relevant],
        );
        let contradiction = recorder.derived(ConflictRule::ExactCapacity, [fact]);

        let proof = recorder.extract(contradiction);
        assert_eq!(proof.decisions(), vec![DecisionId(3)]);
        assert_eq!(proof.nodes.len(), 4);
        assert_eq!(
            proof.nodes[usize::try_from(proof.root.0).unwrap()],
            ProvenanceNode::Derived {
                rule: ConflictRule::ExactCapacity,
                parents: vec![ProofNodeId(2)],
            }
        );
    }

    #[test]
    fn recorder_rollback_restores_node_ids_and_interning() {
        let mut recorder = ProvenanceRecorder::new(NoGoodScope::SolveLocal);
        let checkpoint = recorder.checkpoint();
        let first_decision = recorder.decision(DecisionId(11));
        let first_fact = recorder.derived(ConflictRule::ExactPropagation, [first_decision]);
        let first = recorder.extract(first_fact);

        recorder.rollback(checkpoint);
        let replayed_decision = recorder.decision(DecisionId(11));
        let replayed_fact = recorder.derived(ConflictRule::ExactPropagation, [replayed_decision]);
        assert_eq!(first_decision, replayed_decision);
        assert_eq!(first_fact, replayed_fact);
        assert_eq!(first, recorder.extract(replayed_fact));
    }
}
