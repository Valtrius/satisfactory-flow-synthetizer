# product/requirements

Offline Tauri desktop app: exact Satisfactory splitter/merger flow synthesis.
Agent knowledge for product behavior lives here (not in README).

## Product goal

- User sets supply/demand rates, picks a solver, gets validated belt layouts.
- Inspect/edit topology; export SVG.
- Offline Windows desktop (Tauri 2 + Svelte 5 + Rust).

## Hard constraints

- Every physical belt (including discard) carries positive flow ≤ belt capacity.
- Rates are decimals or exact fractions (rationals); capacity is part of problem identity.
- Automatic supply on one belt totaling sum of demand; above capacity requires explicit input belts.
- Returned witnesses pass independent validation before UI.

## Solvers (same UI workflow)

- **Custom** — deterministic exact topology search: proof accounting, independent
  validation, incremental incumbents, complete minimum-node enumeration.
- **Z3** — portfolio SMT: parallel attempts, feedback verification,
  cancellation, minimum-node enumeration.

## Objective (all solvers)

Lexicographic:

1. Minimize physical splitter/merger count (**N**)
2. Then minimize non-discard operator-to-operator link count (**L**)

Streams improving `best_known` under a strict belt cap, then proves nothing better if applicable.

## Exact scopes

| Name            | Meaning                                            |
| --------------- | -------------------------------------------------- |
| `optimal`       | One preferred layout after proving min N and min L |
| `minimum_links` | All layouts at min N and that min L                |
| `all`           | All layouts across every feasible L at min N       |

**Main goal**: resolve the problem using the selected solver and scope **as fast as possible**, while returning a validated layout and proof of optimality (or all layouts at min N and min L, or all layouts at min N).

## Result semantics

- `proven_optimal` — completed Custom proof, or completed Z3 Opt after belt-cap proof.
- `best_known` — independently validated layout; **not** an optimality claim
  (includes live Z3 Opt incumbents before final proof).
- Global UNSAT ≠ incomplete / resource-limited / internal failure.
- Cancel enumeration: keep layouts already delivered.
- Successful enumeration completion: preferred layout → `proven_optimal`; other
  min-N layouts remain validated `best_known` alternatives.
- Cancel mid-improve: best streamed incumbent stays `best_known`.
- Progress may include node bounds, current search size, link obligations/caps,
  incumbent counts. Missing common fields are null. Progress is not mandatory but is a nice-to-have.

## UX / app behavior

- Queued jobs, history, cancellation.
- Topology graph with edit history and SVG export.
- Solver choice per job: Custom or Z3.

## Non-goals / do not confuse

- Dev-build timings are not representative; use release builds for performance claims.
- Custom-opt experiment history → `mem:solver/*`, not this memory.

## Related

- Terms: `mem:product/glossary`
- Custom proof/identity contracts: `mem:solver/contracts`
