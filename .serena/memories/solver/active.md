# solver/active

PREPARED, not launched. User approved experiment48 negative-unit confirmation.
`mem:solver/experiments/48-rref-negative-unit-confirmation`.

32 jobs, four new AB/BA pairs per workload, repeats 3 through 6.
115 all, 238 optimal, 258 minimum_links, hard36 optimal; same preserved before/minus
binaries, p1, hotspots OFF, caps and fixed-CCD/unrestricted placements as initial run.
All jobs must complete. 54m40 search+cleanup plus 5m20 overhead margin under one hour.
Zero-destination held; no production arithmetic or scheduler change.

Manifest `benchmarks/custom/rref-negative-unit-confirmation.json`.
Map `target/parallelism-ladder/rref-elimination-variants-20260902/confirmation-variants.json`.
Planned run `target/parallelism-ladder/rref-negative-unit-confirmation-20260902`.
PlanOnly and independent plan audit pass; 41 tool tests pass. Original variant
hashes and binaries verified; no rebuild. Format and commit manifest/results/handoff,
then launch with CancellationGraceSeconds 15. Record PID/time; end turn after
checking live status. Completion/failure dialog and durable markers are configured
by the launcher. Nothing promoted or pushed.
