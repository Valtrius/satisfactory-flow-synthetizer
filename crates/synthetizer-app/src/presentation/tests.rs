use super::{feedback::feedback_annotation, rates::format_rate};
use solver_api::{
    BestKnownSolution, CanonicalGraphKey, DiscardTerminalIndex, IncompleteResult,
    InputTerminalIndex, OptimalSolution, OutputTerminalIndex, PhysicalNode, ValidationSummary,
};
use solver_api::{
    ConsumerPortRef, IncompleteReason, NodeId, NodeType, PhysicalGraph, PhysicalLink,
    PreparedProblem, ProducerPortRef, ProofSummary, Rational, SolveResult,
};
use std::collections::{BTreeMap, BTreeSet};

use super::*;
use solver_api::{EndpointRequest, ProblemRequest};

fn endpoint(id: &str, rate: &str) -> EndpointRequest {
    EndpointRequest {
        id: id.to_owned(),
        name: id.to_owned(),
        rate: rate.to_owned(),
    }
}

fn prepared(inputs: &[&str], outputs: &[&str], capacity: &str) -> PreparedProblem {
    ProblemRequest {
        inputs: inputs
            .iter()
            .enumerate()
            .map(|(index, rate)| endpoint(&format!("input {index}"), rate))
            .collect(),
        outputs: outputs
            .iter()
            .enumerate()
            .map(|(index, rate)| endpoint(&format!("output {index}"), rate))
            .collect(),
        belt_rate: capacity.to_owned(),
    }
    .prepare()
    .unwrap()
}

fn validation(nodes: u32, links: u32, physical: u32, discard: u32) -> ValidationSummary {
    ValidationSummary {
        validator_version: 2,
        node_count: nodes,
        link_count: links,
        physical_link_count: physical,
        discard_link_count: discard,
        cyclic_scc_count: 0,
    }
}

#[test]
fn discard_is_physical_but_excluded_from_presented_l() {
    let request = prepared(&["1", "1"], &["1"], "1");
    let best = BestKnownSolution {
        node_count: 0,
        link_count: 0,
        physical_link_count: 2,
        discard_link_count: 1,
        canonical_graph_key: CanonicalGraphKey::from_bytes(vec![1]),
        graph: PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                PhysicalLink {
                    producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                    consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                    flow: Rational::one(),
                },
                PhysicalLink {
                    producer: ProducerPortRef::Input(InputTerminalIndex(1)),
                    consumer: ConsumerPortRef::Discard(DiscardTerminalIndex(0)),
                    flow: Rational::one(),
                },
            ],
        },
        validation: validation(0, 0, 2, 1),
    };
    let outcome = SolveResult::Incomplete(IncompleteResult {
        reason: IncompleteReason::Cancelled,
        best_known: Some(best),
        proof: ProofSummary::default(),
    });
    let PresentedSolveOutcome::Incomplete(presented) =
        present_solve_result(&request, &outcome).unwrap()
    else {
        panic!("expected incomplete outcome");
    };
    let solution = presented.best_known.unwrap();
    assert_eq!(solution.status, "best_known");
    assert_eq!(solution.stats.link_count, 0);
    assert_eq!(solution.stats.physical_link_count, 2);
    assert_eq!(solution.stats.discard_link_count, 1);
    assert_eq!(solution.stats.checked_through, None);
    assert_eq!(
        solution.edges.iter().filter(|edge| edge.discarded).count(),
        1
    );
}

#[test]
fn storage_order_does_not_change_presentation() {
    let request = prepared(&["2"], &["1", "1"], "2");
    let node = PhysicalNode {
        id: NodeId(7),
        node_type: NodeType::Splitter2,
    };
    let links = vec![
        PhysicalLink {
            producer: ProducerPortRef::Input(InputTerminalIndex(0)),
            consumer: ConsumerPortRef::Node {
                node: NodeId(7),
                port: 0,
            },
            flow: Rational::from(2_u32),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node {
                node: NodeId(7),
                port: 0,
            },
            consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node {
                node: NodeId(7),
                port: 1,
            },
            consumer: ConsumerPortRef::Output(OutputTerminalIndex(1)),
            flow: Rational::one(),
        },
    ];
    let solution = |mut nodes: Vec<PhysicalNode>, mut links: Vec<PhysicalLink>| {
        nodes.reverse();
        links.reverse();
        SolveResult::Optimal(OptimalSolution {
            node_count: 1,
            link_count: 0,
            physical_link_count: 3,
            discard_link_count: 0,
            canonical_graph_key: CanonicalGraphKey::from_bytes(vec![1]),
            graph: PhysicalGraph { nodes, links },
            proof: ProofSummary::default(),
            validation: validation(1, 0, 3, 0),
        })
    };
    let left =
        present_solve_result(&request, &solution(vec![node.clone()], links.clone())).unwrap();
    let right = present_solve_result(&request, &solution(vec![node], links)).unwrap();
    assert_eq!(left, right);
}

#[test]
fn decimal_preview_is_exact_integer_long_division() {
    assert_eq!(format_rate(&"1/3".parse().unwrap()).decimal, "0.333333");
    assert_eq!(format_rate(&"5/2".parse().unwrap()).decimal, "2.5");
}

#[test]
fn natural_loop_marks_the_return_into_the_merger() {
    let merger = NodeId(0);
    let splitter = NodeId(1);
    let operators = BTreeMap::from([(merger, NodeType::Merger2), (splitter, NodeType::Splitter2)]);
    let links = vec![
        PhysicalLink {
            producer: ProducerPortRef::Input(InputTerminalIndex(0)),
            consumer: ConsumerPortRef::Node {
                node: merger,
                port: 0,
            },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node {
                node: merger,
                port: 0,
            },
            consumer: ConsumerPortRef::Node {
                node: splitter,
                port: 0,
            },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node {
                node: splitter,
                port: 0,
            },
            consumer: ConsumerPortRef::Node {
                node: merger,
                port: 1,
            },
            flow: Rational::one(),
        },
    ];

    let (feedback, count) = feedback_annotation(&operators, &links);

    assert_eq!(
        feedback,
        BTreeSet::from([(
            ProducerPortRef::Node {
                node: splitter,
                port: 0,
            },
            ConsumerPortRef::Node {
                node: merger,
                port: 1,
            },
        )])
    );
    assert_eq!(count, 1);
}

#[test]
fn feedback_count_matches_the_number_of_annotated_feedback_belts() {
    let splitter = NodeId(0);
    let left_merger = NodeId(1);
    let right_merger = NodeId(2);
    let operators = BTreeMap::from([
        (splitter, NodeType::Splitter3),
        (left_merger, NodeType::Merger2),
        (right_merger, NodeType::Merger2),
    ]);
    let links = vec![
        PhysicalLink {
            producer: ProducerPortRef::Input(InputTerminalIndex(0)),
            consumer: ConsumerPortRef::Node {
                node: splitter,
                port: 0,
            },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node {
                node: splitter,
                port: 0,
            },
            consumer: ConsumerPortRef::Node {
                node: left_merger,
                port: 0,
            },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node {
                node: left_merger,
                port: 0,
            },
            consumer: ConsumerPortRef::Node {
                node: splitter,
                port: 0,
            },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node {
                node: splitter,
                port: 1,
            },
            consumer: ConsumerPortRef::Node {
                node: right_merger,
                port: 0,
            },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node {
                node: right_merger,
                port: 0,
            },
            consumer: ConsumerPortRef::Node {
                node: splitter,
                port: 1,
            },
            flow: Rational::one(),
        },
    ];

    let (feedback, count) = feedback_annotation(&operators, &links);

    assert_eq!(
        feedback,
        BTreeSet::from([
            (
                ProducerPortRef::Node {
                    node: left_merger,
                    port: 0,
                },
                ConsumerPortRef::Node {
                    node: splitter,
                    port: 0,
                },
            ),
            (
                ProducerPortRef::Node {
                    node: right_merger,
                    port: 0,
                },
                ConsumerPortRef::Node {
                    node: splitter,
                    port: 1,
                },
            ),
        ])
    );
    assert_eq!(count, 2);
}

#[test]
fn irreducible_cycle_marks_a_dfs_ancestor_edge() {
    let a = NodeId(0);
    let b = NodeId(1);
    let c = NodeId(2);
    let d = NodeId(3);
    let operators = BTreeMap::from([
        (a, NodeType::Merger2),
        (b, NodeType::Splitter2),
        (c, NodeType::Merger2),
        (d, NodeType::Splitter2),
    ]);
    let links = vec![
        PhysicalLink {
            producer: ProducerPortRef::Input(InputTerminalIndex(0)),
            consumer: ConsumerPortRef::Node { node: a, port: 0 },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Input(InputTerminalIndex(1)),
            consumer: ConsumerPortRef::Node { node: c, port: 0 },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node { node: a, port: 0 },
            consumer: ConsumerPortRef::Node { node: b, port: 0 },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node { node: b, port: 0 },
            consumer: ConsumerPortRef::Node { node: c, port: 1 },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node { node: c, port: 0 },
            consumer: ConsumerPortRef::Node { node: d, port: 0 },
            flow: Rational::one(),
        },
        PhysicalLink {
            producer: ProducerPortRef::Node { node: d, port: 0 },
            consumer: ConsumerPortRef::Node { node: a, port: 1 },
            flow: Rational::one(),
        },
    ];

    let (feedback, count) = feedback_annotation(&operators, &links);

    assert_eq!(
        feedback,
        BTreeSet::from([(
            ProducerPortRef::Node { node: d, port: 0 },
            ConsumerPortRef::Node { node: a, port: 1 },
        )])
    );
    assert_eq!(count, 1);
}
