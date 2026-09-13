//! Shared application orchestration and deterministic solution presentation.

pub mod jobs;
pub mod presentation;
#[cfg(feature = "native-runtime")]
pub mod runtime;
pub mod solution;
