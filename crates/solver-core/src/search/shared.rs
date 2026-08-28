//! Completed proofs only. In-flight duplicates remain independent and never wait.
use super::{
    BTreeMap, CanonicalGraphKey, HashMap, NodeProfile, ProfileWitness, StateKey, StateStatus,
    state_cache_entry_bytes,
};
use std::sync::Mutex;

#[derive(Default)]
struct Store {
    states: HashMap<(Option<u32>, StateKey), StateStatus>,
    witnesses: BTreeMap<NodeProfile, BTreeMap<CanonicalGraphKey, ProfileWitness>>,
    bytes: u64,
}

/// The group owns all validated witnesses until result collection finishes.
#[derive(Default)]
pub(crate) struct SharedStateCache {
    store: Mutex<Store>,
}

impl SharedStateCache {
    pub(super) fn lookup(&self, target_l: Option<u32>, key: &StateKey) -> Option<StateStatus> {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .states
            .get(&(target_l, key.clone()))
            .cloned()
    }

    pub(super) fn insert(&self, target_l: Option<u32>, key: StateKey, status: StateStatus) {
        assert!(!matches!(status, StateStatus::InProgress));
        let mut store = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let bytes = state_cache_entry_bytes(&key, &status);
        if let std::collections::hash_map::Entry::Vacant(entry) =
            store.states.entry((target_l, key))
        {
            entry.insert(status);
            store.bytes = store.bytes.saturating_add(bytes);
        }
    }

    pub(super) fn retain(&self, profile: NodeProfile, witness: &ProfileWitness) {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .witnesses
            .entry(profile)
            .or_default()
            .entry(witness.canonical_graph_key.clone())
            .or_insert_with(|| witness.clone());
    }

    pub(super) fn witness(
        &self,
        profile: NodeProfile,
        key: &CanonicalGraphKey,
    ) -> Option<ProfileWitness> {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .witnesses
            .get(&profile)?
            .get(key)
            .cloned()
    }

    pub(crate) fn witnesses(&self, profile: NodeProfile) -> Vec<ProfileWitness> {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .witnesses
            .get(&profile)
            .map(|w| w.values().cloned().collect())
            .unwrap_or_default()
    }

    pub(crate) fn bytes(&self) -> u64 {
        self.store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .bytes
    }
}
