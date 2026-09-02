# 48. Independent RREF elimination shortcuts

Date: 2026-09-02. State: validated and frozen; ready to launch.
Prior evidence: `mem:solver/experiments/47-rref-arithmetic-profile-results`.
Prior analysis/docs committed separately as da2c89f.

## Hypothesis and scope

Zero destinations occur in 78–80% and factor -1 in 26–30% of profiled coefficient
updates. Test whether removing general rational subtraction or multiplication
improves completed solve wall time. Those percentages overlap and are not savings
estimates. Do not combine candidates in this first screen.

- `zero`: for a zero destination, use direct negation of the existing product,
  or direct pivot negation on the existing factor +1 path.
- `minus`: classify factor -1 once per eliminated row using sign/magnitude, then
  add the pivot coefficient instead of multiplying and subtracting.
- Both keep exact arbitrary-size Rational operations, all pivots, zero-factor/
  positive-unit skips, contradiction normalization and exact canonical row order.
- Update the diagnostic twin too, preserving its operand-count definitions.
- No cyclicity, case-name, memory, affinity or scheduler policy in production.

## Isolation

Current checkout keeps baseline production arithmetic. Only shared exact-oracle and release-safe heartbeat tests are added there. Candidates are standalone patches in
`benchmarks/custom/variants/rref-zero-destination.patch` and
`benchmarks/custom/variants/rref-negative-unit.patch`, both against da2c89f.
Each includes the same 20 targeted matrix cases and heartbeat-test repair in addition
to existing tests.
Do not apply both patches together; compare independently before combining.

Detached checkouts:
`C:/Users/jakez/.codex/tmp/sf48-zero`, `C:/Users/jakez/.codex/tmp/sf48-minus`.
Separate short Cargo target directories `sf48z` and `sf48m`, no artifact sharing.
Use VS2019 BuildTools and main checkout's ignored Cargo config through explicit
`cargo --config`; no machine config copied or changed. Release builds, default
features for timed binaries; tests also cover bench-internals.
Native dependencies build from scratch, so preparation takes longer than a cached
main-checkout build. Preparation time is not benchmark timing.

## Planned timing screen

`benchmarks/custom/rref-elimination-screening.json`.
32 jobs, 16 matched pairs, two adjacent AB/BA pairs per case/candidate. Same baseline
for both candidate comparisons, with distinct case aliases to avoid duplicate
run identities. All p1, capacity 1200, hotspots OFF, reference cohort:

- 115 all, 16 workers, CCD96 `ffff`, N<=10, 65s per job.
- 238 optimal, 16 workers, CCD32 `ffff0000`, N<=10, 15s.
- 258 minimum_links, 16 workers, CCD96, N<=12, 245s.
- Hard36 optimal, 32 workers unrestricted, N<=9, 25s.
  All cells require completion. Hard10 omitted here because its capped samples do not
  establish completion speed; prior diagnostic remains documented separately.
  Unrestricted 36 remains a distinct cohort, not normalized or pooled with one-CCD work.

2800s search caps + 32*15s cleanup = 3280s, 54m40. Reserve 320s for startup/verification
within the user one-hour limit. Frozen source/binaries/manifest and completion/failure
dialog required before ending the launch turn. This two-pair first screen is for
candidate selection; small/noisy effects require more pairs before promotion.

## Validation and results

Initial release test runs exposed the existing heartbeat test's timing assumption:
both candidates passed 191 tests but failed
`periodic_progress_reports_live_work_before_profiles_close`. An unchanged-baseline
release test reproduces the same cancellation assertion failure in 0.63s.
The 65=40+25 case finishes before three 250ms heartbeats; this is not candidate
correctness/performance evidence. Original failed logs retained.

Shared test repair uses 10=6.04+3.96 at capacity 1200, N<=11 and the existing 10s
watchdog. It still requires three live timer updates, cancellation, monotonic
telemetry and reporter-join event order. It checks no additional profile closes
during the sampled interval instead of assuming the cumulative count starts at 0.
This edits tests only; source decisions/scheduler unchanged. Both isolated
patches also include the repair. Full release reruns pass on both candidates: 223 tests with bench-internals and
213 with default features, two ignored per suite. The first strict Clippy pass
rejected the expanded heartbeat test at 102 lines. Reused the existing fixture
builder for the same exact rates, bringing it below the limit; no lint exemption.
Final full reruns, both strict Clippy configurations and executable builds pass. Baseline's repaired heartbeat passes debug and release.

Baseline's new exact matrix oracle passes; 41 tooling tests pass. Candidate release tests, strict Clippy and exact CLI validation pass. New test uses 20 consistent two-row/three-variable systems across unit,
negative-unit, general fractional and large factors, and zero/nonzero/large
destinations. Exact dense-oracle comparison checks production and diagnostic paths;
consistency avoids a contradiction collapse hiding wrong coefficients.
Existing generated oracle covers 384 exact-order systems plus broader solver tests.
No timing results, no speedup or promotion claim.

## Final validation and frozen identities

- Each candidate: 223 release solver-core tests with bench-internals, 213 with
  default features; 2 ignored per suite. Both strict release Clippy configurations pass.
- Main checkout: 223 solver-core tests with bench-internals and strict Clippy pass;
  baseline heartbeat repair passes debug/release; new matrix oracle passes.
- 41 tooling tests pass. Formatting and diff checks pass.
- 15 real CLI smokes across optimal/minimum_links/all on 65: preserved baseline and
  both candidates, candidate hotspots ON/OFF. Full solutions, keys, preferred
  witnesses and complete outcomes/proofs agree. Instrumented operand counts agree;
  hotspots-OFF counters stay zero. These are correctness checks, not timing samples.
- 32-job PlanOnly succeeds, verifying identities, budgets and requested L3 domains.
  Plan: `target/parallelism-ladder/rref-elimination-plan-20260902`.
- No production-prefix differences in main canonical.rs or solver.rs versus the
  preserved baseline. Shared modifications are tests only.

Variant map: `target/parallelism-ladder/rref-elimination-variants-20260902/variants.json`.
Before is the unchanged experiment 47 diagnostic binary, hotspots OFF during timing:
`353d5863592991bcd86531248b996dd8c0f31383c99df97e2fc066fba35483ed`.
Zero SHA256:
`9f7e67925ab474d1919c3c0553f3ba4be02ab750e0b39017e339c8974dc7d3ee`.
Minus SHA256:
`6465d5dca89b185c37a2b471428fa3732ede69a03fe6afb77ae0aa48b2d9037e`.
Standalone patch SHA256:

- zero: `b62073b8da71db9bc0a3ed7c96890c74716aa647f7006d1cf8767a76fe91c9cf`.
- minus: `ebd3a38213a632568a47ec46f78a7f48c8510977aa749625e640de883751a165`.

Each frozen candidate has 70 source files plus its standalone patch, metadata and
binary hashes. Only canonical.rs and solver.rs differ from before; solver.rs changes
are confined to tests. Freeze verifies the solver's entire pre-test source prefix.
Worktree base da2c89f plus committed candidate patch reconstructs each source.
Local Cargo config/rustc identities match the preserved baseline.

Logs/helpers: `target/exp48-{zero,minus}-validated-build.log`,
`target/exp48-main-core-tests.log`, `target/exp48-main-clippy.log`,
`target/exp48-{baseline-oracle,baseline-heartbeat-final,baseline-heartbeat-debug,tool-tests,release-smoke,freeze,plan}.log`.
Original failed logs: `exp48-{zero,minus}-build.log`,
`exp48-baseline-heartbeat.log`, `exp48-{zero,minus}-build-final.log`.
Smoke JSON/checks: `target/exp48-release-smoke/`.
Freeze/smoke helpers: `target/exp48-freeze.py`, `target/exp48-release-smoke.py`.
Final profile_case link/build steps 47.39s and 47.73s, respectively; not performance data.

## Commit state / next

This record accompanies the tests, two independent candidate patches and manifest
commit. Commit hash will be recorded in the launch handoff. No push.
Current production arithmetic remains baseline; candidates are only in isolated
worktrees/frozen executables. Nothing promoted and no performance results yet.
Prepared run: `target/parallelism-ladder/rref-elimination-20260902`.
Launch with 15s cleanup, verify live status, record PID/time, then end turn.
