//! Disjoint proof covers, extracted without changing their accounting rules.
use crate::{Failure, profile::AccountedProfile};
use solver_api::ProofSummary;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Root {
    pub profile: usize,
    pub source: Option<usize>,
    pub second_source: Option<usize>,
    pub impossible: bool,
}

/// Children partition the parent by the second output's unique producer.
/// Only children enter the ledger; all must finish before the profile is exhausted.
pub(super) fn refine_roots(
    roots: Vec<Root>,
    tasks: &[AccountedProfile],
    inputs: usize,
    outputs: usize,
    workers: usize,
) -> Vec<Root> {
    let live = roots.iter().filter(|root| !root.impossible).count();
    if workers <= 1 || outputs < 2 || live >= workers {
        return roots;
    }
    let mut refined: Vec<_> = roots
        .into_iter()
        .flat_map(|root| {
            let sources = if root.impossible || root.source.is_none() {
                0
            } else {
                inputs + tasks[root.profile].profile.node_count() as usize
            };
            if sources == 0 {
                vec![root]
            } else {
                (0..sources)
                    .map(|source| Root {
                        second_source: Some(source),
                        ..root
                    })
                    .collect()
            }
        })
        .collect();
    refined.sort_by_key(|root| root.second_source);
    refined
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AdaptiveChoice {
    Parent,
    Children,
}

pub(super) struct AdaptiveLedger {
    pub(super) base: RootLedger,
    pub(super) choice: Vec<Option<AdaptiveChoice>>,
    pub(super) children: Vec<Option<Vec<bool>>>,
}

impl AdaptiveLedger {
    pub(super) fn new(roots: &[Root], profiles: usize) -> Self {
        Self {
            base: RootLedger::new(roots, profiles),
            choice: vec![None; roots.len()],
            children: vec![None; roots.len()],
        }
    }

    pub(super) fn finish(
        &mut self,
        parent: usize,
        child: Option<usize>,
        proof: &mut ProofSummary,
    ) -> Result<(), Failure> {
        let choice = self
            .choice
            .get_mut(parent)
            .ok_or_else(|| Failure::Worker("unknown adaptive parent".into()))?;
        // A competing parent or child may finish after the other proof won.
        if choice.is_some() {
            return Ok(());
        }
        let (selected, units) = if let Some(child) = child {
            let children = self.children[parent]
                .as_mut()
                .ok_or_else(|| Failure::Worker("unregistered adaptive children".into()))?;
            let done = children
                .get_mut(child)
                .ok_or_else(|| Failure::Worker("unknown adaptive child".into()))?;
            if *done {
                return Err(Failure::Worker("duplicate adaptive child".into()));
            }
            *done = true;
            if !children.iter().all(|done| *done) {
                return Ok(());
            }
            (AdaptiveChoice::Children, children.len())
        } else {
            (AdaptiveChoice::Parent, 1)
        };
        self.base.finish(parent, proof)?;
        proof.root_partitions_exhausted += u64::try_from(units - 1).expect("root count fits u64");
        *choice = Some(selected);
        Ok(())
    }

    pub(super) fn committed(&self, parent: usize, child: Option<usize>) -> bool {
        self.choice[parent]
            == Some(if child.is_some() {
                AdaptiveChoice::Children
            } else {
                AdaptiveChoice::Parent
            })
    }
}

#[cfg(test)]
mod adaptive_tests {
    use super::*;
    fn ledger() -> AdaptiveLedger {
        let roots = vec![Root {
            profile: 0,
            source: Some(0),
            second_source: None,
            impossible: false,
        }];
        let mut ledger = AdaptiveLedger::new(&roots, 1);
        ledger.children[0] = Some(vec![false; 3]);
        ledger
    }
    #[test]
    fn partial_children_never_exhaust_and_duplicate_children_fail() {
        let mut ledger = ledger();
        let mut proof = ProofSummary::default();
        ledger.finish(0, Some(0), &mut proof).unwrap();
        ledger.finish(0, Some(1), &mut proof).unwrap();
        assert!(!ledger.base.complete());
        assert_eq!(proof.root_partitions_exhausted, 0);
        assert!(ledger.finish(0, Some(1), &mut proof).is_err());
        assert!(ledger.finish(0, Some(9), &mut proof).is_err());
    }
    #[test]
    fn parent_and_complete_children_are_alternative_proofs() {
        for parent_first in [true, false] {
            let mut ledger = ledger();
            let mut proof = ProofSummary::default();
            ledger.finish(0, Some(0), &mut proof).unwrap();
            if parent_first {
                ledger.finish(0, None, &mut proof).unwrap();
            }
            ledger.finish(0, Some(1), &mut proof).unwrap();
            ledger.finish(0, Some(2), &mut proof).unwrap();
            ledger.finish(0, None, &mut proof).unwrap();
            assert!(ledger.base.complete());
            assert_eq!(proof.profiles_exhausted, 1);
            assert_eq!(
                proof.root_partitions_exhausted,
                if parent_first { 1 } else { 3 }
            );
            assert_eq!(ledger.committed(0, None), parent_first);
            assert_eq!(ledger.committed(0, Some(0)), !parent_first);
        }
    }
}

pub(super) struct RootLedger {
    owners: Vec<usize>,
    completed: Vec<bool>,
    remaining: Vec<usize>,
}

impl RootLedger {
    pub(super) fn new(roots: &[Root], profiles: usize) -> Self {
        let mut remaining = vec![0; profiles];
        for root in roots {
            remaining[root.profile] += 1;
        }
        Self {
            owners: roots.iter().map(|root| root.profile).collect(),
            completed: vec![false; roots.len()],
            remaining,
        }
    }

    pub(super) fn finish(&mut self, root: usize, proof: &mut ProofSummary) -> Result<(), Failure> {
        let completed = self
            .completed
            .get_mut(root)
            .ok_or_else(|| Failure::Worker("unknown Solver root completion".into()))?;
        if *completed {
            return Err(Failure::Worker("duplicate Solver root completion".into()));
        }
        *completed = true;
        proof.root_partitions_exhausted += 1;
        let remaining = &mut self.remaining[self.owners[root]];
        *remaining -= 1;
        if *remaining == 0 {
            proof.profiles_exhausted += 1;
        }
        Ok(())
    }

    pub(super) fn complete(&self) -> bool {
        self.remaining.iter().all(|&count| count == 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_roots_and_duplicate_completions_cannot_discharge_a_profile() {
        let roots = [
            Root {
                profile: 0,
                source: Some(0),
                second_source: None,
                impossible: false,
            },
            Root {
                profile: 0,
                source: Some(1),
                second_source: None,
                impossible: false,
            },
            Root {
                profile: 1,
                source: None,
                second_source: None,
                impossible: true,
            },
        ];
        let mut ledger = RootLedger::new(&roots, 2);
        let mut proof = ProofSummary::default();
        ledger.finish(0, &mut proof).unwrap();
        assert_eq!(proof.profiles_exhausted, 0);
        assert!(!ledger.complete());
        assert!(ledger.finish(0, &mut proof).is_err());
        assert!(ledger.finish(3, &mut proof).is_err());
        assert_eq!(proof.root_partitions_exhausted, 1);
        ledger.finish(2, &mut proof).unwrap();
        assert_eq!(proof.profiles_exhausted, 1);
        assert!(!ledger.complete());
        ledger.finish(1, &mut proof).unwrap();
        assert_eq!(proof.profiles_exhausted, 2);
        assert_eq!(proof.root_partitions_exhausted, 3);
        assert!(ledger.complete());
    }
}
