//! Stable public types shared by the exact solver, validator, persistence layer, and app.
//!
//! Rates cross serialization boundaries as canonical strings. No type in this crate uses
//! floating point for flow values or solver decisions.

mod graph;
mod problem;
mod progress;
mod rational;
mod result;

pub use graph::{
    CanonicalGraphKey, ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId,
    NodeProfile, NodeType, OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode,
    ProducerPortRef,
};
pub use problem::Problem;
pub use progress::{
    ProofObligation, SearchInstrumentation, SolvePhase, SolverEvent, SolverProgress,
};
pub use rational::{Rational, RationalParseError};
pub use result::{
    BestKnownSolution, ExternalTerminal, GlobalUnsatProof, GlobalUnsatReason, IncompleteReason,
    IncompleteResult, OptimalSolution, ProofSummary, SolveResult, ValidationSummary,
};
