PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;
PRAGMA user_version = 4;
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
  form_belt_rate TEXT NOT NULL,
  form_solve_mode TEXT NOT NULL
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
