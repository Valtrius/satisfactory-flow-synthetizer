use serde::{Deserialize, Serialize};

use crate::Rational;

/// Zero-based position of an external input in [`crate::Problem::inputs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InputTerminalIndex(pub u32);

/// Zero-based position of an external output in [`crate::Problem::outputs`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OutputTerminalIndex(pub u32);

/// Zero-based anonymous discard-line index in one physical witness.
///
/// Discard terminals have no requested rate in [`crate::Problem`]. Their exact
/// positive link flows collectively consume the problem's input surplus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiscardTerminalIndex(pub u32);

/// Stable identifier of a physical splitter or merger inside one graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u32);

/// Supported physical splitter and merger types with effective arity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeType {
    /// One input divided equally between two outputs.
    Splitter2,
    /// One input divided equally between three outputs.
    Splitter3,
    /// Two inputs summed into one output.
    Merger2,
    /// Three inputs summed into one output.
    Merger3,
}

impl NodeType {
    /// Returns the number of active physical input ports.
    #[must_use]
    pub const fn input_port_count(self) -> u8 {
        match self {
            Self::Splitter2 | Self::Splitter3 => 1,
            Self::Merger2 => 2,
            Self::Merger3 => 3,
        }
    }

    /// Returns the number of active physical output ports.
    #[must_use]
    pub const fn output_port_count(self) -> u8 {
        match self {
            Self::Splitter2 => 2,
            Self::Splitter3 => 3,
            Self::Merger2 | Self::Merger3 => 1,
        }
    }
}

/// Exact counts of each physical node type in a fixed search profile.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct NodeProfile {
    /// Number of two-way splitters.
    pub splitter2: u32,
    /// Number of three-way splitters.
    pub splitter3: u32,
    /// Number of two-way mergers.
    pub merger2: u32,
    /// Number of three-way mergers.
    pub merger3: u32,
}

impl NodeProfile {
    /// Returns the total physical node count.
    #[must_use]
    pub const fn node_count(self) -> u32 {
        self.splitter2 + self.splitter3 + self.merger2 + self.merger3
    }

    /// Returns the number of producer ports owned by the profile's nodes.
    #[must_use]
    pub const fn producer_port_count(self) -> u32 {
        2 * self.splitter2 + 3 * self.splitter3 + self.merger2 + self.merger3
    }

    /// Returns the number of consumer ports owned by the profile's nodes.
    #[must_use]
    pub const fn consumer_port_count(self) -> u32 {
        self.splitter2 + self.splitter3 + 2 * self.merger2 + 3 * self.merger3
    }
}

/// A directed producer endpoint of one physical link.
///
/// External inputs have one implicit producer port. Node port numbers are zero-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(
    tag = "owner",
    content = "port",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ProducerPortRef {
    /// The sole producer port of an external input.
    Input(InputTerminalIndex),
    /// One output port of a physical node.
    Node {
        /// Owning physical node.
        node: NodeId,
        /// Zero-based output port number.
        port: u8,
    },
}

/// A directed consumer endpoint of one physical link.
///
/// External outputs and anonymous discard lines have one implicit consumer port.
/// Node port numbers are zero-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(
    tag = "owner",
    content = "port",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum ConsumerPortRef {
    /// The sole consumer port of an external output.
    Output(OutputTerminalIndex),
    /// One anonymous surplus-discard line.
    Discard(DiscardTerminalIndex),
    /// One input port of a physical node.
    Node {
        /// Owning physical node.
        node: NodeId,
        /// Zero-based input port number.
        port: u8,
    },
}

/// One physical splitter or merger in a concrete witness.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalNode {
    /// Graph-local identifier.
    pub id: NodeId,
    /// Physical operator and effective arity.
    pub node_type: NodeType,
}

/// One directed physical belt with its exact steady flow.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalLink {
    /// Producer port used by this link.
    pub producer: ProducerPortRef,
    /// Consumer port used by this link.
    pub consumer: ConsumerPortRef,
    /// Exact positive flow, which validation must prove is at most the problem capacity.
    pub flow: Rational,
}

/// A fully materialized physical witness.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PhysicalGraph {
    /// Physical splitters and mergers.
    pub nodes: Vec<PhysicalNode>,
    /// Directed one-to-one physical port connections.
    pub links: Vec<PhysicalLink>,
}

/// Opaque canonical encoding used to compare isomorphic physical witnesses.
///
/// Lexicographic byte order is the authoritative deterministic witness order.
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CanonicalGraphKey(Vec<u8>);

impl CanonicalGraphKey {
    /// Wraps an authoritative canonical byte encoding.
    #[must_use]
    pub fn from_bytes(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Borrows the canonical encoding.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the key and returns its canonical encoding.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_arities_and_profile_counts_match_physical_ports() {
        assert_eq!(NodeType::Splitter3.input_port_count(), 1);
        assert_eq!(NodeType::Splitter3.output_port_count(), 3);
        assert_eq!(NodeType::Merger3.input_port_count(), 3);
        assert_eq!(NodeType::Merger3.output_port_count(), 1);

        let profile = NodeProfile {
            splitter2: 1,
            splitter3: 2,
            merger2: 3,
            merger3: 4,
        };
        assert_eq!(profile.node_count(), 10);
        assert_eq!(profile.producer_port_count(), 15);
        assert_eq!(profile.consumer_port_count(), 21);
    }

    #[test]
    fn graph_serialization_keeps_flow_exact() {
        let graph = PhysicalGraph {
            nodes: vec![PhysicalNode {
                id: NodeId(0),
                node_type: NodeType::Splitter2,
            }],
            links: vec![PhysicalLink {
                producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                consumer: ConsumerPortRef::Node {
                    node: NodeId(0),
                    port: 0,
                },
                flow: "1/3".parse().unwrap(),
            }],
        };
        let json = serde_json::to_value(&graph).unwrap();
        assert_eq!(json["links"][0]["flow"], "1/3");
        assert_eq!(
            serde_json::from_value::<PhysicalGraph>(json).unwrap(),
            graph
        );
    }

    #[test]
    fn canonical_keys_use_lexicographic_byte_order() {
        assert!(
            CanonicalGraphKey::from_bytes(vec![1, 2]) < CanonicalGraphKey::from_bytes(vec![1, 3])
        );
    }

    #[test]
    fn anonymous_discard_endpoint_round_trips_with_an_exact_index() {
        let endpoint = ConsumerPortRef::Discard(DiscardTerminalIndex(7));
        let json = serde_json::to_value(endpoint).unwrap();
        assert_eq!(json["owner"], "discard");
        assert_eq!(json["port"], 7);
        assert_eq!(
            serde_json::from_value::<ConsumerPortRef>(json).unwrap(),
            endpoint
        );
    }
}
