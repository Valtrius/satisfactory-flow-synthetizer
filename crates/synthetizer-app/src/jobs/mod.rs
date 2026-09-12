//! Transport-neutral snapshots and terminal projection.
//! Hosts own scheduling, cancellation acceptance, and the completion seal.
mod projection;
mod snapshot;

pub use projection::{cancel_before_presentation, interruption_packet, project_outcome};
pub use snapshot::{JobSnapshot, JobStatus, SolveRequest};

#[cfg(test)]
mod tests;
