//! Bounded, resumable, capacity-independent component prewarming.
//!
//! A persisted component remains only an accelerator: it is reconstructed and
//! re-certified before use, and no solver branch may depend on database
//! coverage for completeness.  The coverage ledger nevertheless follows a
//! strict publication rule. Individual deterministic root partitions are
//! committed atomically only after exhaustive enumeration; a cell becomes
//! complete only after every expected partition and its component membership
//! have been committed and cross-checked in one final transaction.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::AtomicBool,
    thread,
};

use rusqlite::{OptionalExtension, Transaction, params};
use solver_api::{CanonicalGraphKey, NodeProfile};
use solver_core::{
    component_enumerator::{
        CompleteComponentPartition, ComponentCell, ComponentCellManifest,
        ComponentEnumerationBounds, ComponentPartitionEnumeration, ComponentPartitionManifest,
        ComponentRootPartition, component_cells, component_partitions,
        enumerate_component_partition,
    },
    components::Component,
};

use crate::{
    ComponentDatabase, DatabaseError, Decoder, Encoder, RawRecordId, SemanticFingerprint,
    content_id,
};

const CELL_MAGIC: &[u8] = b"satisfactory-prewarm-cell";
const CELL_VERSION: u16 = 1;
const PARTITION_MAGIC: &[u8] = b"satisfactory-prewarm-partition";
const PARTITION_VERSION: u16 = 1;
const CELL_MANIFEST_MAGIC: &[u8] = b"satisfactory-prewarm-cell-manifest";
const CELL_MANIFEST_VERSION: u16 = 1;
const PARTITION_MANIFEST_MAGIC: &[u8] = b"satisfactory-prewarm-partition-manifest";
const PARTITION_MANIFEST_VERSION: u16 = 1;
const MEMBERSHIP_MAGIC: &[u8] = b"satisfactory-prewarm-membership";
const MEMBERSHIP_VERSION: u16 = 1;
const PARTITION_ROOT_MAGIC: &[u8] = b"satisfactory-prewarm-partition-root";
const PARTITION_ROOT_VERSION: u16 = 1;
const MAX_MANIFEST_ITEMS: usize = 1_000_000;

/// Scheduling intent for a synchronous prewarm call.
///
/// `Background` yields between root partitions so live solver work can run
/// first. `Live` performs the same deterministic proof work without voluntary
/// yields. It does not alter enumeration order or stored results.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PrewarmScheduling {
    /// Cooperatively yield after each completed/reused root partition.
    #[default]
    Background,
    /// Continue immediately between partitions.
    Live,
}

/// Public finite prewarm bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrewarmOptions {
    /// Inclusive physical-node bound. Enumerated cells themselves have `N>=1`.
    pub max_nodes: u32,
    /// Optional inclusive `p+q` boundary-port filter.
    ///
    /// `None` means every boundary shape possible under `max_nodes`, not an
    /// unbounded node universe.
    pub max_boundary_ports: Option<u32>,
    /// Whether the caller considers this background or live work.
    pub scheduling: PrewarmScheduling,
}

/// Explicit interpretation of the boundary portion of a completeness claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrewarmBoundaryCoverage {
    /// Every boundary shape induced by the bounded node universe.
    AllWithinNodeBound,
    /// Only cells with `p+q` at most this value.
    AtMost(u32),
}

impl From<Option<u32>> for PrewarmBoundaryCoverage {
    fn from(maximum: Option<u32>) -> Self {
        maximum.map_or(Self::AllWithinNodeBound, Self::AtMost)
    }
}

/// Why a bounded prewarm run did not publish complete coverage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrewarmPartialReason {
    /// The caller's exact cancellation flag was observed.
    Cancelled,
    /// Persistence was deliberately disabled; primitive solving remains complete.
    DatabaseUnavailable,
    /// `SQLite` or record validation failed; the accelerator is treated as a miss.
    DatabaseFailure,
    /// Exact structural enumeration or component certification failed.
    EnumerationFailure,
}

/// Result of one bounded prewarm request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrewarmResult {
    /// Every in-scope cell was either reverified or transactionally committed.
    CompleteThrough {
        /// Inclusive physical-node bound.
        max_nodes: u32,
        /// The exact optional boundary filter supplied by the caller.
        max_boundary_ports: Option<u32>,
        /// Explicit non-ambiguous interpretation of that filter.
        boundary_coverage: PrewarmBoundaryCoverage,
        /// Number of finite cells covered.
        completed_cells: u64,
        /// Number of cells reused from an already complete compatible ledger.
        reused_cells: u64,
    },
    /// Work stopped without claiming coverage for the next unfinished cell.
    Partial {
        /// Requested inclusive physical-node bound.
        max_nodes: u32,
        /// The exact optional boundary filter supplied by the caller.
        max_boundary_ports: Option<u32>,
        /// Explicit non-ambiguous interpretation of that filter.
        boundary_coverage: PrewarmBoundaryCoverage,
        /// Number of proof-complete cells before interruption/failure.
        completed_cells: u64,
        /// Total finite cell count when bounds were successfully generated.
        total_cells: u64,
        /// First cell for which this call did not establish completion.
        next_cell: Option<ComponentCell>,
        /// Non-completeness reason.
        reason: PrewarmPartialReason,
    },
}

/// Independently digest-checked persisted coverage for one cell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrewarmCellCoverage {
    /// Exact finite cell.
    pub cell: ComponentCell,
    /// Persisted complete topology manifest.
    pub manifest: ComponentCellManifest,
    /// Number of deterministic root partitions forming the manifest.
    pub partition_count: u32,
    /// Digest of the ordered partition-manifest tree.
    pub partition_root_digest: [u8; 32],
    /// Digest of sorted component-record membership.
    pub membership_digest: [u8; 32],
    /// Number of unique persisted component records belonging to the cell.
    pub component_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredPartition {
    id: RawRecordId,
    manifest: ComponentPartitionManifest,
    manifest_hash: [u8; 32],
    membership: Vec<RawRecordId>,
    membership_hash: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StoredCell {
    coverage: PrewarmCellCoverage,
    partitions: Vec<StoredPartition>,
    membership: Vec<RawRecordId>,
}

/// A process-local catalog reconstructed only from complete, verified prewarm cells.
///
/// This remains an accelerator input. `verified_cells` describes which finite
/// ledgers survived the current read; it is not a proof consumed by live search.
pub(crate) struct VerifiedPrewarmCatalog {
    pub expected_cells: u64,
    pub verified_cells: u64,
    pub components: Vec<Component>,
}

impl ComponentDatabase {
    /// Prewarms a bounded component universe without cancellation.
    ///
    /// Database and enumeration failures are reported as [`PrewarmResult::Partial`]
    /// and never affect primitive solver completeness.
    #[must_use]
    pub fn prewarm(&self, options: PrewarmOptions) -> PrewarmResult {
        self.prewarm_cancellable(options, &AtomicBool::new(false))
    }

    /// Prewarms a bounded component universe with exact cooperative cancellation.
    ///
    /// The flag is checked within every structural DFS state. An interrupted
    /// root partition is discarded rather than persisted; previously committed
    /// complete partitions remain available for a later resume.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn prewarm_cancellable(
        &self,
        options: PrewarmOptions,
        cancel: &AtomicBool,
    ) -> PrewarmResult {
        let boundary_coverage = options.max_boundary_ports.into();
        let Ok(cells) = component_cells(ComponentEnumerationBounds {
            max_nodes: options.max_nodes,
            max_boundary_ports: options.max_boundary_ports,
        }) else {
            return PrewarmResult::Partial {
                max_nodes: options.max_nodes,
                max_boundary_ports: options.max_boundary_ports,
                boundary_coverage,
                completed_cells: 0,
                total_cells: 0,
                next_cell: None,
                reason: PrewarmPartialReason::EnumerationFailure,
            };
        };
        let total_cells = u64::try_from(cells.len()).unwrap_or(u64::MAX);
        if !self.is_enabled() {
            return PrewarmResult::Partial {
                max_nodes: options.max_nodes,
                max_boundary_ports: options.max_boundary_ports,
                boundary_coverage,
                completed_cells: 0,
                total_cells,
                next_cell: cells.first().copied(),
                reason: PrewarmPartialReason::DatabaseUnavailable,
            };
        }

        let mut completed_cells = 0_u64;
        let mut reused_cells = 0_u64;
        for cell in cells.iter().copied() {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return partial(
                    options,
                    boundary_coverage,
                    completed_cells,
                    total_cells,
                    Some(cell),
                    PrewarmPartialReason::Cancelled,
                );
            }

            match self.load_complete_prewarm_cell(cell) {
                Ok(Some(stored)) => match self.validate_stored_cell_components(&stored) {
                    Ok(true) => {
                        completed_cells += 1;
                        reused_cells += 1;
                        continue;
                    }
                    Ok(false) => {}
                    Err(_) => {
                        return partial(
                            options,
                            boundary_coverage,
                            completed_cells,
                            total_cells,
                            Some(cell),
                            PrewarmPartialReason::DatabaseFailure,
                        );
                    }
                },
                Ok(None) => {}
                Err(_) => {
                    return partial(
                        options,
                        boundary_coverage,
                        completed_cells,
                        total_cells,
                        Some(cell),
                        PrewarmPartialReason::DatabaseFailure,
                    );
                }
            }

            let Ok(partitions) = component_partitions(cell) else {
                return partial(
                    options,
                    boundary_coverage,
                    completed_cells,
                    total_cells,
                    Some(cell),
                    PrewarmPartialReason::EnumerationFailure,
                );
            };
            let mut stored_partitions = Vec::with_capacity(partitions.len());
            for partition in partitions {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return partial(
                        options,
                        boundary_coverage,
                        completed_cells,
                        total_cells,
                        Some(cell),
                        PrewarmPartialReason::Cancelled,
                    );
                }
                let reused = match self.load_prewarm_partition(cell, partition) {
                    Ok(Some(stored)) => match self.validate_partition_components(cell, &stored) {
                        Ok(true) => Some(stored),
                        Ok(false) => {
                            if self.invalidate_prewarm_partition(cell, partition).is_err() {
                                return partial(
                                    options,
                                    boundary_coverage,
                                    completed_cells,
                                    total_cells,
                                    Some(cell),
                                    PrewarmPartialReason::DatabaseFailure,
                                );
                            }
                            None
                        }
                        Err(_) => {
                            return partial(
                                options,
                                boundary_coverage,
                                completed_cells,
                                total_cells,
                                Some(cell),
                                PrewarmPartialReason::DatabaseFailure,
                            );
                        }
                    },
                    Ok(None) => None,
                    Err(_) => {
                        return partial(
                            options,
                            boundary_coverage,
                            completed_cells,
                            total_cells,
                            Some(cell),
                            PrewarmPartialReason::DatabaseFailure,
                        );
                    }
                };
                let stored = if let Some(stored) = reused {
                    stored
                } else {
                    let complete = match enumerate_component_partition(cell, partition, cancel) {
                        Ok(ComponentPartitionEnumeration::Complete(complete)) => complete,
                        Ok(ComponentPartitionEnumeration::Cancelled { .. }) => {
                            return partial(
                                options,
                                boundary_coverage,
                                completed_cells,
                                total_cells,
                                Some(cell),
                                PrewarmPartialReason::Cancelled,
                            );
                        }
                        Err(_) => {
                            return partial(
                                options,
                                boundary_coverage,
                                completed_cells,
                                total_cells,
                                Some(cell),
                                PrewarmPartialReason::EnumerationFailure,
                            );
                        }
                    };
                    let Ok(membership) = self.store_partition_components(&complete) else {
                        return partial(
                            options,
                            boundary_coverage,
                            completed_cells,
                            total_cells,
                            Some(cell),
                            PrewarmPartialReason::DatabaseFailure,
                        );
                    };
                    match self.commit_prewarm_partition(&complete.manifest, &membership) {
                        Ok(stored) => stored,
                        Err(_) => {
                            return partial(
                                options,
                                boundary_coverage,
                                completed_cells,
                                total_cells,
                                Some(cell),
                                PrewarmPartialReason::DatabaseFailure,
                            );
                        }
                    }
                };
                stored_partitions.push(stored);
                if options.scheduling == PrewarmScheduling::Background {
                    thread::yield_now();
                }
            }

            if self
                .finalize_prewarm_cell(cell, &stored_partitions)
                .is_err()
            {
                return partial(
                    options,
                    boundary_coverage,
                    completed_cells,
                    total_cells,
                    Some(cell),
                    PrewarmPartialReason::DatabaseFailure,
                );
            }
            completed_cells += 1;
        }
        PrewarmResult::CompleteThrough {
            max_nodes: options.max_nodes,
            max_boundary_ports: options.max_boundary_ports,
            boundary_coverage,
            completed_cells,
            reused_cells,
        }
    }

    /// Returns digest-checked complete cell coverage within the exact requested bounds.
    ///
    /// This is diagnostic coverage only. Solving remains complete without it.
    ///
    /// # Errors
    ///
    /// Returns an operational database error. Corrupt rows are quarantined and
    /// omitted rather than returned.
    pub fn prewarm_coverage(
        &self,
        options: PrewarmOptions,
    ) -> Result<Vec<PrewarmCellCoverage>, DatabaseError> {
        if !self.is_enabled() {
            return Ok(Vec::new());
        }
        let cells = component_cells(ComponentEnumerationBounds {
            max_nodes: options.max_nodes,
            max_boundary_ports: options.max_boundary_ports,
        })
        .map_err(|_| DatabaseError::InvalidPrewarmRecord)?;
        let mut coverage = Vec::new();
        for cell in cells {
            let Some(stored) = self.load_complete_prewarm_cell(cell)? else {
                continue;
            };
            if self.validate_stored_cell_components(&stored)? {
                coverage.push(stored.coverage);
            }
        }
        Ok(coverage)
    }

    /// Reconstructs the symbolic macro catalog covered by complete prewarm cells.
    ///
    /// Every returned component passed content-address checks, exact physical
    /// reconstruction, cell-membership checks, and the complete cell-ledger
    /// cross-check during this call. Missing or corrupt cells are cache misses.
    /// No caller may use the returned set to suppress primitive transitions or
    /// discharge a concrete application-optimality obligation.
    pub(crate) fn load_verified_prewarm_catalog(
        &self,
        options: PrewarmOptions,
    ) -> Result<VerifiedPrewarmCatalog, DatabaseError> {
        if !self.is_enabled() {
            return Err(DatabaseError::DatabaseDisabled);
        }
        let cells = component_cells(ComponentEnumerationBounds {
            max_nodes: options.max_nodes,
            max_boundary_ports: options.max_boundary_ports,
        })
        .map_err(|_| DatabaseError::InvalidPrewarmRecord)?;
        let expected_cells = u64::try_from(cells.len()).unwrap_or(u64::MAX);
        let mut verified_cells = 0_u64;
        let mut components = BTreeMap::new();

        for cell in cells {
            let Some(stored) = self.load_complete_prewarm_cell(cell)? else {
                continue;
            };
            if !self.validate_stored_cell_components(&stored)? {
                continue;
            }

            // Load into a cell-local buffer first. If another connection alters
            // a member between the ledger check above and reconstruction here,
            // the whole cell remains a miss instead of becoming partial input.
            let mut cell_components = Vec::with_capacity(stored.membership.len());
            let mut cell_is_complete = true;
            for id in stored.membership {
                let Some(component) = self.load_component(id)? else {
                    cell_is_complete = false;
                    break;
                };
                if !component_belongs_to_cell(&component, cell) {
                    cell_is_complete = false;
                    break;
                }
                cell_components.push(component);
            }
            if !cell_is_complete {
                continue;
            }
            verified_cells = verified_cells.saturating_add(1);
            for component in cell_components {
                components
                    .entry(component.canonical_key().clone())
                    .or_insert(component);
            }
        }

        Ok(VerifiedPrewarmCatalog {
            expected_cells,
            verified_cells,
            components: components.into_values().collect(),
        })
    }

    fn store_partition_components(
        &self,
        complete: &CompleteComponentPartition,
    ) -> Result<Vec<RawRecordId>, DatabaseError> {
        let mut membership = Vec::with_capacity(complete.components.len());
        for component in &complete.components {
            let id = self
                .store_component(component)?
                .ok_or(DatabaseError::DatabaseDisabled)?;
            membership.push(id);
        }
        membership.sort_unstable();
        membership.dedup();
        Ok(membership)
    }

    fn validate_stored_cell_components(&self, cell: &StoredCell) -> Result<bool, DatabaseError> {
        for partition in &cell.partitions {
            if !self.validate_partition_components(cell.coverage.cell, partition)? {
                self.invalidate_prewarm_partition(
                    cell.coverage.cell,
                    partition.manifest.partition,
                )?;
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn validate_partition_components(
        &self,
        cell: ComponentCell,
        partition: &StoredPartition,
    ) -> Result<bool, DatabaseError> {
        for &id in &partition.membership {
            let Some(component) = self.load_component(id)? else {
                return Ok(false);
            };
            if !component_belongs_to_cell(&component, cell) {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

const CELL_ID_TAG: &[u8] = b"prewarm-cell-id-v1";
const PARTITION_ID_TAG: &[u8] = b"prewarm-partition-id-v1";
const CELL_MANIFEST_TAG: &[u8] = b"prewarm-cell-manifest-v1";
const PARTITION_MANIFEST_TAG: &[u8] = b"prewarm-partition-manifest-v1";
const MEMBERSHIP_TAG: &[u8] = b"prewarm-membership-v1";
const PARTITION_ROOT_TAG: &[u8] = b"prewarm-partition-root-v1";

#[derive(Debug)]
struct CellRow {
    cell_payload: Vec<u8>,
    manifest_payload: Vec<u8>,
    manifest_hash: [u8; 32],
    partition_root_hash: [u8; 32],
    membership_payload: Vec<u8>,
    membership_hash: [u8; 32],
    status: i64,
}

#[derive(Debug)]
struct PartitionRow {
    partition_payload: Vec<u8>,
    manifest_payload: Vec<u8>,
    manifest_hash: [u8; 32],
    membership_payload: Vec<u8>,
    membership_hash: [u8; 32],
}

fn encode_cell(cell: ComponentCell) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(CELL_MAGIC);
    encoder.u16(CELL_VERSION);
    encode_profile(&mut encoder, cell.profile);
    encoder.u32(cell.boundary_input_count);
    encoder.u32(cell.boundary_output_count);
    encoder.u32(cell.internal_link_count);
    encoder.finish()
}

fn decode_cell(payload: &[u8]) -> Result<ComponentCell, String> {
    let mut decoder = Decoder::new(payload);
    require_bytes(&mut decoder, CELL_MAGIC)?;
    require_version(&mut decoder, CELL_VERSION)?;
    let profile = decode_profile(&mut decoder)?;
    let boundary_input_count = decoder.u32().map_err(display)?;
    let boundary_output_count = decoder.u32().map_err(display)?;
    let stored_internal_link_count = decoder.u32().map_err(display)?;
    decoder.finish().map_err(display)?;
    let cell = ComponentCell::new(profile, boundary_input_count, boundary_output_count)
        .map_err(display)?;
    if cell.internal_link_count != stored_internal_link_count {
        return Err("prewarm cell stores an inconsistent internal-link count".to_owned());
    }
    Ok(cell)
}

fn encode_partition(partition: ComponentRootPartition) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(PARTITION_MAGIC);
    encoder.u16(PARTITION_VERSION);
    encoder.u32(partition.root_consumer_index);
    encoder.finish()
}

fn decode_partition(payload: &[u8]) -> Result<ComponentRootPartition, String> {
    let mut decoder = Decoder::new(payload);
    require_bytes(&mut decoder, PARTITION_MAGIC)?;
    require_version(&mut decoder, PARTITION_VERSION)?;
    let partition = ComponentRootPartition {
        root_consumer_index: decoder.u32().map_err(display)?,
    };
    decoder.finish().map_err(display)?;
    Ok(partition)
}

fn encode_cell_manifest(manifest: &ComponentCellManifest) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(CELL_MANIFEST_MAGIC);
    encoder.u16(CELL_MANIFEST_VERSION);
    encoder.bytes(&encode_cell(manifest.cell));
    encode_keys(&mut encoder, &manifest.canonical_topology_keys);
    encoder.finish()
}

fn decode_cell_manifest(payload: &[u8]) -> Result<ComponentCellManifest, String> {
    let mut decoder = Decoder::new(payload);
    require_bytes(&mut decoder, CELL_MANIFEST_MAGIC)?;
    require_version(&mut decoder, CELL_MANIFEST_VERSION)?;
    let cell = decode_cell(decoder.bytes().map_err(display)?)?;
    let canonical_topology_keys = decode_keys(&mut decoder)?;
    decoder.finish().map_err(display)?;
    if !strictly_sorted_unique_keys(&canonical_topology_keys) {
        return Err("prewarm cell manifest keys are not strictly sorted".to_owned());
    }
    Ok(ComponentCellManifest {
        cell,
        canonical_topology_keys,
    })
}

fn encode_partition_manifest(manifest: &ComponentPartitionManifest) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(PARTITION_MANIFEST_MAGIC);
    encoder.u16(PARTITION_MANIFEST_VERSION);
    encoder.bytes(&encode_cell(manifest.cell));
    encoder.bytes(&encode_partition(manifest.partition));
    encode_keys(&mut encoder, &manifest.canonical_topology_keys);
    encoder.finish()
}

fn decode_partition_manifest(payload: &[u8]) -> Result<ComponentPartitionManifest, String> {
    let mut decoder = Decoder::new(payload);
    require_bytes(&mut decoder, PARTITION_MANIFEST_MAGIC)?;
    require_version(&mut decoder, PARTITION_MANIFEST_VERSION)?;
    let cell = decode_cell(decoder.bytes().map_err(display)?)?;
    let partition = decode_partition(decoder.bytes().map_err(display)?)?;
    let canonical_topology_keys = decode_keys(&mut decoder)?;
    decoder.finish().map_err(display)?;
    if !strictly_sorted_unique_keys(&canonical_topology_keys) {
        return Err("prewarm partition manifest keys are not strictly sorted".to_owned());
    }
    Ok(ComponentPartitionManifest {
        cell,
        partition,
        canonical_topology_keys,
    })
}

fn encode_membership(membership: &[RawRecordId]) -> Vec<u8> {
    let mut encoder = Encoder::new();
    encoder.bytes(MEMBERSHIP_MAGIC);
    encoder.u16(MEMBERSHIP_VERSION);
    encode_len(&mut encoder, membership.len());
    for id in membership {
        encoder.bytes(&id.as_bytes());
    }
    encoder.finish()
}

fn decode_membership(payload: &[u8]) -> Result<Vec<RawRecordId>, String> {
    let mut decoder = Decoder::new(payload);
    require_bytes(&mut decoder, MEMBERSHIP_MAGIC)?;
    require_version(&mut decoder, MEMBERSHIP_VERSION)?;
    let count = decode_len(&mut decoder)?;
    let mut membership = Vec::with_capacity(count);
    for _ in 0..count {
        let bytes: [u8; 32] = decoder
            .bytes()
            .map_err(display)?
            .try_into()
            .map_err(|_| "prewarm component id is not 32 bytes".to_owned())?;
        membership.push(RawRecordId::from_bytes(bytes));
    }
    decoder.finish().map_err(display)?;
    if !strictly_sorted_unique(&membership) {
        return Err("prewarm membership is not strictly sorted".to_owned());
    }
    Ok(membership)
}

fn encode_profile(encoder: &mut Encoder, profile: NodeProfile) {
    encoder.u32(profile.splitter2);
    encoder.u32(profile.splitter3);
    encoder.u32(profile.merger2);
    encoder.u32(profile.merger3);
}

fn decode_profile(decoder: &mut Decoder<'_>) -> Result<NodeProfile, String> {
    Ok(NodeProfile {
        splitter2: decoder.u32().map_err(display)?,
        splitter3: decoder.u32().map_err(display)?,
        merger2: decoder.u32().map_err(display)?,
        merger3: decoder.u32().map_err(display)?,
    })
}

fn encode_keys(encoder: &mut Encoder, keys: &[CanonicalGraphKey]) {
    encode_len(encoder, keys.len());
    for key in keys {
        encoder.bytes(key.as_bytes());
    }
}

fn decode_keys(decoder: &mut Decoder<'_>) -> Result<Vec<CanonicalGraphKey>, String> {
    let count = decode_len(decoder)?;
    let mut keys = Vec::with_capacity(count);
    for _ in 0..count {
        keys.push(CanonicalGraphKey::from_bytes(
            decoder.bytes().map_err(display)?.to_vec(),
        ));
    }
    Ok(keys)
}

fn encode_len(encoder: &mut Encoder, length: usize) {
    encoder.u32(u32::try_from(length).expect("bounded prewarm sequence must fit u32"));
}

fn decode_len(decoder: &mut Decoder<'_>) -> Result<usize, String> {
    let count = usize::try_from(decoder.u32().map_err(display)?)
        .map_err(|_| "prewarm sequence length cannot fit this platform".to_owned())?;
    if count > MAX_MANIFEST_ITEMS {
        return Err("prewarm sequence exceeds the persistent codec limit".to_owned());
    }
    Ok(count)
}

fn require_bytes(decoder: &mut Decoder<'_>, expected: &[u8]) -> Result<(), String> {
    if decoder.bytes().map_err(display)? == expected {
        Ok(())
    } else {
        Err("prewarm record magic does not match".to_owned())
    }
}

fn require_version(decoder: &mut Decoder<'_>, expected: u16) -> Result<(), String> {
    let actual = decoder.u16().map_err(display)?;
    if actual == expected {
        Ok(())
    } else {
        Err(format!(
            "unsupported prewarm record version {actual}, expected {expected}"
        ))
    }
}

fn display(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn strictly_sorted_unique<T: Ord>(values: &[T]) -> bool {
    values.windows(2).all(|pair| pair[0] < pair[1])
}

fn strictly_sorted_unique_keys(values: &[CanonicalGraphKey]) -> bool {
    strictly_sorted_unique(values)
}

fn prewarm_cell_id(fingerprint: SemanticFingerprint, payload: &[u8]) -> RawRecordId {
    RawRecordId::from_bytes(content_id(fingerprint, CELL_ID_TAG, payload))
}

fn prewarm_partition_id(
    fingerprint: SemanticFingerprint,
    cell_payload: &[u8],
    partition_payload: &[u8],
) -> RawRecordId {
    let mut encoder = Encoder::new();
    encoder.bytes(cell_payload);
    encoder.bytes(partition_payload);
    RawRecordId::from_bytes(content_id(fingerprint, PARTITION_ID_TAG, &encoder.finish()))
}

fn prewarm_cell_manifest_hash(
    fingerprint: SemanticFingerprint,
    manifest: &ComponentCellManifest,
) -> [u8; 32] {
    content_id(
        fingerprint,
        CELL_MANIFEST_TAG,
        &encode_cell_manifest(manifest),
    )
}

fn prewarm_partition_manifest_hash(
    fingerprint: SemanticFingerprint,
    manifest: &ComponentPartitionManifest,
) -> [u8; 32] {
    content_id(
        fingerprint,
        PARTITION_MANIFEST_TAG,
        &encode_partition_manifest(manifest),
    )
}

fn prewarm_membership_hash(
    fingerprint: SemanticFingerprint,
    membership_payload: &[u8],
) -> [u8; 32] {
    content_id(fingerprint, MEMBERSHIP_TAG, membership_payload)
}

fn prewarm_partition_root_hash(
    fingerprint: SemanticFingerprint,
    partitions: &[StoredPartition],
) -> [u8; 32] {
    let mut encoder = Encoder::new();
    encoder.bytes(PARTITION_ROOT_MAGIC);
    encoder.u16(PARTITION_ROOT_VERSION);
    encode_len(&mut encoder, partitions.len());
    for partition in partitions {
        encoder.bytes(&partition.id.as_bytes());
        encoder.bytes(&partition.manifest_hash);
        encoder.bytes(&partition.membership_hash);
    }
    content_id(fingerprint, PARTITION_ROOT_TAG, &encoder.finish())
}

fn select_cell(
    transaction: &Transaction<'_>,
    fingerprint: SemanticFingerprint,
    cell_id: RawRecordId,
) -> Result<Option<CellRow>, DatabaseError> {
    transaction
        .query_row(
            "SELECT cell_payload, manifest_payload, manifest_hash,
                    partition_root_hash, membership_payload, membership_hash, status
             FROM prewarm_cells WHERE fingerprint=?1 AND cell_id=?2",
            params![
                fingerprint.as_bytes().as_slice(),
                cell_id.as_bytes().as_slice(),
            ],
            |row| {
                Ok(CellRow {
                    cell_payload: row.get(0)?,
                    manifest_payload: row.get(1)?,
                    manifest_hash: row_digest(row, 2)?,
                    partition_root_hash: row_digest(row, 3)?,
                    membership_payload: row.get(4)?,
                    membership_hash: row_digest(row, 5)?,
                    status: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(DatabaseError::from)
}

fn select_partition(
    transaction: &Transaction<'_>,
    fingerprint: SemanticFingerprint,
    cell_id: RawRecordId,
    partition_id: RawRecordId,
) -> Result<Option<PartitionRow>, DatabaseError> {
    transaction
        .query_row(
            "SELECT partition_payload, manifest_payload, manifest_hash,
                    membership_payload, membership_hash
             FROM prewarm_partitions
             WHERE fingerprint=?1 AND cell_id=?2 AND partition_id=?3",
            params![
                fingerprint.as_bytes().as_slice(),
                cell_id.as_bytes().as_slice(),
                partition_id.as_bytes().as_slice(),
            ],
            |row| {
                Ok(PartitionRow {
                    partition_payload: row.get(0)?,
                    manifest_payload: row.get(1)?,
                    manifest_hash: row_digest(row, 2)?,
                    membership_payload: row.get(3)?,
                    membership_hash: row_digest(row, 4)?,
                })
            },
        )
        .optional()
        .map_err(DatabaseError::from)
}

fn row_digest(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<[u8; 32]> {
    let bytes: Vec<u8> = row.get(index)?;
    bytes.try_into().map_err(|bytes: Vec<u8>| {
        rusqlite::Error::FromSqlConversionFailure(
            bytes.len(),
            rusqlite::types::Type::Blob,
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "persistent digest is not 32 bytes",
            )),
        )
    })
}

fn ensure_cell_row(
    transaction: &Transaction<'_>,
    fingerprint: SemanticFingerprint,
    cell_id: RawRecordId,
    cell_payload: &[u8],
) -> Result<(), DatabaseError> {
    transaction.execute(
        "INSERT OR IGNORE INTO prewarm_cells
         (fingerprint, cell_id, cell_payload, manifest_payload, manifest_hash,
          partition_root_hash, membership_payload, membership_hash, status)
         VALUES (?1, ?2, ?3, X'', X'', X'', X'', X'', 0)",
        params![
            fingerprint.as_bytes().as_slice(),
            cell_id.as_bytes().as_slice(),
            cell_payload,
        ],
    )?;
    let stored: Vec<u8> = transaction.query_row(
        "SELECT cell_payload FROM prewarm_cells WHERE fingerprint=?1 AND cell_id=?2",
        params![
            fingerprint.as_bytes().as_slice(),
            cell_id.as_bytes().as_slice(),
        ],
        |row| row.get(0),
    )?;
    if stored != cell_payload {
        return Err(DatabaseError::ContentCollision);
    }
    Ok(())
}

fn quarantine_prewarm_row(
    transaction: &Transaction<'_>,
    source_table: &str,
    record_id: RawRecordId,
    fingerprint: SemanticFingerprint,
    payload: &[u8],
    reason: &str,
) -> Result<(), DatabaseError> {
    transaction.execute(
        "INSERT INTO quarantine(source_table, record_key, fingerprint, payload, reason)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            source_table,
            record_id.as_bytes().as_slice(),
            fingerprint.as_bytes().as_slice(),
            payload,
            reason,
        ],
    )?;
    Ok(())
}

fn decode_partition_row(
    fingerprint: SemanticFingerprint,
    expected_cell: ComponentCell,
    expected_partition: ComponentRootPartition,
    expected_id: RawRecordId,
    expected_partition_payload: &[u8],
    row: &PartitionRow,
) -> Result<StoredPartition, String> {
    if row.partition_payload != expected_partition_payload
        || decode_partition(&row.partition_payload)? != expected_partition
    {
        return Err("stored prewarm root partition does not match its index".to_owned());
    }
    let expected_id_again = prewarm_partition_id(
        fingerprint,
        &encode_cell(expected_cell),
        expected_partition_payload,
    );
    if expected_id != expected_id_again {
        return Err("stored prewarm partition id is not content-addressed".to_owned());
    }
    let manifest = decode_partition_manifest(&row.manifest_payload)?;
    if manifest.cell != expected_cell || manifest.partition != expected_partition {
        return Err("stored prewarm partition manifest has the wrong scope".to_owned());
    }
    let manifest_hash = prewarm_partition_manifest_hash(fingerprint, &manifest);
    if row.manifest_hash != manifest_hash {
        return Err("stored prewarm partition manifest digest does not match".to_owned());
    }
    let membership = decode_membership(&row.membership_payload)?;
    let membership_hash = prewarm_membership_hash(fingerprint, &row.membership_payload);
    if row.membership_hash != membership_hash {
        return Err("stored prewarm partition membership digest does not match".to_owned());
    }
    Ok(StoredPartition {
        id: expected_id,
        manifest,
        manifest_hash,
        membership,
        membership_hash,
    })
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn decode_complete_cell_row(
    fingerprint: SemanticFingerprint,
    expected_cell: ComponentCell,
    expected_cell_payload: &[u8],
    expected_partitions: &[ComponentRootPartition],
    transaction: &Transaction<'_>,
    cell_id: RawRecordId,
    row: &CellRow,
) -> Result<StoredCell, String> {
    if row.cell_payload != expected_cell_payload || decode_cell(&row.cell_payload)? != expected_cell
    {
        return Err("stored prewarm cell does not match its index".to_owned());
    }
    if prewarm_cell_id(fingerprint, &row.cell_payload) != cell_id {
        return Err("stored prewarm cell id is not content-addressed".to_owned());
    }
    let manifest = decode_cell_manifest(&row.manifest_payload)?;
    if manifest.cell != expected_cell {
        return Err("stored prewarm cell manifest has the wrong scope".to_owned());
    }
    if prewarm_cell_manifest_hash(fingerprint, &manifest) != row.manifest_hash {
        return Err("stored prewarm cell manifest digest does not match".to_owned());
    }

    let mut partitions = Vec::with_capacity(expected_partitions.len());
    for &partition in expected_partitions {
        let partition_payload = encode_partition(partition);
        let partition_id =
            prewarm_partition_id(fingerprint, expected_cell_payload, &partition_payload);
        let partition_row = select_partition(transaction, fingerprint, cell_id, partition_id)
            .map_err(display)?
            .ok_or_else(|| "complete prewarm cell is missing a root partition".to_owned())?;
        partitions.push(decode_partition_row(
            fingerprint,
            expected_cell,
            partition,
            partition_id,
            &partition_payload,
            &partition_row,
        )?);
    }
    let stored_partition_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM prewarm_partitions WHERE fingerprint=?1 AND cell_id=?2",
            params![
                fingerprint.as_bytes().as_slice(),
                cell_id.as_bytes().as_slice(),
            ],
            |query_row| query_row.get(0),
        )
        .map_err(display)?;
    if usize::try_from(stored_partition_count).ok() != Some(partitions.len()) {
        return Err("complete prewarm cell has an unexpected partition count".to_owned());
    }
    if prewarm_partition_root_hash(fingerprint, &partitions) != row.partition_root_hash {
        return Err("stored prewarm partition-root digest does not match".to_owned());
    }

    let topology_keys = partitions
        .iter()
        .flat_map(|partition| partition.manifest.canonical_topology_keys.iter().cloned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if manifest.canonical_topology_keys != topology_keys {
        return Err("cell topology manifest is not the exact partition union".to_owned());
    }
    let membership = decode_membership(&row.membership_payload)?;
    let expected_membership = partitions
        .iter()
        .flat_map(|partition| partition.membership.iter().copied())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    if membership != expected_membership
        || prewarm_membership_hash(fingerprint, &row.membership_payload) != row.membership_hash
    {
        return Err("cell component membership is not the exact partition union".to_owned());
    }
    let mut statement = transaction
        .prepare(
            "SELECT component_id FROM prewarm_membership
             WHERE fingerprint=?1 AND cell_id=?2 ORDER BY component_id",
        )
        .map_err(display)?;
    let rows = statement
        .query_map(
            params![
                fingerprint.as_bytes().as_slice(),
                cell_id.as_bytes().as_slice(),
            ],
            |query_row| query_row.get::<_, Vec<u8>>(0),
        )
        .map_err(display)?;
    let mut indexed_membership = Vec::new();
    for value in rows {
        let bytes: [u8; 32] = value
            .map_err(display)?
            .try_into()
            .map_err(|_| "indexed prewarm component id is not 32 bytes".to_owned())?;
        indexed_membership.push(RawRecordId::from_bytes(bytes));
    }
    drop(statement);
    if indexed_membership != membership {
        return Err("prewarm membership index disagrees with the cell manifest".to_owned());
    }
    let partition_count = u32::try_from(partitions.len())
        .map_err(|_| "prewarm partition count does not fit u32".to_owned())?;
    let component_count = u32::try_from(membership.len())
        .map_err(|_| "prewarm component count does not fit u32".to_owned())?;
    Ok(StoredCell {
        coverage: PrewarmCellCoverage {
            cell: expected_cell,
            manifest,
            partition_count,
            partition_root_digest: row.partition_root_hash,
            membership_digest: row.membership_hash,
            component_count,
        },
        partitions,
        membership,
    })
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use solver_reference::{
        ComponentCell as ReferenceCell, ComponentCellEnumeration as ReferenceEnumeration,
        enumerate_component_cell as enumerate_reference_cell,
    };

    use super::*;
    use crate::{ComponentDatabaseConfig, DatabaseMode};

    fn memory() -> ComponentDatabase {
        ComponentDatabase::open(&ComponentDatabaseConfig {
            mode: DatabaseMode::InMemory,
            ..ComponentDatabaseConfig::default()
        })
        .unwrap()
    }

    fn one_node_options() -> PrewarmOptions {
        PrewarmOptions {
            max_nodes: 1,
            max_boundary_ports: None,
            scheduling: PrewarmScheduling::Live,
        }
    }

    #[test]
    fn bounded_prewarm_is_reusable_and_matches_the_independent_oracle() {
        let database = memory();
        let first = database.prewarm(one_node_options());
        assert!(matches!(
            first,
            PrewarmResult::CompleteThrough {
                completed_cells: 4,
                reused_cells: 0,
                boundary_coverage: PrewarmBoundaryCoverage::AllWithinNodeBound,
                ..
            }
        ));
        let coverage = database.prewarm_coverage(one_node_options()).unwrap();
        assert_eq!(coverage.len(), 4);
        for stored in &coverage {
            let reference_cell = ReferenceCell::new(
                stored.cell.profile,
                stored.cell.boundary_input_count,
                stored.cell.boundary_output_count,
            )
            .unwrap();
            let reference =
                enumerate_reference_cell(reference_cell, &AtomicBool::new(false)).unwrap();
            let ReferenceEnumeration::Complete { manifest, .. } = reference else {
                panic!("uncancelled reference enumeration must complete");
            };
            assert_eq!(
                stored.manifest.canonical_topology_keys,
                manifest.canonical_topology_keys
            );
            assert_eq!(
                stored.manifest.root_digest_input(),
                manifest.root_digest_input()
            );
        }
        assert!(matches!(
            database.prewarm(one_node_options()),
            PrewarmResult::CompleteThrough {
                completed_cells: 4,
                reused_cells: 4,
                ..
            }
        ));
    }

    #[test]
    fn filtered_prewarm_never_claims_unrestricted_boundary_coverage() {
        let database = memory();
        let options = PrewarmOptions {
            max_nodes: 1,
            max_boundary_ports: Some(3),
            scheduling: PrewarmScheduling::Live,
        };
        assert!(matches!(
            database.prewarm(options),
            PrewarmResult::CompleteThrough {
                boundary_coverage: PrewarmBoundaryCoverage::AtMost(3),
                completed_cells: 2,
                ..
            }
        ));
    }

    #[test]
    fn cancellation_publishes_no_partial_cell_or_partition() {
        let database = memory();
        let cancel = AtomicBool::new(true);
        assert!(matches!(
            database.prewarm_cancellable(one_node_options(), &cancel),
            PrewarmResult::Partial {
                completed_cells: 0,
                reason: PrewarmPartialReason::Cancelled,
                ..
            }
        ));
        let connection = database.connection.as_ref().unwrap().lock().unwrap();
        let cells: u32 = connection
            .query_row("SELECT COUNT(*) FROM prewarm_cells", [], |row| row.get(0))
            .unwrap();
        let partitions: u32 = connection
            .query_row("SELECT COUNT(*) FROM prewarm_partitions", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!((cells, partitions), (0, 0));
    }

    #[test]
    fn corrupt_complete_cell_is_quarantined_and_becomes_a_cache_miss() {
        let database = memory();
        assert!(matches!(
            database.prewarm(one_node_options()),
            PrewarmResult::CompleteThrough { .. }
        ));
        let cell = database.prewarm_coverage(one_node_options()).unwrap()[0].cell;
        let cell_id = prewarm_cell_id(database.fingerprint(), &encode_cell(cell));
        database
            .connection
            .as_ref()
            .unwrap()
            .lock()
            .unwrap()
            .execute(
                "UPDATE prewarm_cells SET manifest_hash=?3
                 WHERE fingerprint=?1 AND cell_id=?2",
                params![
                    database.fingerprint().as_bytes().as_slice(),
                    cell_id.as_bytes().as_slice(),
                    [0_u8; 32].as_slice(),
                ],
            )
            .unwrap();
        let remaining = database.prewarm_coverage(one_node_options()).unwrap();
        assert_eq!(remaining.len(), 3);
        assert_eq!(database.status().unwrap().quarantined_records, 1);
    }

    #[test]
    fn completed_coverage_reopens_byte_for_byte() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("components.sqlite3");
        let config = ComponentDatabaseConfig {
            mode: DatabaseMode::Path(path),
            ..ComponentDatabaseConfig::default()
        };
        let expected = {
            let database = ComponentDatabase::open(&config).unwrap();
            assert!(matches!(
                database.prewarm(one_node_options()),
                PrewarmResult::CompleteThrough { .. }
            ));
            database.prewarm_coverage(one_node_options()).unwrap()
        };
        let reopened = ComponentDatabase::open(&config).unwrap();
        assert_eq!(
            reopened.prewarm_coverage(one_node_options()).unwrap(),
            expected
        );
        assert!(matches!(
            reopened.prewarm(one_node_options()),
            PrewarmResult::CompleteThrough {
                reused_cells: 4,
                ..
            }
        ));
    }
}

fn component_belongs_to_cell(component: &Component, cell: ComponentCell) -> bool {
    component.profile() == cell.profile
        && component.internal_link_count() == cell.internal_link_count
        && u32::try_from(component.boundary().input_count()).ok() == Some(cell.boundary_input_count)
        && u32::try_from(component.boundary().output_count()).ok()
            == Some(cell.boundary_output_count)
}

fn partial(
    options: PrewarmOptions,
    boundary_coverage: PrewarmBoundaryCoverage,
    completed_cells: u64,
    total_cells: u64,
    next_cell: Option<ComponentCell>,
    reason: PrewarmPartialReason,
) -> PrewarmResult {
    PrewarmResult::Partial {
        max_nodes: options.max_nodes,
        max_boundary_ports: options.max_boundary_ports,
        boundary_coverage,
        completed_cells,
        total_cells,
        next_cell,
        reason,
    }
}

impl ComponentDatabase {
    fn load_complete_prewarm_cell(
        &self,
        expected_cell: ComponentCell,
    ) -> Result<Option<StoredCell>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let cell_payload = encode_cell(expected_cell);
        let cell_id = prewarm_cell_id(self.fingerprint(), &cell_payload);
        let expected_partitions =
            component_partitions(expected_cell).map_err(|_| DatabaseError::InvalidPrewarmRecord)?;
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let row = select_cell(&transaction, self.fingerprint(), cell_id)?;
        let Some(row) = row else {
            transaction.commit()?;
            return Ok(None);
        };
        if row.status != 1 {
            transaction.commit()?;
            return Ok(None);
        }

        let decoded = decode_complete_cell_row(
            self.fingerprint(),
            expected_cell,
            &cell_payload,
            &expected_partitions,
            &transaction,
            cell_id,
            &row,
        );
        let stored = match decoded {
            Ok(stored) => Some(stored),
            Err(reason) => {
                quarantine_prewarm_row(
                    &transaction,
                    "prewarm_cells",
                    cell_id,
                    self.fingerprint(),
                    &row.cell_payload,
                    &reason,
                )?;
                transaction.execute(
                    "UPDATE prewarm_cells
                     SET manifest_payload=X'', manifest_hash=X'', partition_root_hash=X'',
                         membership_payload=X'', membership_hash=X'', status=0
                     WHERE fingerprint=?1 AND cell_id=?2",
                    params![
                        self.fingerprint().as_bytes().as_slice(),
                        cell_id.as_bytes().as_slice(),
                    ],
                )?;
                None
            }
        };
        transaction.commit()?;
        Ok(stored)
    }

    fn load_prewarm_partition(
        &self,
        cell: ComponentCell,
        partition: ComponentRootPartition,
    ) -> Result<Option<StoredPartition>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let cell_payload = encode_cell(cell);
        let cell_id = prewarm_cell_id(self.fingerprint(), &cell_payload);
        let partition_payload = encode_partition(partition);
        let partition_id =
            prewarm_partition_id(self.fingerprint(), &cell_payload, &partition_payload);
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let row = select_partition(&transaction, self.fingerprint(), cell_id, partition_id)?;
        let Some(row) = row else {
            transaction.commit()?;
            return Ok(None);
        };
        let stored = match decode_partition_row(
            self.fingerprint(),
            cell,
            partition,
            partition_id,
            &partition_payload,
            &row,
        ) {
            Ok(stored) => Some(stored),
            Err(reason) => {
                quarantine_prewarm_row(
                    &transaction,
                    "prewarm_partitions",
                    partition_id,
                    self.fingerprint(),
                    &row.manifest_payload,
                    &reason,
                )?;
                transaction.execute(
                    "DELETE FROM prewarm_partitions
                     WHERE fingerprint=?1 AND cell_id=?2 AND partition_id=?3",
                    params![
                        self.fingerprint().as_bytes().as_slice(),
                        cell_id.as_bytes().as_slice(),
                        partition_id.as_bytes().as_slice(),
                    ],
                )?;
                transaction.execute(
                    "UPDATE prewarm_cells
                     SET manifest_payload=X'', manifest_hash=X'', partition_root_hash=X'',
                         membership_payload=X'', membership_hash=X'', status=0
                     WHERE fingerprint=?1 AND cell_id=?2",
                    params![
                        self.fingerprint().as_bytes().as_slice(),
                        cell_id.as_bytes().as_slice(),
                    ],
                )?;
                None
            }
        };
        transaction.commit()?;
        Ok(stored)
    }

    fn invalidate_prewarm_partition(
        &self,
        cell: ComponentCell,
        partition: ComponentRootPartition,
    ) -> Result<(), DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(());
        };
        let cell_payload = encode_cell(cell);
        let cell_id = prewarm_cell_id(self.fingerprint(), &cell_payload);
        let partition_payload = encode_partition(partition);
        let partition_id =
            prewarm_partition_id(self.fingerprint(), &cell_payload, &partition_payload);
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        if let Some(row) =
            select_partition(&transaction, self.fingerprint(), cell_id, partition_id)?
        {
            quarantine_prewarm_row(
                &transaction,
                "prewarm_partitions",
                partition_id,
                self.fingerprint(),
                &row.manifest_payload,
                "partition references an invalid component record",
            )?;
            transaction.execute(
                "DELETE FROM prewarm_partitions
                 WHERE fingerprint=?1 AND cell_id=?2 AND partition_id=?3",
                params![
                    self.fingerprint().as_bytes().as_slice(),
                    cell_id.as_bytes().as_slice(),
                    partition_id.as_bytes().as_slice(),
                ],
            )?;
        }
        transaction.execute(
            "UPDATE prewarm_cells
             SET manifest_payload=X'', manifest_hash=X'', partition_root_hash=X'',
                 membership_payload=X'', membership_hash=X'', status=0
             WHERE fingerprint=?1 AND cell_id=?2",
            params![
                self.fingerprint().as_bytes().as_slice(),
                cell_id.as_bytes().as_slice(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    fn commit_prewarm_partition(
        &self,
        manifest: &ComponentPartitionManifest,
        membership: &[RawRecordId],
    ) -> Result<StoredPartition, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Err(DatabaseError::DatabaseDisabled);
        };
        if !strictly_sorted_unique_keys(&manifest.canonical_topology_keys)
            || !strictly_sorted_unique(membership)
        {
            return Err(DatabaseError::InvalidPrewarmRecord);
        }
        let cell_payload = encode_cell(manifest.cell);
        let cell_id = prewarm_cell_id(self.fingerprint(), &cell_payload);
        let partition_payload = encode_partition(manifest.partition);
        let partition_id =
            prewarm_partition_id(self.fingerprint(), &cell_payload, &partition_payload);
        let manifest_payload = encode_partition_manifest(manifest);
        let manifest_hash = prewarm_partition_manifest_hash(self.fingerprint(), manifest);
        let membership_payload = encode_membership(membership);
        let membership_hash = prewarm_membership_hash(self.fingerprint(), &membership_payload);
        let desired = StoredPartition {
            id: partition_id,
            manifest: manifest.clone(),
            manifest_hash,
            membership: membership.to_vec(),
            membership_hash,
        };

        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        ensure_cell_row(&transaction, self.fingerprint(), cell_id, &cell_payload)?;
        let existing = select_partition(&transaction, self.fingerprint(), cell_id, partition_id)?;
        let identical = existing.as_ref().is_some_and(|row| {
            row.partition_payload == partition_payload
                && row.manifest_payload == manifest_payload
                && row.manifest_hash == manifest_hash
                && row.membership_payload == membership_payload
                && row.membership_hash == membership_hash
        });
        if !identical {
            if let Some(existing) = existing {
                quarantine_prewarm_row(
                    &transaction,
                    "prewarm_partitions",
                    partition_id,
                    self.fingerprint(),
                    &existing.manifest_payload,
                    "partition bytes disagree with deterministic exhaustive enumeration",
                )?;
                transaction.execute(
                    "DELETE FROM prewarm_partitions
                     WHERE fingerprint=?1 AND cell_id=?2 AND partition_id=?3",
                    params![
                        self.fingerprint().as_bytes().as_slice(),
                        cell_id.as_bytes().as_slice(),
                        partition_id.as_bytes().as_slice(),
                    ],
                )?;
            }
            transaction.execute(
                "INSERT INTO prewarm_partitions
                 (fingerprint, cell_id, partition_id, partition_payload,
                  manifest_payload, manifest_hash, membership_payload, membership_hash)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    self.fingerprint().as_bytes().as_slice(),
                    cell_id.as_bytes().as_slice(),
                    partition_id.as_bytes().as_slice(),
                    &partition_payload,
                    &manifest_payload,
                    manifest_hash.as_slice(),
                    &membership_payload,
                    membership_hash.as_slice(),
                ],
            )?;
            transaction.execute(
                "UPDATE prewarm_cells
                 SET manifest_payload=X'', manifest_hash=X'', partition_root_hash=X'',
                     membership_payload=X'', membership_hash=X'', status=0
                 WHERE fingerprint=?1 AND cell_id=?2",
                params![
                    self.fingerprint().as_bytes().as_slice(),
                    cell_id.as_bytes().as_slice(),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(desired)
    }

    #[allow(clippy::too_many_lines)]
    fn finalize_prewarm_cell(
        &self,
        cell: ComponentCell,
        partitions: &[StoredPartition],
    ) -> Result<(), DatabaseError> {
        let Some(connection) = &self.connection else {
            return Err(DatabaseError::DatabaseDisabled);
        };
        let expected_partitions =
            component_partitions(cell).map_err(|_| DatabaseError::InvalidPrewarmRecord)?;
        if partitions.len() != expected_partitions.len()
            || partitions
                .iter()
                .zip(&expected_partitions)
                .any(|(stored, expected)| {
                    stored.manifest.cell != cell || stored.manifest.partition != *expected
                })
        {
            return Err(DatabaseError::InvalidPrewarmRecord);
        }
        let topology_keys = partitions
            .iter()
            .flat_map(|partition| partition.manifest.canonical_topology_keys.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let manifest = ComponentCellManifest {
            cell,
            canonical_topology_keys: topology_keys,
        };
        let membership = partitions
            .iter()
            .flat_map(|partition| partition.membership.iter().copied())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let cell_payload = encode_cell(cell);
        let cell_id = prewarm_cell_id(self.fingerprint(), &cell_payload);
        let manifest_payload = encode_cell_manifest(&manifest);
        let manifest_hash = prewarm_cell_manifest_hash(self.fingerprint(), &manifest);
        let partition_root_hash = prewarm_partition_root_hash(self.fingerprint(), partitions);
        let membership_payload = encode_membership(&membership);
        let membership_hash = prewarm_membership_hash(self.fingerprint(), &membership_payload);

        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        ensure_cell_row(&transaction, self.fingerprint(), cell_id, &cell_payload)?;
        for partition in partitions {
            let Some(row) =
                select_partition(&transaction, self.fingerprint(), cell_id, partition.id)?
            else {
                return Err(DatabaseError::InvalidPrewarmRecord);
            };
            if row.partition_payload != encode_partition(partition.manifest.partition)
                || row.manifest_payload != encode_partition_manifest(&partition.manifest)
                || row.manifest_hash != partition.manifest_hash
                || row.membership_payload != encode_membership(&partition.membership)
                || row.membership_hash != partition.membership_hash
            {
                return Err(DatabaseError::InvalidPrewarmRecord);
            }
        }
        let stored_partition_count: i64 = transaction.query_row(
            "SELECT COUNT(*) FROM prewarm_partitions WHERE fingerprint=?1 AND cell_id=?2",
            params![
                self.fingerprint().as_bytes().as_slice(),
                cell_id.as_bytes().as_slice(),
            ],
            |row| row.get(0),
        )?;
        if usize::try_from(stored_partition_count).ok() != Some(partitions.len()) {
            return Err(DatabaseError::InvalidPrewarmRecord);
        }
        for id in &membership {
            let exists: bool = transaction.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM component_records
                    WHERE fingerprint=?1 AND component_id=?2
                 )",
                params![
                    self.fingerprint().as_bytes().as_slice(),
                    id.as_bytes().as_slice(),
                ],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(DatabaseError::InvalidPrewarmRecord);
            }
        }
        transaction.execute(
            "DELETE FROM prewarm_membership WHERE fingerprint=?1 AND cell_id=?2",
            params![
                self.fingerprint().as_bytes().as_slice(),
                cell_id.as_bytes().as_slice(),
            ],
        )?;
        for id in &membership {
            transaction.execute(
                "INSERT INTO prewarm_membership(fingerprint, cell_id, component_id)
                 VALUES (?1, ?2, ?3)",
                params![
                    self.fingerprint().as_bytes().as_slice(),
                    cell_id.as_bytes().as_slice(),
                    id.as_bytes().as_slice(),
                ],
            )?;
        }
        transaction.execute(
            "UPDATE prewarm_cells
             SET manifest_payload=?3, manifest_hash=?4, partition_root_hash=?5,
                 membership_payload=?6, membership_hash=?7, status=1
             WHERE fingerprint=?1 AND cell_id=?2 AND cell_payload=?8",
            params![
                self.fingerprint().as_bytes().as_slice(),
                cell_id.as_bytes().as_slice(),
                &manifest_payload,
                manifest_hash.as_slice(),
                partition_root_hash.as_slice(),
                &membership_payload,
                membership_hash.as_slice(),
                &cell_payload,
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }
}
