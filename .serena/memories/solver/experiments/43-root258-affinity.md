# 43. Isolated 258 roots and CPU affinity

Date: 2026-09-02. State: completed and analyzed.
Basis: `mem:solver/experiments/42-258-confirmation-results`.
The user authorized more experiments while AFK for a few hours.

## Hypothesis and controls

Roots 7 and 23 dominate the 258 tail; cleanup is under 0.6s on each longest root.
Whole timings vary by tens of seconds within the same binary. Isolated root replay
with a fixed logical CPU can distinguish calculation effects from CPU-placement
sensitivity. No claim yet about clock, cache topology or the cause of the variation.

Problem 258 = 195+63, capacity 1200. N=9/L=14, profile S2=5/S3=1/M2=0/M3=3.
Use the original p1 planning budget of 32, target 128. The complete ordered 76-key
plan and selected key are identical across four newly built replay executables.
Each selected root executes serially with fresh state. Never replan with workers=1.
Mode `all` means all witnesses in that local root, not whole minimum-N enumeration.

## Screen

Manifest: `benchmarks/custom/root258-affinity-screening.json`, seed 902043.
28 sequential randomized jobs:

- Root 23: four variants x CPUs 0/31 x two repeats, 16 hotspot-off jobs.
- Root 7: four variants x CPUs 0/31 x one repeat, eight hotspot-off controls.
- Root 23 diagnostics: reference/variables x CPUs 0/31, four hotspot-on jobs.

Ordinary caps 300s, diagnostic caps 360s. Sum of search caps 8,640s, 144 minutes.
With the existing 60s per-job cleanup watchdog, maximum scheduled allowance is
172 minutes plus launch/verification overhead. Expected roughly 1-2 hours, but
completion is not guaranteed. All jobs are must-exhaust controls; any capped root
fails that gate, and partial work cannot become a completion-speed result.

Masks `1` and `80000000` select logical CPUs 0 and 31. They are validated against
the runner's available mask, observed as `ffffffff` on this host. No claim that the
chosen CPUs represent particular cache/CCD types. Compare variants within each
mask first; never pool both masks or instrumented/uninstrumented timing.

## Benchmark-only changes

`scripts/hard-profile.py` supports optional `processor_affinity` hex masks in the
job envelope, not the solver request. It normalizes/validates masks and verifies
requested vs observed mask plus application time in process metrics. Summaries
retain the affinity label. Exact-result comparisons remain independent of placement.

`scripts/start-hard-profile.ps1` applies affinity only to the started benchmark
child, verifies it, records application delay and terminates that child on failure.
Affinity is applied after process launch, not before its first instruction. The
smokes recorded 8.4-28.5ms delays; early setup or a few early search steps may be
unpinned. No host or production policy is changed.

The fixed-work preparer still defaults to a 40-minute search-cap budget. An explicit
`max_search_seconds` may raise it, bounded to three hours. This manifest declares 8640. Cleanup grace is separate, as calculated above.

## Frozen builds and validation

Variant root: `target/parallelism-ladder/root258-variants-20260902`.
Sources are the same factorial variants: before 90df7e2, integer cd46fae,
variables 90df7e2 with 520b352 sparse/propagation, combined 520b352.
Builds use release + bench-internals, profile_obligation, separate short Cargo
targets and the recorded VS2019/znver4 environment. No Rust solver source changes.
Do not compare these executable timings directly with profile_case whole timings.

| Variant   | profile_obligation.exe SHA-256                                   |
| --------- | ---------------------------------------------------------------- |
| before    | 6d73704ad8e665096b64f31ac8b4897ca8a295312aa872cc9ec183f906b97c19 |
| integer   | 5fb8d1eb2237f6635bf2e8a26f55be729fd7f9d92fbc6fb89ccd06f1816ae04b |
| variables | d09f4e1391230f0792abe820fb399172a1d20a61fd14dcee2910653cd8a526d6 |
| combined  | 7cb018c835b9cf8afe39205efead583f19f88b91efeb5cc9d4eda950ed1188b8 |

- All four release builds pass. Metadata/source/binary hashes saved per variant.
- All 37 runner/analyzer tests pass, including actual Windows child affinity and
  its evidence, invalid masks, explicit budget bounds and unchanged default budget.
- Frozen 28-job plan passes; root identities agree between variants and CPUs.
- `npm run format` and `git diff --check` pass; no unrelated formatting changes.
- Four real root-23 two-second affinity/cancellation smokes pass, reference/variables
  on CPUs 0/31, with verified masks and original target-128/76-key certificates.
- Four tiny completed profile runs using the actual new binaries preserve full
  solution objects and local proof fields. An initial overbroad comparison included
  timing diagnostics and failed; comparing the exact result/proof fields passes.
  No solver discrepancy was found.

Evidence: `target/root258-tool-tests.log`,
`target/parallelism-ladder/root258-affinity-plan-20260902`,
`target/parallelism-ladder/root258-affinity-smoke-20260902`,
`target/parallelism-ladder/root258-tiny-validation-20260902`.
Prior Rust validation applies to unchanged sources; no new Rust suite was required.

## Result and next decision

All 28 jobs exhausted their selected roots and verify exactly. Finished 2026-09-02
01:13:05 Europe/Paris, 51.578 process minutes. Results:
`mem:solver/experiments/43-root258-affinity-results`.

Complete variables save 0.29-1.70% wall in all same-CPU/root comparisons; integer
rows cost 0.60-2.30%. CPU placement moves wall by 14-20%, much more than candidate
effects. Empty root results measure proof search, not witness discovery.
Recommend restoring integer rows and doing a final whole check with variables
alone. Neither action has been applied. Scheduler extras stay paused.

Tooling/manifest/preparation committed in `986e330`; no push.
Launched 2026-09-02 00:21:29 Europe/Paris, runner PID33544, with a completion dialog.
Post-run results and current-state memories uncommitted. No new benchmark in flight.
