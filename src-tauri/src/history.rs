//! Persistent job history in `SQLite` under the app local data directory.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, State};

const DB_FILE: &str = "history.sqlite";

pub struct HistoryStore {
    conn: Mutex<Connection>,
}

impl HistoryStore {
    pub fn open(app_local_data: &Path) -> Result<Self, String> {
        fs::create_dir_all(app_local_data)
            .map_err(|error| format!("create app data dir: {error}"))?;
        let db_path = app_local_data.join(DB_FILE);
        let conn = Connection::open(&db_path)
            .map_err(|error| format!("open history database: {error}"))?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS meta (
              key TEXT PRIMARY KEY NOT NULL,
              value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS entries (
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
              request_json TEXT NOT NULL,
              form_json TEXT NOT NULL,
              result_json TEXT,
              results_json TEXT NOT NULL DEFAULT '[]',
              sort_columns_json TEXT NOT NULL,
              layouts_json TEXT NOT NULL DEFAULT '{}'
            );
            ",
        )
        .map_err(|error| format!("init history schema: {error}"))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn load_document(&self) -> Result<Value, String> {
        let conn = self.conn.lock().map_err(|_| "history db lock poisoned")?;
        let version = meta_get(&conn, "version")?
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(1);
        let selected_entry_id = meta_get(&conn, "selected_entry_id")?;

        let mut stmt = conn
            .prepare(
                "
                SELECT
                  id, title, status, created_at_ms, updated_at_ms, started_at_ms,
                  finished_at_ms, enumeration_complete, error, selected_source_index,
                  request_json, form_json, result_json, results_json,
                  sort_columns_json, layouts_json
                FROM entries
                ORDER BY sort_order ASC
                ",
            )
            .map_err(|error| format!("prepare history load: {error}"))?;

        let rows = stmt
            .query_map([], |row| {
                Ok(EntryRow {
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
                    request_json: row.get(10)?,
                    form_json: row.get(11)?,
                    result_json: row.get(12)?,
                    results_json: row.get(13)?,
                    sort_columns_json: row.get(14)?,
                    layouts_json: row.get(15)?,
                })
            })
            .map_err(|error| format!("query history entries: {error}"))?;

        let mut entries = Vec::new();
        for row in rows {
            let row = row.map_err(|error| format!("read history entry: {error}"))?;
            entries.push(row_to_json(&row)?);
        }

        Ok(json!({
            "version": version,
            "selectedEntryId": selected_entry_id,
            "entries": entries,
        }))
    }

    pub fn save_document(&self, document: &Value) -> Result<(), String> {
        let entries = document
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let version = document.get("version").and_then(Value::as_u64).unwrap_or(1);
        let selected = document.get("selectedEntryId").and_then(|value| {
            if value.is_null() {
                None
            } else {
                value.as_str().map(str::to_owned)
            }
        });

        let mut conn = self.conn.lock().map_err(|_| "history db lock poisoned")?;
        let tx = conn
            .transaction()
            .map_err(|error| format!("begin history save: {error}"))?;

        tx.execute("DELETE FROM entries", [])
            .map_err(|error| format!("clear history entries: {error}"))?;

        for (sort_order, entry) in entries.iter().enumerate() {
            let id = required_str(entry, "id")?;
            let status = required_str(entry, "status")?;
            let title = entry.get("title").and_then(Value::as_str);
            let created_at_ms = required_i64(entry, "createdAtMs")?;
            let updated_at_ms = required_i64(entry, "updatedAtMs")?;
            let started_at_ms = entry.get("startedAtMs").and_then(Value::as_i64);
            let finished_at_ms = entry.get("finishedAtMs").and_then(Value::as_i64);
            let enumeration_complete = entry
                .get("enumerationComplete")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let error = entry.get("error").and_then(Value::as_str);
            let selected_source_index = entry
                .get("selectedSourceIndex")
                .and_then(Value::as_i64)
                .unwrap_or(0);
            let request_json = compact_json(required_value(entry, "request")?)?;
            let form_json = compact_json(required_value(entry, "form")?)?;
            let selected_result_json = entry
                .get("result")
                .filter(|value| !value.is_null())
                .map(compact_json)
                .transpose()?;
            let layouts_result_json = compact_json(entry.get("results").unwrap_or(&json!([])))?;
            let sort_columns_json = compact_json(entry.get("sortColumns").unwrap_or(&json!([])))?;
            let layouts_json = compact_json(entry.get("layouts").unwrap_or(&json!({})))?;

            tx.execute(
                "
                INSERT INTO entries (
                  id, sort_order, title, status, created_at_ms, updated_at_ms, started_at_ms,
                  finished_at_ms, enumeration_complete, error, selected_source_index,
                  request_json, form_json, result_json, results_json,
                  sort_columns_json, layouts_json
                ) VALUES (
                  ?1, ?2, ?3, ?4, ?5, ?6, ?7,
                  ?8, ?9, ?10, ?11,
                  ?12, ?13, ?14, ?15,
                  ?16, ?17
                )
                ",
                params![
                    id,
                    i64::try_from(sort_order)
                        .map_err(|_| "history entry count exceeds SQLite integer range")?,
                    title,
                    status,
                    created_at_ms,
                    updated_at_ms,
                    started_at_ms,
                    finished_at_ms,
                    i64::from(enumeration_complete),
                    error,
                    selected_source_index,
                    request_json,
                    form_json,
                    selected_result_json,
                    layouts_result_json,
                    sort_columns_json,
                    layouts_json,
                ],
            )
            .map_err(|error| format!("insert history entry: {error}"))?;
        }

        meta_set(&tx, "version", &version.to_string())?;
        match selected {
            Some(id) => meta_set(&tx, "selected_entry_id", &id)?,
            None => {
                tx.execute("DELETE FROM meta WHERE key = 'selected_entry_id'", [])
                    .map_err(|error| format!("clear selected history id: {error}"))?;
            }
        }

        tx.commit()
            .map_err(|error| format!("commit history save: {error}"))?;
        Ok(())
    }
}

struct EntryRow {
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
    request_json: String,
    form_json: String,
    result_json: Option<String>,
    results_json: String,
    sort_columns_json: String,
    layouts_json: String,
}

fn row_to_json(row: &EntryRow) -> Result<Value, String> {
    Ok(json!({
        "id": row.id,
        "title": row.title,
        "status": row.status,
        "createdAtMs": row.created_at_ms,
        "updatedAtMs": row.updated_at_ms,
        "startedAtMs": row.started_at_ms,
        "finishedAtMs": row.finished_at_ms,
        "enumerationComplete": row.enumeration_complete,
        "error": row.error,
        "selectedSourceIndex": row.selected_source_index,
        "request": parse_json(&row.request_json, "request")?,
        "form": parse_json(&row.form_json, "form")?,
        "result": match &row.result_json {
            Some(text) => parse_json(text, "result")?,
            None => Value::Null,
        },
        "results": parse_json(&row.results_json, "results")?,
        "sortColumns": parse_json(&row.sort_columns_json, "sortColumns")?,
        "layouts": parse_json(&row.layouts_json, "layouts")?,
        "jobId": Value::Null,
        "progress": Value::Null,
    }))
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
pub fn save_history(state: State<'_, HistoryStore>, document: Value) -> Result<(), String> {
    state.save_document(&document)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_database_round_trips_engine_and_terminal_status() {
        let directory = tempfile::tempdir().unwrap();
        let store = HistoryStore::open(directory.path()).unwrap();
        let document = json!({
            "version": 1,
            "selectedEntryId": "entry-1",
            "entries": [{
                "id": "entry-1",
                "title": "Z3 layout",
                "status": "unsat",
                "createdAtMs": 1,
                "updatedAtMs": 2,
                "startedAtMs": 1,
                "finishedAtMs": 2,
                "enumerationComplete": true,
                "error": null,
                "selectedSourceIndex": 0,
                "request": {
                    "inputs": [],
                    "outputs": [{"id": "o1", "name": "Output", "rate": "60"}],
                    "beltRate": "1200",
                    "enumerateAllAtN": false,
                    "engine": "z3"
                },
                "form": {
                    "inputs": [],
                    "outputs": [{"id": "o1", "name": "Output", "rate": "60", "multiplier": "1"}],
                    "beltRate": "1200",
                    "enumerateAllAtN": false,
                    "engine": "z3"
                },
                "result": null,
                "results": [],
                "sortColumns": [],
                "layouts": {}
            }]
        });

        store.save_document(&document).unwrap();
        let loaded = store.load_document().unwrap();

        assert_eq!(loaded["selectedEntryId"], "entry-1");
        assert_eq!(loaded["entries"][0]["status"], "unsat");
        assert_eq!(loaded["entries"][0]["request"]["engine"], "z3");
        assert_eq!(loaded["entries"][0]["form"]["engine"], "z3");
    }
}
