use solver_api::{ConsumerPortRef, NodeId, NodeType, PhysicalLink, ProducerPortRef};
use std::collections::{BTreeMap, BTreeSet};

/// Marks feedback belts with the same natural-loop / irreducible-cycle rules as
/// the graph structure: dominator back-edges first, then a DFS for leftover cycles.
pub(super) fn feedback_annotation(
    operators: &BTreeMap<NodeId, NodeType>,
    links: &[PhysicalLink],
) -> (BTreeSet<(ProducerPortRef, ConsumerPortRef)>, u32) {
    let ids = operators.keys().copied().collect::<Vec<_>>();
    let indexes = ids
        .iter()
        .enumerate()
        .map(|(index, &id)| (id, index))
        .collect::<BTreeMap<_, _>>();
    let node_count = ids.len();

    let mut operator_links = Vec::new();
    let mut entries = Vec::new();
    for link in links {
        match (link.producer, link.consumer) {
            (
                ProducerPortRef::Node { node: source, .. },
                ConsumerPortRef::Node { node: target, .. },
            ) => {
                let (Some(&source), Some(&target)) = (indexes.get(&source), indexes.get(&target))
                else {
                    continue;
                };
                let edge = operator_links.len();
                operator_links.push((source, target, edge, link.producer, link.consumer));
            }
            (ProducerPortRef::Input(_), ConsumerPortRef::Node { node: target, .. }) => {
                if let Some(&target) = indexes.get(&target) {
                    entries.push(target);
                }
            }
            _ => {}
        }
    }
    entries.sort_unstable();
    entries.dedup();

    let mut successors = vec![Vec::new(); node_count];
    let algo_links = operator_links
        .iter()
        .map(|&(source, target, edge, _, _)| {
            successors[source].push(target);
            (source, target, edge)
        })
        .collect::<Vec<_>>();

    let idom = immediate_dominators(node_count, &successors, &entries);
    let virtual_source = node_count;
    let mut flags = vec![false; operator_links.len()];
    for &(node, target, edge) in &algo_links {
        if dominates(&idom, target, node, virtual_source) {
            flags[edge] = true;
        }
    }
    mark_irreducible_back_edges(node_count, &algo_links, &entries, &mut flags);

    let mut feedback = BTreeSet::new();
    for (edge, marked) in flags.into_iter().enumerate() {
        if marked {
            let (_, _, _, producer, consumer) = operator_links[edge];
            feedback.insert((producer, consumer));
        }
    }
    let feedback_count = u32::try_from(feedback.len()).unwrap_or(u32::MAX);
    (feedback, feedback_count)
}

fn reverse_postorder(graph: &[Vec<usize>], start: usize) -> Vec<usize> {
    let successors = graph
        .iter()
        .map(|edges| {
            let mut nodes = edges.clone();
            nodes.sort_unstable();
            nodes.dedup();
            nodes
        })
        .collect::<Vec<_>>();
    let mut seen = vec![false; graph.len()];
    let mut post = Vec::new();
    let mut stack = vec![(start, 0_usize)];
    seen[start] = true;
    while let Some((node, child)) = stack.pop() {
        if child < successors[node].len() {
            stack.push((node, child + 1));
            let next = successors[node][child];
            if !seen[next] {
                seen[next] = true;
                stack.push((next, 0));
            }
        } else {
            post.push(node);
        }
    }
    post.reverse();
    post
}

fn intersect_dominators(
    mut first: usize,
    mut second: usize,
    idom: &[Option<usize>],
    rpo_index: &[usize],
) -> usize {
    while first != second {
        while rpo_index[first] > rpo_index[second] {
            first = idom[first].expect("a processed predecessor has an immediate dominator");
        }
        while rpo_index[second] > rpo_index[first] {
            second = idom[second].expect("a processed predecessor has an immediate dominator");
        }
    }
    first
}

fn immediate_dominators(
    node_count: usize,
    successors: &[Vec<usize>],
    entries: &[usize],
) -> Vec<Option<usize>> {
    let source = node_count;
    let mut graph = successors.to_vec();
    graph.push(entries.to_vec());
    let mut predecessors = vec![Vec::new(); node_count + 1];
    for (node, targets) in graph.iter().enumerate() {
        for &target in targets {
            predecessors[target].push(node);
        }
    }
    let rpo = reverse_postorder(&graph, source);
    let mut rpo_index = vec![usize::MAX; node_count + 1];
    for (index, &node) in rpo.iter().enumerate() {
        rpo_index[node] = index;
    }
    let mut idom = vec![None; node_count + 1];
    idom[source] = Some(source);
    let mut changed = true;
    while changed {
        changed = false;
        for &node in &rpo {
            if node == source {
                continue;
            }
            let mut new_idom = None;
            for &predecessor in &predecessors[node] {
                if idom[predecessor].is_none() {
                    continue;
                }
                new_idom = Some(match new_idom {
                    None => predecessor,
                    Some(current) => intersect_dominators(predecessor, current, &idom, &rpo_index),
                });
            }
            if new_idom.is_some() && idom[node] != new_idom {
                idom[node] = new_idom;
                changed = true;
            }
        }
    }
    idom
}

fn dominates(idom: &[Option<usize>], header: usize, node: usize, source: usize) -> bool {
    if header == node {
        return true;
    }
    let mut cursor = node;
    while cursor != source {
        let Some(parent) = idom[cursor] else {
            return false;
        };
        if parent == cursor {
            break;
        }
        cursor = parent;
        if cursor == header {
            return true;
        }
    }
    header == source
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DfsColor {
    White,
    Grey,
    Black,
}

fn mark_irreducible_back_edges(
    node_count: usize,
    links: &[(usize, usize, usize)],
    entries: &[usize],
    feedback: &mut [bool],
) {
    let mut adjacency = vec![Vec::new(); node_count];
    for &(source, target, edge) in links {
        if !feedback[edge] {
            adjacency[source].push((target, edge));
        }
    }
    for edges in &mut adjacency {
        edges.sort_unstable();
    }

    let mut color = vec![DfsColor::White; node_count];
    let mut stack = Vec::new();
    for &entry in entries {
        if color[entry] == DfsColor::White {
            color[entry] = DfsColor::Grey;
            stack.push((entry, 0_usize));
        }
        while let Some((node, child)) = stack.pop() {
            if child < adjacency[node].len() {
                stack.push((node, child + 1));
                let (target, edge) = adjacency[node][child];
                match color[target] {
                    DfsColor::Grey => feedback[edge] = true,
                    DfsColor::White => {
                        color[target] = DfsColor::Grey;
                        stack.push((target, 0));
                    }
                    DfsColor::Black => {}
                }
            } else {
                color[node] = DfsColor::Black;
            }
        }
    }
}
