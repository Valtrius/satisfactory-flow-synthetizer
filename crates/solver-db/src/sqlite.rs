use std::{path::PathBuf, sync::Mutex, time::Duration};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{SemanticFingerprint, content_id};

const SQL_SCHEMA_VERSION: u32 = 4;

const SCHEMA: &str = r"
CREATE TABLE IF NOT EXISTS schema_meta (
    key TEXT PRIMARY KEY,
    value BLOB NOT NULL
) STRICT;

CREATE TABLE IF NOT EXISTS component_records (
    fingerprint BLOB NOT NULL,
    component_id BLOB NOT NULL,
    behavior_id BLOB NOT NULL,
    payload BLOB NOT NULL,
    payload_hash BLOB NOT NULL,
    proof_payload BLOB NOT NULL,
    PRIMARY KEY (fingerprint, component_id)
) STRICT;

CREATE INDEX IF NOT EXISTS component_behavior_lookup
ON component_records(fingerprint, behavior_id, component_id);

CREATE TABLE IF NOT EXISTS dominance_edges (
    fingerprint BLOB NOT NULL,
    dominator_id BLOB NOT NULL,
    dominated_id BLOB NOT NULL,
    proof_payload BLOB NOT NULL,
    PRIMARY KEY (fingerprint, dominator_id, dominated_id),
    FOREIGN KEY (fingerprint, dominator_id)
        REFERENCES component_records(fingerprint, component_id) ON DELETE CASCADE,
    FOREIGN KEY (fingerprint, dominated_id)
        REFERENCES component_records(fingerprint, component_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS symbolic_frontier (
    fingerprint BLOB NOT NULL,
    behavior_id BLOB NOT NULL,
    component_id BLOB NOT NULL,
    PRIMARY KEY (fingerprint, behavior_id, component_id),
    FOREIGN KEY (fingerprint, component_id)
        REFERENCES component_records(fingerprint, component_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS application_records (
    fingerprint BLOB NOT NULL,
    application_id BLOB NOT NULL,
    key_payload BLOB NOT NULL,
    key_hash BLOB NOT NULL,
    winner_id BLOB NOT NULL,
    node_count INTEGER NOT NULL CHECK (node_count >= 0),
    internal_link_count INTEGER NOT NULL CHECK (internal_link_count >= 0),
    proof_payload BLOB NOT NULL,
    PRIMARY KEY (fingerprint, application_id),
    FOREIGN KEY (fingerprint, winner_id)
        REFERENCES component_records(fingerprint, component_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS concrete_index (
    fingerprint BLOB NOT NULL,
    pattern_id BLOB NOT NULL,
    application_id BLOB NOT NULL,
    PRIMARY KEY (fingerprint, pattern_id),
    FOREIGN KEY (fingerprint, application_id)
        REFERENCES application_records(fingerprint, application_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS prewarm_cells (
    fingerprint BLOB NOT NULL,
    cell_id BLOB NOT NULL,
    cell_payload BLOB NOT NULL,
    manifest_payload BLOB NOT NULL,
    manifest_hash BLOB NOT NULL,
    partition_root_hash BLOB NOT NULL,
    membership_payload BLOB NOT NULL,
    membership_hash BLOB NOT NULL,
    status INTEGER NOT NULL CHECK (status IN (0, 1)),
    PRIMARY KEY (fingerprint, cell_id)
) STRICT;

CREATE TABLE IF NOT EXISTS prewarm_partitions (
    fingerprint BLOB NOT NULL,
    cell_id BLOB NOT NULL,
    partition_id BLOB NOT NULL,
    partition_payload BLOB NOT NULL,
    manifest_payload BLOB NOT NULL,
    manifest_hash BLOB NOT NULL,
    membership_payload BLOB NOT NULL,
    membership_hash BLOB NOT NULL,
    PRIMARY KEY (fingerprint, cell_id, partition_id),
    FOREIGN KEY (fingerprint, cell_id)
        REFERENCES prewarm_cells(fingerprint, cell_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS prewarm_membership (
    fingerprint BLOB NOT NULL,
    cell_id BLOB NOT NULL,
    component_id BLOB NOT NULL,
    PRIMARY KEY (fingerprint, cell_id, component_id),
    FOREIGN KEY (fingerprint, cell_id)
        REFERENCES prewarm_cells(fingerprint, cell_id) ON DELETE CASCADE,
    FOREIGN KEY (fingerprint, component_id)
        REFERENCES component_records(fingerprint, component_id) ON DELETE CASCADE
) STRICT;

CREATE TABLE IF NOT EXISTS quarantine (
    quarantine_id INTEGER PRIMARY KEY,
    source_table TEXT NOT NULL,
    record_key BLOB NOT NULL,
    fingerprint BLOB NOT NULL,
    payload BLOB NOT NULL,
    reason TEXT NOT NULL,
    quarantined_at INTEGER NOT NULL DEFAULT (unixepoch())
) STRICT;
";

/// Stable content identifier used by the database API.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawRecordId([u8; 32]);

impl RawRecordId {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Where the optional accelerator database is stored.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum DatabaseMode {
    /// Persistence and lookup are disabled. Primitive solving remains complete.
    #[default]
    Disabled,
    /// One process-local `SQLite` database, primarily for tests.
    InMemory,
    /// Explicit persistent `SQLite` path.
    Path(PathBuf),
}

/// `SQLite` configuration with no implicit application path policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentDatabaseConfig {
    pub mode: DatabaseMode,
    pub busy_timeout: Duration,
}

impl Default for ComponentDatabaseConfig {
    fn default() -> Self {
        Self {
            mode: DatabaseMode::Disabled,
            busy_timeout: Duration::from_secs(5),
        }
    }
}

/// Untrusted raw component bytes returned in deterministic content-ID order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentRecordBlob {
    pub id: RawRecordId,
    pub behavior_id: RawRecordId,
    pub payload: Vec<u8>,
    pub proof_payload: Vec<u8>,
}

/// One concrete application-optimal proof row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ApplicationRecord {
    pub id: RawRecordId,
    pub key_payload: Vec<u8>,
    pub winner_id: RawRecordId,
    pub node_count: u32,
    pub internal_link_count: u32,
    pub proof_payload: Vec<u8>,
}

/// One persisted exact symbolic dominance certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DominanceRecord {
    pub dominator_id: RawRecordId,
    pub dominated_id: RawRecordId,
    pub proof_payload: Vec<u8>,
}

/// Read-only diagnostics; none of these counts carries a completeness claim.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ComponentDatabaseStatus {
    pub enabled: bool,
    pub compatible_component_records: u64,
    pub incompatible_component_records: u64,
    pub application_records: u64,
    pub quarantined_records: u64,
}

/// SQLite/record-layer failure. Callers must treat this as an accelerator miss.
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("SQLite component accelerator failed: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("database record identifier has length {actual}, expected 32")]
    InvalidIdLength { actual: usize },
    #[error("content-addressed row exists with different bytes")]
    ContentCollision,
    #[error("application winner does not exist in the compatible component namespace")]
    MissingApplicationWinner,
    #[error("application result does not carry a valid exact proof summary")]
    InvalidApplicationProof,
    #[error("component frontier does not carry a valid exact dominance proof")]
    InvalidDominanceProof,
    #[error("bounded prewarm record is malformed or internally inconsistent")]
    InvalidPrewarmRecord,
    #[error("component database is disabled")]
    DatabaseDisabled,
    #[error("public database count cannot fit u32")]
    CountOverflow,
    #[error("database mutex is poisoned")]
    LockPoisoned,
}

/// Thread-safe `SQLite` accelerator scoped to one semantic fingerprint.
pub struct ComponentDatabase {
    pub(crate) connection: Option<Mutex<Connection>>,
    fingerprint: SemanticFingerprint,
}

impl ComponentDatabase {
    /// Opens or disables the accelerator according to an explicit configuration.
    ///
    /// # Errors
    ///
    /// Returns an operational or schema-version database error. Callers must
    /// fall back to a disabled accelerator without changing solver semantics.
    pub fn open(config: &ComponentDatabaseConfig) -> Result<Self, DatabaseError> {
        let Some(connection) = (match &config.mode {
            DatabaseMode::Disabled => None,
            DatabaseMode::InMemory => Some(Connection::open_in_memory()?),
            DatabaseMode::Path(path) => Some(Connection::open(path)?),
        }) else {
            return Ok(Self {
                connection: None,
                fingerprint: SemanticFingerprint::current(),
            });
        };

        connection.busy_timeout(config.busy_timeout)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        connection.pragma_update(None, "synchronous", "FULL")?;
        if matches!(config.mode, DatabaseMode::Path(_)) {
            connection.pragma_update(None, "journal_mode", "WAL")?;
        }
        connection.execute_batch(SCHEMA)?;
        let version =
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))?;
        if version == 0 {
            connection.pragma_update(None, "user_version", SQL_SCHEMA_VERSION)?;
        } else if version != SQL_SCHEMA_VERSION {
            return Err(DatabaseError::Sqlite(rusqlite::Error::InvalidQuery));
        }
        connection.execute(
            "INSERT OR IGNORE INTO schema_meta(key, value) VALUES ('sql_schema_version', ?1)",
            [SQL_SCHEMA_VERSION.to_be_bytes().as_slice()],
        )?;

        Ok(Self {
            connection: Some(Mutex::new(connection)),
            fingerprint: SemanticFingerprint::current(),
        })
    }

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            connection: None,
            fingerprint: SemanticFingerprint::current(),
        }
    }

    #[must_use]
    pub const fn is_enabled(&self) -> bool {
        self.connection.is_some()
    }

    #[must_use]
    pub const fn fingerprint(&self) -> SemanticFingerprint {
        self.fingerprint
    }

    pub(crate) fn behavior_record_id(&self, payload: &[u8]) -> RawRecordId {
        RawRecordId(content_id(self.fingerprint, b"component-behavior", payload))
    }

    pub(crate) fn quarantine_component_blob(
        &self,
        id: RawRecordId,
        reason: &str,
    ) -> Result<(), DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(());
        };
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let payload = select_component_unchecked(&transaction, self.fingerprint, id)?
            .map_or_else(Vec::new, |row| row.payload);
        quarantine_component(&transaction, self.fingerprint, id, &payload, reason)?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn quarantine_application_record(
        &self,
        id: RawRecordId,
        reason: &str,
    ) -> Result<(), DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(());
        };
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let payload = select_application_unchecked(&transaction, self.fingerprint, id)?
            .map_or_else(Vec::new, |row| row.key_payload);
        quarantine_application(&transaction, self.fingerprint, id, &payload, reason)?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn quarantine_dominance_record(
        &self,
        dominator_id: RawRecordId,
        dominated_id: RawRecordId,
        reason: &str,
    ) -> Result<(), DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(());
        };
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let proof_payload = transaction
            .query_row(
                "SELECT proof_payload FROM dominance_edges
                 WHERE fingerprint=?1 AND dominator_id=?2 AND dominated_id=?3",
                params![
                    self.fingerprint.as_bytes().as_slice(),
                    dominator_id.0.as_slice(),
                    dominated_id.0.as_slice(),
                ],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
            .unwrap_or_default();
        let mut record_payload = Vec::with_capacity(64 + proof_payload.len());
        record_payload.extend_from_slice(&dominator_id.0);
        record_payload.extend_from_slice(&dominated_id.0);
        record_payload.extend_from_slice(&proof_payload);
        let record_id = RawRecordId(content_id(
            self.fingerprint,
            b"dominance-record",
            &record_payload,
        ));
        quarantine(
            &transaction,
            "dominance_edges",
            record_id,
            self.fingerprint,
            &record_payload,
            reason,
        )?;
        transaction.execute(
            "DELETE FROM dominance_edges
             WHERE fingerprint=?1 AND dominator_id=?2 AND dominated_id=?3",
            params![
                self.fingerprint.as_bytes().as_slice(),
                dominator_id.0.as_slice(),
                dominated_id.0.as_slice(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    /// Transactionally inserts one immutable component record and verifies any
    /// pre-existing content-addressed row byte-for-byte.
    ///
    /// # Errors
    ///
    /// Returns an operational, collision, or poisoned-lock database error.
    pub fn store_component_blob(
        &self,
        behavior_payload: &[u8],
        payload: &[u8],
        proof_payload: &[u8],
    ) -> Result<Option<RawRecordId>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let component_id = RawRecordId(content_id(self.fingerprint, b"component", payload));
        let behavior_id = RawRecordId(content_id(
            self.fingerprint,
            b"component-behavior",
            behavior_payload,
        ));
        let payload_hash = raw_hash(payload);
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO component_records
             (fingerprint, component_id, behavior_id, payload, payload_hash, proof_payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                self.fingerprint.as_bytes().as_slice(),
                component_id.0.as_slice(),
                behavior_id.0.as_slice(),
                payload,
                payload_hash.as_slice(),
                proof_payload,
            ],
        )?;
        let stored = select_component_unchecked(&transaction, self.fingerprint, component_id)?
            .ok_or(DatabaseError::ContentCollision)?;
        if stored.behavior_id != behavior_id.0
            || stored.payload != payload
            || stored.payload_hash != payload_hash
            || stored.proof_payload != proof_payload
        {
            return Err(DatabaseError::ContentCollision);
        }
        transaction.execute(
            "INSERT OR IGNORE INTO symbolic_frontier
             (fingerprint, behavior_id, component_id) VALUES (?1, ?2, ?3)",
            params![
                self.fingerprint.as_bytes().as_slice(),
                behavior_id.0.as_slice(),
                component_id.0.as_slice(),
            ],
        )?;
        transaction.commit()?;
        Ok(Some(component_id))
    }

    /// Loads one raw row after checking its hash and content address. Invalid
    /// rows are quarantined and reported as cache misses.
    ///
    /// # Errors
    ///
    /// Returns only operational database/quarantine failures.
    pub fn load_component_blob(
        &self,
        id: RawRecordId,
    ) -> Result<Option<ComponentRecordBlob>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let row = select_component_unchecked(&transaction, self.fingerprint, id)?;
        let Some(row) = row else {
            transaction.commit()?;
            return Ok(None);
        };
        if row.payload_hash != raw_hash(&row.payload)
            || id.0 != content_id(self.fingerprint, b"component", &row.payload)
            || row.behavior_id.len() != 32
        {
            quarantine_component(
                &transaction,
                self.fingerprint,
                id,
                &row.payload,
                "component hash, content address, or behavior identifier is invalid",
            )?;
            transaction.commit()?;
            return Ok(None);
        }
        let mut behavior_id = [0_u8; 32];
        behavior_id.copy_from_slice(&row.behavior_id);
        let record = ComponentRecordBlob {
            id,
            behavior_id: RawRecordId(behavior_id),
            payload: row.payload,
            proof_payload: row.proof_payload,
        };
        transaction.commit()?;
        Ok(Some(record))
    }

    /// Returns compatible raw components in deterministic content-ID order.
    ///
    /// # Errors
    ///
    /// Returns an operational database error or malformed identifier error.
    pub fn component_blobs(&self) -> Result<Vec<ComponentRecordBlob>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let ids = {
            let connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
            let mut statement = connection.prepare(
                "SELECT component_id FROM component_records
                 WHERE fingerprint=?1 ORDER BY component_id",
            )?;
            statement
                .query_map([self.fingerprint.as_bytes().as_slice()], |row| {
                    row.get::<_, Vec<u8>>(0)
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        let mut records = Vec::new();
        for bytes in ids {
            let id = RawRecordId(to_id(&bytes)?);
            if let Some(record) = self.load_component_blob(id)? {
                records.push(record);
            }
        }
        Ok(records)
    }

    pub(crate) fn component_ids_for_behavior(
        &self,
        behavior_id: RawRecordId,
    ) -> Result<Vec<RawRecordId>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT component_id FROM component_records
             WHERE fingerprint=?1 AND behavior_id=?2 ORDER BY component_id",
        )?;
        statement
            .query_map(
                params![
                    self.fingerprint.as_bytes().as_slice(),
                    behavior_id.0.as_slice(),
                ],
                |row| row.get::<_, Vec<u8>>(0),
            )?
            .map(|bytes| {
                let bytes = bytes?;
                Ok(RawRecordId(to_id(&bytes)?))
            })
            .collect()
    }

    pub(crate) fn symbolic_frontier_ids(&self) -> Result<Vec<RawRecordId>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT component_id FROM symbolic_frontier
             WHERE fingerprint=?1 ORDER BY component_id",
        )?;
        statement
            .query_map([self.fingerprint.as_bytes().as_slice()], |row| {
                row.get::<_, Vec<u8>>(0)
            })?
            .map(|bytes| {
                let bytes = bytes?;
                Ok(RawRecordId(to_id(&bytes)?))
            })
            .collect()
    }

    pub(crate) fn dominance_records(&self) -> Result<Vec<DominanceRecord>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(Vec::new());
        };
        let connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT dominator_id, dominated_id, proof_payload FROM dominance_edges
             WHERE fingerprint=?1 ORDER BY dominator_id, dominated_id",
        )?;
        statement
            .query_map([self.fingerprint.as_bytes().as_slice()], |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                ))
            })?
            .map(|row| {
                let (dominator, dominated, proof_payload) = row?;
                Ok(DominanceRecord {
                    dominator_id: RawRecordId(to_id(&dominator)?),
                    dominated_id: RawRecordId(to_id(&dominated)?),
                    proof_payload,
                })
            })
            .collect()
    }

    pub(crate) fn replace_symbolic_frontier(
        &self,
        behavior_id: RawRecordId,
        retained_ids: &[RawRecordId],
        proofs: &[DominanceRecord],
    ) -> Result<(), DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(());
        };
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        for id in retained_ids {
            let stored_behavior = transaction
                .query_row(
                    "SELECT behavior_id FROM component_records
                     WHERE fingerprint=?1 AND component_id=?2",
                    params![self.fingerprint.as_bytes().as_slice(), id.0.as_slice(),],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?;
            if stored_behavior.as_deref() != Some(behavior_id.0.as_slice()) {
                return Err(DatabaseError::MissingApplicationWinner);
            }
        }
        for proof in proofs {
            if !component_exists(&transaction, self.fingerprint, proof.dominator_id)?
                || !component_exists(&transaction, self.fingerprint, proof.dominated_id)?
            {
                return Err(DatabaseError::MissingApplicationWinner);
            }
            transaction.execute(
                "INSERT INTO dominance_edges
                 (fingerprint, dominator_id, dominated_id, proof_payload)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(fingerprint, dominator_id, dominated_id)
                 DO UPDATE SET proof_payload=excluded.proof_payload",
                params![
                    self.fingerprint.as_bytes().as_slice(),
                    proof.dominator_id.0.as_slice(),
                    proof.dominated_id.0.as_slice(),
                    &proof.proof_payload,
                ],
            )?;
        }
        transaction.execute(
            "DELETE FROM symbolic_frontier WHERE fingerprint=?1 AND behavior_id=?2",
            params![
                self.fingerprint.as_bytes().as_slice(),
                behavior_id.0.as_slice(),
            ],
        )?;
        for id in retained_ids {
            transaction.execute(
                "INSERT INTO symbolic_frontier
                 (fingerprint, behavior_id, component_id) VALUES (?1, ?2, ?3)",
                params![
                    self.fingerprint.as_bytes().as_slice(),
                    behavior_id.0.as_slice(),
                    id.0.as_slice(),
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    /// Stores a proof-complete concrete application and its derived common-scale index atomically.
    ///
    /// # Errors
    ///
    /// Returns an operational error, content collision, or missing-winner error.
    pub fn store_application(
        &self,
        key_payload: &[u8],
        winner_id: RawRecordId,
        node_count: u32,
        internal_link_count: u32,
        proof_payload: &[u8],
    ) -> Result<Option<RawRecordId>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let application_id = RawRecordId(content_id(
            self.fingerprint,
            b"application-record",
            &application_address_payload(
                key_payload,
                winner_id,
                node_count,
                internal_link_count,
                proof_payload,
            ),
        ));
        let pattern_id = content_id(self.fingerprint, b"concrete-pattern", key_payload);
        let key_hash = raw_hash(key_payload);
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        if !component_exists(&transaction, self.fingerprint, winner_id)? {
            return Err(DatabaseError::MissingApplicationWinner);
        }
        transaction.execute(
            "INSERT OR IGNORE INTO application_records
             (fingerprint, application_id, key_payload, key_hash, winner_id,
              node_count, internal_link_count, proof_payload)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                self.fingerprint.as_bytes().as_slice(),
                application_id.0.as_slice(),
                key_payload,
                key_hash.as_slice(),
                winner_id.0.as_slice(),
                i64::from(node_count),
                i64::from(internal_link_count),
                proof_payload,
            ],
        )?;
        let stored = select_application_unchecked(&transaction, self.fingerprint, application_id)?
            .ok_or(DatabaseError::ContentCollision)?;
        if stored.key_payload != key_payload
            || stored.key_hash != key_hash
            || stored.winner_id != winner_id.0
            || stored.node_count != i64::from(node_count)
            || stored.internal_link_count != i64::from(internal_link_count)
            || stored.proof_payload != proof_payload
        {
            return Err(DatabaseError::ContentCollision);
        }
        transaction.execute(
            "INSERT OR IGNORE INTO concrete_index
             (fingerprint, pattern_id, application_id) VALUES (?1, ?2, ?3)",
            params![
                self.fingerprint.as_bytes().as_slice(),
                pattern_id.as_slice(),
                application_id.0.as_slice(),
            ],
        )?;
        let indexed: Vec<u8> = transaction.query_row(
            "SELECT application_id FROM concrete_index
             WHERE fingerprint=?1 AND pattern_id=?2",
            params![
                self.fingerprint.as_bytes().as_slice(),
                pattern_id.as_slice()
            ],
            |row| row.get(0),
        )?;
        if to_id(&indexed)? != application_id.0 {
            return Err(DatabaseError::ContentCollision);
        }
        transaction.commit()?;
        Ok(Some(application_id))
    }

    /// Looks up the exact concrete key. Hash/proof/component verification above
    /// the raw layer remains mandatory before a macro is returned.
    ///
    /// # Errors
    ///
    /// Returns only operational database/quarantine failures or a malformed
    /// identifier error.
    pub fn lookup_application(
        &self,
        key_payload: &[u8],
    ) -> Result<Option<ApplicationRecord>, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(None);
        };
        let pattern_id = content_id(self.fingerprint, b"concrete-pattern", key_payload);
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let indexed =
            select_indexed_application(&transaction, self.fingerprint, pattern_id, key_payload)?;
        let Some((application_id, row)) = indexed else {
            transaction.commit()?;
            return Ok(None);
        };
        let (Ok(winner_bytes), Ok(node_count), Ok(internal_link_count)) = (
            to_id(&row.winner_id),
            u32::try_from(row.node_count),
            u32::try_from(row.internal_link_count),
        ) else {
            quarantine_application(
                &transaction,
                self.fingerprint,
                application_id,
                &row.key_payload,
                "application winner identifier or cost is invalid",
            )?;
            transaction.commit()?;
            return Ok(None);
        };
        let winner_id = RawRecordId(winner_bytes);
        let expected_application_id = content_id(
            self.fingerprint,
            b"application-record",
            &application_address_payload(
                &row.key_payload,
                winner_id,
                node_count,
                internal_link_count,
                &row.proof_payload,
            ),
        );
        let valid = row.key_payload == key_payload
            && row.key_hash == raw_hash(&row.key_payload)
            && application_id.0 == expected_application_id
            && component_exists(&transaction, self.fingerprint, winner_id)?;
        if !valid {
            quarantine_application(
                &transaction,
                self.fingerprint,
                application_id,
                &row.key_payload,
                "application key, content address, or winner is invalid",
            )?;
            transaction.commit()?;
            return Ok(None);
        }
        let record = ApplicationRecord {
            id: application_id,
            key_payload: row.key_payload,
            winner_id,
            node_count,
            internal_link_count,
            proof_payload: row.proof_payload,
        };
        transaction.commit()?;
        Ok(Some(record))
    }

    /// Persists a re-verifiable dominance proof between two existing components.
    ///
    /// # Errors
    ///
    /// Returns an operational error, content collision, or missing-component error.
    pub fn store_dominance(&self, record: &DominanceRecord) -> Result<(), DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(());
        };
        let connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        if !component_exists(&connection, self.fingerprint, record.dominator_id)?
            || !component_exists(&connection, self.fingerprint, record.dominated_id)?
        {
            return Err(DatabaseError::MissingApplicationWinner);
        }
        connection.execute(
            "INSERT OR IGNORE INTO dominance_edges
             (fingerprint, dominator_id, dominated_id, proof_payload)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                self.fingerprint.as_bytes().as_slice(),
                record.dominator_id.0.as_slice(),
                record.dominated_id.0.as_slice(),
                &record.proof_payload,
            ],
        )?;
        let stored: Vec<u8> = connection.query_row(
            "SELECT proof_payload FROM dominance_edges
             WHERE fingerprint=?1 AND dominator_id=?2 AND dominated_id=?3",
            params![
                self.fingerprint.as_bytes().as_slice(),
                record.dominator_id.0.as_slice(),
                record.dominated_id.0.as_slice(),
            ],
            |row| row.get(0),
        )?;
        if stored != record.proof_payload {
            return Err(DatabaseError::ContentCollision);
        }
        Ok(())
    }

    /// Returns diagnostic record counts for this semantic fingerprint.
    ///
    /// # Errors
    ///
    /// Returns an operational database or count-conversion error.
    pub fn status(&self) -> Result<ComponentDatabaseStatus, DatabaseError> {
        let Some(connection) = &self.connection else {
            return Ok(ComponentDatabaseStatus::default());
        };
        let connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let compatible = count(
            &connection,
            "SELECT COUNT(*) FROM component_records WHERE fingerprint=?1",
            self.fingerprint,
        )?;
        let incompatible = count(
            &connection,
            "SELECT COUNT(*) FROM component_records WHERE fingerprint<>?1",
            self.fingerprint,
        )?;
        let applications = count(
            &connection,
            "SELECT COUNT(*) FROM application_records WHERE fingerprint=?1",
            self.fingerprint,
        )?;
        let quarantined = connection
            .query_row("SELECT COUNT(*) FROM quarantine", [], |row| {
                row.get::<_, i64>(0)
            })?
            .try_into()
            .map_err(|_| DatabaseError::CountOverflow)?;
        Ok(ComponentDatabaseStatus {
            enabled: true,
            compatible_component_records: compatible,
            incompatible_component_records: incompatible,
            application_records: applications,
            quarantined_records: quarantined,
        })
    }

    #[cfg(test)]
    fn inject_rolled_back_component(&self, payload: &[u8]) -> Result<RawRecordId, DatabaseError> {
        let id = RawRecordId(content_id(self.fingerprint, b"component", payload));
        let Some(connection) = &self.connection else {
            return Ok(id);
        };
        let mut connection = connection.lock().map_err(|_| DatabaseError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO component_records
             (fingerprint, component_id, behavior_id, payload, payload_hash, proof_payload)
             VALUES (?1, ?2, ?3, ?4, ?5, X'')",
            params![
                self.fingerprint.as_bytes().as_slice(),
                id.0.as_slice(),
                id.0.as_slice(),
                payload,
                raw_hash(payload).as_slice(),
            ],
        )?;
        transaction.rollback()?;
        Ok(id)
    }
}

fn raw_hash(payload: &[u8]) -> [u8; 32] {
    Sha256::digest(payload).into()
}

fn application_address_payload(
    key_payload: &[u8],
    winner_id: RawRecordId,
    node_count: u32,
    internal_link_count: u32,
    proof_payload: &[u8],
) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(8 + key_payload.len() + 32 + 4 + 4 + 8 + proof_payload.len());
    payload.extend_from_slice(
        &u64::try_from(key_payload.len())
            .expect("in-memory application key length fits u64")
            .to_be_bytes(),
    );
    payload.extend_from_slice(key_payload);
    payload.extend_from_slice(&winner_id.0);
    payload.extend_from_slice(&node_count.to_be_bytes());
    payload.extend_from_slice(&internal_link_count.to_be_bytes());
    payload.extend_from_slice(
        &u64::try_from(proof_payload.len())
            .expect("in-memory application proof length fits u64")
            .to_be_bytes(),
    );
    payload.extend_from_slice(proof_payload);
    payload
}

fn to_id(bytes: &[u8]) -> Result<[u8; 32], DatabaseError> {
    bytes
        .try_into()
        .map_err(|_| DatabaseError::InvalidIdLength {
            actual: bytes.len(),
        })
}

fn select_component_unchecked(
    connection: &Connection,
    fingerprint: SemanticFingerprint,
    id: RawRecordId,
) -> Result<Option<UncheckedComponentRow>, DatabaseError> {
    let row = connection
        .query_row(
            "SELECT behavior_id, payload, payload_hash, proof_payload
             FROM component_records WHERE fingerprint=?1 AND component_id=?2",
            params![fingerprint.as_bytes().as_slice(), id.0.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            },
        )
        .optional()?;
    Ok(row.map(
        |(behavior_id, payload, payload_hash, proof_payload)| UncheckedComponentRow {
            behavior_id,
            payload,
            payload_hash,
            proof_payload,
        },
    ))
}

struct UncheckedComponentRow {
    behavior_id: Vec<u8>,
    payload: Vec<u8>,
    payload_hash: Vec<u8>,
    proof_payload: Vec<u8>,
}

fn select_application_unchecked(
    connection: &Connection,
    fingerprint: SemanticFingerprint,
    id: RawRecordId,
) -> Result<Option<UncheckedApplicationRow>, DatabaseError> {
    let row = connection
        .query_row(
            "SELECT key_payload, key_hash, winner_id, node_count, internal_link_count, proof_payload
             FROM application_records WHERE fingerprint=?1 AND application_id=?2",
            params![fingerprint.as_bytes().as_slice(), id.0.as_slice()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                ))
            },
        )
        .optional()?;
    Ok(row.map(
        |(key_payload, key_hash, winner_id, node_count, internal_link_count, proof_payload)| {
            UncheckedApplicationRow {
                key_payload,
                key_hash,
                winner_id,
                node_count,
                internal_link_count,
                proof_payload,
            }
        },
    ))
}

struct UncheckedApplicationRow {
    key_payload: Vec<u8>,
    key_hash: Vec<u8>,
    winner_id: Vec<u8>,
    node_count: i64,
    internal_link_count: i64,
    proof_payload: Vec<u8>,
}

fn select_indexed_application(
    transaction: &Transaction<'_>,
    fingerprint: SemanticFingerprint,
    pattern_id: [u8; 32],
    key_payload: &[u8],
) -> Result<Option<(RawRecordId, UncheckedApplicationRow)>, DatabaseError> {
    let indexed = transaction
        .query_row(
            "SELECT application_id FROM concrete_index
             WHERE fingerprint=?1 AND pattern_id=?2",
            params![fingerprint.as_bytes().as_slice(), pattern_id.as_slice()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    let Some(indexed) = indexed else {
        return Ok(None);
    };
    let Ok(application_bytes) = to_id(&indexed) else {
        quarantine_concrete_index(
            transaction,
            fingerprint,
            pattern_id,
            key_payload,
            "concrete application index identifier is invalid",
        )?;
        return Ok(None);
    };
    let application_id = RawRecordId(application_bytes);
    let row = select_application_unchecked(transaction, fingerprint, application_id)?;
    let Some(row) = row else {
        quarantine_concrete_index(
            transaction,
            fingerprint,
            pattern_id,
            key_payload,
            "concrete application index references a missing record",
        )?;
        return Ok(None);
    };
    Ok(Some((application_id, row)))
}

fn quarantine_concrete_index(
    transaction: &Transaction<'_>,
    fingerprint: SemanticFingerprint,
    pattern_id: [u8; 32],
    key_payload: &[u8],
    reason: &str,
) -> Result<(), DatabaseError> {
    quarantine(
        transaction,
        "concrete_index",
        RawRecordId(pattern_id),
        fingerprint,
        key_payload,
        reason,
    )?;
    transaction.execute(
        "DELETE FROM concrete_index WHERE fingerprint=?1 AND pattern_id=?2",
        params![fingerprint.as_bytes().as_slice(), pattern_id.as_slice()],
    )?;
    Ok(())
}

fn component_exists(
    connection: &Connection,
    fingerprint: SemanticFingerprint,
    id: RawRecordId,
) -> Result<bool, DatabaseError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM component_records WHERE fingerprint=?1 AND component_id=?2)",
        params![fingerprint.as_bytes().as_slice(), id.0.as_slice()],
        |row| row.get(0),
    )?)
}

fn quarantine_component(
    transaction: &Transaction<'_>,
    fingerprint: SemanticFingerprint,
    id: RawRecordId,
    payload: &[u8],
    reason: &str,
) -> Result<(), DatabaseError> {
    quarantine(
        transaction,
        "component_records",
        id,
        fingerprint,
        payload,
        reason,
    )?;
    transaction.execute(
        "DELETE FROM component_records WHERE fingerprint=?1 AND component_id=?2",
        params![fingerprint.as_bytes().as_slice(), id.0.as_slice()],
    )?;
    Ok(())
}

fn quarantine_application(
    transaction: &Transaction<'_>,
    fingerprint: SemanticFingerprint,
    id: RawRecordId,
    payload: &[u8],
    reason: &str,
) -> Result<(), DatabaseError> {
    quarantine(
        transaction,
        "application_records",
        id,
        fingerprint,
        payload,
        reason,
    )?;
    transaction.execute(
        "DELETE FROM application_records WHERE fingerprint=?1 AND application_id=?2",
        params![fingerprint.as_bytes().as_slice(), id.0.as_slice()],
    )?;
    Ok(())
}

fn quarantine(
    transaction: &Transaction<'_>,
    source_table: &str,
    id: RawRecordId,
    fingerprint: SemanticFingerprint,
    payload: &[u8],
    reason: &str,
) -> Result<(), DatabaseError> {
    transaction.execute(
        "INSERT INTO quarantine(source_table, record_key, fingerprint, payload, reason)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            source_table,
            id.0.as_slice(),
            fingerprint.as_bytes().as_slice(),
            payload,
            reason,
        ],
    )?;
    Ok(())
}

fn count(
    connection: &Connection,
    query: &str,
    fingerprint: SemanticFingerprint,
) -> Result<u64, DatabaseError> {
    connection
        .query_row(query, [fingerprint.as_bytes().as_slice()], |row| {
            row.get::<_, i64>(0)
        })?
        .try_into()
        .map_err(|_| DatabaseError::CountOverflow)
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use tempfile::tempdir;

    use super::*;

    fn memory() -> ComponentDatabase {
        ComponentDatabase::open(&ComponentDatabaseConfig {
            mode: DatabaseMode::InMemory,
            ..ComponentDatabaseConfig::default()
        })
        .unwrap()
    }

    #[test]
    fn disabled_database_is_an_exact_empty_accelerator() {
        let database = ComponentDatabase::disabled();
        assert!(!database.is_enabled());
        assert_eq!(database.component_blobs().unwrap(), Vec::new());
        assert_eq!(
            database.status().unwrap(),
            ComponentDatabaseStatus::default()
        );
        assert_eq!(
            database
                .store_component_blob(b"behavior", b"component", b"proof")
                .unwrap(),
            None
        );
    }

    #[test]
    fn component_and_application_transactions_reopen_in_deterministic_order() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("components.sqlite3");
        let config = ComponentDatabaseConfig {
            mode: DatabaseMode::Path(path),
            ..ComponentDatabaseConfig::default()
        };
        let database = ComponentDatabase::open(&config).unwrap();
        let second = database
            .store_component_blob(b"z", b"component-z", b"proof-z")
            .unwrap()
            .unwrap();
        let first = database
            .store_component_blob(b"a", b"component-a", b"proof-a")
            .unwrap()
            .unwrap();
        let application = database
            .store_application(b"key", first, 2, 3, b"complete-ledger")
            .unwrap()
            .unwrap();
        drop(database);

        let reopened = ComponentDatabase::open(&config).unwrap();
        let records = reopened.component_blobs().unwrap();
        assert!(records.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert!(records.iter().any(|record| record.id == first));
        assert!(records.iter().any(|record| record.id == second));
        assert_eq!(
            reopened.lookup_application(b"key").unwrap().unwrap().id,
            application
        );
    }

    #[test]
    fn content_collision_and_transaction_rollback_never_publish_partial_records() {
        let database = memory();
        let id = database
            .store_component_blob(b"behavior", b"payload", b"proof")
            .unwrap()
            .unwrap();
        assert!(matches!(
            database.store_component_blob(b"different", b"payload", b"proof"),
            Err(DatabaseError::ContentCollision)
        ));
        assert!(database.load_component_blob(id).unwrap().is_some());

        let rolled_back = database
            .inject_rolled_back_component(b"uncommitted")
            .unwrap();
        assert_eq!(database.load_component_blob(rolled_back).unwrap(), None);
    }

    #[test]
    fn corrupted_payload_is_quarantined_and_becomes_a_miss() {
        let database = memory();
        let id = database
            .store_component_blob(b"behavior", b"payload", b"proof")
            .unwrap()
            .unwrap();
        database
            .store_application(b"key", id, 1, 2, b"proof")
            .unwrap()
            .unwrap();
        {
            let connection = database.connection.as_ref().unwrap().lock().unwrap();
            connection
                .execute(
                    "UPDATE component_records SET payload=X'00' WHERE fingerprint=?1 AND component_id=?2",
                    params![database.fingerprint.as_bytes().as_slice(), id.0.as_slice()],
                )
                .unwrap();
        }
        assert_eq!(database.load_component_blob(id).unwrap(), None);
        let status = database.status().unwrap();
        assert_eq!(status.compatible_component_records, 0);
        assert_eq!(status.application_records, 0);
        assert_eq!(status.quarantined_records, 1);
        assert_eq!(database.lookup_application(b"key").unwrap(), None);
    }

    #[test]
    fn corrupted_application_hash_is_quarantined_with_its_index() {
        let database = memory();
        let component = database
            .store_component_blob(b"behavior", b"payload", b"proof")
            .unwrap()
            .unwrap();
        database
            .store_application(b"key", component, 1, 2, b"proof")
            .unwrap()
            .unwrap();
        {
            let connection = database.connection.as_ref().unwrap().lock().unwrap();
            connection
                .execute(
                    "UPDATE application_records SET key_hash=X'00' WHERE fingerprint=?1",
                    [database.fingerprint.as_bytes().as_slice()],
                )
                .unwrap();
        }

        assert_eq!(database.lookup_application(b"key").unwrap(), None);
        let status = database.status().unwrap();
        assert_eq!(status.compatible_component_records, 1);
        assert_eq!(status.application_records, 0);
        assert_eq!(status.quarantined_records, 1);
        let connection = database.connection.as_ref().unwrap().lock().unwrap();
        let concrete_indexes: i64 = connection
            .query_row("SELECT COUNT(*) FROM concrete_index", [], |row| row.get(0))
            .unwrap();
        assert_eq!(concrete_indexes, 0);
    }

    #[test]
    fn concurrent_wal_writers_converge_on_one_content_address() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("concurrent.sqlite3");
        let config = ComponentDatabaseConfig {
            mode: DatabaseMode::Path(path),
            busy_timeout: Duration::from_secs(10),
        };
        ComponentDatabase::open(&config).unwrap();
        let config = Arc::new(config);
        let handles = (0..8)
            .map(|_| {
                let config = Arc::clone(&config);
                thread::spawn(move || {
                    ComponentDatabase::open(&config)
                        .unwrap()
                        .store_component_blob(b"behavior", b"same", b"proof")
                        .unwrap()
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        let ids = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>();
        assert!(ids.iter().all(|id| *id == ids[0]));
        assert_eq!(
            ComponentDatabase::open(&config)
                .unwrap()
                .component_blobs()
                .unwrap()
                .len(),
            1
        );
    }
}
