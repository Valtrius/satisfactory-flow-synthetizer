//! Persistent job history in SQLite under the app local data directory.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use tauri::{AppHandle, Manager, State};

const DB_FILE: &str = "history.sqlite";
const HISTORY_DOCUMENT_VERSION: u32 = 2;
const SCHEMA_USER_VERSION: i32 = 3;

const SCHEMA_SQL: &str = "
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA user_version = 3;
CREATE TABLE meta (
  key TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
);
CREATE TABLE entries (
  id TEXT PRIMARY KEY NOT NULL,
  sort_order INTEGER NOT NULL,
  title TEXT,
  status TEXT NOT NULL,
  created_at_ms INTEGER NOT NULL,
  updated_at_ms INTEGER NOT NULL,
  started_at_ms INTEGER,
  finished_at_ms INTEGER,
  enumeration_complete INTEGER NOT NULL DEFAULT 0,
  error TEXT,
  selected_source_index INTEGER NOT NULL DEFAULT 0,
  request_belt_rate TEXT NOT NULL,
  request_solve_mode TEXT NOT NULL,
  request_engine TEXT NOT NULL,
  form_belt_rate TEXT NOT NULL,
  form_solve_mode TEXT NOT NULL,
  form_engine TEXT NOT NULL
);
CREATE TABLE entry_endpoints (
  entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  snapshot TEXT NOT NULL CHECK (snapshot IN ('request', 'form')),
  side TEXT NOT NULL CHECK (side IN ('input', 'output')),
  ordinal INTEGER NOT NULL,
  port_id TEXT NOT NULL,
  name TEXT NOT NULL,
  rate TEXT NOT NULL,
  multiplier TEXT,
  PRIMARY KEY (entry_id, snapshot, side, ordinal)
);
CREATE TABLE solutions (
  entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  source_index INTEGER NOT NULL,
  engine TEXT NOT NULL,
  status TEXT NOT NULL,
  model_version INTEGER NOT NULL,
  proof_version INTEGER,
  initial_node_lower_bound INTEGER,
  node_counts_exhausted_through INTEGER,
  link_groups_exhausted INTEGER,
  profiles_exhausted INTEGER,
  root_partitions_exhausted INTEGER,
  validator_version INTEGER,
  validation_node_count INTEGER,
  validation_link_count INTEGER,
  validation_physical_link_count INTEGER,
  validation_discard_link_count INTEGER,
  cyclic_scc_count INTEGER,
  node_count INTEGER NOT NULL,
  splitters INTEGER NOT NULL,
  mergers INTEGER NOT NULL,
  feedback_loops INTEGER NOT NULL,
  link_count INTEGER NOT NULL,
  checked_through INTEGER,
  belt_count INTEGER,
  internal_max_throughput_exact TEXT,
  internal_max_throughput_decimal TEXT,
  stats_physical_link_count INTEGER,
  stats_discard_link_count INTEGER,
  total_input_exact TEXT NOT NULL,
  total_input_decimal TEXT NOT NULL,
  total_output_exact TEXT NOT NULL,
  total_output_decimal TEXT NOT NULL,
  discard_rate_exact TEXT NOT NULL,
  discard_rate_decimal TEXT NOT NULL,
  belt_rate_exact TEXT NOT NULL,
  belt_rate_decimal TEXT NOT NULL,
  PRIMARY KEY (entry_id, source_index)
);
CREATE TABLE solution_nodes (
  entry_id TEXT NOT NULL,
  source_index INTEGER NOT NULL,
  node_id TEXT NOT NULL,
  kind TEXT NOT NULL,
  label TEXT NOT NULL,
  PRIMARY KEY (entry_id, source_index, node_id),
  FOREIGN KEY (entry_id, source_index) REFERENCES solutions(entry_id, source_index) ON DELETE CASCADE
);
CREATE TABLE solution_edges (
  entry_id TEXT NOT NULL,
  source_index INTEGER NOT NULL,
  edge_id TEXT NOT NULL,
  source TEXT NOT NULL,
  target TEXT NOT NULL,
  source_port INTEGER NOT NULL,
  target_port INTEGER NOT NULL,
  rate_exact TEXT NOT NULL,
  rate_decimal TEXT NOT NULL,
  feedback INTEGER NOT NULL,
  discarded INTEGER NOT NULL,
  PRIMARY KEY (entry_id, source_index, edge_id),
  FOREIGN KEY (entry_id, source_index) REFERENCES solutions(entry_id, source_index) ON DELETE CASCADE
);
CREATE TABLE solution_build_steps (
  entry_id TEXT NOT NULL,
  source_index INTEGER NOT NULL,
  ordinal INTEGER NOT NULL,
  body TEXT NOT NULL,
  PRIMARY KEY (entry_id, source_index, ordinal),
  FOREIGN KEY (entry_id, source_index) REFERENCES solutions(entry_id, source_index) ON DELETE CASCADE
);
CREATE TABLE sort_columns (
  entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  ordinal INTEGER NOT NULL,
  key TEXT NOT NULL,
  dir TEXT NOT NULL,
  PRIMARY KEY (entry_id, ordinal)
);
CREATE TABLE layouts (
  entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  source_index INTEGER NOT NULL,
  layout_key TEXT NOT NULL,
  nodes_json TEXT NOT NULL,
  edges_json TEXT NOT NULL,
  PRIMARY KEY (entry_id, source_index)
);
CREATE TABLE solver_state (
  entry_id TEXT PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
  progress_json TEXT,
  proof_json TEXT,
  sequence INTEGER NOT NULL DEFAULT 0
);
";

pub struct HistoryStore {
    conn: Mutex<Connection>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum HistoryOp {
    UpsertEntry {
        entry: Value,
    },
    PatchEntry {
        id: String,
        fields: Value,
    },
    SaveLayouts {
        id: String,
        layouts: Value,
    },
    SaveSortColumns {
        id: String,
        sort_columns: Value,
    },
    SaveSolverState {
        id: String,
        progress: Option<Value>,
        proof: Option<Value>,
        sequence: Option<i64>,
    },
    DeleteEntries {
        ids: Vec<String>,
    },
    ReorderEntries {
        ids: Vec<String>,
    },
    SetSelected {
        id: Option<String>,
    },
}

impl HistoryStore {
    pub fn open(app_local_data: &Path) -> Result<Self, String> {
        fs::create_dir_all(app_local_data)
            .map_err(|error| format!("create app data dir: {error}"))?;
        let db_path = app_local_data.join(DB_FILE);
        let conn = open_connection(&db_path)?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn load_document(&self) -> Result<Value, String> {
        let conn = self.conn.lock().map_err(|_| "history db lock poisoned")?;
        let selected_entry_id = meta_get(&conn, "selected_entry_id")?;
        let entries = load_entries(&conn)?;
        Ok(json!({
            "version": HISTORY_DOCUMENT_VERSION,
            "selectedEntryId": selected_entry_id,
            "entries": entries,
        }))
    }

    pub fn apply_ops(&self, ops: &[HistoryOp]) -> Result<(), String> {
        if ops.is_empty() {
            return Ok(());
        }
        let mut conn = self.conn.lock().map_err(|_| "history db lock poisoned")?;
        let tx = conn
            .transaction()
            .map_err(|error| format!("begin history ops: {error}"))?;
        for op in ops {
            apply_op(&tx, op)?;
        }
        tx.commit()
            .map_err(|error| format!("commit history ops: {error}"))?;
        Ok(())
    }
}

fn open_connection(db_path: &Path) -> Result<Connection, String> {
    let conn =
        Connection::open(db_path).map_err(|error| format!("open history database: {error}"))?;
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|error| format!("read history schema version: {error}"))?;
    if version == SCHEMA_USER_VERSION {
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
            .map_err(|error| format!("configure history database: {error}"))?;
        return Ok(conn);
    }
    drop(conn);
    remove_db_files(db_path)?;
    let conn =
        Connection::open(db_path).map_err(|error| format!("open history database: {error}"))?;
    conn.execute_batch(SCHEMA_SQL)
        .map_err(|error| format!("init history schema: {error}"))?;
    Ok(conn)
}

fn remove_db_files(db_path: &Path) -> Result<(), String> {
    let name = db_path
        .file_name()
        .ok_or_else(|| "history database path missing file name".to_owned())?;
    let name = name.to_string_lossy();
    for suffix in ["", "-wal", "-shm"] {
        let path = db_path.with_file_name(format!("{name}{suffix}"));
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("remove {}: {error}", path.display())),
        }
    }
    Ok(())
}

fn apply_op(tx: &Transaction<'_>, op: &HistoryOp) -> Result<(), String> {
    match op {
        HistoryOp::UpsertEntry { entry } => upsert_entry(tx, entry),
        HistoryOp::PatchEntry { id, fields } => patch_entry(tx, id, fields),
        HistoryOp::SaveLayouts { id, layouts } => replace_layouts(tx, id, layouts),
        HistoryOp::SaveSortColumns { id, sort_columns } => {
            replace_sort_columns(tx, id, sort_columns)
        }
        HistoryOp::SaveSolverState {
            id,
            progress,
            proof,
            sequence,
        } => replace_solver_state(
            tx,
            id,
            progress.as_ref(),
            proof.as_ref(),
            sequence.unwrap_or(0),
        ),
        HistoryOp::DeleteEntries { ids } => {
            for id in ids {
                tx.execute("DELETE FROM entries WHERE id = ?1", params![id])
                    .map_err(|error| format!("delete history entry: {error}"))?;
            }
            Ok(())
        }
        HistoryOp::ReorderEntries { ids } => {
            for (sort_order, id) in ids.iter().enumerate() {
                let order = i64::try_from(sort_order)
                    .map_err(|_| "history entry count exceeds SQLite integer range")?;
                tx.execute(
                    "UPDATE entries SET sort_order = ?1 WHERE id = ?2",
                    params![order, id],
                )
                .map_err(|error| format!("reorder history entry: {error}"))?;
            }
            Ok(())
        }
        HistoryOp::SetSelected { id } => match id {
            Some(id) => meta_set(tx, "selected_entry_id", id),
            None => {
                tx.execute("DELETE FROM meta WHERE key = 'selected_entry_id'", [])
                    .map_err(|error| format!("clear selected history id: {error}"))?;
                Ok(())
            }
        },
    }
}

fn upsert_entry(tx: &Transaction<'_>, entry: &Value) -> Result<(), String> {
    let id = required_str(entry, "id")?;
    let sort_order = existing_sort_order(tx, id)?.unwrap_or(next_sort_order(tx)?);
    tx.execute("DELETE FROM entries WHERE id = ?1", params![id])
        .map_err(|error| format!("replace history entry: {error}"))?;

    let request = required_value(entry, "request")?;
    let form = required_value(entry, "form")?;
    tx.execute(
        "
        INSERT INTO entries (
          id, sort_order, title, status, created_at_ms, updated_at_ms, started_at_ms,
          finished_at_ms, enumeration_complete, error, selected_source_index,
          request_belt_rate, request_solve_mode, request_engine,
          form_belt_rate, form_solve_mode, form_engine
        ) VALUES (
          ?1, ?2, ?3, ?4, ?5, ?6, ?7,
          ?8, ?9, ?10, ?11,
          ?12, ?13, ?14,
          ?15, ?16, ?17
        )
        ",
        params![
            id,
            sort_order,
            optional_str(entry, "title"),
            required_str(entry, "status")?,
            required_i64(entry, "createdAtMs")?,
            required_i64(entry, "updatedAtMs")?,
            optional_i64(entry, "startedAtMs"),
            optional_i64(entry, "finishedAtMs"),
            i64::from(
                entry
                    .get("enumerationComplete")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            ),
            optional_str(entry, "error"),
            entry
                .get("selectedSourceIndex")
                .and_then(Value::as_i64)
                .unwrap_or(0),
            required_str(request, "beltRate")?,
            solve_mode(request),
            engine(request),
            required_str(form, "beltRate")?,
            solve_mode(form),
            engine(form),
        ],
    )
    .map_err(|error| format!("insert history entry: {error}"))?;

    insert_endpoints(tx, id, "request", request, false)?;
    insert_endpoints(tx, id, "form", form, true)?;
    insert_solutions(tx, id, entry)?;
    replace_sort_columns(tx, id, entry.get("sortColumns").unwrap_or(&json!([])))?;
    replace_layouts(tx, id, entry.get("layouts").unwrap_or(&json!({})))?;
    replace_solver_state(
        tx,
        id,
        entry.get("progress"),
        entry.get("proof"),
        entry.get("sequence").and_then(Value::as_i64).unwrap_or(0),
    )?;
    Ok(())
}

fn patch_entry(tx: &Transaction<'_>, id: &str, fields: &Value) -> Result<(), String> {
    let changed = tx
        .execute(
            "
            UPDATE entries SET
              title = ?1,
              status = ?2,
              created_at_ms = ?3,
              updated_at_ms = ?4,
              started_at_ms = ?5,
              finished_at_ms = ?6,
              enumeration_complete = ?7,
              error = ?8,
              selected_source_index = ?9
            WHERE id = ?10
            ",
            params![
                optional_str(fields, "title"),
                required_str(fields, "status")?,
                required_i64(fields, "createdAtMs")?,
                required_i64(fields, "updatedAtMs")?,
                optional_i64(fields, "startedAtMs"),
                optional_i64(fields, "finishedAtMs"),
                i64::from(
                    fields
                        .get("enumerationComplete")
                        .and_then(Value::as_bool)
                        .unwrap_or(false)
                ),
                optional_str(fields, "error"),
                fields
                    .get("selectedSourceIndex")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                id,
            ],
        )
        .map_err(|error| format!("patch history entry: {error}"))?;
    if changed == 0 {
        return Err(format!("history entry not found: {id}"));
    }
    Ok(())
}

fn insert_endpoints(
    tx: &Transaction<'_>,
    entry_id: &str,
    snapshot: &str,
    owner: &Value,
    with_multiplier: bool,
) -> Result<(), String> {
    insert_endpoint_side(
        tx,
        entry_id,
        snapshot,
        "input",
        owner.get("inputs"),
        with_multiplier,
    )?;
    insert_endpoint_side(
        tx,
        entry_id,
        snapshot,
        "output",
        owner.get("outputs"),
        with_multiplier,
    )?;
    Ok(())
}

fn insert_endpoint_side(
    tx: &Transaction<'_>,
    entry_id: &str,
    snapshot: &str,
    side: &str,
    rows: Option<&Value>,
    with_multiplier: bool,
) -> Result<(), String> {
    let Some(rows) = rows.and_then(Value::as_array) else {
        return Ok(());
    };
    for (ordinal, row) in rows.iter().enumerate() {
        let order =
            i64::try_from(ordinal).map_err(|_| "endpoint count exceeds SQLite integer range")?;
        tx.execute(
            "
            INSERT INTO entry_endpoints (
              entry_id, snapshot, side, ordinal, port_id, name, rate, multiplier
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ",
            params![
                entry_id,
                snapshot,
                side,
                order,
                required_str(row, "id")?,
                row.get("name").and_then(Value::as_str).unwrap_or(""),
                required_str(row, "rate")?,
                if with_multiplier {
                    row.get("multiplier").and_then(Value::as_str)
                } else {
                    None
                },
            ],
        )
        .map_err(|error| format!("insert history endpoint: {error}"))?;
    }
    Ok(())
}

fn insert_solutions(tx: &Transaction<'_>, entry_id: &str, entry: &Value) -> Result<(), String> {
    let mut solutions = entry
        .get("results")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if solutions.is_empty()
        && let Some(result) = entry.get("result").filter(|value| value.is_object())
    {
        solutions.push(result.clone());
    }
    for (source_index, solution) in solutions.iter().enumerate() {
        let index = i64::try_from(source_index)
            .map_err(|_| "solution count exceeds SQLite integer range")?;
        insert_solution(tx, entry_id, index, solution)?;
    }
    Ok(())
}

fn insert_solution(
    tx: &Transaction<'_>,
    entry_id: &str,
    source_index: i64,
    solution: &Value,
) -> Result<(), String> {
    let stats = required_value(solution, "stats")?;
    let proof = solution.get("proof").filter(|value| value.is_object());
    let validation = solution.get("validation").filter(|value| value.is_object());
    let (in_exact, in_decimal) =
        display_rate(required_value(solution, "totalInput")?, "totalInput")?;
    let (out_exact, out_decimal) =
        display_rate(required_value(solution, "totalOutput")?, "totalOutput")?;
    let (discard_exact, discard_decimal) =
        display_rate(required_value(solution, "discardRate")?, "discardRate")?;
    let (belt_exact, belt_decimal) =
        display_rate(required_value(solution, "beltRate")?, "beltRate")?;
    let throughput = solution
        .get("stats")
        .and_then(|stats| stats.get("internalMaxThroughput"))
        .filter(|value| value.is_object());
    let (throughput_exact, throughput_decimal) = match throughput {
        Some(rate) => {
            let (exact, decimal) = display_rate(rate, "internalMaxThroughput")?;
            (Some(exact), Some(decimal))
        }
        None => (None, None),
    };

    tx.execute(
        "
        INSERT INTO solutions (
          entry_id, source_index, engine, status, model_version,
          proof_version, initial_node_lower_bound, node_counts_exhausted_through,
          link_groups_exhausted, profiles_exhausted, root_partitions_exhausted,
          validator_version, validation_node_count, validation_link_count,
          validation_physical_link_count, validation_discard_link_count, cyclic_scc_count,
          node_count, splitters, mergers, feedback_loops, link_count, checked_through, belt_count,
          internal_max_throughput_exact, internal_max_throughput_decimal,
          stats_physical_link_count, stats_discard_link_count,
          total_input_exact, total_input_decimal, total_output_exact, total_output_decimal,
          discard_rate_exact, discard_rate_decimal, belt_rate_exact, belt_rate_decimal
        ) VALUES (
          ?1, ?2, ?3, ?4, ?5,
          ?6, ?7, ?8,
          ?9, ?10, ?11,
          ?12, ?13, ?14,
          ?15, ?16, ?17,
          ?18, ?19, ?20, ?21, ?22, ?23, ?24,
          ?25, ?26,
          ?27, ?28,
          ?29, ?30, ?31, ?32,
          ?33, ?34, ?35, ?36
        )
        ",
        params![
            entry_id,
            source_index,
            engine(solution),
            required_str(solution, "status")?,
            required_i64(solution, "modelVersion")?,
            proof.and_then(|value| value.get("proofVersion").and_then(Value::as_i64)),
            proof.and_then(|value| value.get("initialNodeLowerBound").and_then(Value::as_i64)),
            proof.and_then(|value| value
                .get("nodeCountsExhaustedThrough")
                .and_then(Value::as_i64)),
            proof.and_then(|value| value.get("linkGroupsExhausted").and_then(Value::as_i64)),
            proof.and_then(|value| value.get("profilesExhausted").and_then(Value::as_i64)),
            proof.and_then(|value| value.get("rootPartitionsExhausted").and_then(Value::as_i64)),
            validation.and_then(|value| value.get("validatorVersion").and_then(Value::as_i64)),
            validation.and_then(|value| value.get("nodeCount").and_then(Value::as_i64)),
            validation.and_then(|value| value.get("linkCount").and_then(Value::as_i64)),
            validation.and_then(|value| value.get("physicalLinkCount").and_then(Value::as_i64)),
            validation.and_then(|value| value.get("discardLinkCount").and_then(Value::as_i64)),
            validation.and_then(|value| value.get("cyclicSccCount").and_then(Value::as_i64)),
            required_i64(stats, "nodeCount")?,
            required_i64(stats, "splitters")?,
            required_i64(stats, "mergers")?,
            required_i64(stats, "feedbackLoops")?,
            stats
                .get("linkCount")
                .and_then(Value::as_i64)
                .or_else(|| stats.get("beltCount").and_then(Value::as_i64))
                .unwrap_or(0),
            stats.get("checkedThrough").and_then(Value::as_i64),
            stats.get("beltCount").and_then(Value::as_i64),
            throughput_exact,
            throughput_decimal,
            stats.get("physicalLinkCount").and_then(Value::as_i64),
            stats.get("discardLinkCount").and_then(Value::as_i64),
            in_exact,
            in_decimal,
            out_exact,
            out_decimal,
            discard_exact,
            discard_decimal,
            belt_exact,
            belt_decimal,
        ],
    )
    .map_err(|error| format!("insert history solution: {error}"))?;

    if let Some(nodes) = solution.get("nodes").and_then(Value::as_array) {
        for node in nodes {
            tx.execute(
                "
                INSERT INTO solution_nodes (entry_id, source_index, node_id, kind, label)
                VALUES (?1, ?2, ?3, ?4, ?5)
                ",
                params![
                    entry_id,
                    source_index,
                    required_str(node, "id")?,
                    required_str(node, "kind")?,
                    node.get("label").and_then(Value::as_str).unwrap_or(""),
                ],
            )
            .map_err(|error| format!("insert history solution node: {error}"))?;
        }
    }
    if let Some(edges) = solution.get("edges").and_then(Value::as_array) {
        for edge in edges {
            let (rate_exact, rate_decimal) =
                display_rate(required_value(edge, "rate")?, "edge rate")?;
            tx.execute(
                "
                INSERT INTO solution_edges (
                  entry_id, source_index, edge_id, source, target, source_port, target_port,
                  rate_exact, rate_decimal, feedback, discarded
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                ",
                params![
                    entry_id,
                    source_index,
                    required_str(edge, "id")?,
                    required_str(edge, "source")?,
                    required_str(edge, "target")?,
                    required_i64(edge, "sourcePort")?,
                    required_i64(edge, "targetPort")?,
                    rate_exact,
                    rate_decimal,
                    i64::from(
                        edge.get("feedback")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                    i64::from(
                        edge.get("discarded")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    ),
                ],
            )
            .map_err(|error| format!("insert history solution edge: {error}"))?;
        }
    }
    if let Some(steps) = solution.get("buildSteps").and_then(Value::as_array) {
        for (ordinal, step) in steps.iter().enumerate() {
            let order = i64::try_from(ordinal)
                .map_err(|_| "build step count exceeds SQLite integer range")?;
            let body = step
                .as_str()
                .ok_or_else(|| "history solution buildSteps must be strings".to_owned())?;
            tx.execute(
                "
                INSERT INTO solution_build_steps (entry_id, source_index, ordinal, body)
                VALUES (?1, ?2, ?3, ?4)
                ",
                params![entry_id, source_index, order, body],
            )
            .map_err(|error| format!("insert history build step: {error}"))?;
        }
    }
    Ok(())
}

fn replace_sort_columns(
    tx: &Transaction<'_>,
    entry_id: &str,
    sort_columns: &Value,
) -> Result<(), String> {
    tx.execute(
        "DELETE FROM sort_columns WHERE entry_id = ?1",
        params![entry_id],
    )
    .map_err(|error| format!("clear history sort columns: {error}"))?;
    let Some(columns) = sort_columns.as_array() else {
        return Ok(());
    };
    for (ordinal, column) in columns.iter().enumerate() {
        let order =
            i64::try_from(ordinal).map_err(|_| "sort column count exceeds SQLite integer range")?;
        tx.execute(
            "INSERT INTO sort_columns (entry_id, ordinal, key, dir) VALUES (?1, ?2, ?3, ?4)",
            params![
                entry_id,
                order,
                required_str(column, "key")?,
                required_str(column, "dir")?,
            ],
        )
        .map_err(|error| format!("insert history sort column: {error}"))?;
    }
    Ok(())
}

fn replace_layouts(tx: &Transaction<'_>, entry_id: &str, layouts: &Value) -> Result<(), String> {
    tx.execute("DELETE FROM layouts WHERE entry_id = ?1", params![entry_id])
        .map_err(|error| format!("clear history layouts: {error}"))?;
    let Some(object) = layouts.as_object() else {
        return Ok(());
    };
    for (key, layout) in object {
        let source_index = key
            .parse::<i64>()
            .map_err(|_| format!("history layout key must be an integer: {key}"))?;
        tx.execute(
            "
            INSERT INTO layouts (entry_id, source_index, layout_key, nodes_json, edges_json)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ",
            params![
                entry_id,
                source_index,
                required_str(layout, "layoutKey")?,
                compact_json(layout.get("nodes").unwrap_or(&json!([])))?,
                compact_json(layout.get("edges").unwrap_or(&json!([])))?,
            ],
        )
        .map_err(|error| format!("insert history layout: {error}"))?;
    }
    Ok(())
}

fn replace_solver_state(
    tx: &Transaction<'_>,
    entry_id: &str,
    progress: Option<&Value>,
    proof: Option<&Value>,
    sequence: i64,
) -> Result<(), String> {
    tx.execute(
        "DELETE FROM solver_state WHERE entry_id = ?1",
        params![entry_id],
    )
    .map_err(|error| format!("clear history solver state: {error}"))?;
    let progress_json = progress
        .filter(|value| !value.is_null())
        .map(compact_json)
        .transpose()?;
    let proof_json = proof
        .filter(|value| !value.is_null())
        .map(compact_json)
        .transpose()?;
    if progress_json.is_none() && proof_json.is_none() && sequence == 0 {
        return Ok(());
    }
    tx.execute(
        "INSERT INTO solver_state (entry_id, progress_json, proof_json, sequence) VALUES (?1, ?2, ?3, ?4)",
        params![entry_id, progress_json, proof_json, sequence],
    )
    .map_err(|error| format!("save history solver state: {error}"))?;
    Ok(())
}

fn existing_sort_order(tx: &Transaction<'_>, id: &str) -> Result<Option<i64>, String> {
    tx.query_row(
        "SELECT sort_order FROM entries WHERE id = ?1",
        params![id],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| format!("read history sort order: {error}"))
}

fn next_sort_order(tx: &Transaction<'_>) -> Result<i64, String> {
    let max: Option<i64> = tx
        .query_row("SELECT MAX(sort_order) FROM entries", [], |row| row.get(0))
        .map_err(|error| format!("read history max sort order: {error}"))?;
    Ok(max.map_or(0, |value| value + 1))
}

#[allow(clippy::too_many_lines)]
fn load_entries(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    let mut stmt = conn
        .prepare(
            "
            SELECT
              id, title, status, created_at_ms, updated_at_ms, started_at_ms, finished_at_ms,
              enumeration_complete, error, selected_source_index,
              request_belt_rate, request_solve_mode, request_engine,
              form_belt_rate, form_solve_mode, form_engine,
              solver_state.progress_json, solver_state.proof_json, COALESCE(solver_state.sequence, 0)
            FROM entries
            LEFT JOIN solver_state ON entries.id = solver_state.entry_id
            ORDER BY sort_order ASC
            ",
        )
        .map_err(|error| format!("prepare history load: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LoadedEntry {
                id: row.get(0)?,
                title: row.get(1)?,
                status: row.get(2)?,
                created_at_ms: row.get(3)?,
                updated_at_ms: row.get(4)?,
                started_at_ms: row.get(5)?,
                finished_at_ms: row.get(6)?,
                enumeration_complete: row.get::<_, i64>(7)? != 0,
                error: row.get(8)?,
                selected_source_index: row.get(9)?,
                request_belt_rate: row.get(10)?,
                request_solve_mode: row.get(11)?,
                request_engine: row.get(12)?,
                form_belt_rate: row.get(13)?,
                form_solve_mode: row.get(14)?,
                form_engine: row.get(15)?,
                progress_json: row.get(16)?,
                proof_json: row.get(17)?,
                sequence: row.get(18)?,
            })
        })
        .map_err(|error| format!("query history entries: {error}"))?;
    for row in rows {
        entries.push(row.map_err(|error| format!("read history entry: {error}"))?);
    }

    let endpoints = load_all_endpoints(conn)?;
    let solutions = load_all_solutions(conn)?;
    let sort_columns = load_all_sort_columns(conn)?;
    let layouts = load_all_layouts(conn)?;

    entries
        .into_iter()
        .map(|entry| {
            let id = entry.id.clone();
            let request_inputs = endpoints_json(&endpoints, &id, "request", "input", false);
            let request_outputs = endpoints_json(&endpoints, &id, "request", "output", false);
            let form_inputs = endpoints_json(&endpoints, &id, "form", "input", true);
            let form_outputs = endpoints_json(&endpoints, &id, "form", "output", true);
            let results = solutions.get(&id).cloned().unwrap_or_default();
            let selected = usize::try_from(entry.selected_source_index).ok();
            let result = selected
                .and_then(|index| results.get(index))
                .or_else(|| results.first())
                .cloned()
                .unwrap_or(Value::Null);
            Ok(json!({
                "id": id,
                "title": entry.title,
                "status": entry.status,
                "createdAtMs": entry.created_at_ms,
                "updatedAtMs": entry.updated_at_ms,
                "startedAtMs": entry.started_at_ms,
                "finishedAtMs": entry.finished_at_ms,
                "enumerationComplete": entry.enumeration_complete,
                "error": entry.error,
                "selectedSourceIndex": entry.selected_source_index,
                "request": {
                    "inputs": request_inputs,
                    "outputs": request_outputs,
                    "beltRate": entry.request_belt_rate,
                    "solveMode": entry.request_solve_mode,
                    "engine": entry.request_engine,
                },
                "form": {
                    "inputs": form_inputs,
                    "outputs": form_outputs,
                    "beltRate": entry.form_belt_rate,
                    "solveMode": entry.form_solve_mode,
                    "engine": entry.form_engine,
                },
                "result": result,
                "results": results,
                "sortColumns": sort_columns.get(&id).cloned().unwrap_or_else(|| json!([])),
                "layouts": layouts.get(&id).cloned().unwrap_or_else(|| json!({})),
                "jobId": Value::Null,
                "progress": entry.progress_json.as_deref().map(|text| parse_json(text, "progress")).transpose()?,
                "proof": entry.proof_json.as_deref().map(|text| parse_json(text, "proof")).transpose()?,
                "sequence": entry.sequence,
            }))
        })
        .collect()
}

struct LoadedEntry {
    id: String,
    title: Option<String>,
    status: String,
    created_at_ms: i64,
    updated_at_ms: i64,
    started_at_ms: Option<i64>,
    finished_at_ms: Option<i64>,
    enumeration_complete: bool,
    error: Option<String>,
    selected_source_index: i64,
    request_belt_rate: String,
    request_solve_mode: String,
    request_engine: String,
    form_belt_rate: String,
    form_solve_mode: String,
    form_engine: String,
    progress_json: Option<String>,
    proof_json: Option<String>,
    sequence: i64,
}

type EndpointKey = (String, String, String);
type EndpointRow = (i64, String, String, String, Option<String>);

fn load_all_endpoints(conn: &Connection) -> Result<HashMap<EndpointKey, Vec<EndpointRow>>, String> {
    let mut map: HashMap<EndpointKey, Vec<EndpointRow>> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "
            SELECT entry_id, snapshot, side, ordinal, port_id, name, rate, multiplier
            FROM entry_endpoints
            ORDER BY entry_id, snapshot, side, ordinal
            ",
        )
        .map_err(|error| format!("prepare history endpoints: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })
        .map_err(|error| format!("query history endpoints: {error}"))?;
    for row in rows {
        let (entry_id, snapshot, side, ordinal, port_id, name, rate, multiplier) =
            row.map_err(|error| format!("read history endpoint: {error}"))?;
        map.entry((entry_id, snapshot, side))
            .or_default()
            .push((ordinal, port_id, name, rate, multiplier));
    }
    Ok(map)
}

fn endpoints_json(
    endpoints: &HashMap<EndpointKey, Vec<EndpointRow>>,
    entry_id: &str,
    snapshot: &str,
    side: &str,
    with_multiplier: bool,
) -> Vec<Value> {
    endpoints
        .get(&(entry_id.to_owned(), snapshot.to_owned(), side.to_owned()))
        .map(|rows| {
            rows.iter()
                .map(|(_, port_id, name, rate, multiplier)| {
                    let mut object = json!({
                        "id": port_id,
                        "name": name,
                        "rate": rate,
                    });
                    if with_multiplier {
                        object["multiplier"] =
                            json!(multiplier.clone().unwrap_or_else(|| "1".to_owned()));
                    }
                    object
                })
                .collect()
        })
        .unwrap_or_default()
}

fn load_all_solutions(conn: &Connection) -> Result<HashMap<String, Vec<Value>>, String> {
    let nodes = load_solution_nodes(conn)?;
    let edges = load_solution_edges(conn)?;
    let steps = load_solution_steps(conn)?;
    let mut map: HashMap<String, Vec<(i64, Value)>> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "
            SELECT
              entry_id, source_index, engine, status, model_version,
              proof_version, initial_node_lower_bound, node_counts_exhausted_through,
              link_groups_exhausted, profiles_exhausted, root_partitions_exhausted,
              validator_version, validation_node_count, validation_link_count,
              validation_physical_link_count, validation_discard_link_count, cyclic_scc_count,
              node_count, splitters, mergers, feedback_loops, link_count, checked_through, belt_count,
              internal_max_throughput_exact, internal_max_throughput_decimal,
              stats_physical_link_count, stats_discard_link_count,
              total_input_exact, total_input_decimal, total_output_exact, total_output_decimal,
              discard_rate_exact, discard_rate_decimal, belt_rate_exact, belt_rate_decimal
            FROM solutions
            ORDER BY entry_id, source_index
            ",
        )
        .map_err(|error| format!("prepare history solutions: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok(LoadedSolution {
                entry_id: row.get(0)?,
                source_index: row.get(1)?,
                engine: row.get(2)?,
                status: row.get(3)?,
                model_version: row.get(4)?,
                proof_version: row.get(5)?,
                initial_node_lower_bound: row.get(6)?,
                node_counts_exhausted_through: row.get(7)?,
                link_groups_exhausted: row.get(8)?,
                profiles_exhausted: row.get(9)?,
                root_partitions_exhausted: row.get(10)?,
                validator_version: row.get(11)?,
                validation_node_count: row.get(12)?,
                validation_link_count: row.get(13)?,
                validation_physical_link_count: row.get(14)?,
                validation_discard_link_count: row.get(15)?,
                cyclic_scc_count: row.get(16)?,
                node_count: row.get(17)?,
                splitters: row.get(18)?,
                mergers: row.get(19)?,
                feedback_loops: row.get(20)?,
                link_count: row.get(21)?,
                checked_through: row.get(22)?,
                belt_count: row.get(23)?,
                internal_max_throughput_exact: row.get(24)?,
                internal_max_throughput_decimal: row.get(25)?,
                stats_physical_link_count: row.get(26)?,
                stats_discard_link_count: row.get(27)?,
                total_input_exact: row.get(28)?,
                total_input_decimal: row.get(29)?,
                total_output_exact: row.get(30)?,
                total_output_decimal: row.get(31)?,
                discard_rate_exact: row.get(32)?,
                discard_rate_decimal: row.get(33)?,
                belt_rate_exact: row.get(34)?,
                belt_rate_decimal: row.get(35)?,
            })
        })
        .map_err(|error| format!("query history solutions: {error}"))?;
    for row in rows {
        let row = row.map_err(|error| format!("read history solution: {error}"))?;
        let key = (row.entry_id.clone(), row.source_index);
        let mut stats = json!({
            "nodeCount": row.node_count,
            "splitters": row.splitters,
            "mergers": row.mergers,
            "feedbackLoops": row.feedback_loops,
            "linkCount": row.link_count,
        });
        if let Some(value) = row.checked_through {
            stats["checkedThrough"] = json!(value);
        }
        if let Some(value) = row.belt_count {
            stats["beltCount"] = json!(value);
        }
        if let (Some(exact), Some(decimal)) = (
            row.internal_max_throughput_exact.as_ref(),
            row.internal_max_throughput_decimal.as_ref(),
        ) {
            stats["internalMaxThroughput"] = json!({ "exact": exact, "decimal": decimal });
        }
        if let Some(value) = row.stats_physical_link_count {
            stats["physicalLinkCount"] = json!(value);
        }
        if let Some(value) = row.stats_discard_link_count {
            stats["discardLinkCount"] = json!(value);
        }
        let mut object = json!({
            "engine": row.engine,
            "status": row.status,
            "modelVersion": row.model_version,
            "stats": stats,
            "totalInput": { "exact": row.total_input_exact, "decimal": row.total_input_decimal },
            "totalOutput": { "exact": row.total_output_exact, "decimal": row.total_output_decimal },
            "discardRate": { "exact": row.discard_rate_exact, "decimal": row.discard_rate_decimal },
            "beltRate": { "exact": row.belt_rate_exact, "decimal": row.belt_rate_decimal },
            "nodes": nodes.get(&key).cloned().unwrap_or_default(),
            "edges": edges.get(&key).cloned().unwrap_or_default(),
            "buildSteps": steps.get(&key).cloned().unwrap_or_default(),
        });
        if let Some(proof) = solution_proof_json(&row) {
            object["proof"] = proof;
        }
        if let Some(validation) = solution_validation_json(&row) {
            object["validation"] = validation;
        }
        map.entry(row.entry_id)
            .or_default()
            .push((row.source_index, object));
    }
    Ok(map
        .into_iter()
        .map(|(id, mut rows)| {
            rows.sort_by_key(|(index, _)| *index);
            (id, rows.into_iter().map(|(_, value)| value).collect())
        })
        .collect())
}

struct LoadedSolution {
    entry_id: String,
    source_index: i64,
    engine: String,
    status: String,
    model_version: i64,
    proof_version: Option<i64>,
    initial_node_lower_bound: Option<i64>,
    node_counts_exhausted_through: Option<i64>,
    link_groups_exhausted: Option<i64>,
    profiles_exhausted: Option<i64>,
    root_partitions_exhausted: Option<i64>,
    validator_version: Option<i64>,
    validation_node_count: Option<i64>,
    validation_link_count: Option<i64>,
    validation_physical_link_count: Option<i64>,
    validation_discard_link_count: Option<i64>,
    cyclic_scc_count: Option<i64>,
    node_count: i64,
    splitters: i64,
    mergers: i64,
    feedback_loops: i64,
    link_count: i64,
    checked_through: Option<i64>,
    belt_count: Option<i64>,
    internal_max_throughput_exact: Option<String>,
    internal_max_throughput_decimal: Option<String>,
    stats_physical_link_count: Option<i64>,
    stats_discard_link_count: Option<i64>,
    total_input_exact: String,
    total_input_decimal: String,
    total_output_exact: String,
    total_output_decimal: String,
    discard_rate_exact: String,
    discard_rate_decimal: String,
    belt_rate_exact: String,
    belt_rate_decimal: String,
}

fn solution_proof_json(row: &LoadedSolution) -> Option<Value> {
    if row.proof_version.is_none() && row.initial_node_lower_bound.is_none() {
        return None;
    }
    Some(json!({
        "proofVersion": row.proof_version,
        "initialNodeLowerBound": row.initial_node_lower_bound,
        "nodeCountsExhaustedThrough": row.node_counts_exhausted_through,
        "linkGroupsExhausted": row.link_groups_exhausted,
        "profilesExhausted": row.profiles_exhausted,
        "rootPartitionsExhausted": row.root_partitions_exhausted,
    }))
}

fn solution_validation_json(row: &LoadedSolution) -> Option<Value> {
    row.validator_version.map(|validator_version| {
        json!({
            "validatorVersion": validator_version,
            "nodeCount": row.validation_node_count,
            "linkCount": row.validation_link_count,
            "physicalLinkCount": row.validation_physical_link_count,
            "discardLinkCount": row.validation_discard_link_count,
            "cyclicSccCount": row.cyclic_scc_count,
        })
    })
}

fn load_solution_nodes(conn: &Connection) -> Result<HashMap<(String, i64), Vec<Value>>, String> {
    let mut map: HashMap<(String, i64), Vec<Value>> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT entry_id, source_index, node_id, kind, label FROM solution_nodes ORDER BY entry_id, source_index, node_id",
        )
        .map_err(|error| format!("prepare history solution nodes: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| format!("query history solution nodes: {error}"))?;
    for row in rows {
        let (entry_id, source_index, node_id, kind, label) =
            row.map_err(|error| format!("read history solution node: {error}"))?;
        map.entry((entry_id, source_index))
            .or_default()
            .push(json!({
                "id": node_id,
                "kind": kind,
                "label": label,
            }));
    }
    Ok(map)
}

fn load_solution_edges(conn: &Connection) -> Result<HashMap<(String, i64), Vec<Value>>, String> {
    let mut map: HashMap<(String, i64), Vec<Value>> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "
            SELECT entry_id, source_index, edge_id, source, target, source_port, target_port,
                   rate_exact, rate_decimal, feedback, discarded
            FROM solution_edges
            ORDER BY entry_id, source_index, edge_id
            ",
        )
        .map_err(|error| format!("prepare history solution edges: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, i64>(9)? != 0,
                row.get::<_, i64>(10)? != 0,
            ))
        })
        .map_err(|error| format!("query history solution edges: {error}"))?;
    for row in rows {
        let (
            entry_id,
            source_index,
            edge_id,
            source,
            target,
            source_port,
            target_port,
            rate_exact,
            rate_decimal,
            feedback,
            discarded,
        ) = row.map_err(|error| format!("read history solution edge: {error}"))?;
        map.entry((entry_id, source_index))
            .or_default()
            .push(json!({
                "id": edge_id,
                "source": source,
                "target": target,
                "sourcePort": source_port,
                "targetPort": target_port,
                "rate": { "exact": rate_exact, "decimal": rate_decimal },
                "feedback": feedback,
                "discarded": discarded,
            }));
    }
    Ok(map)
}

fn load_solution_steps(conn: &Connection) -> Result<HashMap<(String, i64), Vec<Value>>, String> {
    let mut map: HashMap<(String, i64), Vec<Value>> = HashMap::new();
    let mut stmt = conn
        .prepare(
            "SELECT entry_id, source_index, ordinal, body FROM solution_build_steps ORDER BY entry_id, source_index, ordinal",
        )
        .map_err(|error| format!("prepare history build steps: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| format!("query history build steps: {error}"))?;
    for row in rows {
        let (entry_id, source_index, body) =
            row.map_err(|error| format!("read history build step: {error}"))?;
        map.entry((entry_id, source_index))
            .or_default()
            .push(json!(body));
    }
    Ok(map)
}

fn load_all_sort_columns(conn: &Connection) -> Result<HashMap<String, Value>, String> {
    let mut map: HashMap<String, Vec<Value>> = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT entry_id, key, dir FROM sort_columns ORDER BY entry_id, ordinal")
        .map_err(|error| format!("prepare history sort columns: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| format!("query history sort columns: {error}"))?;
    for row in rows {
        let (entry_id, key, dir) =
            row.map_err(|error| format!("read history sort column: {error}"))?;
        map.entry(entry_id)
            .or_default()
            .push(json!({ "key": key, "dir": dir }));
    }
    Ok(map
        .into_iter()
        .map(|(id, columns)| (id, Value::Array(columns)))
        .collect())
}

fn load_all_layouts(conn: &Connection) -> Result<HashMap<String, Value>, String> {
    let mut map: HashMap<String, Map<String, Value>> = HashMap::new();
    let mut stmt = conn
        .prepare("SELECT entry_id, source_index, layout_key, nodes_json, edges_json FROM layouts")
        .map_err(|error| format!("prepare history layouts: {error}"))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| format!("query history layouts: {error}"))?;
    for row in rows {
        let (entry_id, source_index, layout_key, nodes_json, edges_json) =
            row.map_err(|error| format!("read history layout: {error}"))?;
        map.entry(entry_id).or_default().insert(
            source_index.to_string(),
            json!({
                "layoutKey": layout_key,
                "nodes": parse_json(&nodes_json, "layout nodes")?,
                "edges": parse_json(&edges_json, "layout edges")?,
            }),
        );
    }
    Ok(map
        .into_iter()
        .map(|(id, object)| (id, Value::Object(object)))
        .collect())
}

fn solve_mode(value: &Value) -> String {
    match value.get("solveMode").and_then(Value::as_str) {
        Some("all_at_minimum_nodes_and_minimum_links") => {
            "all_at_minimum_nodes_and_minimum_links".to_owned()
        }
        Some("all_at_minimum_nodes") => "all_at_minimum_nodes".to_owned(),
        _ => "optimal".to_owned(),
    }
}

fn engine(value: &Value) -> String {
    match value.get("engine").and_then(Value::as_str) {
        Some("z3") => "z3".to_owned(),
        _ => "custom".to_owned(),
    }
}

fn display_rate(value: &Value, label: &str) -> Result<(String, String), String> {
    Ok((
        value
            .get("exact")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("history {label} missing exact"))?
            .to_owned(),
        value
            .get("decimal")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("history {label} missing decimal"))?
            .to_owned(),
    ))
}

fn parse_json(text: &str, label: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("invalid {label} json: {error}"))
}

fn compact_json(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("encode json: {error}"))
}

fn required_value<'a>(entry: &'a Value, key: &str) -> Result<&'a Value, String> {
    entry
        .get(key)
        .ok_or_else(|| format!("history entry missing {key}"))
}

fn required_str<'a>(entry: &'a Value, key: &str) -> Result<&'a str, String> {
    required_value(entry, key)?
        .as_str()
        .ok_or_else(|| format!("history entry {key} must be a string"))
}

fn required_i64(entry: &Value, key: &str) -> Result<i64, String> {
    required_value(entry, key)?
        .as_i64()
        .ok_or_else(|| format!("history entry {key} must be an integer"))
}

fn optional_str<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(Value::as_str)
}

fn optional_i64(entry: &Value, key: &str) -> Option<i64> {
    entry.get(key).and_then(Value::as_i64)
}

fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| format!("read meta {key}: {error}"))
}

fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
    conn.execute(
        "
        INSERT INTO meta (key, value) VALUES (?1, ?2)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        ",
        params![key, value],
    )
    .map_err(|error| format!("write meta {key}: {error}"))?;
    Ok(())
}

pub fn init_history_store(app: &AppHandle) -> Result<HistoryStore, String> {
    let dir: PathBuf = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("resolve app local data dir: {error}"))?;
    HistoryStore::open(&dir)
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn load_history(state: State<'_, HistoryStore>) -> Result<Value, String> {
    state.load_document()
}

#[tauri::command]
#[allow(clippy::needless_pass_by_value)]
pub fn apply_history_ops(
    state: State<'_, HistoryStore>,
    ops: Vec<HistoryOp>,
) -> Result<(), String> {
    state.apply_ops(&ops)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry() -> Value {
        json!({
            "id": "entry-1",
            "title": "Z3 layout",
            "status": "cancelled",
            "progress": {
                "phase": "searching",
                "elapsedMs": 12,
                "nodeCount": 2,
                "linkConstraint": null,
                "nodeLowerBound": 2,
                "bestNodeCount": 2,
                "bestLinkCount": null,
                "solutionsFound": 1,
                "custom": [{
                    "name": "test.counter",
                    "label": "Counter",
                    "value": { "type": "integer", "value": "18446744073709551615" },
                    "unit": null
                }]
            },
            "proof": { "minimumNodeCount": 2, "minimumLinkCount": null },
            "sequence": 7,
            "createdAtMs": 1,
            "updatedAtMs": 2,
            "startedAtMs": 1,
            "finishedAtMs": 2,
            "enumerationComplete": true,
            "error": null,
            "selectedSourceIndex": 0,
            "request": {
                "inputs": [{ "id": "i1", "name": "Iron", "rate": "60" }],
                "outputs": [{ "id": "o1", "name": "Output", "rate": "60" }],
                "beltRate": "1200",
                "solveMode": "optimal",
                "engine": "z3"
            },
            "form": {
                "inputs": [{ "id": "i1", "name": "Iron", "rate": "60", "multiplier": "1" }],
                "outputs": [{ "id": "o1", "name": "Output", "rate": "60", "multiplier": "1" }],
                "beltRate": "1200",
                "solveMode": "optimal",
                "engine": "z3"
            },
            "result": null,
            "results": [{
                "engine": "z3",
                "status": "best_known",
                "modelVersion": 1,
                "stats": {
                    "nodeCount": 2,
                    "splitters": 1,
                    "mergers": 0,
                    "feedbackLoops": 0,
                    "linkCount": 1
                },
                "totalInput": { "exact": "60", "decimal": "60" },
                "totalOutput": { "exact": "60", "decimal": "60" },
                "discardRate": { "exact": "0", "decimal": "0" },
                "beltRate": { "exact": "1200", "decimal": "1200" },
                "nodes": [{ "id": "n1", "kind": "splitter2", "label": "S" }],
                "edges": [{
                    "id": "e1",
                    "source": "n1",
                    "target": "n1",
                    "sourcePort": 0,
                    "targetPort": 1,
                    "rate": { "exact": "60", "decimal": "60" },
                    "feedback": false,
                    "discarded": false
                }],
                "buildSteps": ["place splitter"]
            }],
            "sortColumns": [{ "key": "nodes", "dir": "asc" }],
            "layouts": {
                "0": {
                    "layoutKey": "layout-a",
                    "nodes": [{ "id": "flow-1", "position": { "x": 1, "y": 2 } }],
                    "edges": []
                }
            }
        })
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn upsert_round_trips_relational_entry() {
        let directory = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(directory.path()).unwrap();
        let entry = sample_entry();
        store
            .apply_ops(&[
                HistoryOp::UpsertEntry {
                    entry: entry.clone(),
                },
                HistoryOp::SetSelected {
                    id: Some("entry-1".to_owned()),
                },
            ])
            .unwrap();

        {
            let conn = store.conn.lock().unwrap();
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM entries"), 1);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM entry_endpoints"), 4);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM solutions"), 1);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM solution_nodes"), 1);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM solution_edges"), 1);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM solution_build_steps"), 1);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM sort_columns"), 1);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM layouts"), 1);
        }

        let loaded = store.load_document().unwrap();
        assert_eq!(loaded["version"], HISTORY_DOCUMENT_VERSION);
        assert_eq!(loaded["selectedEntryId"], "entry-1");
        assert_eq!(loaded["entries"][0]["request"]["solveMode"], "optimal");
        assert_eq!(loaded["entries"][0]["request"]["engine"], "z3");
        assert_eq!(loaded["entries"][0]["results"][0]["nodes"][0]["id"], "n1");
        assert_eq!(loaded["entries"][0]["result"]["status"], "best_known");
        assert_eq!(
            loaded["entries"][0]["layouts"]["0"]["layoutKey"],
            "layout-a"
        );
        assert_eq!(loaded["entries"][0]["sequence"], 7);
        assert_eq!(loaded["entries"][0]["proof"]["minimumNodeCount"], 2);
    }

    #[test]
    fn patch_and_layout_ops_leave_graphs_in_place() {
        let directory = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(directory.path()).unwrap();
        store
            .apply_ops(&[HistoryOp::UpsertEntry {
                entry: sample_entry(),
            }])
            .unwrap();
        store
            .apply_ops(&[HistoryOp::PatchEntry {
                id: "entry-1".to_owned(),
                fields: json!({
                    "title": "Renamed",
                    "status": "cancelled",
                    "createdAtMs": 1,
                    "updatedAtMs": 9,
                    "startedAtMs": 1,
                    "finishedAtMs": 2,
                    "enumerationComplete": true,
                    "error": null,
                    "selectedSourceIndex": 0
                }),
            }])
            .unwrap();
        store
            .apply_ops(&[HistoryOp::SaveLayouts {
                id: "entry-1".to_owned(),
                layouts: json!({
                    "0": { "layoutKey": "layout-b", "nodes": [], "edges": [] }
                }),
            }])
            .unwrap();

        let loaded = store.load_document().unwrap();
        assert_eq!(loaded["entries"][0]["title"], "Renamed");
        assert_eq!(
            loaded["entries"][0]["layouts"]["0"]["layoutKey"],
            "layout-b"
        );
        assert_eq!(loaded["entries"][0]["results"][0]["nodes"][0]["id"], "n1");
        let conn = store.conn.lock().unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM solution_nodes"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM layouts"), 1);
    }

    #[test]
    fn delete_cascades_and_reorder_updates_sort_order() {
        let directory = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(directory.path()).unwrap();
        let mut second = sample_entry();
        second["id"] = json!("entry-2");
        store
            .apply_ops(&[
                HistoryOp::UpsertEntry {
                    entry: sample_entry(),
                },
                HistoryOp::UpsertEntry { entry: second },
                HistoryOp::ReorderEntries {
                    ids: vec!["entry-2".to_owned(), "entry-1".to_owned()],
                },
            ])
            .unwrap();
        let loaded = store.load_document().unwrap();
        assert_eq!(loaded["entries"][0]["id"], "entry-2");
        assert_eq!(loaded["entries"][1]["id"], "entry-1");

        store
            .apply_ops(&[HistoryOp::DeleteEntries {
                ids: vec!["entry-1".to_owned()],
            }])
            .unwrap();
        {
            let conn = store.conn.lock().unwrap();
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM entries"), 1);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM solutions"), 1);
            assert_eq!(count(&conn, "SELECT COUNT(*) FROM solution_nodes"), 1);
        }
        let loaded = store.load_document().unwrap();
        assert_eq!(loaded["entries"].as_array().unwrap().len(), 1);
        assert_eq!(loaded["entries"][0]["id"], "entry-2");
    }

    #[test]
    fn wrong_schema_version_replaces_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(directory.path()).unwrap();
        store
            .apply_ops(&[HistoryOp::UpsertEntry {
                entry: sample_entry(),
            }])
            .unwrap();
        drop(store);

        {
            let conn = Connection::open(directory.path().join(DB_FILE)).unwrap();
            conn.pragma_update(None, "user_version", 1).unwrap();
        }

        let loaded = HistoryStore::open(directory.path())
            .unwrap()
            .load_document()
            .unwrap();
        assert_eq!(loaded["entries"].as_array().unwrap().len(), 0);
    }
}
