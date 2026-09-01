# 40. Equality and inequality subphase profile

Date: 2026-09-01. State: completed and verified.
Related: canonicalization reprofile (`mem:solver/experiments/39-canonicalization-reprofile-results`),
results (`mem:solver/experiments/40-equality-inequality-subphase-profile-results`).

## Question

Experiment 39 puts exact equality and physical-bound inequality encoding at
58.9-67.2% of combined state/SCC canonicalization. Which calculation inside those
two broad timers should change first?

## Instrumentation

The existing hotspot recorder now splits equality work into:

- canonical producer/consumer lists and index maps;
- dense equation-row construction;
- exact rational RREF.

It splits inequality work into:

- RREF pivot indexing;
- positive/capacity row construction;
- positive-scale primitive normalization;
- final sort and deduplication.

The outer equality and inequality timers remain unchanged so every group can be
reconciled with unclassified overhead. Counters appear in the existing hotspot JSON.
They do not enter production telemetry or solver decisions.

Ordinary hotspot-off solves retain the existing inequality implementation and add no
new clock reads. The hotspot-only inequality path performs the same construction,
normalization, sort, and deduplication as separate passes so each pass has one timer.
An exhaustive oracle compares that path with production and the dense exact reference.

## Frozen screen

Reuse `benchmarks/custom/canonical-purpose-screening.json` and its four Experiment 39
workloads:

- completed 115 all and 238 all controls;
- hard 36 all and hard 10 optimal with 60-second caps;
- production p1, 32 workers, and hotspot recording throughout.

Production reference source: `1fd478e`. Candidate source: `90df7e2`. Candidate release
binary SHA-256:
`9A1852C6A945DE535442336E99EB89085E070EC7D0DC81EF01CC679C585C3E35`. Run:
`target/parallelism-ladder/equality-inequality-profile-20260901`.

The completed controls must preserve exact layout sets. Capped results can establish
cost shares only. Timing cannot be compared with hotspot-off completion runs.

## Prelaunch validation

- the benchmark-feature solver-core suite passes 188 tests, with two manual
  benchmarks ignored, plus the exhaustive outer reference test and four parallelism
  integrations;
- the full workspace suite passes, with five manual benchmarks ignored;
- strict all-target benchmark-feature Clippy and all 33 analyzer tests pass;
- Rust and Markdown formatting checks pass, and the release profiler builds;
- a two-second 115 hotspot smoke stops cleanly at its cap and records nonzero values
  for all seven new subphases. The equality children total 190.802 ms inside the
  191.406 ms outer timer; the inequality children total 238.230 ms inside the
  238.997 ms outer timer;
- the exhaustive sparse-elimination oracle compares the production path, profiled
  path, and dense exact reference over the existing generated matrix.

The smoke is a plumbing and reconciliation check only. Its incomplete status is not
performance evidence.

The detached four-job screen finished and reanalysis reproduced the runner's verified
summary byte for byte. See the linked results for measured shares and the next
candidate.

## Decision rule

Target the largest repeated subphase across completed and capped work. Prefer a local
byte-preserving change before replacing the full canonical basis representation.
Every later candidate must compare state and SCC key bytes against the existing exact
oracles. Scheduling stays paused.
