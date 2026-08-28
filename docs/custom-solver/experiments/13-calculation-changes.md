# 13 - Isolated calculation candidates

Subsequent promotion is recorded in [14](14-calculation-promotion.md). The results
and uncommitted-state descriptions below retain the analysis-time context.

Date: 2026-08-28. Implemented, validated and measured in 80 verified jobs.
All three changes are active and uncommitted; separate promotion is recommended.
[Basis for the work](12-next-experiments.md). [Measurement protocol](13-calculation-screen.md).
[Isolated evidence](13-calculation-results.md). [Whole results and next steps](13-whole-results.md).

## Exact-L rejection before witness labeling

`search.rs::evaluate_complete_state` now counts internal node-to-node links after
successful topology solving, before full-witness canonicalization. A mismatch with
the requested exact L records a rejected candidate and a dead state in that scope.
Canonicalization only relabels endpoints, so this count cannot change afterwards.

Accepted candidates still undergo the existing canonical-witness validation and
all accounting checks. Topology-solving failures retain their previous handling.
The new test covers matching, mismatching and unspecified L, each with and without
cancellation. A mismatching complete candidate can be rejected without labeling;
cancellation during an accepted candidate still returns incomplete.

The hypothesis from 12 was that this removes expensive labeling of candidates
later rejected by L. The isolated comparison confirms zero witness calls after
the change versus two before, and 4.44-4.45x faster completion on that profile.

## Cached full-witness leaf encoding

The exhaustive permutation search is unchanged. `WitnessEncoder` prepares the
invariant public-byte header, rate encodings and endpoint rank lookups once.
Each leaf reuses a link-order buffer and byte buffer, without cloning rational
flows or rebuilding an owned topology. Only a new minimum constructs the winning
topology through the existing relabeler. Public protocol tags and ordering stay fixed.

Rank offsets follow the same terminal-rate and node-type classes as the enumerated
groups. Symmetric ports use their existing group ranks. Unique physical producer
ports, checked by incidence construction, make endpoint ordering sufficient to
sort links; no flow-based sorting tie is omitted for valid witnesses.

The old leaf encoder remains test-only. Added differential checks compare exact
bytes across 128 shuffled rank assignments per fixture, including duplicate and
unequal terminal rates, cycles, repeated node types, ternary ports and discards.
Existing independent-reference comparisons verify the minimum key and full graph;
active cancellation still discards unfinished minimization.

## Direct bounds from RREF pivots

`primitive_inequality_basis` builds one pivot lookup from the existing exact RREF.
For a pivot equation `x_i + a*x = b`, positivity reduces to `a*x < b`, and capacity
to `-a*x <= C-b`. A free variable retains its unit bounds. Other pivot columns are
zero, so no other equality can contribute to this unit-row reduction.

Positive-scale normalization, strictness, exact rational arithmetic, sorting and
deduplication are unchanged. Constant contradiction rows remain in the equality
basis and are ignored by bound reduction exactly as before.
The previous general reducer remains test-only. New checks cover widths 0/1/4/31/64,
free and fixed variables, redundant equations, contradictions and large rationals.
Existing randomized dense-reference comparisons and sparse round trips also pass.

## Isolation and validation

Frozen builds are under `target/parallelism-ladder/calculation-variants-20260828/`.
Each has all crate sources, workspace Cargo files, three release examples, revision,
source diff and per-file SHA-256 values. They are cumulative:

| Variant     | Difference from preceding variant         | Solver-core tests              |
| ----------- | ----------------------------------------- | ------------------------------ |
| `reference` | Launch commit `1b558a6`, unchanged solver | Prior experiment 12 validation |
| `exact_l`   | Earlier exact-L rejection                 | 209 passed, two ignored        |
| `witness`   | Cached witness leaves                     | 209 passed, two ignored        |
| `basis`     | Direct RREF bounds                        | 210 passed, two ignored        |

The witness snapshot also adds a local Clippy line-count allowance to keep the
complete-candidate failure sequence together; this has no runtime effect.
Strict all-target solver-core Clippy passes for witness and basis, and without
`bench-internals` for the final candidate. All release example builds pass.
No new full-workspace validation is claimed.

Logs: `target/calculation-{exact-l,witness,basis}-tests.log`, corresponding
`*-build.log`, `calculation-witness-clippy.log`, `calculation-basis-clippy.log` and
`calculation-default-clippy.log`. The focused logs retain implementation checks.
28 Python tooling tests pass, including actual tiny proof/replay and hard cancellation.
Log: `target/calculation-final-tool-tests.log`.

Portable sequential patches are in `benchmarks/custom/variants/`:
`early-exact-l.patch`, `cached-witness-leaves.patch`, `direct-rref-bounds.patch`.
Apply to the preceding source, never to the already combined working tree.
Patch checks against frozen CRLF copies use `git apply --check --ignore-space-change`.

## Decision boundary

No scheduling defaults, worker heuristics or mathematical pruning rules changed.
No benchmark labels inform solver behavior. Isolated evidence now supports each
step: 4.44-4.45x faster targeted exact-L proof, 14.87x witness replay, and 7.3-8.3%
shorter completed basis workloads. Whole optimal/all regression medians also improve.
Keep the source split and include related docs in separate promotion commits.
Retain whole optimal/all coverage because empty profiles cannot prove SAT completeness.
