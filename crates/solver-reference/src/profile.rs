use std::ops::ControlFlow;

use solver_api::{
    ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId, NodeProfile, NodeType,
    OutputTerminalIndex, PhysicalNode, ProducerPortRef, Rational,
};

/// One exhaustive fixed-profile obligation with exact discard accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProfilePlan {
    pub profile: NodeProfile,
    /// Belts whose producer and consumer are both physical operators.
    pub link_count: u32,
    pub discard_link_count: u32,
    pub physical_link_count: u32,
}

/// One complete labeled physical topology for a fixed node profile.
///
/// Flows are intentionally absent. The reference solver assigns and checks them only after this
/// structural enumerator has produced a complete port bijection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileTopology {
    /// Profile whose eager labeled nodes appear in this topology.
    pub profile: NodeProfile,
    /// Nodes ordered by type and then identifier.
    pub nodes: Vec<PhysicalNode>,
    /// Complete producer-to-consumer port bijection in producer order.
    pub links: Vec<(ProducerPortRef, ConsumerPortRef)>,
    /// Anonymous discard consumers materialized in this topology.
    pub discard_link_count: u32,
}

impl ProfileTopology {
    /// Returns the operator-to-operator belt count.
    #[must_use]
    pub fn link_count(&self) -> usize {
        self.links
            .iter()
            .filter(|(producer, consumer)| {
                matches!(producer, ProducerPortRef::Node { .. })
                    && matches!(consumer, ConsumerPortRef::Node { .. })
            })
            .count()
    }

    /// Returns the total physical link count, including discard lines.
    #[must_use]
    pub fn physical_link_count(&self) -> usize {
        self.links.len()
    }
}

/// Enumerates every fixed profile admitted by exact surplus and discard capacity.
#[must_use]
pub fn enumerate_profile_plans(
    node_count: u32,
    input_count: u32,
    output_count: u32,
    surplus: &Rational,
    max_link_rate: &Rational,
) -> Vec<ProfilePlan> {
    if surplus.is_negative() || !max_link_rate.is_positive() {
        return Vec::new();
    }
    let mut plans = Vec::new();
    for splitter2 in 0..=node_count {
        let non_splitter2 = node_count - splitter2;
        for splitter3 in 0..=non_splitter2 {
            let merger_total = non_splitter2 - splitter3;
            for merger2 in 0..=merger_total {
                let profile = NodeProfile {
                    splitter2,
                    splitter3,
                    merger2,
                    merger3: merger_total - merger2,
                };
                plans.extend(profile_plans(
                    profile,
                    input_count,
                    output_count,
                    surplus,
                    max_link_rate,
                ));
            }
        }
    }
    plans.sort_by_key(|plan| (plan.link_count, plan.profile, plan.discard_link_count));
    plans
}

/// Enumerates every balanced node profile at one fixed node count.
///
/// Results are ordered first by their exact link count `I + P`, then by [`NodeProfile`]. This
/// makes equal-link groups adjacent without removing any profile.
#[must_use]
pub fn enumerate_profiles(
    node_count: u32,
    input_count: u32,
    output_count: u32,
) -> Vec<NodeProfile> {
    let mut profiles = Vec::new();
    for plan in enumerate_profile_plans(
        node_count,
        input_count,
        output_count,
        &Rational::zero(),
        &Rational::one(),
    )
    {
        if !profiles.contains(&plan.profile) {
            profiles.push(plan.profile);
        }
    }
    profiles
}

fn profile_plans(
    profile: NodeProfile,
    input_count: u32,
    output_count: u32,
    surplus: &Rational,
    max_link_rate: &Rational,
) -> Vec<ProfilePlan> {
    let Some(physical) = u64::from(input_count).checked_add(producer_port_count(profile)) else {
        return Vec::new();
    };
    let Some(modeled) = u64::from(output_count).checked_add(consumer_port_count(profile)) else {
        return Vec::new();
    };
    let discard = if surplus.is_zero() {
        if physical != modeled {
            return Vec::new();
        }
        0
    } else {
        let Some(discard) = physical.checked_sub(modeled) else {
            return Vec::new();
        };
        if discard == 0 || surplus > &(max_link_rate * &Rational::from(discard)) {
            return Vec::new();
        }
        discard
    };
    let node_producers = producer_port_count(profile);
    let node_consumers = consumer_port_count(profile);
    let external_consumers = u64::from(output_count) + discard;
    let minimum = node_consumers
        .saturating_sub(u64::from(input_count))
        .max(node_producers.saturating_sub(external_consumers));
    let maximum = node_producers.min(node_consumers);
    let (Ok(discard_link_count), Ok(physical_link_count)) =
        (u32::try_from(discard), u32::try_from(physical))
    else {
        return Vec::new();
    };
    (minimum..=maximum)
        .filter_map(|link_count| {
            Some(ProfilePlan {
                profile,
                link_count: u32::try_from(link_count).ok()?,
                discard_link_count,
                physical_link_count,
            })
        })
        .collect()
}

/// Enumerates every legal explicit port bijection for one labeled profile.
///
/// Producers and their candidate consumers use fixed physical order. The only rejected pair is a
/// node output connected to an input of that same node. Different ports may form parallel links
/// between the same two distinct nodes.
///
/// The visitor may return [`ControlFlow::Break`] to stop enumeration. A returned
/// [`ControlFlow::Continue`] proves that every legal bijection was visited.
#[must_use = "Continue means exhaustive enumeration; Break means the visitor stopped it"]
pub fn enumerate_topologies<F>(
    profile: NodeProfile,
    input_count: u32,
    output_count: u32,
    visitor: F,
) -> ControlFlow<()>
where
    F: FnMut(ProfileTopology) -> ControlFlow<()>,
{
    enumerate_topologies_with_discard(profile, input_count, output_count, 0, visitor)
}

/// Enumerates every legal labeled topology for an exact discard count.
#[must_use = "Continue means exhaustive enumeration; Break means the visitor stopped it"]
pub fn enumerate_topologies_with_discard<F>(
    profile: NodeProfile,
    input_count: u32,
    output_count: u32,
    discard_link_count: u32,
    mut visitor: F,
) -> ControlFlow<()>
where
    F: FnMut(ProfileTopology) -> ControlFlow<()>,
{
    if !profile_has_port_balance(profile, input_count, output_count, discard_link_count) {
        return ControlFlow::Continue(());
    }
    let Some(nodes) = materialize_nodes(profile) else {
        return ControlFlow::Break(());
    };
    let producers = producer_ports(input_count, &nodes);
    let consumers = consumer_ports(output_count, discard_link_count, &nodes);
    if producers.len() != consumers.len() {
        return ControlFlow::Continue(());
    }

    let mut used_consumers = vec![false; consumers.len()];
    let mut links = Vec::with_capacity(producers.len());
    enumerate_bijections(
        profile,
        discard_link_count,
        &nodes,
        &producers,
        &consumers,
        0,
        &mut used_consumers,
        &mut links,
        &mut visitor,
    )
}

#[allow(clippy::too_many_arguments)]
fn enumerate_bijections<F>(
    profile: NodeProfile,
    discard_link_count: u32,
    nodes: &[PhysicalNode],
    producers: &[ProducerPortRef],
    consumers: &[ConsumerPortRef],
    producer_index: usize,
    used_consumers: &mut [bool],
    links: &mut Vec<(ProducerPortRef, ConsumerPortRef)>,
    visitor: &mut F,
) -> ControlFlow<()>
where
    F: FnMut(ProfileTopology) -> ControlFlow<()>,
{
    if producer_index == producers.len() {
        return visitor(ProfileTopology {
            profile,
            nodes: nodes.to_vec(),
            links: links.clone(),
            discard_link_count,
        });
    }

    let producer = producers[producer_index];
    for (consumer_index, &consumer) in consumers.iter().enumerate() {
        if used_consumers[consumer_index] || is_direct_node_self_link(producer, consumer) {
            continue;
        }
        used_consumers[consumer_index] = true;
        links.push((producer, consumer));
        if enumerate_bijections(
            profile,
            discard_link_count,
            nodes,
            producers,
            consumers,
            producer_index + 1,
            used_consumers,
            links,
            visitor,
        )
        .is_break()
        {
            links.pop();
            used_consumers[consumer_index] = false;
            return ControlFlow::Break(());
        }
        links.pop();
        used_consumers[consumer_index] = false;
    }
    ControlFlow::Continue(())
}

fn profile_has_port_balance(
    profile: NodeProfile,
    input_count: u32,
    output_count: u32,
    discard_link_count: u32,
) -> bool {
    u64::from(input_count) + producer_port_count(profile)
        == u64::from(output_count) + consumer_port_count(profile) + u64::from(discard_link_count)
}

fn producer_port_count(profile: NodeProfile) -> u64 {
    2 * u64::from(profile.splitter2)
        + 3 * u64::from(profile.splitter3)
        + u64::from(profile.merger2)
        + u64::from(profile.merger3)
}

fn consumer_port_count(profile: NodeProfile) -> u64 {
    u64::from(profile.splitter2)
        + u64::from(profile.splitter3)
        + 2 * u64::from(profile.merger2)
        + 3 * u64::from(profile.merger3)
}

fn materialize_nodes(profile: NodeProfile) -> Option<Vec<PhysicalNode>> {
    let total = u64::from(profile.splitter2)
        + u64::from(profile.splitter3)
        + u64::from(profile.merger2)
        + u64::from(profile.merger3);
    usize::try_from(total).ok()?;

    let mut nodes = Vec::new();
    append_nodes(&mut nodes, profile.splitter2, NodeType::Splitter2)?;
    append_nodes(&mut nodes, profile.splitter3, NodeType::Splitter3)?;
    append_nodes(&mut nodes, profile.merger2, NodeType::Merger2)?;
    append_nodes(&mut nodes, profile.merger3, NodeType::Merger3)?;
    Some(nodes)
}

fn append_nodes(nodes: &mut Vec<PhysicalNode>, count: u32, node_type: NodeType) -> Option<()> {
    for _ in 0..count {
        let id = NodeId(u32::try_from(nodes.len()).ok()?);
        nodes.push(PhysicalNode { id, node_type });
    }
    Some(())
}

fn producer_ports(input_count: u32, nodes: &[PhysicalNode]) -> Vec<ProducerPortRef> {
    let mut producers = (0..input_count)
        .map(|index| ProducerPortRef::Input(InputTerminalIndex(index)))
        .collect::<Vec<_>>();
    for node in nodes {
        for port in 0..node.node_type.output_port_count() {
            producers.push(ProducerPortRef::Node {
                node: node.id,
                port,
            });
        }
    }
    producers
}

fn consumer_ports(
    output_count: u32,
    discard_link_count: u32,
    nodes: &[PhysicalNode],
) -> Vec<ConsumerPortRef> {
    let mut consumers = Vec::new();
    for node in nodes {
        for port in 0..node.node_type.input_port_count() {
            consumers.push(ConsumerPortRef::Node {
                node: node.id,
                port,
            });
        }
    }
    consumers
        .extend((0..output_count).map(|index| ConsumerPortRef::Output(OutputTerminalIndex(index))));
    consumers.extend(
        (0..discard_link_count).map(|index| ConsumerPortRef::Discard(DiscardTerminalIndex(index))),
    );
    consumers
}

fn is_direct_node_self_link(producer: ProducerPortRef, consumer: ConsumerPortRef) -> bool {
    matches!(
        (producer, consumer),
        (
            ProducerPortRef::Node { node: producer, .. },
            ConsumerPortRef::Node { node: consumer, .. }
        ) if producer == consumer
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(splitter2: u32, splitter3: u32, merger2: u32, merger3: u32) -> NodeProfile {
        NodeProfile {
            splitter2,
            splitter3,
            merger2,
            merger3,
        }
    }

    fn collect_topologies(
        profile: NodeProfile,
        input_count: u32,
        output_count: u32,
    ) -> Vec<ProfileTopology> {
        let mut topologies = Vec::new();
        let result = enumerate_topologies(profile, input_count, output_count, |topology| {
            topologies.push(topology);
            ControlFlow::Continue(())
        });
        assert_eq!(result, ControlFlow::Continue(()));
        topologies
    }

    fn collect_discard_topologies(
        profile: NodeProfile,
        input_count: u32,
        output_count: u32,
        discard_link_count: u32,
    ) -> Vec<ProfileTopology> {
        let mut topologies = Vec::new();
        let result = enumerate_topologies_with_discard(
            profile,
            input_count,
            output_count,
            discard_link_count,
            |topology| {
                topologies.push(topology);
                ControlFlow::Continue(())
            },
        );
        assert_eq!(result, ControlFlow::Continue(()));
        topologies
    }

    #[test]
    fn profiles_are_balanced_and_grouped_by_exact_link_count() {
        let profiles = enumerate_profiles(3, 1, 2);
        assert_eq!(profiles, vec![profile(2, 0, 1, 0), profile(1, 1, 0, 1)]);

        let plans = enumerate_profile_plans(3, 1, 2, &Rational::zero(), &Rational::one());
        assert_eq!(
            plans.iter().map(|plan| plan.link_count).collect::<Vec<_>>(),
            vec![3, 4, 4, 5]
        );
        for candidate in profiles {
            assert_eq!(
                1 + producer_port_count(candidate),
                2 + consumer_port_count(candidate)
            );
        }
        assert!(enumerate_profiles(0, 1, 2).is_empty());
    }

    #[test]
    fn zero_node_profiles_enumerate_direct_link_bijections() {
        let empty = profile(0, 0, 0, 0);
        assert_eq!(enumerate_profiles(0, 1, 1), vec![empty]);

        let direct = collect_topologies(empty, 1, 1);
        assert_eq!(direct.len(), 1);
        assert_eq!(direct[0].nodes, Vec::new());
        assert_eq!(
            direct[0].links,
            vec![(
                ProducerPortRef::Input(InputTerminalIndex(0)),
                ConsumerPortRef::Output(OutputTerminalIndex(0)),
            )]
        );
        let two_by_two = collect_topologies(empty, 2, 2);
        assert_eq!(two_by_two.len(), 2);
        assert_eq!(
            two_by_two[0].links,
            vec![
                (
                    ProducerPortRef::Input(InputTerminalIndex(0)),
                    ConsumerPortRef::Output(OutputTerminalIndex(0)),
                ),
                (
                    ProducerPortRef::Input(InputTerminalIndex(1)),
                    ConsumerPortRef::Output(OutputTerminalIndex(1)),
                ),
            ]
        );
        assert_eq!(
            two_by_two[1].links,
            vec![
                (
                    ProducerPortRef::Input(InputTerminalIndex(0)),
                    ConsumerPortRef::Output(OutputTerminalIndex(1)),
                ),
                (
                    ProducerPortRef::Input(InputTerminalIndex(1)),
                    ConsumerPortRef::Output(OutputTerminalIndex(0)),
                ),
            ]
        );
    }

    #[test]
    fn one_node_splitter_and_merger_counts_are_exhaustive() {
        assert_eq!(collect_topologies(profile(1, 0, 0, 0), 1, 2).len(), 2);
        assert_eq!(collect_topologies(profile(0, 1, 0, 0), 1, 3).len(), 6);
        assert_eq!(collect_topologies(profile(0, 0, 1, 0), 2, 1).len(), 2);
        assert_eq!(collect_topologies(profile(0, 0, 0, 1), 3, 1).len(), 6);
    }

    #[test]
    fn direct_node_self_links_are_forbidden() {
        // This profile is port-balanced, but both producers belong to the splitter. Only one
        // external consumer exists, so the remaining producer would have to self-link.
        assert!(collect_topologies(profile(1, 0, 0, 0), 0, 1).is_empty());

        for topology in collect_topologies(profile(1, 0, 1, 0), 1, 1) {
            assert!(
                topology
                    .links
                    .iter()
                    .all(|(producer, consumer)| !is_direct_node_self_link(*producer, *consumer))
            );
        }
    }

    #[test]
    fn different_ports_may_form_parallel_links_between_two_nodes() {
        let topologies = collect_topologies(profile(1, 0, 1, 0), 1, 1);
        assert_eq!(topologies.len(), 8);
        assert!(topologies.iter().any(|topology| {
            topology
                .links
                .iter()
                .filter(|(producer, consumer)| {
                    matches!(
                        (producer, consumer),
                        (
                            ProducerPortRef::Node {
                                node: NodeId(0),
                                ..
                            },
                            ConsumerPortRef::Node {
                                node: NodeId(1),
                                ..
                            }
                        )
                    )
                })
                .count()
                == 2
        }));
    }

    #[test]
    fn visitor_break_stops_before_exhaustion() {
        let mut visited = 0;
        let result = enumerate_topologies(profile(0, 0, 0, 0), 3, 3, |_| {
            visited += 1;
            ControlFlow::Break(())
        });
        assert_eq!(result, ControlFlow::Break(()));
        assert_eq!(visited, 1);
    }

    #[test]
    fn surplus_capacity_selects_the_minimum_exact_discard_count() {
        let plans = enumerate_profile_plans(1, 1, 1, &"3/2".parse().unwrap(), &Rational::one());
        assert_eq!(
            plans,
            vec![
                ProfilePlan {
                    profile: profile(0, 1, 0, 0),
                    link_count: 0,
                    discard_link_count: 2,
                    physical_link_count: 4,
                },
                ProfilePlan {
                    profile: profile(0, 1, 0, 0),
                    link_count: 1,
                    discard_link_count: 2,
                    physical_link_count: 4,
                },
            ]
        );

        let equal_link_plans =
            enumerate_profile_plans(1, 1, 1, &"1/2".parse().unwrap(), &Rational::one());
        assert_eq!(equal_link_plans.len(), 4);
        assert_eq!(
            equal_link_plans
                .iter()
                .map(|plan| plan.link_count)
                .collect::<Vec<_>>(),
            vec![0, 0, 1, 1]
        );
        let mut discard_counts = equal_link_plans
            .iter()
            .map(|plan| plan.discard_link_count)
            .collect::<Vec<_>>();
        discard_counts.sort_unstable();
        assert_eq!(discard_counts, vec![1, 1, 2, 2]);
    }

    #[test]
    fn discard_consumers_are_explicit_physical_links_but_not_modeled_links() {
        let topologies = collect_discard_topologies(NodeProfile::default(), 2, 1, 1);
        assert_eq!(topologies.len(), 2);
        for topology in topologies {
            assert_eq!(topology.link_count(), 0);
            assert_eq!(topology.physical_link_count(), 2);
            assert_eq!(topology.discard_link_count, 1);
            assert_eq!(
                topology
                    .links
                    .iter()
                    .filter(|(_, consumer)| matches!(consumer, ConsumerPortRef::Discard(_)))
                    .count(),
                1
            );
        }
    }
}
