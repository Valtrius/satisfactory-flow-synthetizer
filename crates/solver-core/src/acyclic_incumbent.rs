//! Deterministic exact construction of small acyclic fixed-profile witnesses.
//!
//! This is an optional SAT-certificate path, never an impossibility oracle. A
//! miss falls back to exhaustive production search. Every hit is independently
//! validated before it can affect an incumbent or optimality claim.

use std::sync::atomic::{AtomicBool, Ordering};

use solver_api::{
    ConsumerPortRef, InputTerminalIndex, NodeId, NodeProfile, NodeType, OutputTerminalIndex,
    PhysicalGraph, PhysicalLink, PhysicalNode, ProducerPortRef, Rational,
};

use crate::problem::NormalizedProblem;

const MAX_CONSTRUCTIVE_SPLITTERS: u32 = 8;
const MAX_CONSTRUCTIVE_LEAVES: usize = 20;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Leaf {
    flow: Rational,
    producer: ProducerPortRef,
}

#[must_use]
pub(crate) fn find_small_acyclic_witness(
    problem: &NormalizedProblem,
    profile: NodeProfile,
    cancel: &AtomicBool,
) -> Option<PhysicalGraph> {
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
    for mask in 1..maximum_mask {
        if output_index + 1 == problem.outputs.len() && mask + 1 != maximum_mask {
            continue;
        }
        let selected = leaves
            .iter()
            .enumerate()
            .filter(|(index, _)| mask & (1_u64 << index) != 0)
            .map(|(_, leaf)| leaf.clone())
            .collect::<Vec<_>>();
        let sum = selected
            .iter()
            .fold(Rational::zero(), |total, leaf| &total + &leaf.flow);
        if &sum != target {
            continue;
        }
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
        assert_eq!((validation.node_count, validation.link_count), (7, 14));
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
