use super::values::{parse_json, solve_mode};
use rusqlite::Connection;
use serde_json::{Map, Value, json};
use std::collections::HashMap;

#[allow(clippy::too_many_lines)]
pub(super) fn load_entries(conn: &Connection) -> Result<Vec<Value>, String> {
    let mut entries = Vec::new();
    let mut stmt = conn
        .prepare(
            "
            SELECT
              id, title, status, created_at_ms, updated_at_ms, started_at_ms, finished_at_ms,
              enumeration_complete, error, selected_source_index,
              request_belt_rate, request_solve_mode,
              form_belt_rate, form_solve_mode,
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
                form_belt_rate: row.get(12)?,
                form_solve_mode: row.get(13)?,
                progress_json: row.get(14)?,
                proof_json: row.get(15)?,
                sequence: row.get(16)?,
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
                    "solveMode": solve_mode(&json!({"solveMode": entry.request_solve_mode})),
                },
                "form": {
                    "inputs": form_inputs,
                    "outputs": form_outputs,
                    "beltRate": entry.form_belt_rate,
                    "solveMode": solve_mode(&json!({"solveMode": entry.form_solve_mode})),
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
    form_belt_rate: String,
    form_solve_mode: String,
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
                status: row.get(2)?,
                model_version: row.get(3)?,
                proof_version: row.get(4)?,
                initial_node_lower_bound: row.get(5)?,
                node_counts_exhausted_through: row.get(6)?,
                link_groups_exhausted: row.get(7)?,
                profiles_exhausted: row.get(8)?,
                root_partitions_exhausted: row.get(9)?,
                validator_version: row.get(10)?,
                validation_node_count: row.get(11)?,
                validation_link_count: row.get(12)?,
                validation_physical_link_count: row.get(13)?,
                validation_discard_link_count: row.get(14)?,
                cyclic_scc_count: row.get(15)?,
                node_count: row.get(16)?,
                splitters: row.get(17)?,
                mergers: row.get(18)?,
                feedback_loops: row.get(19)?,
                link_count: row.get(20)?,
                checked_through: row.get(21)?,
                belt_count: row.get(22)?,
                internal_max_throughput_exact: row.get(23)?,
                internal_max_throughput_decimal: row.get(24)?,
                stats_physical_link_count: row.get(25)?,
                stats_discard_link_count: row.get(26)?,
                total_input_exact: row.get(27)?,
                total_input_decimal: row.get(28)?,
                total_output_exact: row.get(29)?,
                total_output_decimal: row.get(30)?,
                discard_rate_exact: row.get(31)?,
                discard_rate_decimal: row.get(32)?,
                belt_rate_exact: row.get(33)?,
                belt_rate_decimal: row.get(34)?,
            })
        })
        .map_err(|error| format!("query history solutions: {error}"))?;
    for row in rows {
        let row = row.map_err(|error| format!("read history solution: {error}"))?;
        let key = (row.entry_id.clone(), row.source_index);
        let stats = solution_stats_json(&row);
        let mut object = json!({
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

fn solution_stats_json(row: &LoadedSolution) -> Value {
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
    stats
}

struct LoadedSolution {
    entry_id: String,
    source_index: i64,
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
