//! Persistent SQLite history. Each operation batch commits as one transaction.
mod migrations;
mod read;
mod values;
mod write;

use migrations::open_connection;
use read::load_entries;
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager, State};
use values::meta_get;
use write::apply_op;

const DB_FILE: &str = "history.sqlite";
const HISTORY_DOCUMENT_VERSION: u32 = 2;

pub struct HistoryStore {
    conn: Mutex<Connection>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum HistoryOp {
    AppendSolutions {
        id: String,
        expected_count: usize,
        solutions: Vec<Value>,
    },
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
        #[serde(alias = "sort_columns")]
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
mod tests;
