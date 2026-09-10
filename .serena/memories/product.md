# Product and exact scopes

Offline Windows Tauri/Svelte/Rust app for exact splitter/merger flow synthesis, queued cancellable jobs, SQLite history, editable graphs and SVG export.

N counts splitters/mergers. L counts operator-to-operator belts, excluding external stubs and discard belts. Minimize N, then L. Physical flows are positive exact rationals within capacity; cycles and surplus discards are allowed. Automatic supply is one belt totaling demand; above capacity requires explicit input belts.

| Label       | Serialized scope | Required result                                          |
| ----------- | ---------------- | -------------------------------------------------------- |
| One min N/L | one_min_nl       | First validated witness after proving minimum N and L    |
| All min N/L | all_min_nl       | Every distinct layout at minimum N and L                 |
| All min N   | all_min_n        | Every distinct layout across all feasible L at minimum N |

Ordering and equal-optimum tie choice do not matter. A validated incumbent is an upper bound, not proof of optimality. Objective proof and enumeration completion are separate. Cancellation preserves delivered layouts and incumbents; timeout/cap/failure stays incomplete even when every known layout was found.

History stores no solver type, accepts equivalent imported scope tags, and shows proved minimum L with a route icon; unknown L stays unknown. Keep exact integer formatting and currently used diagnostics. See `mem:solver/contracts` for migration and packaging behavior.
