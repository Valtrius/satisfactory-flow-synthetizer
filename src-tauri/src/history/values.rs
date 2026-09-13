use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

pub(super) fn solve_mode(value: &Value) -> String {
    value
        .get("solveMode")
        .and_then(|mode| serde_json::from_value::<solver_api::SolveMode>(mode.clone()).ok())
        .map_or_else(
            || {
                if value.get("enumerateAllAtN").and_then(Value::as_bool) == Some(true) {
                    "all_min_n".to_owned()
                } else {
                    "one_min_nl".to_owned()
                }
            },
            |mode| {
                serde_json::to_value(mode)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .to_owned()
            },
        )
}

pub(super) fn display_rate(value: &Value, label: &str) -> Result<(String, String), String> {
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

pub(super) fn parse_json(text: &str, label: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|error| format!("invalid {label} json: {error}"))
}

pub(super) fn compact_json(value: &Value) -> Result<String, String> {
    serde_json::to_string(value).map_err(|error| format!("encode json: {error}"))
}

pub(super) fn required_value<'a>(entry: &'a Value, key: &str) -> Result<&'a Value, String> {
    entry
        .get(key)
        .ok_or_else(|| format!("history entry missing {key}"))
}

pub(super) fn required_str<'a>(entry: &'a Value, key: &str) -> Result<&'a str, String> {
    required_value(entry, key)?
        .as_str()
        .ok_or_else(|| format!("history entry {key} must be a string"))
}

pub(super) fn required_i64(entry: &Value, key: &str) -> Result<i64, String> {
    required_value(entry, key)?
        .as_i64()
        .ok_or_else(|| format!("history entry {key} must be an integer"))
}

pub(super) fn optional_str<'a>(entry: &'a Value, key: &str) -> Option<&'a str> {
    entry.get(key).and_then(Value::as_str)
}

pub(super) fn optional_i64(entry: &Value, key: &str) -> Option<i64> {
    entry.get(key).and_then(Value::as_i64)
}

pub(super) fn meta_get(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    conn.query_row(
        "SELECT value FROM meta WHERE key = ?1",
        params![key],
        |row| row.get(0),
    )
    .optional()
    .map_err(|error| format!("read meta {key}: {error}"))
}

pub(super) fn meta_set(conn: &Connection, key: &str, value: &str) -> Result<(), String> {
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
