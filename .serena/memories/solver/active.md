# solver/active

Experiment: 42 — complete propagation variable set
Record: `mem:solver/experiments/42-complete-propagation-variable-set`
Related: `mem:solver/experiments/41-primitive-integer-inequality-rows`
Manifest: `benchmarks/custom/registered-variable-factorial.json`
Candidate commit: `520b352` (benchmark infrastructure separate / not yet committed)

## Hypothesis

Propagation already has every production flow variable in
`PropagationState::registered_ports`. Skip rebuilding a `BTreeSet` and rescanning
every sparse row for variables on each exact analysis. Profiles put 6.24–15.32% of
sparse time in that collection.

## Status

validated; factorial screen pending (not yet launched)

## Screen shape

36 randomized fresh processes, p1/32, hotspots off.
Variants: `before` | `integer` | `variables` | `combined` (factorial vs exp 41).
Adds one `258 = 195+63` minimum-link sample per variant.

## Do not

- Infer completion from a running screen
- Edit solver sources during a timing run
- Treat uncommitted tree as disabled in production

When closed: fold outcome into `mem:solver/core` and `mem:solver/status`, clear or
replace this memory with idle.
