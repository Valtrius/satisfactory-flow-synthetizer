# Product and glossary

Offline Windows Tauri 2 + Svelte 5 + Rust app for exact splitter/merger flow synthesis, queued cancellable jobs, SQLite history, editable graphs and SVG export.

N = physical splitter/merger count. L = operator-to-operator belts, excluding external stubs/discards. Optimize N first, then L at minimum N. All physical belts carry strictly positive exact rational flow <= capacity; cycles and surplus discards are allowed. Automatic supply is one belt totaling demand; above capacity requires explicit inputs.

| Label       | Serialized scope | Meaning                                                |
| ----------- | ---------------- | ------------------------------------------------------ |
| One min N/L | one_min_nl       | First validated witness after proving minimum N and L  |
| All min N/L | all_min_nl       | Every distinct layout at minimum N and L               |
| All min N   | all_min_n        | Every distinct layout over all feasible L at minimum N |

Solution order and the choice between equal optimal witnesses do not matter.
best_known = independently validated incumbent, an upper bound. proven_optimal = completed objective proof. Enumeration completion is separate from individual witness status. Cancellation keeps delivered layouts/incumbents; cap, timeout and failure are incomplete, never global UNSAT.

History and preferences accept equivalent scope values. Saved data has no solver type. History shows minimum proved L with a route icon; unknown L remains unknown. Keep current diagnostics and exact integer formatting.
