# 47. Canonical RREF arithmetic profile

Date: 2026-09-02. State: implemented; validated; release smoke passed, not launched.
Related: permanent RREF ordering in `mem:solver/experiments/46-rref-order-confirmation-results`.
Promotion documentation committed separately as c106d4c; source a79ecf7.

## Question and hypothesis

Where does canonical RREF spend arithmetic time after the ordering shortcut:
pivot normalization, nonzero suffix construction, elimination, or finalization?
Which exact operand patterns could support another small shortcut? No gain assumed.

## Change

`canonical.rs` dispatches to a diagnostic twin only when the existing hotspot
subphase recorder is active. Normal `rational_rref` arithmetic is unchanged.
The twin preserves pivot selection, divisions, update expressions, contradiction
collapse and final row reversal. Local counters merge once per reducer call.
`hotspot_profile.rs` resets/aggregates them; `profile_support/mod.rs` exports flat JSON.

Four disjoint nested elapsed-time sums: normalization (including pivot clone),
suffix construction, row elimination and finalization. They overlap the outer
RREF/equality timer and must not be summed with it. The outer timer also includes
zero-row filtering, pivot search, bookkeeping, clock overhead and atomic merge.
Subphase times include local instrumentation: they are diagnostic, not intrinsic
arithmetic timings or projected speedups.

Counts: calls, pivots, nonunit pivots, normalized coefficients, eliminated rows,
positive/negative unit factors, integer factors, coefficient updates, positive/
negative unit-factor updates, zero destinations and unit/integer pivot entries.
Integer categories include units; overlapping categories are not additive.
Update-weighted factors distinguish useful suffix work from empty suffixes.
Maximum numerator/denominator bit lengths cover observed arithmetic operands and
stored results, not hidden BigRational intermediates or a size distribution.
Existing positive-unit/zero shortcuts are already production behavior.

## Planned comparison

Manifest `benchmarks/custom/rref-arithmetic-profile.json`.
14 jobs, all p1, 16 workers, capacity1200, hotspots ON:

- 115 all: CCD96 (`ffff`), N<=10, 150s, two runs per binary.
- 238 optimal: CCD32 (`ffff0000`), N<=10, 60s, two runs per binary.
- 258 minimum_links: CCD96, N<=12, 480s, one run per binary.
- Hard36 all: CCD96, N<=9, 120s, one run per binary, stress.
- Hard10 optimal: CCD32, N<=11, 120s, one run per binary, stress.

Frozen prior RREF binary is `before`, SHA256
`a431c5ce82fe435c75831a7d4403e84e9c5c6e53353ba04bcde05a40e00678d7`.
New `profile` SHA256
`353d5863592991bcd86531248b996dd8c0f31383c99df97e2fc066fba35483ed`.
Release build passed in 47.94s. Diagnostic source+memories committed as 1006e6f.
70 source files frozen; only canonical.rs, hotspot_profile.rs and profile_support/
mod.rs differ from the prior RREF snapshot. Compiler and local Cargo config match.
Variant map: `target/parallelism-ladder/rref-arithmetic-variants-20260902/variants.json`.
Metadata/source/binary hashes retained beside each variant; before aliases the
unchanged RREF binary, not the older pre-variable/accounting solver.
Freeze log/helper: `target/exp47-freeze.log`, `target/exp47-freeze.py`.
Plan-only validation passed all 14 jobs and topology/cache masks:
`target/parallelism-ladder/rref-arithmetic-plan-20260902`, `target/exp47-plan.log`. No candidate optimization.
Randomized fresh processes, fixed before-resume affinity; no paired speedup
estimate or cross-CCD pooling. Reference workloads must complete and preserve exact
saved solutions/keys/preferred witnesses. Capped stress samples describe encountered
work only, never completion gains.

2280s total search caps + 14*15s cleanup = 2490s (41m30).
18m30 remains inside the user one-hour limit for startup and verification.
Windows completion/failure dialog plus durable status files; end turn after launch.

## Validation and results

Final checks:

- 222 solver-core all-target tests with bench-internals pass (2 ignored).
- 331 workspace all-target tests pass (5 ignored).
- Strict Clippy passes with and without bench-internals; 41 tooling tests pass.
- Formatting and diff checks pass; release profile_case built in 47.94s.
- Nine real release CLI checks on 65, all three solve modes: preserved before,
  current hotspots off, current hotspots on. Exact outcome/proof, solution objects,
  keys and preferred witness agree. Profile off exports zeros; on exports nonzero
  subphase/call counters, valid category bounds and nested-time accounting.
  These are correctness smokes, not timing samples.

The exact dense-oracle test compares 384 profiled bases in exact order, including
permutations/scaling, duplicates, zero rows, large coefficients and contradictions.
A hand-counted three-row fixture validates normalization, positive/negative unit
factors, zero destinations and bit maxima.
Initial Clippy caught two doc-markup issues and the expanded flat snapshot mapping
crossing its line threshold. Fixed markup; a reasoned local expect retains the flat
atomic-field mapping. Final strict passes above; original failure log retained.
Logs: `target/exp47-{core-tests,workspace-tests,clippy-feature-final,clippy-default,tool-tests,format,build}.log`;
`target/exp47-release-smoke.log` and its nine JSON outputs/verification.json.
No benchmark results yet. No speed claim.

## Decision and commit state

Diagnostic only; no production arithmetic, scheduler, affinity or cyclicity policy
change. No push. Diagnostic source and its documentation committed as 1006e6f; no push.
Benchmark manifest and prepared launch handoff accompany a separate commit.
Prepared run directory: `target/parallelism-ladder/rref-arithmetic-20260902`.
Next: launch with 15-second cancellation grace and record PID/time. After completion,
recheck frozen hashes, exact outputs/proofs and recorder invariants before interpreting
subphase shares or operand patterns. Do not infer gain from instrumented wall time.
