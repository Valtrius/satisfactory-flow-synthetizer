use super::{HistoryOp, values::*};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

pub(super) fn apply_op(tx: &Transaction<'_>, op: &HistoryOp) -> Result<(), String> {
    match op {
        HistoryOp::AppendSolutions {
            id,
            expected_count,
            solutions,
        } => append_solutions(tx, id, *expected_count, solutions),
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
        HistoryOp::SetSelected { id } => {
            if let Some(id) = id {
                meta_set(tx, "selected_entry_id", id)
            } else {
                tx.execute("DELETE FROM meta WHERE key = 'selected_entry_id'", [])
                    .map_err(|error| format!("clear selected history id: {error}"))?;
                Ok(())
            }
        }
    }
}

fn append_solutions(
    tx: &Transaction<'_>,
    id: &str,
    expected_count: usize,
    solutions: &[Value],
) -> Result<(), String> {
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE id = ?1)",
            params![id],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    if !exists {
        return Err(format!("history entry not found: {id}"));
    }
    let expected = i64::try_from(expected_count).map_err(|_| "history result count overflow")?;
    let (count, first, next): (i64, i64, i64) = tx.query_row(
        "SELECT COUNT(*), COALESCE(MIN(source_index), 0), COALESCE(MAX(source_index) + 1, 0) FROM solutions WHERE entry_id = ?1",
        params![id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).map_err(|e| e.to_string())?;
    if count != expected || first != 0 || next != count {
        return Err(format!("history_append_prefix_mismatch: {id}"));
    }
    for (offset, solution) in solutions.iter().enumerate() {
        let index = expected_count
            .checked_add(offset)
            .and_then(|index| i64::try_from(index).ok())
            .ok_or("history result index overflow")?;
        insert_solution(tx, id, index, solution)?;
    }
    Ok(())
}

pub(super) fn upsert_entry(tx: &Transaction<'_>, entry: &Value) -> Result<(), String> {
    let id = required_str(entry, "id")?;
    let sort_order = match existing_sort_order(tx, id)? {
        Some(order) => order,
        None => next_sort_order(tx)?,
    };
    tx.execute("DELETE FROM entries WHERE id = ?1", params![id])
        .map_err(|error| format!("replace history entry: {error}"))?;

    let request = required_value(entry, "request")?;
    let form = required_value(entry, "form")?;
    tx.execute(
        "
        INSERT INTO entries (
          id, sort_order, title, status, created_at_ms, updated_at_ms, started_at_ms,
          finished_at_ms, enumeration_complete, error, selected_source_index,
          request_belt_rate, request_solve_mode,
          form_belt_rate, form_solve_mode
        ) VALUES (
          ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15
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
            required_str(form, "beltRate")?,
            solve_mode(form),
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

pub(super) fn insert_solution(
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
          entry_id, source_index, status, model_version,
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
          ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34, ?35
        )
        ",
        params![
            entry_id,
            source_index,
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

    insert_solution_graph(tx, entry_id, source_index, solution)
}

fn insert_solution_graph(
    tx: &Transaction<'_>,
    entry_id: &str,
    source_index: i64,
    solution: &Value,
) -> Result<(), String> {
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

fn layout_cache_key(key: &str) -> &str {
    ["custom::", "z3::", "astra::"]
        .iter()
        .find_map(|prefix| key.strip_prefix(prefix))
        .unwrap_or(key)
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
                layout_cache_key(required_str(layout, "layoutKey")?),
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
