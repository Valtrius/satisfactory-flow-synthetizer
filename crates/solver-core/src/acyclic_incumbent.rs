//! Deterministic exact construction of small acyclic fixed-profile witnesses.
//!
//! This is an optional SAT-certificate path, never an impossibility oracle. A
//! miss falls back to exhaustive production search. Every hit is independently
//! validated before it can affect an incumbent or optimality claim.

use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicBool, Ordering},
};

use num::{BigInt, Integer};

use solver_api::{
    ConsumerPortRef, InputTerminalIndex, NodeId, NodeProfile, NodeType, OutputTerminalIndex,
    PhysicalGraph, PhysicalLink, PhysicalNode, ProducerPortRef, Rational,
};

use crate::{
    lower_bound::{profile_impossibility, required_source_denominator},
    problem::NormalizedProblem,
    profile::ProfileLinkAccounting,
};

const MAX_CONSTRUCTIVE_SPLITTERS: u32 = 8;
const MAX_CONSTRUCTIVE_LEAVES: usize = 20;

/// Optional helper results, scoped to one problem and the current node count.
/// A cached miss is never a proof about the exhaustive cyclic/acyclic search.
pub(crate) struct AcyclicConstructor<'a> {
    problem: &'a NormalizedProblem,
    source_denominator: BigInt,
    node_count: Option<u32>,
    completed: BTreeMap<NodeProfile, Option<PhysicalGraph>>,
}

impl<'a> AcyclicConstructor<'a> {
    pub(crate) fn new(problem: &'a NormalizedProblem) -> Self {
        Self {
            problem,
            source_denominator: required_source_denominator(problem),
            node_count: None,
            completed: BTreeMap::new(),
        }
    }

    fn eligible(&self, profile: NodeProfile) -> bool {
        if !self.problem.surplus.is_zero()
            || profile
                .splitter2
                .checked_add(profile.splitter3)
                .is_none_or(|n| n > MAX_CONSTRUCTIVE_SPLITTERS)
        {
            return false;
        }
        // In an acyclic network every source coefficient has a denominator
        // dividing the product of splitter arities. Summing paths cannot add
        // denominator factors. This uses all input rates through their GCD;
        // it does not assume a single source, or that passing proves existence.
        let product =
            BigInt::from(2_u8).pow(profile.splitter2) * BigInt::from(3_u8).pow(profile.splitter3);
        product.is_multiple_of(&self.source_denominator)
    }

    pub(crate) fn find(
        &mut self,
        profile: NodeProfile,
        accounting: ProfileLinkAccounting,
        cancel: &AtomicBool,
    ) -> Option<PhysicalGraph> {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if self.node_count != Some(profile.node_count()) {
            self.completed.clear();
            self.node_count = Some(profile.node_count());
        }
        if !self.eligible(profile)
            || profile_impossibility(self.problem, profile, accounting).is_some()
        {
            drop(crate::diagnostics::ActivitySpan::start(
                "acyclic_ineligible",
                profile.node_count(),
                Some(accounting.link_count),
                Some(profile),
                None,
                1,
            ));
            return None;
        }
        if let Some(outcome) = self.completed.get(&profile) {
            drop(crate::diagnostics::ActivitySpan::start(
                "acyclic_reuse",
                profile.node_count(),
                Some(accounting.link_count),
                Some(profile),
                None,
                1,
            ));
            return outcome.clone();
        }
        let outcome = find_small_acyclic_witness(self.problem, profile, cancel);
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        // Cancellation can interrupt an attempt; do not retain it as a finished miss.
        self.completed.insert(profile, outcome.clone());
        outcome
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Leaf {
    flow: Rational,
    producer: ProducerPortRef,
}

#[must_use]
fn find_small_acyclic_witness(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
) -> Option<PhysicalGraph> {
    let _activity = crate::diagnostics::ActivitySpan::start(
        "acyclic_construct",
        profile.node_count(),
        None,
        Some(profile),
        None,
        1,
    );
    if !problem.surplus.is_zero()
        || profile.splitter2.checked_add(profile.splitter3)? > MAX_CONSTRUCTIVE_SPLITTERS
    {
        return None;
    }
    let leaves = problem
        .inputs
        .iter()
        .enumerate()
        .map(|(index, flow)| {
            Some(Leaf {
                flow: flow.clone(),
                producer: ProducerPortRef::Input(InputTerminalIndex(u32::try_from(index).ok()?)),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    split_recursively(
        problem,
        profile,
        profile.splitter2,
        profile.splitter3,
        leaves,
        PhysicalGraph::default(),
        cancel,
    )
}

fn split_recursively(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    splitter2: u32,
    splitter3: u32,
    mut leaves: Vec<Leaf>,
    graph: PhysicalGraph,
    cancel: &AtomicBool,
) -> Option<PhysicalGraph> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    leaves.sort();
    if splitter2 == 0 && splitter3 == 0 {
        if leaves.len() > MAX_CONSTRUCTIVE_LEAVES {
            return None;
        }
        return assign_outputs(
            problem,
            profile,
            &leaves,
            graph,
            0,
            profile.merger2,
            profile.merger3,
            cancel,
        );
    }

    // Three-way first reaches the common denominator-grain constructions early;
    // both arities and every concrete leaf remain exhaustively represented.
    for arity in [3_u8, 2_u8] {
        let remaining = if arity == 2 { splitter2 } else { splitter3 };
        if remaining == 0 {
            continue;
        }
        for leaf_index in 0..leaves.len() {
            if cancel.load(Ordering::Relaxed) {
                return None;
            }
            let mut next_leaves = leaves.clone();
            let leaf = next_leaves.remove(leaf_index);
            let output_flow = &leaf.flow / &Rational::from(u32::from(arity));
            let node_id = NodeId(u32::try_from(graph.nodes.len()).ok()?);
            let node_type = if arity == 2 {
                NodeType::Splitter2
            } else {
                NodeType::Splitter3
            };
            let mut next_graph = graph.clone();
            next_graph.nodes.push(PhysicalNode {
                id: node_id,
                node_type,
            });
            next_graph.links.push(PhysicalLink {
                producer: leaf.producer,
                consumer: ConsumerPortRef::Node {
                    node: node_id,
                    port: 0,
                },
                flow: leaf.flow,
            });
            for port in 0..arity {
                next_leaves.push(Leaf {
                    flow: output_flow.clone(),
                    producer: ProducerPortRef::Node {
                        node: node_id,
                        port,
                    },
                });
            }
            let (next_two, next_three) = if arity == 2 {
                (splitter2 - 1, splitter3)
            } else {
                (splitter2, splitter3 - 1)
            };
            if let Some(witness) = split_recursively(
                problem,
                profile,
                next_two,
                next_three,
                next_leaves,
                next_graph,
                cancel,
            ) {
                return Some(witness);
            }
        }
    }
    None
}

#[allow(clippy::too_many_arguments)]
fn assign_outputs(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    leaves: &[Leaf],
    graph: PhysicalGraph,
    output_index: usize,
    merger2: u32,
    merger3: u32,
    cancel: &AtomicBool,
) -> Option<PhysicalGraph> {
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    if output_index == problem.outputs.len() {
        return (leaves.is_empty() && merger2 == 0 && merger3 == 0).then_some(graph);
    }
    let target = &problem.outputs.as_slice()[output_index];
    let maximum_mask = 1_u64.checked_shl(u32::try_from(leaves.len()).ok()?)?;
    let mut sums = SubsetSums::new(leaves, target);
    let first_mask = if output_index + 1 == problem.outputs.len() {
        maximum_mask - 1
    } else {
        1
    };
    for mask in first_mask..maximum_mask {
        // A failed subset scan can otherwise run to 2^20 masks without reaching
        // another recursive cancellation check.
        if (mask == first_mask || mask & 255 == 1) && cancel.load(Ordering::Relaxed) {
            return None;
        }
        sums.select(mask);
        if sums.sum != sums.target
            || u64::from(mask.count_ones()) > 1 + u64::from(merger2) + 2 * u64::from(merger3)
        {
            continue;
        }
        let selected = leaves
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1_u64 << index) != 0)
            .map(|(_, leaf)| leaf.clone())
            .collect::<Vec<_>>();
        let reduction = u32::try_from(selected.len()).ok()?.checked_sub(1)?;
        for used_merger3 in 0..=merger3.min(reduction / 2) {
            let used_merger2 = reduction.checked_sub(used_merger3.checked_mul(2)?)?;
            if used_merger2 > merger2 {
                continue;
            }
            let (mut next_graph, producer) =
                merge_group(graph.clone(), selected.clone(), used_merger2, used_merger3)?;
            next_graph.links.push(PhysicalLink {
                producer,
                consumer: ConsumerPortRef::Output(OutputTerminalIndex(
                    u32::try_from(output_index).ok()?,
                )),
                flow: target.clone(),
            });
            let remaining = leaves
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1_u64 << index) == 0)
                .map(|(_, leaf)| leaf.clone())
                .collect::<Vec<_>>();
            if let Some(witness) = assign_outputs(
                problem,
                profile,
                &remaining,
                next_graph,
                output_index + 1,
                merger2 - used_merger2,
                merger3 - used_merger3,
                cancel,
            ) {
                debug_assert_eq!(witness.nodes.len(), profile.node_count() as usize);
                return Some(witness);
            }
        }
    }
    None
}

/// Preserve numeric mask order, but update an exact integer sum only for bits
/// that changed. No per-mask leaf cloning or rational normalization is needed.
struct SubsetSums {
    values: Vec<BigInt>,
    target: BigInt,
    sum: BigInt,
    mask: u64,
}

impl SubsetSums {
    fn new(leaves: &[Leaf], target: &Rational) -> Self {
        let denominator = leaves.iter().fold(target.denominator().clone(), |d, leaf| {
            d.lcm(leaf.flow.denominator())
        });
        let scale = |value: &Rational| value.numerator() * (&denominator / value.denominator());
        Self {
            values: leaves.iter().map(|leaf| scale(&leaf.flow)).collect(),
            target: scale(target),
            sum: BigInt::from(0),
            mask: 0,
        }
    }

    fn select(&mut self, mask: u64) {
        let mut changed = mask ^ self.mask;
        while changed != 0 {
            let index = changed.trailing_zeros() as usize;
            if mask & (1_u64 << index) == 0 {
                self.sum -= &self.values[index];
            } else {
                self.sum += &self.values[index];
            }
            changed &= changed - 1;
        }
        self.mask = mask;
    }
}

fn merge_group(
    mut graph: PhysicalGraph,
    mut leaves: Vec<Leaf>,
    merger2: u32,
    merger3: u32,
) -> Option<(PhysicalGraph, ProducerPortRef)> {
    leaves.sort();
    for arity in std::iter::repeat_n(3_u8, usize::try_from(merger3).ok()?)
        .chain(std::iter::repeat_n(2_u8, usize::try_from(merger2).ok()?))
    {
        if leaves.len() < usize::from(arity) {
            return None;
        }
        let inputs = leaves.drain(..usize::from(arity)).collect::<Vec<_>>();
        let flow = inputs
            .iter()
            .fold(Rational::zero(), |total, leaf| &total + &leaf.flow);
        let node_id = NodeId(u32::try_from(graph.nodes.len()).ok()?);
        graph.nodes.push(PhysicalNode {
            id: node_id,
            node_type: if arity == 2 {
                NodeType::Merger2
            } else {
                NodeType::Merger3
            },
        });
        for (port, input) in inputs.into_iter().enumerate() {
            graph.links.push(PhysicalLink {
                producer: input.producer,
                consumer: ConsumerPortRef::Node {
                    node: node_id,
                    port: u8::try_from(port).ok()?,
                },
                flow: input.flow,
            });
        }
        leaves.push(Leaf {
            flow,
            producer: ProducerPortRef::Node {
                node: node_id,
                port: 0,
            },
        });
        leaves.sort();
    }
    (leaves.len() == 1).then(|| (graph, leaves[0].producer))
}

#[cfg(test)]
mod tests {
    use solver_api::Problem;
    use solver_validation::validate_solution;

    use super::*;
    use crate::{Preparation, prepare_problem};

    fn normalized(inputs: &[u32], outputs: &[u32]) -> NormalizedProblem {
        let problem = Problem {
            inputs: inputs.iter().copied().map(Rational::from).collect(),
            outputs: outputs.iter().copied().map(Rational::from).collect(),
            max_link_rate: Rational::from(1_200),
        };
        let Preparation::Prepared(problem) = prepare_problem(&problem).unwrap() else {
            panic!("fixture must pass global checks");
        };
        problem
    }

    #[test]
    fn incremental_subsets_match_rational_sums_in_the_original_mask_order() {
        for count in 1..=9 {
            let leaves = (0..count)
                .map(|i| Leaf {
                    flow: Rational::new(1 + i % 4, 1 + i % 3).unwrap(),
                    producer: ProducerPortRef::Input(InputTerminalIndex(i)),
                })
                .collect::<Vec<_>>();
            for target in [Rational::new(7, 6).unwrap(), Rational::from(3)] {
                let mut sums = SubsetSums::new(&leaves, &target);
                let mut actual = Vec::new();
                let mut expected = Vec::new();
                for mask in 1..(1_u64 << count) {
                    sums.select(mask);
                    let exact = leaves
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| mask & (1 << i) != 0)
                        .fold(Rational::zero(), |total, (_, leaf)| &total + &leaf.flow);
                    if sums.sum == sums.target {
                        actual.push(mask);
                    }
                    if exact == target {
                        expected.push(mask);
                    }
                }
                assert_eq!(actual, expected);
                let mut whole = SubsetSums::new(&leaves, &target);
                whole.select((1 << count) - 1);
                assert_eq!(
                    whole.sum, sums.sum,
                    "last output can jump directly to the full mask"
                );
            }
        }
    }

    #[test]
    fn eligibility_uses_all_rates_and_exact_profile_divisibility() {
        let binary = NodeProfile {
            splitter2: 3,
            merger2: 2,
            ..NodeProfile::default()
        };
        for scale in [1, 7, 11] {
            let requires_feedback = normalized(&[15 * scale], &[6 * scale, 9 * scale]);
            assert!(!AcyclicConstructor::new(&requires_feedback).eligible(binary));
            let requires_three = normalized(&[18 * scale], &[6 * scale, 12 * scale]);
            let constructor = AcyclicConstructor::new(&requires_three);
            assert!(!constructor.eligible(binary));
            assert!(constructor.eligible(NodeProfile {
                splitter3: 1,
                merger2: 1,
                ..NodeProfile::default()
            }));
            let multiple_inputs = normalized(&[3 * scale, 2 * scale], &[4 * scale, scale]);
            assert!(
                AcyclicConstructor::new(&multiple_inputs).eligible(NodeProfile {
                    splitter2: 1,
                    merger2: 1,
                    ..NodeProfile::default()
                })
            );
        }
    }

    #[test]
    fn completed_helper_hits_and_misses_are_reused_across_link_groups() {
        let profile = NodeProfile {
            splitter2: 1,
            merger2: 1,
            ..NodeProfile::default()
        };
        for (inputs, outputs, succeeds) in [([2, 3], [1, 4], true), ([1, 7], [3, 5], false)] {
            let problem = normalized(&inputs, &outputs);
            let accounts = crate::profile::profile_link_accountings(
                profile,
                2,
                2,
                &problem.surplus,
                &problem.max_link_rate,
            )
            .unwrap();
            assert!(accounts.len() > 1);
            let mut constructor = AcyclicConstructor::new(&problem);
            let cancel = AtomicBool::new(true);
            assert!(constructor.find(profile, accounts[0], &cancel).is_none());
            assert!(constructor.completed.is_empty());
            cancel.store(false, Ordering::Relaxed);
            let first = constructor.find(profile, accounts[0], &cancel);
            assert_eq!(first.is_some(), succeeds);
            assert_eq!(constructor.completed.get(&profile), Some(&first));
            assert_eq!(constructor.find(profile, accounts[1], &cancel), first);
            assert_eq!(constructor.completed.len(), 1);
            // Returning to another node count cannot accumulate an unbounded cache.
            let empty = NodeProfile::default();
            let account = crate::profile::profile_link_accounting(
                empty,
                2,
                2,
                &problem.surplus,
                &problem.max_link_rate,
            )
            .unwrap()
            .unwrap();
            constructor.find(empty, account, &cancel);
            assert!(!constructor.completed.contains_key(&profile));
        }
    }

    #[test]
    fn eligibility_never_rejects_a_constructed_small_witness() {
        // Generated rates and profiles, with no benchmark classifications.
        for total in 2..=6 {
            for first_output in 1..total {
                let problem = normalized(&[total], &[first_output, total - first_output]);
                let mut constructor = AcyclicConstructor::new(&problem);
                for nodes in 0..=3 {
                    let groups = crate::profile::enumerate_accounted_profile_groups(
                        nodes,
                        1,
                        2,
                        &problem.surplus,
                        &problem.max_link_rate,
                    )
                    .unwrap();
                    for accounted in groups.iter().flat_map(|group| &group.profiles) {
                        let cancel = AtomicBool::new(false);
                        let raw = find_small_acyclic_witness(&problem, accounted.profile, &cancel);
                        if raw.is_some() {
                            assert!(constructor.eligible(accounted.profile));
                        }
                        assert_eq!(
                            constructor.find(accounted.profile, accounted.accounting, &cancel),
                            raw
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn hard_case_constructor_produces_an_exact_valid_certificate() {
        let problem = normalized(&[216], &[66, 150]);
        let profile = NodeProfile {
            splitter2: 2,
            splitter3: 2,
            merger2: 1,
            merger3: 2,
        };
        let graph = find_small_acyclic_witness(&problem, profile, &AtomicBool::new(false))
            .expect("the smooth source-grain construction must be found");
        let public_problem = Problem {
            inputs: problem.inputs.as_slice().to_vec(),
            outputs: problem.outputs.as_slice().to_vec(),
            max_link_rate: problem.max_link_rate,
        };
        let validation = validate_solution(&public_problem, &graph).unwrap();
        assert_eq!((validation.node_count, validation.link_count), (7, 11));
        assert_eq!(validation.cyclic_scc_count, 0);
    }

    #[test]
    fn constructor_handles_multiple_input_lines_exactly() {
        let problem = normalized(&[2, 3], &[1, 4]);
        let profile = NodeProfile {
            splitter2: 1,
            merger2: 1,
            ..NodeProfile::default()
        };
        let graph = find_small_acyclic_witness(&problem, profile, &AtomicBool::new(false))
            .expect("one split and one merge realize the two-input fixture");
        let public_problem = Problem {
            inputs: problem.inputs.as_slice().to_vec(),
            outputs: problem.outputs.as_slice().to_vec(),
            max_link_rate: problem.max_link_rate,
        };
        validate_solution(&public_problem, &graph).unwrap();
    }

    #[test]
    fn cancellation_interrupts_an_unsuccessful_large_subset_scan() {
        use std::{
            thread,
            time::{Duration, Instant},
        };
        let problem = normalized(&[1; 20], &[11, 9]);
        let leaves = (0..20)
            .map(|index| Leaf {
                flow: 1.into(),
                producer: ProducerPortRef::Input(InputTerminalIndex(index)),
            })
            .collect::<Vec<_>>();
        let cancel = AtomicBool::new(false);
        let started = Instant::now();
        thread::scope(|scope| {
            scope.spawn(|| {
                thread::sleep(Duration::from_millis(20));
                cancel.store(true, Ordering::Relaxed);
            });
            assert!(
                assign_outputs(
                    &problem,
                    NodeProfile::default(),
                    &leaves,
                    PhysicalGraph::default(),
                    0,
                    0,
                    0,
                    &cancel
                )
                .is_none()
            );
        });
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "subset scan ignored cancellation"
        );
    }

    #[test]
    fn cancellation_and_surplus_only_disable_the_optional_constructor() {
        let balanced = normalized(&[6], &[3, 3]);
        let cancelled = AtomicBool::new(true);
        assert!(
            find_small_acyclic_witness(
                &balanced,
                NodeProfile {
                    splitter2: 1,
                    ..NodeProfile::default()
                },
                &cancelled,
            )
            .is_none()
        );

        let surplus = normalized(&[3], &[2]);
        assert!(
            find_small_acyclic_witness(&surplus, NodeProfile::default(), &AtomicBool::new(false))
                .is_none()
        );
    }
}
