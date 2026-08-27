//! Stable public types shared by the exact solver, validator, persistence layer, and app.
//!
//! Rates cross serialization boundaries as canonical strings. No type in this crate uses
//! floating point for flow values or solver decisions.

mod execution;
mod graph;
mod problem;
mod progress;
mod rational;
mod request;
mod result;

pub use graph::{
    CanonicalGraphKey, ConsumerPortRef, DiscardTerminalIndex, InputTerminalIndex, NodeId,
    NodeProfile, NodeType, OutputTerminalIndex, PhysicalGraph, PhysicalLink, PhysicalNode,
    ProducerPortRef,
};
pub use problem::Problem;
pub use progress::{
    Diagnostic, DiagnosticValue, LinkConstraint, SolvePhase, SolverEvent, SolverProgress,
};
pub use rational::{Rational, RationalParseError};
pub use result::{
    BestKnownSolution, ExternalTerminal, GlobalUnsatProof, GlobalUnsatReason, IncompleteReason,
    IncompleteResult, OptimalSolution, ProofSummary, SolveResult, ValidationSummary,
};

pub use execution::{
    EnumerationStatus, OptimalityProof, RunOptions, SolveMode, SolveObserver, SolveOutcome,
    SolverError,
};
pub use request::{
    EndpointRequest, PrepareRequestError, PreparedProblem, ProblemRequest, TerminalMetadata,
};
