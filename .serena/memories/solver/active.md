# solver/active

Experiment 42 focused follow-up. Validated; ready to launch.
Record: `mem:solver/experiments/42-258-confirmation`.
Prior results: `mem:solver/experiments/42-complete-propagation-variable-set-results`.

## Question and scope

The user approved eight more 258 minimum_links timing jobs, two per frozen variant,
and two separate reference/complete-variable diagnostic jobs. No solver or scheduler
change; no rebuild. Both optimizations remain active committed candidates.
Timing aliases/repeats match the original run, reaching n=3 per variant afterward.
Diagnostics use `medium258_diagnostic` and must not enter ordinary timing medians.

Manifest: `benchmarks/custom/registered-variable-258-confirmation.json`.
p1/32, N<=12, max link rate 1200, 600s solver cap, 60s cleanup watchdog, seed 270826.
Expected full result: N=9/L=14, two identical layouts. No speed claims before completion.

## Validation and launch

All four reused binary hashes and 280 source hashes match the completed run.
Ten-job plan, 34 tooling tests and two two-second diagnostic startup/cancel smokes pass.
Diagnostics capture root/finish/cache-drop spans and calculation counters with no
dropped or open activity records in those smokes.
Ready output: `target/parallelism-ladder/registered-variable-258-confirmation-20260901`.
Map: `target/parallelism-ladder/registered-variable-factorial-variants-20260901-a/variants.json`.
Freeze and launch with completion/failure dialog, then end turn. No builds/tests during timing.

## After results

Verify schedule, identities, exact result sets/proof and structural work.
Analyze new/old hotspot-off records together, retaining every sample.
Inspect diagnostic root/search/finish/cache-drop durations and aggregate subphases.
Do not infer individual root calculation costs from aggregate timers.
Memory is not a promotion gate; scheduling remains paused.

Manifest and preparation/results memories not yet committed; no push.
