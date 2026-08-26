use std::collections::{BTreeMap, BTreeSet, VecDeque};

use num::{BigRational, One, Zero};
use solver_api::{
    ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeType,
    OutputTerminalIndex, PhysicalGraph, Problem, ProducerPortRef, Rational, ValidationSummary,
};
use thiserror::Error;

use crate::{LinearSolveError, solve_fraction_free};

/// Schema version of the checks performed by this crate.
pub const VALIDATOR_VERSION: u32 = 2;

/// A concrete graph or exact-flow failure found without trusting production state.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum ValidationError {
    #[error("the problem has no external inputs")]
    ProblemHasNoInputs,
    #[error("the problem has no external outputs")]
    ProblemHasNoOutputs,
    #[error("the maximum physical-link rate must be strictly positive, got {rate}")]
    NonPositiveCapacity { rate: Rational },
    #[error("input {index} has a nonpositive rate {rate}")]
    NonPositiveInput { index: u32, rate: Rational },
    #[error("output {index} has a nonpositive rate {rate}")]
    NonPositiveOutput { index: u32, rate: Rational },
    #[error("input {index} rate {rate} exceeds physical-link capacity {capacity}")]
    InputAboveCapacity {
        index: u32,
        rate: Rational,
        capacity: Box<Rational>,
    },
    #[error("output {index} rate {rate} exceeds physical-link capacity {capacity}")]
    OutputAboveCapacity {
        index: u32,
        rate: Rational,
        capacity: Box<Rational>,
    },
    #[error("requested output {total_output} exceeds total external input {total_input}")]
    InsufficientInput {
        total_input: Rational,
        total_output: Box<Rational>,
    },
    #[error("the problem or graph is too large for its u32 public indexes")]
    GraphTooLarge,
    #[error("physical node id {node:?} occurs more than once")]
    DuplicateNodeId { node: NodeId },
    #[error("link {link} refers to missing producer node {node:?}")]
    MissingProducerNode { link: usize, node: NodeId },
    #[error("link {link} refers to missing consumer node {node:?}")]
    MissingConsumerNode { link: usize, node: NodeId },
    #[error("link {link} refers to missing input terminal {index:?}")]
    MissingInputTerminal {
        link: usize,
        index: InputTerminalIndex,
    },
    #[error("link {link} refers to missing output terminal {index:?}")]
    MissingOutputTerminal {
        link: usize,
        index: OutputTerminalIndex,
    },
    #[error("discard terminal indexes are not contiguous: expected {expected:?}, found {found:?}")]
    NonContiguousDiscardTerminal {
        expected: DiscardTerminalIndex,
        found: DiscardTerminalIndex,
    },
    #[error("discard links are forbidden when exact input surplus is zero")]
    DiscardWithoutSurplus,
    #[error("positive input surplus {surplus} requires at least one discard link")]
    MissingDiscardLink { surplus: Rational },
    #[error("link {link} uses invalid producer port {port} on {node:?} ({node_type:?})")]
    InvalidProducerPort {
        link: usize,
        node: NodeId,
        node_type: NodeType,
        port: u8,
    },
    #[error("link {link} uses invalid consumer port {port} on {node:?} ({node_type:?})")]
    InvalidConsumerPort {
        link: usize,
        node: NodeId,
        node_type: NodeType,
        port: u8,
    },
    #[error("link {link} directly connects node {node:?} to itself")]
    DirectNodeSelfLink { link: usize, node: NodeId },
    #[error("producer port {producer:?} is used by links {first_link} and {second_link}")]
    DuplicateProducerLink {
        producer: ProducerPortRef,
        first_link: usize,
        second_link: usize,
    },
    #[error("consumer port {consumer:?} is used by links {first_link} and {second_link}")]
    DuplicateConsumerLink {
        consumer: ConsumerPortRef,
        first_link: usize,
        second_link: usize,
    },
    #[error("mandatory producer port {producer:?} has no physical link")]
    MissingProducerLink { producer: ProducerPortRef },
    #[error("mandatory consumer port {consumer:?} has no physical link")]
    MissingConsumerLink { consumer: ConsumerPortRef },
    #[error("physical node {node:?} is not reachable from any external input")]
    NodeUnreachableFromInput { node: NodeId },
    #[error("physical node {node:?} has no path to any requested output or discard sink")]
    NodeCannotReachOutput { node: NodeId },
    #[error("the complete exact steady-state equations are inconsistent")]
    InconsistentSteadyState,
    #[error(
        "the complete exact steady-state equations are not unique: rank {rank} for {variables} physical-link flows"
    )]
    NonUniqueSteadyState { rank: usize, variables: usize },
    #[error("cyclic SCC {nodes:?} has inconsistent exact local equations")]
    InconsistentCyclicScc { nodes: Vec<NodeId> },
    #[error(
        "cyclic SCC {nodes:?} is not locally unique: rank {rank} for {variables} SCC-produced flows"
    )]
    NonUniqueCyclicScc {
        nodes: Vec<NodeId>,
        rank: usize,
        variables: usize,
    },
    #[error("independent exact algebra failed: {0}")]
    ExactAlgebra(LinearSolveError),
    #[error("resolved output {index} has rate {actual}, expected {expected}")]
    IncorrectOutputRate {
        index: u32,
        expected: Rational,
        actual: Box<Rational>,
    },
    #[error("resolved link {link} has nonpositive flow {flow}")]
    NonPositiveResolvedFlow { link: usize, flow: Rational },
    #[error("resolved link {link} flow {flow} exceeds capacity {capacity}")]
    ResolvedFlowAboveCapacity {
        link: usize,
        flow: Rational,
        capacity: Box<Rational>,
    },
    #[error("supplied link {link} has nonpositive flow {flow}")]
    NonPositiveSuppliedFlow { link: usize, flow: Rational },
    #[error("supplied link {link} flow {flow} exceeds capacity {capacity}")]
    SuppliedFlowAboveCapacity {
        link: usize,
        flow: Rational,
        capacity: Box<Rational>,
    },
    #[error("input {index} link {link} supplies {actual}, expected {expected}")]
    IncorrectSuppliedInputRate {
        link: usize,
        index: u32,
        expected: Rational,
        actual: Box<Rational>,
    },
    #[error("output {index} link {link} supplies {actual}, expected {expected}")]
    IncorrectSuppliedOutputRate {
        link: usize,
        index: u32,
        expected: Rational,
        actual: Box<Rational>,
    },
    #[error("supplied discard links total {actual}, expected exact surplus {expected}")]
    IncorrectSuppliedDiscardTotal {
        expected: Rational,
        actual: Box<Rational>,
    },
    #[error("resolved discard links total {actual}, expected exact surplus {expected}")]
    IncorrectResolvedDiscardTotal {
        expected: Rational,
        actual: Box<Rational>,
    },
    #[error("link {link} supplies {actual}, but independent algebra resolves {expected}")]
    IncorrectSuppliedFlow {
        link: usize,
        expected: Rational,
        actual: Box<Rational>,
    },
}

/// Solve a complete physical topology without trusting any supplied link flow.
///
/// The returned graph has every `flow` replaced by the unique exact value derived
/// from external inputs and node equations. Requested outputs, positivity, capacity,
/// anonymous discard surplus, and live source-to-terminal structure are then checked
/// independently.
///
/// # Errors
///
/// Returns [`ValidationError`] when the problem is invalid, the physical topology
/// is malformed, its exact steady state is absent or nonunique, a node is dead, or
/// a resolved link/output violates the requested exact semantics.
pub fn solve_topology(
    problem: &Problem,
    graph_topology: &PhysicalGraph,
) -> Result<PhysicalGraph, ValidationError> {
    solve_topology_internal(problem, graph_topology).map(|solved| solved.graph)
}

/// Validate a claimed concrete witness and return a summary only after every check succeeds.
///
/// # Errors
///
/// Returns [`ValidationError`] for every error detected by [`solve_topology`], and
/// also when a supplied flow differs from the independently reconstructed flow or
/// lies outside the mandatory exact capacity interval.
pub fn validate_solution(
    problem: &Problem,
    graph: &PhysicalGraph,
) -> Result<ValidationSummary, ValidationError> {
    let solved = solve_topology_internal(problem, graph)?;

    for (link, physical_link) in graph.links.iter().enumerate() {
        if !physical_link.flow.is_positive() {
            return Err(ValidationError::NonPositiveSuppliedFlow {
                link,
                flow: physical_link.flow.clone(),
            });
        }
        if physical_link.flow > problem.max_link_rate {
            return Err(ValidationError::SuppliedFlowAboveCapacity {
                link,
                flow: physical_link.flow.clone(),
                capacity: Box::new(problem.max_link_rate.clone()),
            });
        }
    }

    let expected_discard = discard_surplus(problem)?;
    let supplied_discard = graph
        .links
        .iter()
        .filter(|link| matches!(link.consumer, ConsumerPortRef::Discard(_)))
        .map(|link| link.flow.clone())
        .sum::<Rational>();
    if supplied_discard != expected_discard {
        return Err(ValidationError::IncorrectSuppliedDiscardTotal {
            expected: expected_discard,
            actual: Box::new(supplied_discard),
        });
    }

    for (link, physical_link) in graph.links.iter().enumerate() {
        if let ProducerPortRef::Input(index) = physical_link.producer {
            let expected = problem.inputs[index.0 as usize].clone();
            if physical_link.flow != expected {
                return Err(ValidationError::IncorrectSuppliedInputRate {
                    link,
                    index: index.0,
                    expected,
                    actual: Box::new(physical_link.flow.clone()),
                });
            }
        }
        if let ConsumerPortRef::Output(index) = physical_link.consumer {
            let expected = problem.outputs[index.0 as usize].clone();
            if physical_link.flow != expected {
                return Err(ValidationError::IncorrectSuppliedOutputRate {
                    link,
                    index: index.0,
                    expected,
                    actual: Box::new(physical_link.flow.clone()),
                });
            }
        }
        let expected = &solved.graph.links[link].flow;
        if &physical_link.flow != expected {
            return Err(ValidationError::IncorrectSuppliedFlow {
                link,
                expected: expected.clone(),
                actual: Box::new(physical_link.flow.clone()),
            });
        }
    }

    let physical_link_count =
        u32::try_from(graph.links.len()).map_err(|_| ValidationError::GraphTooLarge)?;
    let discard_link_count = u32::try_from(
        graph
            .links
            .iter()
            .filter(|link| matches!(link.consumer, ConsumerPortRef::Discard(_)))
            .count(),
    )
    .map_err(|_| ValidationError::GraphTooLarge)?;
    let link_count = u32::try_from(
        graph
            .links
            .iter()
            .filter(|link| {
                matches!(link.producer, ProducerPortRef::Node { .. })
                    && matches!(link.consumer, ConsumerPortRef::Node { .. })
            })
            .count(),
    )
    .map_err(|_| ValidationError::GraphTooLarge)?;
    Ok(ValidationSummary {
        validator_version: VALIDATOR_VERSION,
        node_count: u32::try_from(graph.nodes.len()).map_err(|_| ValidationError::GraphTooLarge)?,
        link_count,
        physical_link_count,
        discard_link_count,
        cyclic_scc_count: solved.cyclic_scc_count,
    })
}

struct SolvedTopology {
    graph: PhysicalGraph,
    cyclic_scc_count: u32,
}

fn solve_topology_internal(
    problem: &Problem,
    graph: &PhysicalGraph,
) -> Result<SolvedTopology, ValidationError> {
    validate_problem(problem)?;
    let topology = Topology::new(problem, graph)?;
    let flows = topology.solve_flows(problem, graph)?;
    topology.check_cyclic_scc_uniqueness(problem, graph, &flows)?;
    topology.check_reachability()?;

    for (index, expected) in problem.outputs.iter().enumerate() {
        let consumer = ConsumerPortRef::Output(OutputTerminalIndex(
            u32::try_from(index).map_err(|_| ValidationError::GraphTooLarge)?,
        ));
        let link = topology.consumer_links[&consumer];
        let actual = Rational::from(flows[link].clone());
        if &actual != expected {
            return Err(ValidationError::IncorrectOutputRate {
                index: u32::try_from(index).map_err(|_| ValidationError::GraphTooLarge)?,
                expected: expected.clone(),
                actual: Box::new(actual),
            });
        }
    }

    let expected_discard = discard_surplus(problem)?;
    let resolved_discard = topology
        .discard_links
        .iter()
        .fold(BigRational::zero(), |sum, &link| sum + &flows[link]);
    let resolved_discard = Rational::from(resolved_discard);
    if resolved_discard != expected_discard {
        return Err(ValidationError::IncorrectResolvedDiscardTotal {
            expected: expected_discard,
            actual: Box::new(resolved_discard),
        });
    }

    for (link, flow) in flows.iter().enumerate() {
        let flow = Rational::from(flow.clone());
        if !flow.is_positive() {
            return Err(ValidationError::NonPositiveResolvedFlow { link, flow });
        }
        if flow > problem.max_link_rate {
            return Err(ValidationError::ResolvedFlowAboveCapacity {
                link,
                flow,
                capacity: Box::new(problem.max_link_rate.clone()),
            });
        }
    }

    let mut solved_graph = graph.clone();
    for (link, flow) in solved_graph.links.iter_mut().zip(flows) {
        link.flow = Rational::from(flow);
    }
    Ok(SolvedTopology {
        graph: solved_graph,
        cyclic_scc_count: u32::try_from(topology.cyclic_scc_count())
            .map_err(|_| ValidationError::GraphTooLarge)?,
    })
}

fn validate_problem(problem: &Problem) -> Result<(), ValidationError> {
    if problem.inputs.is_empty() {
        return Err(ValidationError::ProblemHasNoInputs);
    }
    if problem.outputs.is_empty() {
        return Err(ValidationError::ProblemHasNoOutputs);
    }
    if !problem.max_link_rate.is_positive() {
        return Err(ValidationError::NonPositiveCapacity {
            rate: problem.max_link_rate.clone(),
        });
    }
    if problem.inputs.len() > u32::MAX as usize || problem.outputs.len() > u32::MAX as usize {
        return Err(ValidationError::GraphTooLarge);
    }
    for (index, rate) in problem.inputs.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| ValidationError::GraphTooLarge)?;
        if !rate.is_positive() {
            return Err(ValidationError::NonPositiveInput {
                index,
                rate: rate.clone(),
            });
        }
        if rate > &problem.max_link_rate {
            return Err(ValidationError::InputAboveCapacity {
                index,
                rate: rate.clone(),
                capacity: Box::new(problem.max_link_rate.clone()),
            });
        }
    }
    for (index, rate) in problem.outputs.iter().enumerate() {
        let index = u32::try_from(index).map_err(|_| ValidationError::GraphTooLarge)?;
        if !rate.is_positive() {
            return Err(ValidationError::NonPositiveOutput {
                index,
                rate: rate.clone(),
            });
        }
        if rate > &problem.max_link_rate {
            return Err(ValidationError::OutputAboveCapacity {
                index,
                rate: rate.clone(),
                capacity: Box::new(problem.max_link_rate.clone()),
            });
        }
    }
    discard_surplus(problem)?;
    Ok(())
}

fn discard_surplus(problem: &Problem) -> Result<Rational, ValidationError> {
    problem
        .surplus()
        .ok_or_else(|| ValidationError::InsufficientInput {
            total_input: problem.total_input(),
            total_output: Box::new(problem.total_output()),
        })
}

struct Topology {
    nodes: BTreeMap<NodeId, NodeType>,
    producer_links: BTreeMap<ProducerPortRef, usize>,
    consumer_links: BTreeMap<ConsumerPortRef, usize>,
    discard_links: Vec<usize>,
    node_indexes: BTreeMap<NodeId, usize>,
    node_ids: Vec<NodeId>,
    adjacency: Vec<Vec<usize>>,
    reverse_adjacency: Vec<Vec<usize>>,
    source_roots: Vec<bool>,
    output_roots: Vec<bool>,
}

impl Topology {
    #[allow(clippy::too_many_lines)]
    fn new(problem: &Problem, graph: &PhysicalGraph) -> Result<Self, ValidationError> {
        if graph.nodes.len() > u32::MAX as usize || graph.links.len() > u32::MAX as usize {
            return Err(ValidationError::GraphTooLarge);
        }

        let mut nodes = BTreeMap::new();
        for physical_node in &graph.nodes {
            if nodes
                .insert(physical_node.id, physical_node.node_type)
                .is_some()
            {
                return Err(ValidationError::DuplicateNodeId {
                    node: physical_node.id,
                });
            }
        }

        let mut producer_links = BTreeMap::new();
        let mut consumer_links = BTreeMap::new();
        for (link, physical_link) in graph.links.iter().enumerate() {
            validate_producer(problem, &nodes, link, physical_link.producer)?;
            validate_consumer(problem, &nodes, link, physical_link.consumer)?;
            if let (
                ProducerPortRef::Node { node: producer, .. },
                ConsumerPortRef::Node { node: consumer, .. },
            ) = (physical_link.producer, physical_link.consumer)
                && producer == consumer
            {
                return Err(ValidationError::DirectNodeSelfLink {
                    link,
                    node: producer,
                });
            }
            if let Some(first_link) = producer_links.insert(physical_link.producer, link) {
                return Err(ValidationError::DuplicateProducerLink {
                    producer: physical_link.producer,
                    first_link,
                    second_link: link,
                });
            }
            if let Some(first_link) = consumer_links.insert(physical_link.consumer, link) {
                return Err(ValidationError::DuplicateConsumerLink {
                    consumer: physical_link.consumer,
                    first_link,
                    second_link: link,
                });
            }
        }

        let discard_terminals = consumer_links
            .iter()
            .filter_map(|(consumer, &link)| match consumer {
                ConsumerPortRef::Discard(index) => Some((*index, link)),
                ConsumerPortRef::Output(_) | ConsumerPortRef::Node { .. } => None,
            })
            .collect::<Vec<_>>();
        for (expected, (found, _)) in discard_terminals.iter().enumerate() {
            let expected = DiscardTerminalIndex(
                u32::try_from(expected).map_err(|_| ValidationError::GraphTooLarge)?,
            );
            if *found != expected {
                return Err(ValidationError::NonContiguousDiscardTerminal {
                    expected,
                    found: *found,
                });
            }
        }
        let surplus = discard_surplus(problem)?;
        if surplus.is_zero() && !discard_terminals.is_empty() {
            return Err(ValidationError::DiscardWithoutSurplus);
        }
        if surplus.is_positive() && discard_terminals.is_empty() {
            return Err(ValidationError::MissingDiscardLink { surplus });
        }
        let discard_links = discard_terminals
            .into_iter()
            .map(|(_, link)| link)
            .collect::<Vec<_>>();

        for input in 0..problem.inputs.len() {
            let producer = ProducerPortRef::Input(InputTerminalIndex(
                u32::try_from(input).map_err(|_| ValidationError::GraphTooLarge)?,
            ));
            if !producer_links.contains_key(&producer) {
                return Err(ValidationError::MissingProducerLink { producer });
            }
        }
        for (&node, &node_type) in &nodes {
            for port in 0..node_type.output_port_count() {
                let producer = ProducerPortRef::Node { node, port };
                if !producer_links.contains_key(&producer) {
                    return Err(ValidationError::MissingProducerLink { producer });
                }
            }
            for port in 0..node_type.input_port_count() {
                let consumer = ConsumerPortRef::Node { node, port };
                if !consumer_links.contains_key(&consumer) {
                    return Err(ValidationError::MissingConsumerLink { consumer });
                }
            }
        }
        for output in 0..problem.outputs.len() {
            let consumer = ConsumerPortRef::Output(OutputTerminalIndex(
                u32::try_from(output).map_err(|_| ValidationError::GraphTooLarge)?,
            ));
            if !consumer_links.contains_key(&consumer) {
                return Err(ValidationError::MissingConsumerLink { consumer });
            }
        }

        let node_indexes = nodes
            .keys()
            .copied()
            .enumerate()
            .map(|(index, node)| (node, index))
            .collect::<BTreeMap<_, _>>();
        let node_ids = nodes.keys().copied().collect::<Vec<_>>();
        let mut adjacency = vec![Vec::new(); nodes.len()];
        let mut reverse_adjacency = vec![Vec::new(); nodes.len()];
        let mut source_roots = vec![false; nodes.len()];
        let mut output_roots = vec![false; nodes.len()];
        for physical_link in &graph.links {
            match (physical_link.producer, physical_link.consumer) {
                (ProducerPortRef::Input(_), ConsumerPortRef::Node { node, .. }) => {
                    source_roots[node_indexes[&node]] = true;
                }
                (
                    ProducerPortRef::Node { node: source, .. },
                    ConsumerPortRef::Node { node: target, .. },
                ) => {
                    let source = node_indexes[&source];
                    let target = node_indexes[&target];
                    adjacency[source].push(target);
                    reverse_adjacency[target].push(source);
                }
                (
                    ProducerPortRef::Node { node, .. },
                    ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_),
                ) => {
                    output_roots[node_indexes[&node]] = true;
                }
                (
                    ProducerPortRef::Input(_),
                    ConsumerPortRef::Output(_) | ConsumerPortRef::Discard(_),
                ) => {}
            }
        }

        Ok(Self {
            nodes,
            producer_links,
            consumer_links,
            discard_links,
            node_indexes,
            node_ids,
            adjacency,
            reverse_adjacency,
            source_roots,
            output_roots,
        })
    }

    fn solve_flows(
        &self,
        problem: &Problem,
        graph: &PhysicalGraph,
    ) -> Result<Vec<BigRational>, ValidationError> {
        let variables = graph.links.len();
        let mut coefficients = Vec::with_capacity(variables);
        let mut constants = Vec::with_capacity(variables);

        for (input, rate) in problem.inputs.iter().enumerate() {
            let producer = ProducerPortRef::Input(InputTerminalIndex(
                u32::try_from(input).map_err(|_| ValidationError::GraphTooLarge)?,
            ));
            let mut row = vec![BigRational::zero(); variables];
            row[self.producer_links[&producer]] = BigRational::one();
            coefficients.push(row);
            constants.push(rate.as_big_rational().clone());
        }

        for (&node, &node_type) in &self.nodes {
            match node_type {
                NodeType::Splitter2 | NodeType::Splitter3 => {
                    let input = self.consumer_links[&ConsumerPortRef::Node { node, port: 0 }];
                    let arity =
                        BigRational::from_integer(i64::from(node_type.output_port_count()).into());
                    for port in 0..node_type.output_port_count() {
                        let output = self.producer_links[&ProducerPortRef::Node { node, port }];
                        let mut row = vec![BigRational::zero(); variables];
                        row[output] = arity.clone();
                        row[input] -= BigRational::one();
                        coefficients.push(row);
                        constants.push(BigRational::zero());
                    }
                }
                NodeType::Merger2 | NodeType::Merger3 => {
                    let output = self.producer_links[&ProducerPortRef::Node { node, port: 0 }];
                    let mut row = vec![BigRational::zero(); variables];
                    row[output] = BigRational::one();
                    for port in 0..node_type.input_port_count() {
                        let input = self.consumer_links[&ConsumerPortRef::Node { node, port }];
                        row[input] -= BigRational::one();
                    }
                    coefficients.push(row);
                    constants.push(BigRational::zero());
                }
            }
        }

        // Anonymous discard rates are not prescribed individually, but their one
        // exact sum is part of the physical steady-state system. Keeping every
        // discard link as its own variable preserves capacity checks on each belt.
        let mut discard_sum = vec![BigRational::zero(); variables];
        for &link in &self.discard_links {
            discard_sum[link] = BigRational::one();
        }
        coefficients.push(discard_sum);
        constants.push(discard_surplus(problem)?.into_big_rational());

        solve_fraction_free(&coefficients, &constants).map_err(|error| match error {
            LinearSolveError::Inconsistent => ValidationError::InconsistentSteadyState,
            LinearSolveError::NonUnique { rank, variables } => {
                ValidationError::NonUniqueSteadyState { rank, variables }
            }
            other => ValidationError::ExactAlgebra(other),
        })
    }

    fn check_reachability(&self) -> Result<(), ValidationError> {
        let mut source_reachable = self.source_roots.clone();
        spread_reachability(&self.adjacency, &mut source_reachable);
        for (&node, &index) in &self.node_indexes {
            if !source_reachable[index] {
                return Err(ValidationError::NodeUnreachableFromInput { node });
            }
        }

        let mut output_reachable = self.output_roots.clone();
        spread_reachability(&self.reverse_adjacency, &mut output_reachable);
        for (&node, &index) in &self.node_indexes {
            if !output_reachable[index] {
                return Err(ValidationError::NodeCannotReachOutput { node });
            }
        }
        Ok(())
    }

    /// Rebuild and rank-check every cyclic SCC independently.
    ///
    /// Variables are exactly the physical links produced by nodes in the SCC,
    /// including links that leave it. A link entering from an external input or
    /// another SCC contributes only a known boundary constant. No downstream node
    /// equation and no requested-output equation is admitted. Full column rank is
    /// therefore a direct proof that the sealed region has one internal steady flow
    /// for its concrete incoming boundary values.
    fn check_cyclic_scc_uniqueness(
        &self,
        problem: &Problem,
        graph: &PhysicalGraph,
        resolved_flows: &[BigRational],
    ) -> Result<(), ValidationError> {
        for nodes in self.cyclic_sccs() {
            let node_set = nodes.iter().copied().collect::<BTreeSet<_>>();
            let mut variables = BTreeMap::new();
            for &node in &nodes {
                let node_type = self.nodes[&node];
                for port in 0..node_type.output_port_count() {
                    let producer = ProducerPortRef::Node { node, port };
                    let next = variables.len();
                    variables.insert(producer, next);
                }
            }

            let mut coefficients = Vec::with_capacity(variables.len());
            let mut constants = Vec::with_capacity(variables.len());
            for &node in &nodes {
                let node_type = self.nodes[&node];
                match node_type {
                    NodeType::Splitter2 | NodeType::Splitter3 => {
                        let input_link =
                            self.consumer_links[&ConsumerPortRef::Node { node, port: 0 }];
                        let arity = BigRational::from_integer(
                            i64::from(node_type.output_port_count()).into(),
                        );
                        for port in 0..node_type.output_port_count() {
                            let mut row = vec![BigRational::zero(); variables.len()];
                            row[variables[&ProducerPortRef::Node { node, port }]] = arity.clone();
                            let mut constant = BigRational::zero();
                            add_scc_input(
                                &mut row,
                                &mut constant,
                                &variables,
                                &node_set,
                                graph,
                                resolved_flows,
                                input_link,
                            );
                            coefficients.push(row);
                            constants.push(constant);
                        }
                    }
                    NodeType::Merger2 | NodeType::Merger3 => {
                        let mut row = vec![BigRational::zero(); variables.len()];
                        row[variables[&ProducerPortRef::Node { node, port: 0 }]] =
                            BigRational::one();
                        let mut constant = BigRational::zero();
                        for port in 0..node_type.input_port_count() {
                            let input_link =
                                self.consumer_links[&ConsumerPortRef::Node { node, port }];
                            add_scc_input(
                                &mut row,
                                &mut constant,
                                &variables,
                                &node_set,
                                graph,
                                resolved_flows,
                                input_link,
                            );
                        }
                        coefficients.push(row);
                        constants.push(constant);
                    }
                }
            }

            // Project the single global discard-sum equation onto this SCC. A
            // discard produced inside the SCC remains a local unknown; every
            // discard produced elsewhere is a concrete boundary constant from
            // the independently solved full graph. Thus the row can only add a
            // genuine exact uniqueness constraint and never invent a local one.
            let mut discard_row = vec![BigRational::zero(); variables.len()];
            let mut discard_constant = discard_surplus(problem)?.into_big_rational();
            for &link in &self.discard_links {
                let producer = graph.links[link].producer;
                if let ProducerPortRef::Node { node, .. } = producer
                    && node_set.contains(&node)
                {
                    discard_row[variables[&producer]] += BigRational::one();
                } else {
                    discard_constant -= &resolved_flows[link];
                }
            }
            coefficients.push(discard_row);
            constants.push(discard_constant);

            match solve_fraction_free(&coefficients, &constants) {
                Ok(_) => {}
                Err(LinearSolveError::Inconsistent) => {
                    return Err(ValidationError::InconsistentCyclicScc { nodes });
                }
                Err(LinearSolveError::NonUnique { rank, variables }) => {
                    return Err(ValidationError::NonUniqueCyclicScc {
                        nodes,
                        rank,
                        variables,
                    });
                }
                Err(other) => return Err(ValidationError::ExactAlgebra(other)),
            }
        }
        Ok(())
    }

    fn cyclic_sccs(&self) -> Vec<Vec<NodeId>> {
        cyclic_scc_indexes(&self.adjacency, &self.reverse_adjacency)
            .into_iter()
            .map(|component| {
                let mut nodes = component
                    .into_iter()
                    .map(|index| self.node_ids[index])
                    .collect::<Vec<_>>();
                nodes.sort_unstable();
                nodes
            })
            .collect()
    }

    fn cyclic_scc_count(&self) -> usize {
        self.cyclic_sccs().len()
    }
}

fn add_scc_input(
    row: &mut [BigRational],
    constant: &mut BigRational,
    variables: &BTreeMap<ProducerPortRef, usize>,
    node_set: &BTreeSet<NodeId>,
    graph: &PhysicalGraph,
    resolved_flows: &[BigRational],
    input_link: usize,
) {
    let producer = graph.links[input_link].producer;
    if let ProducerPortRef::Node { node, .. } = producer
        && node_set.contains(&node)
    {
        row[variables[&producer]] -= BigRational::one();
    } else {
        *constant += &resolved_flows[input_link];
    }
}

fn validate_producer(
    problem: &Problem,
    nodes: &BTreeMap<NodeId, NodeType>,
    link: usize,
    producer: ProducerPortRef,
) -> Result<(), ValidationError> {
    match producer {
        ProducerPortRef::Input(index) => {
            if index.0 as usize >= problem.inputs.len() {
                return Err(ValidationError::MissingInputTerminal { link, index });
            }
        }
        ProducerPortRef::Node { node, port } => {
            let Some(&node_type) = nodes.get(&node) else {
                return Err(ValidationError::MissingProducerNode { link, node });
            };
            if port >= node_type.output_port_count() {
                return Err(ValidationError::InvalidProducerPort {
                    link,
                    node,
                    node_type,
                    port,
                });
            }
        }
    }
    Ok(())
}

fn validate_consumer(
    problem: &Problem,
    nodes: &BTreeMap<NodeId, NodeType>,
    link: usize,
    consumer: ConsumerPortRef,
) -> Result<(), ValidationError> {
    match consumer {
        ConsumerPortRef::Output(index) => {
            if index.0 as usize >= problem.outputs.len() {
                return Err(ValidationError::MissingOutputTerminal { link, index });
            }
        }
        ConsumerPortRef::Discard(_) => {}
        ConsumerPortRef::Node { node, port } => {
            let Some(&node_type) = nodes.get(&node) else {
                return Err(ValidationError::MissingConsumerNode { link, node });
            };
            if port >= node_type.input_port_count() {
                return Err(ValidationError::InvalidConsumerPort {
                    link,
                    node,
                    node_type,
                    port,
                });
            }
        }
    }
    Ok(())
}

fn spread_reachability(adjacency: &[Vec<usize>], reachable: &mut [bool]) {
    let mut pending = reachable
        .iter()
        .enumerate()
        .filter_map(|(node, &is_reachable)| is_reachable.then_some(node))
        .collect::<VecDeque<_>>();
    while let Some(node) = pending.pop_front() {
        for &next in &adjacency[node] {
            if !reachable[next] {
                reachable[next] = true;
                pending.push_back(next);
            }
        }
    }
}

fn cyclic_scc_indexes(
    adjacency: &[Vec<usize>],
    reverse_adjacency: &[Vec<usize>],
) -> Vec<Vec<usize>> {
    let mut visited = vec![false; adjacency.len()];
    let mut finish_order = Vec::with_capacity(adjacency.len());
    for start in 0..adjacency.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0)];
        while let Some((node, next_edge)) = stack.last_mut() {
            if *next_edge < adjacency[*node].len() {
                let next = adjacency[*node][*next_edge];
                *next_edge += 1;
                if !visited[next] {
                    visited[next] = true;
                    stack.push((next, 0));
                }
            } else {
                finish_order.push(*node);
                stack.pop();
            }
        }
    }

    visited.fill(false);
    let mut cyclic = Vec::new();
    for &start in finish_order.iter().rev() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut component = Vec::new();
        let mut pending = vec![start];
        while let Some(node) = pending.pop() {
            component.push(node);
            for &next in &reverse_adjacency[node] {
                if !visited[next] {
                    visited[next] = true;
                    pending.push(next);
                }
            }
        }
        if component.len() > 1 {
            cyclic.push(component);
        }
    }
    cyclic.sort_by_key(|component| component.iter().copied().min().unwrap_or(usize::MAX));
    cyclic
}

#[cfg(test)]
mod tests {
    use super::*;
    use solver_api::{PhysicalLink, PhysicalNode};

    fn rational(value: &str) -> Rational {
        value.parse().unwrap()
    }

    fn problem(inputs: &[&str], outputs: &[&str], capacity: &str) -> Problem {
        Problem {
            inputs: inputs.iter().map(|value| rational(value)).collect(),
            outputs: outputs.iter().map(|value| rational(value)).collect(),
            max_link_rate: rational(capacity),
        }
    }

    fn node(id: u32, node_type: NodeType) -> PhysicalNode {
        PhysicalNode {
            id: NodeId(id),
            node_type,
        }
    }

    fn link(producer: ProducerPortRef, consumer: ConsumerPortRef, flow: &str) -> PhysicalLink {
        PhysicalLink {
            producer,
            consumer,
            flow: rational(flow),
        }
    }

    const fn input(index: u32) -> ProducerPortRef {
        ProducerPortRef::Input(InputTerminalIndex(index))
    }

    const fn output(index: u32) -> ConsumerPortRef {
        ConsumerPortRef::Output(OutputTerminalIndex(index))
    }

    const fn discard(index: u32) -> ConsumerPortRef {
        ConsumerPortRef::Discard(DiscardTerminalIndex(index))
    }

    const fn node_output(node: u32, port: u8) -> ProducerPortRef {
        ProducerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    const fn node_input(node: u32, port: u8) -> ConsumerPortRef {
        ConsumerPortRef::Node {
            node: NodeId(node),
            port,
        }
    }

    fn direct_graph(flow: &str) -> PhysicalGraph {
        PhysicalGraph {
            nodes: Vec::new(),
            links: vec![link(input(0), output(0), flow)],
        }
    }

    fn split_graph() -> PhysicalGraph {
        PhysicalGraph {
            nodes: vec![node(0, NodeType::Splitter2)],
            links: vec![
                link(input(0), node_input(0, 0), "120"),
                link(node_output(0, 0), output(0), "60"),
                link(node_output(0, 1), output(1), "60"),
            ],
        }
    }

    fn feedback_graph() -> PhysicalGraph {
        PhysicalGraph {
            nodes: vec![
                node(0, NodeType::Merger2),
                node(1, NodeType::Splitter2),
                node(2, NodeType::Splitter2),
            ],
            links: vec![
                link(input(0), node_input(0, 0), "5"),
                link(node_output(2, 0), node_input(0, 1), "5/3"),
                link(node_output(0, 0), node_input(1, 0), "20/3"),
                link(node_output(1, 0), node_input(2, 0), "10/3"),
                link(node_output(1, 1), output(0), "10/3"),
                link(node_output(2, 1), output(1), "5/3"),
            ],
        }
    }

    fn discard_split_graph() -> PhysicalGraph {
        PhysicalGraph {
            nodes: vec![node(0, NodeType::Splitter3)],
            links: vec![
                link(input(0), node_input(0, 0), "120"),
                link(node_output(0, 0), output(0), "40"),
                link(node_output(0, 1), discard(0), "40"),
                link(node_output(0, 2), discard(1), "40"),
            ],
        }
    }

    fn feedback_discard_graph() -> PhysicalGraph {
        let mut graph = feedback_graph();
        graph.links[5].consumer = discard(0);
        graph
    }

    #[test]
    fn validates_a_direct_link_and_replaces_untrusted_flow_when_solving() {
        let problem = problem(&["1"], &["1"], "1000");
        let summary = validate_solution(&problem, &direct_graph("1")).unwrap();
        assert_eq!(summary.node_count, 0);
        assert_eq!(summary.link_count, 0);
        assert_eq!(summary.physical_link_count, 1);
        assert_eq!(summary.discard_link_count, 0);
        assert_eq!(summary.cyclic_scc_count, 0);

        let solved = solve_topology(&problem, &direct_graph("999")).unwrap();
        assert_eq!(solved.links[0].flow, rational("1"));
    }

    #[test]
    fn validates_exact_splitter_and_merger_equations_at_capacity() {
        let split_problem = problem(&["120"], &["60", "60"], "120");
        let split = validate_solution(&split_problem, &split_graph()).unwrap();
        assert_eq!((split.node_count, split.link_count), (1, 0));

        let merge_problem = problem(&["30", "90"], &["120"], "120");
        let merge_graph = PhysicalGraph {
            nodes: vec![node(7, NodeType::Merger2)],
            links: vec![
                link(input(0), node_input(7, 0), "30"),
                link(input(1), node_input(7, 1), "90"),
                link(node_output(7, 0), output(0), "120"),
            ],
        };
        let merge = validate_solution(&merge_problem, &merge_graph).unwrap();
        assert_eq!((merge.node_count, merge.link_count), (1, 0));
    }

    #[test]
    fn validates_discard_at_capacity_and_separates_physical_from_modeled_links() {
        let problem = problem(&["10", "5"], &["5"], "10");
        let graph = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(0), discard(0), "10"),
                link(input(1), output(0), "5"),
            ],
        };

        let summary = validate_solution(&problem, &graph).unwrap();
        assert_eq!(summary.node_count, 0);
        assert_eq!(summary.link_count, 0);
        assert_eq!(summary.physical_link_count, 2);
        assert_eq!(summary.discard_link_count, 1);
    }

    #[test]
    fn validates_multiple_anonymous_discards_and_excludes_them_from_cost() {
        let problem = problem(&["120"], &["40"], "120");
        let summary = validate_solution(&problem, &discard_split_graph()).unwrap();

        assert_eq!(summary.node_count, 1);
        assert_eq!(summary.link_count, 0);
        assert_eq!(summary.physical_link_count, 4);
        assert_eq!(summary.discard_link_count, 2);
    }

    #[test]
    fn rejects_resolved_discard_flow_above_capacity() {
        let problem = problem(&["6", "6", "1"], &["1"], "6");
        let graph = PhysicalGraph {
            nodes: vec![node(0, NodeType::Merger2)],
            links: vec![
                link(input(0), node_input(0, 0), "6"),
                link(input(1), node_input(0, 1), "6"),
                link(node_output(0, 0), discard(0), "12"),
                link(input(2), output(0), "1"),
            ],
        };

        assert!(matches!(
            validate_solution(&problem, &graph),
            Err(ValidationError::ResolvedFlowAboveCapacity { link: 2, .. })
        ));
    }

    #[test]
    fn rejects_missing_zero_surplus_and_wrong_discard_totals() {
        let missing_problem = problem(&["2"], &["1"], "2");
        assert_eq!(
            solve_topology(&missing_problem, &direct_graph("2")),
            Err(ValidationError::MissingDiscardLink {
                surplus: rational("1")
            })
        );

        let zero_surplus_problem = problem(&["1", "1"], &["1", "1"], "2");
        let zero_surplus_graph = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(0), output(0), "1"),
                link(input(1), discard(0), "1"),
            ],
        };
        assert_eq!(
            solve_topology(&zero_surplus_problem, &zero_surplus_graph),
            Err(ValidationError::DiscardWithoutSurplus)
        );

        let wrong_topology_problem = problem(&["120"], &["50"], "120");
        assert_eq!(
            solve_topology(&wrong_topology_problem, &discard_split_graph()),
            Err(ValidationError::InconsistentSteadyState)
        );

        let correct_problem = problem(&["120"], &["40"], "120");
        let mut wrong_claim = discard_split_graph();
        wrong_claim.links[2].flow = rational("30");
        wrong_claim.links[3].flow = rational("30");
        assert_eq!(
            validate_solution(&correct_problem, &wrong_claim),
            Err(ValidationError::IncorrectSuppliedDiscardTotal {
                expected: rational("80"),
                actual: Box::new(rational("60")),
            })
        );
    }

    #[test]
    fn rejects_output_demand_above_total_input() {
        let problem = problem(&["1"], &["2"], "2");
        assert_eq!(
            solve_topology(&problem, &direct_graph("1")),
            Err(ValidationError::InsufficientInput {
                total_input: rational("1"),
                total_output: Box::new(rational("2")),
            })
        );
    }

    #[test]
    fn requires_unique_contiguous_discard_indexes() {
        let problem = problem(&["7", "3", "2"], &["2"], "10");
        let noncontiguous = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(2), output(0), "2"),
                link(input(0), discard(0), "7"),
                link(input(1), discard(2), "3"),
            ],
        };
        assert_eq!(
            validate_solution(&problem, &noncontiguous),
            Err(ValidationError::NonContiguousDiscardTerminal {
                expected: DiscardTerminalIndex(1),
                found: DiscardTerminalIndex(2),
            })
        );

        let reused = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(2), output(0), "2"),
                link(input(0), discard(0), "7"),
                link(input(1), discard(0), "3"),
            ],
        };
        assert!(matches!(
            validate_solution(&problem, &reused),
            Err(ValidationError::DuplicateConsumerLink {
                consumer: ConsumerPortRef::Discard(DiscardTerminalIndex(0)),
                ..
            })
        ));
    }

    #[test]
    fn anonymous_discard_relabeling_preserves_validation() {
        let problem = problem(&["7", "3", "2"], &["2"], "10");
        let first = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(2), output(0), "2"),
                link(input(0), discard(0), "7"),
                link(input(1), discard(1), "3"),
            ],
        };
        let mut relabeled = first.clone();
        relabeled.links.swap(1, 2);
        relabeled.links[1].consumer = discard(0);
        relabeled.links[2].consumer = discard(1);

        assert_eq!(
            validate_solution(&problem, &first).unwrap(),
            validate_solution(&problem, &relabeled).unwrap()
        );
    }

    #[test]
    fn solves_and_validates_a_unique_feedback_scc() {
        let problem = problem(&["5"], &["10/3", "5/3"], "20");
        let summary = validate_solution(&problem, &feedback_graph()).unwrap();
        assert_eq!(summary.node_count, 3);
        assert_eq!(summary.link_count, 3);
        assert_eq!(summary.cyclic_scc_count, 1);
    }

    #[test]
    fn discard_sum_participates_in_cyclic_uniqueness_validation() {
        let problem = problem(&["5"], &["10/3"], "20");
        let summary = validate_solution(&problem, &feedback_discard_graph()).unwrap();

        assert_eq!(summary.node_count, 3);
        assert_eq!(summary.link_count, 3);
        assert_eq!(summary.physical_link_count, 6);
        assert_eq!(summary.discard_link_count, 1);
        assert_eq!(summary.cyclic_scc_count, 1);
    }

    #[test]
    fn rejects_internal_flow_above_the_inclusive_capacity() {
        let problem = problem(&["6", "6"], &["6", "6"], "10");
        let graph = PhysicalGraph {
            nodes: vec![node(0, NodeType::Merger2), node(1, NodeType::Splitter2)],
            links: vec![
                link(input(0), node_input(0, 0), "6"),
                link(input(1), node_input(0, 1), "6"),
                link(node_output(0, 0), node_input(1, 0), "12"),
                link(node_output(1, 0), output(0), "6"),
                link(node_output(1, 1), output(1), "6"),
            ],
        };
        assert!(matches!(
            solve_topology(&problem, &graph),
            Err(ValidationError::ResolvedFlowAboveCapacity { link: 2, .. })
        ));
    }

    #[test]
    fn rejects_zero_and_wrong_supplied_flows() {
        let direct_problem = problem(&["1"], &["1"], "10");
        assert!(matches!(
            validate_solution(&direct_problem, &direct_graph("0")),
            Err(ValidationError::NonPositiveSuppliedFlow { link: 0, .. })
        ));
        assert!(matches!(
            validate_solution(&direct_problem, &direct_graph("2")),
            Err(ValidationError::IncorrectSuppliedInputRate { link: 0, .. })
        ));

        let feedback_problem = problem(&["5"], &["10/3", "5/3"], "20");
        let mut graph = feedback_graph();
        graph.links[2].flow = rational("7");
        assert!(matches!(
            validate_solution(&feedback_problem, &graph),
            Err(ValidationError::IncorrectSuppliedFlow { link: 2, .. })
        ));
    }

    #[test]
    fn rejects_requested_output_rates_that_node_equations_do_not_produce() {
        let problem = problem(&["120"], &["50", "70"], "120");
        assert!(matches!(
            solve_topology(&problem, &split_graph()),
            Err(ValidationError::IncorrectOutputRate { index: 0, .. })
        ));
    }

    #[test]
    fn rejects_duplicate_missing_and_malformed_ports() {
        let duplicate_problem = problem(&["1"], &["1/2", "1/2"], "10");
        let duplicate = PhysicalGraph {
            nodes: Vec::new(),
            links: vec![
                link(input(0), output(0), "1/2"),
                link(input(0), output(1), "1/2"),
            ],
        };
        assert!(matches!(
            validate_solution(&duplicate_problem, &duplicate),
            Err(ValidationError::DuplicateProducerLink { .. })
        ));

        let split_problem = problem(&["120"], &["60", "60"], "120");
        let mut missing = split_graph();
        missing.links.pop();
        assert!(matches!(
            validate_solution(&split_problem, &missing),
            Err(ValidationError::MissingProducerLink {
                producer: ProducerPortRef::Node {
                    node: NodeId(0),
                    port: 1
                }
            })
        ));

        let mut malformed = split_graph();
        malformed.links[1].producer = node_output(0, 2);
        assert!(matches!(
            validate_solution(&split_problem, &malformed),
            Err(ValidationError::InvalidProducerPort { link: 1, .. })
        ));
    }

    #[test]
    fn rejects_direct_node_self_links() {
        let problem = problem(&["1"], &["1"], "10");
        let graph = PhysicalGraph {
            nodes: vec![node(0, NodeType::Splitter2)],
            links: vec![
                link(input(0), output(0), "1"),
                link(node_output(0, 0), node_input(0, 0), "1"),
            ],
        };
        assert!(matches!(
            validate_solution(&problem, &graph),
            Err(ValidationError::DirectNodeSelfLink {
                link: 1,
                node: NodeId(0)
            })
        ));
    }

    fn disconnected_circulation() -> (Problem, PhysicalGraph) {
        (
            problem(&["1"], &["1"], "10"),
            PhysicalGraph {
                nodes: vec![node(0, NodeType::Splitter2), node(1, NodeType::Merger2)],
                links: vec![
                    link(input(0), output(0), "1"),
                    link(node_output(0, 0), node_input(1, 0), "1"),
                    link(node_output(0, 1), node_input(1, 1), "1"),
                    link(node_output(1, 0), node_input(0, 0), "2"),
                ],
            },
        )
    }

    #[test]
    fn rejects_a_singular_complete_feedback_system() {
        let (problem, graph) = disconnected_circulation();
        assert!(matches!(
            solve_topology(&problem, &graph),
            Err(ValidationError::NonUniqueSteadyState {
                rank: 3,
                variables: 4
            })
        ));
    }

    #[test]
    fn cyclic_scc_rank_is_checked_without_downstream_equations() {
        let (problem, graph) = disconnected_circulation();
        let topology = Topology::new(&problem, &graph).unwrap();
        let claimed = graph
            .links
            .iter()
            .map(|link| link.flow.as_big_rational().clone())
            .collect::<Vec<_>>();
        assert_eq!(
            topology.check_cyclic_scc_uniqueness(&problem, &graph, &claimed),
            Err(ValidationError::NonUniqueCyclicScc {
                nodes: vec![NodeId(0), NodeId(1)],
                rank: 2,
                variables: 3,
            })
        );
    }

    #[test]
    fn independently_detects_dead_unreachable_structure() {
        let (problem, graph) = disconnected_circulation();
        let topology = Topology::new(&problem, &graph).unwrap();
        assert_eq!(
            topology.check_reachability(),
            Err(ValidationError::NodeUnreachableFromInput { node: NodeId(0) })
        );
    }
}
