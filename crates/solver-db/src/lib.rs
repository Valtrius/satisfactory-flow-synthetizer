//! Persistent exact-component accelerator.
//!
//! `SQLite` contents never participate in completeness: every failed, missing,
//! incompatible, or quarantined lookup is a cache miss and the caller must keep
//! the primitive physical search available.

mod applications;
mod codec;
mod fingerprint;
mod prewarm;
mod records;
mod runtime;
mod sqlite;

pub use applications::{StoredApplicationCandidate, encode_application_key};
pub use codec::{DecodeError, Decoder, Encoder};
pub use fingerprint::{SEMANTIC_SCHEMA_DESCRIPTION, SemanticFingerprint, content_id};
pub use prewarm::{
    PrewarmBoundaryCoverage, PrewarmCellCoverage, PrewarmOptions, PrewarmPartialReason,
    PrewarmResult, PrewarmScheduling,
};
pub use records::encode_component_record;
pub use runtime::{ComponentCatalogLoad, DatabaseApplicationComponentRuntime};
pub use sqlite::{
    ApplicationRecord, ComponentDatabase, ComponentDatabaseConfig, ComponentDatabaseStatus,
    ComponentRecordBlob, DatabaseError, DatabaseMode, DominanceRecord, RawRecordId,
};
