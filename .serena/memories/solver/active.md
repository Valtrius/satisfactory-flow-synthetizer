# solver/active

Experiment: 42 — complete propagation variable set
Record: `mem:solver/experiments/42-complete-propagation-variable-set`
Related: `mem:solver/experiments/41-primitive-integer-inequality-rows`
Manifest: `benchmarks/custom/registered-variable-factorial.json`
Candidate commit: `520b352`; benchmark infrastructure: `4de03bc`. No push this turn.

## Status

Ready to launch; not yet running. All four isolated builds and 12 actual-binary
smoke jobs across three modes pass. 258 startup/cancellation smoke passes (N=9/L=14).
All identities, build-path failures and validation are recorded in experiment 42.

## Question and screen

Can propagation skip rediscovering its complete sorted registered-port set? Compare
`before` | `integer` | `variables` | `combined` in 36 randomized fresh processes,
p1/32, hotspots off: four repeats each on 115 all (120s) and 238 optimal (60s), plus one
`258 = 195+63` minimum_links (600s) per variant. N<=12; max link rate 1200;
seed 270826; cancellation grace 60s. Fresh matched results are primary; older
experiment 41 samples are secondary pending comparability checks.

Planned output: `target/parallelism-ladder/registered-variable-factorial-20260901`.
Source/map: `target/parallelism-ladder/registered-variable-factorial-variants-20260901-a/variants.json`.

## Next

Launch frozen suite with completion/failure dialog and end turn. After completion,
verify schedule, hashes, exact results/proof and structural work before timing analysis.
No builds/tests/source changes during timing. Scheduler extras stay paused.
No performance or permanence claim yet.
