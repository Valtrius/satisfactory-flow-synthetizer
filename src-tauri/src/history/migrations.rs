use super::{
    values::{meta_get, meta_set, parse_json},
    write::upsert_entry,
};
use rusqlite::Connection;
use serde_json::{Map, Value, json};
use std::path::Path;

pub(super) const SCHEMA_USER_VERSION: i32 = 4;
pub(super) const SCHEMA_SQL: &str = include_str!("schema.sql");

pub(super) fn open_connection(db_path: &Path) -> Result<Connection, String> {
    let mut conn = Connection::open(db_path).map_err(|e| format!("open history database: {e}"))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|e| format!("configure history database: {e}"))?;
    let version: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|e| format!("read history schema version: {e}"))?;
    if version == SCHEMA_USER_VERSION {
        return Ok(conn);
    }
    let has_entries: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='entries')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("inspect history schema: {e}"))?;
    if version == 0 && !has_entries {
        conn.execute_batch(SCHEMA_SQL)
            .map_err(|e| format!("init history schema: {e}"))?;
        return Ok(conn);
    }
    if version <= 2 && has_entries {
        let json_schema: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('entries') WHERE name='request_json')", [], |r| r.get(0))
            .map_err(|e| format!("inspect JSON history: {e}"))?;
        if json_schema {
            migrate_json_history(&mut conn)?;
            return Ok(conn);
        }
    }
    if version != 3 {
        return Err(format!(
            "Unsupported history schema {version}; database preserved."
        ));
    }
    let tx = conn
        .transaction()
        .map_err(|e| format!("begin history migration: {e}"))?;
    tx.execute_batch(
        "ALTER TABLE entries DROP COLUMN request_engine;
        ALTER TABLE entries DROP COLUMN form_engine;
        ALTER TABLE solutions DROP COLUMN engine;
        UPDATE layouts SET layout_key = CASE
          WHEN layout_key LIKE 'custom::%' THEN substr(layout_key, 9)
          WHEN layout_key LIKE 'astra::%' THEN substr(layout_key, 8)
          WHEN layout_key LIKE 'z3::%' THEN substr(layout_key, 5)
          ELSE layout_key END;
        PRAGMA user_version = 4;",
    )
    .map_err(|e| format!("migrate history schema: {e}"))?;
    tx.commit()
        .map_err(|e| format!("commit history migration: {e}"))?;
    Ok(conn)
}

/// Migrate the original JSON-column schema atomically, preserving all rows and saved graph edits.
fn migrate_json_history(conn: &mut Connection) -> Result<(), String> {
    let tx = conn
        .transaction()
        .map_err(|e| format!("begin JSON history migration: {e}"))?;
    let has_state: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='solver_state')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| format!("inspect JSON solver state: {e}"))?;
    let mut entries = json_schema_rows(&tx, "SELECT * FROM entries ORDER BY sort_order")?;
    if has_state {
        let states = json_schema_rows(&tx, "SELECT * FROM solver_state")?;
        for entry in &mut entries {
            if let Some(state) = states
                .iter()
                .find(|state| state.get("entryId") == entry.get("id"))
            {
                for field in ["progress", "proof", "sequence"] {
                    if let Some(value) = state.get(field) {
                        entry[field] = value.clone();
                    }
                }
            }
        }
    }
    let selected = meta_get(&tx, "selected_entry_id")?;
    tx.execute_batch("DROP TABLE IF EXISTS solver_state; DROP TABLE entries; DROP TABLE meta;")
        .map_err(|e| format!("replace JSON history schema: {e}"))?;
    let schema = SCHEMA_SQL
        .replace("PRAGMA journal_mode = WAL;", "")
        .replace("PRAGMA foreign_keys = ON;", "");
    tx.execute_batch(&schema)
        .map_err(|e| format!("create migrated history schema: {e}"))?;
    for entry in entries {
        upsert_entry(&tx, &entry)?;
    }
    if let Some(id) = selected {
        meta_set(&tx, "selected_entry_id", &id)?;
    }
    tx.commit()
        .map_err(|e| format!("commit JSON history migration: {e}"))
}

fn json_schema_rows(conn: &Connection, query: &str) -> Result<Vec<Value>, String> {
    use rusqlite::types::ValueRef;
    let mut statement = conn
        .prepare(query)
        .map_err(|e| format!("read JSON history: {e}"))?;
    let columns = statement
        .column_names()
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let mut rows = statement
        .query([])
        .map_err(|e| format!("query JSON history: {e}"))?;
    let mut result = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| format!("read JSON schema row: {e}"))?
    {
        let mut record = Map::new();
        for (index, name) in columns.iter().enumerate() {
            if name == "sort_order" {
                continue;
            }
            let mut parts = name.trim_end_matches("_json").split('_');
            let mut key = parts.next().unwrap().to_owned();
            for part in parts {
                let mut chars = part.chars();
                if let Some(c) = chars.next() {
                    key.extend(c.to_uppercase());
                }
                key.extend(chars);
            }
            let value = match row
                .get_ref(index)
                .map_err(|e| format!("read JSON schema value: {e}"))?
            {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(value) if name == "enumeration_complete" => json!(value != 0),
                ValueRef::Integer(value) => json!(value),
                ValueRef::Text(bytes) => {
                    let text = std::str::from_utf8(bytes)
                        .map_err(|e| format!("JSON history text: {e}"))?;
                    if name.ends_with("_json") {
                        parse_json(text, name)?
                    } else {
                        json!(text)
                    }
                }
                _ => {
                    return Err(format!(
                        "Unsupported JSON schema column {name}; migration rolled back."
                    ));
                }
            };
            record.insert(key, value);
        }
        result.push(Value::Object(record));
    }
    Ok(result)
}
