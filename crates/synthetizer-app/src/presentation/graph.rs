use super::{GraphEdge, GraphNode, GraphNodeKind, PresentationError, rates::format_rate};
use solver_api::{
    ConsumerPortRef, NodeId, NodeType, PhysicalGraph, PhysicalLink, PreparedProblem,
    ProducerPortRef, TerminalMetadata,
};
use std::collections::BTreeMap;

pub(super) fn operator_map(
    graph: &PhysicalGraph,
) -> Result<BTreeMap<NodeId, NodeType>, PresentationError> {
    let mut operators = BTreeMap::new();
    for node in &graph.nodes {
        if operators.insert(node.id, node.node_type).is_some() {
            return Err(PresentationError::DuplicateNode(node.id.0));
        }
    }
    Ok(operators)
}

pub(super) fn terminal_nodes(
    terminals: &[TerminalMetadata],
    kind: GraphNodeKind,
    prefix: &str,
) -> Vec<GraphNode> {
    terminals
        .iter()
        .enumerate()
        .map(|(index, terminal)| GraphNode {
            id: format!("{prefix}-{index}"),
            kind,
            label: format!("{} · {} /min", terminal.name, terminal.rate),
        })
        .collect()
}

pub(super) fn operator_nodes(operators: &BTreeMap<NodeId, NodeType>) -> Vec<GraphNode> {
    operators
        .iter()
        .map(|(&id, &node_type)| GraphNode {
            id: operator_id(id),
            kind: node_kind(node_type),
            label: operator_label(id, node_type),
        })
        .collect()
}

pub(super) fn present_link(
    index: usize,
    link: &PhysicalLink,
    request: &PreparedProblem,
    operators: &BTreeMap<NodeId, NodeType>,
    feedback: bool,
) -> Result<GraphEdge, PresentationError> {
    let (source, source_port) = match link.producer {
        ProducerPortRef::Input(input) => {
            terminal(request.inputs.len(), "input", input.0)?;
            (format!("input-{}", input.0), 0)
        }
        ProducerPortRef::Node { node, port } => {
            require_node(operators, node)?;
            (operator_id(node), usize::from(port))
        }
    };
    let (target, target_port, discarded) = match link.consumer {
        ConsumerPortRef::Output(output) => {
            terminal(request.outputs.len(), "output", output.0)?;
            (format!("output-{}", output.0), 0, false)
        }
        ConsumerPortRef::Discard(discard) => (format!("sink-{}", discard.0), 0, true),
        ConsumerPortRef::Node { node, port } => {
            require_node(operators, node)?;
            (operator_id(node), usize::from(port), false)
        }
    };
    Ok(GraphEdge {
        id: format!("edge-{index}"),
        source,
        target,
        source_port,
        target_port,
        rate: format_rate(&link.flow),
        feedback,
        discarded,
    })
}

fn terminal(count: usize, side: &'static str, index: u32) -> Result<(), PresentationError> {
    if usize::try_from(index).is_ok_and(|index| index < count) {
        Ok(())
    } else {
        Err(PresentationError::TerminalOutOfRange { side, index, count })
    }
}

fn require_node(
    operators: &BTreeMap<NodeId, NodeType>,
    node: NodeId,
) -> Result<(), PresentationError> {
    if operators.contains_key(&node) {
        Ok(())
    } else {
        Err(PresentationError::UnknownNode(node.0))
    }
}

fn node_kind(node_type: NodeType) -> GraphNodeKind {
    match node_type {
        NodeType::Splitter2 => GraphNodeKind::Splitter2,
        NodeType::Splitter3 => GraphNodeKind::Splitter3,
        NodeType::Merger2 => GraphNodeKind::Merger2,
        NodeType::Merger3 => GraphNodeKind::Merger3,
    }
}

fn operator_id(id: NodeId) -> String {
    format!("operator-{}", id.0)
}

fn operator_label(id: NodeId, node_type: NodeType) -> String {
    let number = id.0 + 1;
    match node_type {
        NodeType::Splitter2 => format!("Splitter {number} · 2-way"),
        NodeType::Splitter3 => format!("Splitter {number} · 3-way"),
        NodeType::Merger2 => format!("Merger {number} · 2-way"),
        NodeType::Merger3 => format!("Merger {number} · 3-way"),
    }
}

pub(super) fn build_steps(nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<String> {
    let labels = nodes
        .iter()
        .map(|node| (node.id.as_str(), node.label.as_str()))
        .collect::<BTreeMap<_, _>>();
    edges
        .iter()
        .map(|edge| {
            format!(
                "Connect {} port {} to {} port {} at {} /min{}",
                labels
                    .get(edge.source.as_str())
                    .copied()
                    .unwrap_or(&edge.source),
                edge.source_port + 1,
                labels
                    .get(edge.target.as_str())
                    .copied()
                    .unwrap_or(&edge.target),
                edge.target_port + 1,
                edge.rate.exact,
                if edge.discarded { " (discard)" } else { "" },
            )
        })
        .collect()
}
