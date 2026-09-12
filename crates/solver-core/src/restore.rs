use crate::{Failure, NormalizedProblem};
use solver_api::{
    BestKnownSolution, CanonicalGraphKey, ConsumerPortRef, PhysicalGraph, Problem, ProducerPortRef,
};
use solver_validation::validate_solution;

pub(crate) fn restore(
    original: &Problem,
    normalized: &NormalizedProblem,
    mut graph: PhysicalGraph,
    key: CanonicalGraphKey,
) -> Result<BestKnownSolution, Failure> {
    for link in &mut graph.links {
        if let ProducerPortRef::Input(ref mut id) = link.producer {
            id.0 = u32::try_from(
                normalized
                    .terminal_mapping
                    .original_input(id.0 as usize)
                    .unwrap(),
            )
            .unwrap();
        }
        if let ConsumerPortRef::Output(ref mut id) = link.consumer {
            id.0 = u32::try_from(
                normalized
                    .terminal_mapping
                    .original_output(id.0 as usize)
                    .unwrap(),
            )
            .unwrap();
        }
        link.flow = &link.flow * &normalized.original_scale;
    }
    let validation = validate_solution(original, &graph)
        .map_err(|e| Failure::Worker(format!("Solver witness restoration failed: {e}")))?;
    Ok(BestKnownSolution {
        node_count: validation.node_count,
        link_count: validation.link_count,
        physical_link_count: validation.physical_link_count,
        discard_link_count: validation.discard_link_count,
        canonical_graph_key: key,
        graph,
        validation,
    })
}
