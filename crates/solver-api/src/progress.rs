use serde::{Deserialize, Serialize};

use crate::BestKnownSolution;

/// Current high-level phase of a live solve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SolvePhase {
    /// Computing the proven starting node lower bound.
    ComputingLowerBound,
    /// Exhausting profiles in node-count and structural-link-group order.
    Searching,
    Enumerating,
}

/// A solver-specific measurement. Integer strings preserve u64 precision in JavaScript.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum DiagnosticValue {
    Integer(String),
    Text(String),
    Boolean(bool),
    Rate(crate::Rational),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub name: String,
    pub label: String,
    pub value: DiagnosticValue,
    pub unit: Option<String>,
}

impl Diagnostic {
    #[must_use]
    pub fn counter(name: &str, label: &str, value: impl std::fmt::Display) -> Self {
        Self {
            name: name.to_owned(),
            label: label.to_owned(),
            value: DiagnosticValue::Integer(value.to_string()),
            unit: None,
        }
    }
    #[must_use]
    pub fn text(name: &str, label: &str, value: impl Into<String>) -> Self {
        Self {
            name: name.to_owned(),
            label: label.to_owned(),
            value: DiagnosticValue::Text(value.into()),
            unit: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum LinkConstraint {
    Exact(u32),
}

/// Common facts only. Diagnostics are a complete replacement on each snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolverProgress {
    pub phase: SolvePhase,
    pub elapsed_ms: u64,
    pub node_count: Option<u32>,
    pub link_constraint: Option<LinkConstraint>,
    pub node_lower_bound: Option<u32>,
    pub best_node_count: Option<u32>,
    pub best_link_count: Option<u32>,
    /// Distinct enumeration layouts; Opt incumbents do not increment this count.
    pub solutions_found: u64,
    pub custom: Vec<Diagnostic>,
}

/// Live solver notification delivered to an application or service adapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SolverEvent {
    /// Updated proof progress.
    Progress(SolverProgress),
    /// A newly improved, independently validated upper-bound witness.
    Incumbent(BestKnownSolution),
    /// A distinct validated layout found during complete minimum-N enumeration.
    SolutionFound(BestKnownSolution),
}
