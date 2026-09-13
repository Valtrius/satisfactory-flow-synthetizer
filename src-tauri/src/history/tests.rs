use super::write::insert_solution;
use super::*;
use rusqlite::params;

#[test]
fn frontend_incremental_operations_match_full_replacement_at_every_checkpoint() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../history-fixtures/incremental.json")).unwrap();
    let left_dir = tempfile::tempdir().unwrap();
    let right_dir = tempfile::tempdir().unwrap();
    let left = HistoryStore::open(left_dir.path()).unwrap();
    let right = HistoryStore::open(right_dir.path()).unwrap();
    let mut entry = fixture["initial"].clone();
    for store in [&left, &right] {
        store
            .apply_ops(&[
                HistoryOp::UpsertEntry {
                    entry: entry.clone(),
                },
                HistoryOp::SetSelected {
                    id: Some("entry".into()),
                },
            ])
            .unwrap();
    }
    for checkpoint in fixture["checkpoints"].as_array().unwrap() {
        entry
            .as_object_mut()
            .unwrap()
            .extend(checkpoint["patch"].as_object().unwrap().clone());
        let ops: Vec<HistoryOp> = serde_json::from_value(checkpoint["ops"].clone()).unwrap();
        left.apply_ops(&ops).unwrap();
        right
            .apply_ops(&[HistoryOp::UpsertEntry {
                entry: entry.clone(),
            }])
            .unwrap();
        assert_eq!(
            left.load_document().unwrap(),
            right.load_document().unwrap(),
            "{}",
            checkpoint["name"]
        );
    }
}

#[test]
fn appends_preserve_graph_edits_and_roll_back_the_whole_batch_on_failure() {
    let left_dir = tempfile::tempdir().unwrap();
    let right_dir = tempfile::tempdir().unwrap();
    let left = HistoryStore::open(left_dir.path()).unwrap();
    let right = HistoryStore::open(right_dir.path()).unwrap();
    let mut entry = sample_entry();
    let additional = entry["results"][0].clone();
    left.apply_ops(&[
        HistoryOp::UpsertEntry {
            entry: entry.clone(),
        },
        HistoryOp::SetSelected {
            id: Some("entry-1".into()),
        },
    ])
    .unwrap();
    let append: HistoryOp = serde_json::from_value(json!({
        "op":"appendSolutions", "id":"entry-1", "expectedCount":1, "solutions":[additional.clone()]
    }))
    .unwrap();
    left.apply_ops(&[append]).unwrap();
    entry["results"]
        .as_array_mut()
        .unwrap()
        .push(additional.clone());
    right
        .apply_ops(&[
            HistoryOp::UpsertEntry { entry },
            HistoryOp::SetSelected {
                id: Some("entry-1".into()),
            },
        ])
        .unwrap();
    let expected = right.load_document().unwrap();
    assert_eq!(left.load_document().unwrap(), expected);
    let error = left
        .apply_ops(&[
            HistoryOp::AppendSolutions {
                id: "entry-1".into(),
                expected_count: 2,
                solutions: vec![additional.clone()],
            },
            HistoryOp::SetSelected { id: None },
            HistoryOp::AppendSolutions {
                id: "entry-1".into(),
                expected_count: 99,
                solutions: vec![additional],
            },
        ])
        .unwrap_err();
    assert_eq!(error, "history_append_prefix_mismatch: entry-1");
    assert_eq!(left.load_document().unwrap(), expected);
    assert!(
        left.apply_ops(&[HistoryOp::AppendSolutions {
            id: "entry-1".into(),
            expected_count: 2,
            solutions: vec![json!({})],
        }])
        .is_err()
    );
    assert_eq!(left.load_document().unwrap(), expected);
}

#[test]
fn appends_reject_missing_entries_and_noncontiguous_saved_prefixes() {
    let directory = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(directory.path()).unwrap();
    assert!(
        store
            .apply_ops(&[HistoryOp::AppendSolutions {
                id: "missing".into(),
                expected_count: 0,
                solutions: Vec::new(),
            }])
            .unwrap_err()
            .contains("not found")
    );
    let entry = sample_entry();
    store
        .apply_ops(&[HistoryOp::UpsertEntry {
            entry: entry.clone(),
        }])
        .unwrap();
    {
        let mut conn = store.conn.lock().unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute("DELETE FROM solutions WHERE entry_id = 'entry-1'", [])
            .unwrap();
        insert_solution(&tx, "entry-1", 2, &entry["results"][0]).unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(
        store
            .apply_ops(&[HistoryOp::AppendSolutions {
                id: "entry-1".into(),
                expected_count: 1,
                solutions: Vec::new(),
            }])
            .unwrap_err(),
        "history_append_prefix_mismatch: entry-1"
    );
    assert!(
        serde_json::from_value::<HistoryOp>(json!({
            "op":"appendSolutions", "id":"entry-1", "expectedCount":-1, "solutions":[]
        }))
        .is_err()
    );
}

#[test]
fn frontend_sort_column_payload_persists_without_rewriting_graphs() {
    let directory = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(directory.path()).unwrap();
    store
        .apply_ops(&[HistoryOp::UpsertEntry {
            entry: sample_entry(),
        }])
        .unwrap();
    let columns = json!([{ "key": "belts", "dir": "desc" }]);
    let operation: HistoryOp = serde_json::from_value(json!({
        "op": "saveSortColumns", "id": "entry-1", "sortColumns": columns
    }))
    .unwrap();
    store.apply_ops(&[operation]).unwrap();
    let loaded = store.load_document().unwrap();
    assert_eq!(loaded["entries"][0]["sortColumns"], columns);
    assert_eq!(loaded["entries"][0]["results"][0]["nodes"][0]["id"], "n1");
}

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
fn solver_type_fields_are_accepted_and_never_persisted() {
    let directory = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(directory.path()).unwrap();
    let mut entry = sample_entry();
    entry["form"]["engine"] = json!("astra");
    entry["request"]["engine"] = json!("astra");
    entry["result"]["engine"] = json!("astra");
    entry["results"][0]["engine"] = json!("astra");
    store
        .apply_ops(&[HistoryOp::UpsertEntry { entry }])
        .unwrap();
    let loaded = store.load_document().unwrap();
    let entry = &loaded["entries"][0];
    for field in ["form", "request", "result"] {
        assert!(entry[field].get("engine").is_none(), "{field}");
    }
    assert!(entry["results"][0].get("engine").is_none());
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
    assert_eq!(loaded["entries"][0]["request"]["solveMode"], "one_min_nl");
    assert!(loaded["entries"][0]["request"].get("engine").is_none());
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
fn unsupported_schema_preserves_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(directory.path()).unwrap();
    store
        .apply_ops(&[HistoryOp::UpsertEntry {
            entry: sample_entry(),
        }])
        .unwrap();
    drop(store);
    let conn = Connection::open(directory.path().join(DB_FILE)).unwrap();
    conn.pragma_update(None, "user_version", 99).unwrap();
    assert!(HistoryStore::open(directory.path()).is_err());
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM entries"), 1);
}

#[test]
fn version_three_migration_preserves_complete_document_and_drops_engine_columns() {
    let directory = tempfile::tempdir().unwrap();
    let store = HistoryStore::open(directory.path()).unwrap();
    store
        .apply_ops(&[
            HistoryOp::UpsertEntry {
                entry: sample_entry(),
            },
            HistoryOp::SetSelected {
                id: Some("entry-1".to_owned()),
            },
        ])
        .unwrap();
    let expected = store.load_document().unwrap();
    drop(store);
    let conn = Connection::open(directory.path().join(DB_FILE)).unwrap();
    conn.execute_batch(
        "ALTER TABLE entries ADD COLUMN request_engine TEXT NOT NULL DEFAULT 'custom';
      ALTER TABLE entries ADD COLUMN form_engine TEXT NOT NULL DEFAULT 'z3';
      ALTER TABLE solutions ADD COLUMN engine TEXT NOT NULL DEFAULT 'astra';
      UPDATE layouts SET layout_key = 'z3::' || layout_key;
      PRAGMA user_version = 3;",
    )
    .unwrap();
    drop(conn);
    let migrated = HistoryStore::open(directory.path()).unwrap();
    assert_eq!(migrated.load_document().unwrap(), expected);
    let conn = migrated.conn.lock().unwrap();
    for table in ["entries", "solutions"] {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({table})"))
            .unwrap();
        let names = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(names.iter().all(|name| !name.contains("engine")));
    }
    drop(conn);
    drop(migrated);
    assert_eq!(
        HistoryStore::open(directory.path())
            .unwrap()
            .load_document()
            .unwrap(),
        expected
    );
}
#[test]
fn json_schema_history_migrates_without_losing_graphs_or_selection() {
    let directory = tempfile::tempdir().unwrap();
    let current = HistoryStore::open(directory.path()).unwrap();
    current
        .apply_ops(&[
            HistoryOp::UpsertEntry {
                entry: sample_entry(),
            },
            HistoryOp::SetSelected {
                id: Some("entry-1".to_owned()),
            },
        ])
        .unwrap();
    let expected = current.load_document().unwrap();
    drop(current);
    // Construct the version 2 JSON-column schema in a separate database.
    let json_directory = tempfile::tempdir().unwrap();
    let conn = Connection::open(json_directory.path().join(DB_FILE)).unwrap();
    conn.execute_batch(include_str!("../history-fixtures/v2.sql"))
        .unwrap();
    let entry = sample_entry();
    conn.execute("INSERT INTO entries (id,sort_order,title,status,created_at_ms,updated_at_ms,started_at_ms,finished_at_ms,enumeration_complete,error,selected_source_index,request_json,form_json,result_json,results_json,sort_columns_json,layouts_json)
        VALUES (?1,0,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)", params![
        entry["id"].as_str(), entry["title"].as_str(),entry["status"].as_str(),entry["createdAtMs"].as_i64(),entry["updatedAtMs"].as_i64(),entry["startedAtMs"].as_i64(),entry["finishedAtMs"].as_i64(),i64::from(entry["enumerationComplete"].as_bool().unwrap()),entry["error"].as_str(),entry["selectedSourceIndex"].as_i64(),
        entry["request"].to_string(),entry["form"].to_string(),entry["result"].to_string(),entry["results"].to_string(),entry["sortColumns"].to_string(),entry["layouts"].to_string()]).unwrap();
    conn.execute(
        "INSERT INTO solver_state VALUES (?1,?2,?3,?4)",
        params![
            "entry-1",
            entry["progress"].to_string(),
            entry["proof"].to_string(),
            entry["sequence"].as_i64()
        ],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO meta VALUES ('selected_entry_id','entry-1')",
        [],
    )
    .unwrap();
    // A failed migration must leave the source data and schema intact.
    conn.execute("UPDATE entries SET request_json = 'malformed'", [])
        .unwrap();
    assert!(HistoryStore::open(json_directory.path()).is_err());
    assert_eq!(count(&conn, "SELECT COUNT(*) FROM entries"), 1);
    conn.execute(
        "UPDATE entries SET request_json = ?1",
        params![entry["request"].to_string()],
    )
    .unwrap();
    drop(conn);
    let migrated = HistoryStore::open(json_directory.path()).unwrap();
    assert_eq!(migrated.load_document().unwrap(), expected);
}
