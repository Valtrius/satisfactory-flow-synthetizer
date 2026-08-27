//! Unified IPC contract shared by Custom and Z3 engines.

use serde::{Deserialize, Serialize};
use solver_api::{
    GlobalUnsatProof, ProofObligation, ProofSummary, SearchInstrumentation, SolvePhase,
    ValidationSummary,
};
use solver_z3::{
    DisplayRate as Z3DisplayRate, GraphEdge as Z3GraphEdge, GraphNode as Z3GraphNode,
    Solution as Z3Solution, SolverProgress as Z3Progress,
};

use custom_solver_adapter::presentation::{
    DisplayRate as CustomDisplayRate, GraphEdge as CustomGraphEdge, GraphNode as CustomGraphNode,
    PresentationSolution,
};

/// Which exact engine should execute a job.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolverEngine {
    #[default]
    Custom,
    Z3,
}

/// One caller-named external terminal.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointInput {
    pub id: String,
    pub name: String,
    pub rate: String,
}

/// Dual-engine solve request. Defaults to Custom when `engine` is omitted.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SolveRequest {
    pub inputs: Vec<EndpointInput>,
    pub outputs: Vec<EndpointInput>,
    pub belt_rate: String,
    #[serde(default)]
    pub enumerate_all_at_n: bool,
    #[serde(default)]
    pub engine: SolverEngine,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayRate {
    pub exact: String,
    pub decimal: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GraphNodeKind {
    Input,
    Splitter2,
    Splitter3,
    Merger2,
    Merger3,
    Output,
    Discard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: String,
    pub kind: GraphNodeKind,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub source_port: usize,
    pub target_port: usize,
    pub rate: DisplayRate,
    pub feedback: bool,
    pub discarded: bool,
}

/// Shared solution metrics. Engine-only fields are omitted when dishonest or unavailable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolutionStats {
    pub node_count: usize,
    pub splitters: usize,
    pub mergers: usize,
    pub feedback_loops: usize,
    /// Shared link/belt count: Custom optimized `L`, or Z3 operator↔operator belts.
    pub link_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checked_through: Option<usize>,
    /// Z3-only alias of operator↔operator belts (same value as `link_count` for Z3).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub belt_count: Option<usize>,
    /// Z3-only peak internal belt throughput.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub internal_max_throughput: Option<DisplayRate>,
    /// Custom-only physical belt count including discard.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub physical_link_count: Option<usize>,
    /// Custom-only anonymous discard belt count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discard_link_count: Option<usize>,
}

/// Unified frontend solution. Never labels a Custom incumbent as proven optimal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Solution {
    pub engine: SolverEngine,
    pub status: String,
    pub model_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<ProofSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<ValidationSummary>,
    pub stats: SolutionStats,
    pub total_input: DisplayRate,
    pub total_output: DisplayRate,
    pub discard_rate: DisplayRate,
    pub belt_rate: DisplayRate,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub build_steps: Vec<String>,
}

/// Live progress. Discriminated by `engine`; Z3 keeps its portfolio `kind` fields.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SolverProgress {
    Z3(Z3ProgressWire),
    Custom(CustomProgressWire),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Z3ProgressWire {
    pub engine: SolverEngine,
    #[serde(flatten)]
    pub detail: Z3Progress,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomProgressWire {
    pub engine: SolverEngine,
    pub phase: SolvePhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub obligation: Option<ProofObligation>,
    pub completed_profiles: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_profiles: Option<u32>,
    pub instrumentation: SearchInstrumentation,
}

impl SolverProgress {
    #[must_use]
    pub fn from_z3(detail: Z3Progress) -> Self {
        Self::Z3(Z3ProgressWire {
            engine: SolverEngine::Z3,
            detail,
        })
    }

    #[must_use]
    pub fn from_custom(progress: solver_api::SolverProgress) -> Self {
        Self::Custom(CustomProgressWire {
            engine: SolverEngine::Custom,
            phase: progress.phase,
            obligation: progress.obligation,
            completed_profiles: progress.completed_profiles,
            total_profiles: progress.total_profiles,
            instrumentation: progress.instrumentation,
        })
    }
}

impl Solution {
    #[must_use]
    pub fn has_same_layout(&self, other: &Self) -> bool {
        self.engine == other.engine && self.nodes == other.nodes && self.edges == other.edges
    }

    #[must_use]
    pub fn from_z3(solution: Z3Solution) -> Self {
        Self {
            engine: SolverEngine::Z3,
            status: solution.status,
            model_version: solution.model_version,
            proof: None,
            validation: None,
            stats: SolutionStats {
                node_count: solution.stats.node_count,
                splitters: solution.stats.splitters,
                mergers: solution.stats.mergers,
                feedback_loops: solution.stats.feedback_loops,
                link_count: solution.stats.belt_count,
                checked_through: Some(solution.stats.checked_through),
                belt_count: Some(solution.stats.belt_count),
                internal_max_throughput: Some(display_from_z3(
                    solution.stats.internal_max_throughput,
                )),
                physical_link_count: None,
                discard_link_count: None,
            },
            total_input: display_from_z3(solution.total_input),
            total_output: display_from_z3(solution.total_output),
            discard_rate: display_from_z3(solution.discard_rate),
            belt_rate: display_from_z3(solution.belt_rate),
            nodes: solution.nodes.into_iter().map(node_from_z3).collect(),
            edges: solution.edges.into_iter().map(edge_from_z3).collect(),
            build_steps: solution.build_steps,
        }
    }

    #[must_use]
    pub fn from_custom(solution: PresentationSolution) -> Self {
        Self {
            engine: SolverEngine::Custom,
            status: solution.status,
            model_version: solution.model_version,
            proof: solution.proof,
            validation: Some(solution.validation),
            stats: SolutionStats {
                node_count: usize::try_from(solution.stats.node_count).unwrap_or(usize::MAX),
                splitters: usize::try_from(solution.stats.splitters).unwrap_or(usize::MAX),
                mergers: usize::try_from(solution.stats.mergers).unwrap_or(usize::MAX),
                feedback_loops: usize::try_from(solution.stats.feedback_loops)
                    .unwrap_or(usize::MAX),
                link_count: usize::try_from(solution.stats.link_count).unwrap_or(usize::MAX),
                checked_through: solution
                    .stats
                    .checked_through
                    .map(|value| usize::try_from(value).unwrap_or(usize::MAX)),
                belt_count: None,
                internal_max_throughput: None,
                physical_link_count: Some(
                    usize::try_from(solution.stats.physical_link_count).unwrap_or(usize::MAX),
                ),
                discard_link_count: Some(
                    usize::try_from(solution.stats.discard_link_count).unwrap_or(usize::MAX),
                ),
            },
            total_input: display_from_custom(solution.total_input),
            total_output: display_from_custom(solution.total_output),
            discard_rate: display_from_custom(solution.discard_rate),
            belt_rate: display_from_custom(solution.belt_rate),
            nodes: solution.nodes.into_iter().map(node_from_custom).collect(),
            edges: solution.edges.into_iter().map(edge_from_custom).collect(),
            build_steps: solution.build_steps,
        }
    }
}

/// Optional global-UNSAT payload for Custom terminal jobs.
pub type UnsatProof = GlobalUnsatProof;

fn display_from_z3(rate: Z3DisplayRate) -> DisplayRate {
    DisplayRate {
        exact: rate.exact,
        decimal: rate.decimal,
    }
}

fn display_from_custom(rate: CustomDisplayRate) -> DisplayRate {
    DisplayRate {
        exact: rate.exact,
        decimal: rate.decimal,
    }
}

fn node_from_z3(node: Z3GraphNode) -> GraphNode {
    GraphNode {
        id: node.id,
        kind: match node.kind {
            solver_z3::NodeKind::Input => GraphNodeKind::Input,
            solver_z3::NodeKind::Splitter2 => GraphNodeKind::Splitter2,
            solver_z3::NodeKind::Splitter3 => GraphNodeKind::Splitter3,
            solver_z3::NodeKind::Merger2 => GraphNodeKind::Merger2,
            solver_z3::NodeKind::Merger3 => GraphNodeKind::Merger3,
            solver_z3::NodeKind::Output => GraphNodeKind::Output,
            solver_z3::NodeKind::Discard => GraphNodeKind::Discard,
        },
        label: node.label,
    }
}

fn node_from_custom(node: CustomGraphNode) -> GraphNode {
    GraphNode {
        id: node.id,
        kind: match node.kind {
            custom_solver_adapter::presentation::GraphNodeKind::Input => GraphNodeKind::Input,
            custom_solver_adapter::presentation::GraphNodeKind::Splitter2 => {
                GraphNodeKind::Splitter2
            }
            custom_solver_adapter::presentation::GraphNodeKind::Splitter3 => {
                GraphNodeKind::Splitter3
            }
            custom_solver_adapter::presentation::GraphNodeKind::Merger2 => GraphNodeKind::Merger2,
            custom_solver_adapter::presentation::GraphNodeKind::Merger3 => GraphNodeKind::Merger3,
            custom_solver_adapter::presentation::GraphNodeKind::Output => GraphNodeKind::Output,
            custom_solver_adapter::presentation::GraphNodeKind::Discard => GraphNodeKind::Discard,
        },
        label: node.label,
    }
}

fn edge_from_z3(edge: Z3GraphEdge) -> GraphEdge {
    GraphEdge {
        id: edge.id,
        source: edge.source,
        target: edge.target,
        source_port: edge.source_port,
        target_port: edge.target_port,
        rate: display_from_z3(edge.rate),
        feedback: edge.feedback,
        discarded: edge.discarded,
    }
}

fn edge_from_custom(edge: CustomGraphEdge) -> GraphEdge {
    GraphEdge {
        id: edge.id,
        source: edge.source,
        target: edge.target,
        source_port: edge.source_port,
        target_port: edge.target_port,
        rate: display_from_custom(edge.rate),
        feedback: edge.feedback,
        discarded: edge.discarded,
    }
}

impl SolveRequest {
    #[must_use]
    pub fn to_z3(&self) -> solver_z3::SolveRequest {
        solver_z3::SolveRequest {
            inputs: self
                .inputs
                .iter()
                .map(|endpoint| solver_z3::EndpointRequest {
                    id: endpoint.id.clone(),
                    name: endpoint.name.clone(),
                    rate: endpoint.rate.clone(),
                })
                .collect(),
            outputs: self
                .outputs
                .iter()
                .map(|endpoint| solver_z3::EndpointRequest {
                    id: endpoint.id.clone(),
                    name: endpoint.name.clone(),
                    rate: endpoint.rate.clone(),
                })
                .collect(),
            belt_rate: self.belt_rate.clone(),
            enumerate_all_at_n: self.enumerate_all_at_n,
        }
    }

    #[must_use]
    pub fn to_custom_app(&self) -> custom_solver_adapter::exact_adapter::AppSolveRequest {
        custom_solver_adapter::exact_adapter::AppSolveRequest {
            inputs: self
                .inputs
                .iter()
                .map(
                    |endpoint| custom_solver_adapter::exact_adapter::EndpointRequest {
                        id: endpoint.id.clone(),
                        name: endpoint.name.clone(),
                        rate: endpoint.rate.clone(),
                    },
                )
                .collect(),
            outputs: self
                .outputs
                .iter()
                .map(
                    |endpoint| custom_solver_adapter::exact_adapter::EndpointRequest {
                        id: endpoint.id.clone(),
                        name: endpoint.name.clone(),
                        rate: endpoint.rate.clone(),
                    },
                )
                .collect(),
            belt_rate: self.belt_rate.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solve_request_defaults_to_custom_engine() {
        let request: SolveRequest = serde_json::from_str(
            r#"{
                "inputs": [],
                "outputs": [{"id":"o1","name":"","rate":"60"}],
                "beltRate": "1200"
            }"#,
        )
        .unwrap();
        assert_eq!(request.engine, SolverEngine::Custom);
        assert!(!request.enumerate_all_at_n);
    }

    #[test]
    fn solve_request_round_trips_engine_selection() {
        let request = SolveRequest {
            inputs: Vec::new(),
            outputs: vec![EndpointInput {
                id: "o1".into(),
                name: String::new(),
                rate: "60".into(),
            }],
            belt_rate: "1200".into(),
            enumerate_all_at_n: true,
            engine: SolverEngine::Z3,
        };
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["engine"], "z3");
        assert_eq!(json["enumerateAllAtN"], true);
        let parsed: SolveRequest = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.engine, SolverEngine::Z3);
        assert!(parsed.enumerate_all_at_n);
    }

    #[test]
    fn custom_solution_never_claims_optimal_without_proof() {
        use custom_solver_adapter::exact_adapter::{AppSolveRequest, EndpointRequest};
        use custom_solver_adapter::presentation::present_best_known_solution;
        use solver_api::{
            BestKnownSolution, CanonicalGraphKey, ConsumerPortRef, InputTerminalIndex,
            OutputTerminalIndex, PhysicalGraph, PhysicalLink, ProducerPortRef, ValidationSummary,
        };

        let prepared = AppSolveRequest {
            inputs: vec![EndpointRequest {
                id: "i0".into(),
                name: "in".into(),
                rate: "1".into(),
            }],
            outputs: vec![EndpointRequest {
                id: "o0".into(),
                name: "out".into(),
                rate: "1".into(),
            }],
            belt_rate: "1".into(),
        }
        .prepare()
        .unwrap();
        let best = BestKnownSolution {
            node_count: 0,
            link_count: 1,
            physical_link_count: 1,
            discard_link_count: 0,
            canonical_graph_key: CanonicalGraphKey::from_bytes(vec![1]),
            graph: PhysicalGraph {
                nodes: Vec::new(),
                links: vec![PhysicalLink {
                    producer: ProducerPortRef::Input(InputTerminalIndex(0)),
                    consumer: ConsumerPortRef::Output(OutputTerminalIndex(0)),
                    flow: "1".parse().unwrap(),
                }],
            },
            validation: ValidationSummary {
                validator_version: 2,
                node_count: 0,
                link_count: 1,
                physical_link_count: 1,
                discard_link_count: 0,
                cyclic_scc_count: 0,
            },
        };
        let presented = present_best_known_solution(&prepared, &best, None).unwrap();
        let solution = Solution::from_custom(presented);
        assert_eq!(solution.engine, SolverEngine::Custom);
        assert_eq!(solution.status, "best_known");
        assert!(solution.proof.is_none());
        assert_eq!(solution.stats.link_count, 1);
        assert!(solution.stats.belt_count.is_none());
        assert!(solution.stats.internal_max_throughput.is_none());
        assert_eq!(solution.stats.physical_link_count, Some(1));
    }
}
