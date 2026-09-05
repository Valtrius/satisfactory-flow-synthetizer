# product/glossary

Shared vocabulary for product + Custom solver talk. Prefer these terms in memories
and agent replies. Proof contracts: `mem:solver/contracts`. Product rules:
`mem:product/requirements`.

## Objective variables

- **N** — number of physical splitter/merger nodes (primary lex objective).
- **L** — number of non-discard operator-to-operator links (secondary lex objective).
- **min N / min L** — proven minimums for the exact problem; not a user hint.
- **N cap** — upper bound on searchable N (limits largest N, not the starting N).
- **L group** — all work at a fixed N and fixed L before moving on.
- **lexicographic objective** — minimize N, then L among min-N layouts.

## Result / proof language

- **witness / layout** — a concrete belt topology (validated before UI).
- **preferred witness** — the lex-chosen layout among a completed set.
- **incumbent** — a validated layout that is an upper bound; may be `best_known` while search continues.
- **proven_optimal** — optimality (and preferred layout) established by a completed proof path.
- **best_known** — validated only; not a proof of optimality.
- **global UNSAT** — no feasible layout exists (distinct from timeout/cap/kill/incomplete).
- **canonical layout key** — exact identity used to compare layouts; counts alone ≠ equivalence.
- **time to first** — wall time to the first validator-accepted layout; separate from terminal completion. Prefer this name over inventing other “TTF” aliases.
- **time to min L** — wall time until every layout at min N and that min L is delivered.
  For `minimum_links`, that is terminal completion. For `all`, it is only the end of the
  min-L group — **not** whole-`all` completion (later L groups may still be running).
- **time to all** — wall time to terminal completion of an `all` scope run (every feasible L at min N).

## Topology / flow

- **splitter / merger** — physical operator nodes counted in N.
- **discard belt** — sink belt; still must have positive flow ≤ capacity; excluded from L.
- **operator-to-operator link** — non-discard belt between operators; counted in L.
- **capacity / max link rate** — per-belt flow upper bound; part of problem identity (benchmarks often use 1200).
- **rate** — rational supply/demand (decimal or fraction string).

## Custom search / parallelism

- **p1, p12, p123, p14, …** — parallelism stage shorthand; see `mem:solver/controls`.
- **baseline** — no parallelism stage flags.
- **adaptive partitions / deep partitions** — p1 planning of static roots.
- **shared state cache / donation / parallel remaining groups** — later stages; currently paused in production Custom.
- **root / frontier** — planned search entry points / adaptive frontier leaves (proof-ledger boundaries).
- **DFS / recursive DFS** — depth-first expansion under a planned root.
- **canonicalization / state key / SCC key** — exact identity for partial or full search states.
- **propagation / sparse / RREF** — exact algebra path inside Custom search.
- **hotspots / profile** — optional timers/counters; off for ordinary timing runs.

## Benchmarks / harness

- **case** — file under `benchmarks/custom/cases/` (label only to the solver).
- **manifest / screen** — frozen job set for a comparison run.
- **variant / patch** — isolated source mutation under `benchmarks/custom/variants/`; never apply mid-screen.
- **verified record** — analyzer-accepted result identity for a job.
- **capped / incomplete** — hit time/node resource limit without discharging proof.
- **kill / watchdog** — process killed; failed measurement, no returned proof.
