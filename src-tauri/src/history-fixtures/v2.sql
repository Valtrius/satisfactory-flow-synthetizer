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
            CREATE TABLE IF NOT EXISTS solver_state (
              entry_id TEXT PRIMARY KEY REFERENCES entries(id) ON DELETE CASCADE,
              progress_json TEXT,
              proof_json TEXT,
              sequence INTEGER NOT NULL DEFAULT 0
            );
