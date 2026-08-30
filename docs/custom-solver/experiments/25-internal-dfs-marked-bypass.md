# 25 - Internal DFS marked-child bypass

Date: 2026-08-30. State: screening verified; [promotion repeat](26-internal-dfs-promotion-repeat.md) passed and made the change permanent.
Related: [canonical purpose results](24-canonical-purpose-profile.md).

Run: ignored `target/parallelism-ladder/unkeyed-dfs-20260829/`.

## Hypothesis

Marked-child canonicalization costs 16-32% of accounted worker time on the
experiment 24 workloads, while its call count exceeds traversed decisions by less
than 0.3%. Bypassing that key only inside an already dispatched DFS root may reduce
completion time without changing the exact completion set.

## Scope and correctness argument

Cargo feature `bench-unkeyed-dfs` changes one benchmark path. Production builds
leave it disabled.

- Root partition planning still sorts and deduplicates canonical marked-child keys.
- Adaptive frontier refinement still uses keyed decisions and propagated canonical
  state keys, so proof-ledger partition identities do not change.
- Internal DFS still selects an invariant open-port orbit and uses local physical
  port representatives. It enumerates every remaining raw decision in stable local
  order instead of sorting/deduplicating marked-child keys.
- Each applied child still runs exact propagation, reachability, SCC analysis and
  propagated state canonicalization before memoization. Equal child states therefore
  retain one authoritative completion proof.

The feature can add work when several raw decisions reach one canonical child. It
must not remove a completion. Preferred result and full enumeration still require
differential verification; the argument alone is not promotion evidence.

## Screen

Manifest: `benchmarks/custom/unkeyed-dfs-screening.json`.

One before/after sample each covers 115 all, 238 all/optimal and hard-36 optimal.
Hard-36 all and hard-10 optimal supply matched 60-second diagnostics. All jobs use
production p1 and 32 workers. The 12 jobs have 990 seconds of aggregate search caps.
Hotspot diagnostics are excluded from ordinary timing comparisons.

The before and after binaries must come from the same source revision and differ
only by `bench-unkeyed-dfs`. Completed pairs must preserve preferred keys, full
solution objects, canonical layout-key sets and proof. Capped common partial results
must agree, and neither side may be killed.

This is a screening pass. A favorable single pair identifies completed workloads
for repeated A/B; it does not justify production promotion.

## Validation before launch

With `bench-internals,bench-unkeyed-dfs`, 183 solver-core tests pass with two manual
benchmarks ignored. The exhaustive outer reference integration and all four
parallelism integrations also pass. The default 183-test library suite passes, and
strict all-target Clippy passes for the candidate.

Default and candidate release tiny runs preserve the exact outcome, preferred key,
both full solutions, layout keys and structural counters. The candidate reduces
marked-link graph calls from 171 to 135 because planning remains keyed. Frozen
release binary SHA-256 values are:

- keyed reference: `743ecf14efeda83b708651c2b33042a89d1ce58b555fd3ee4328939e0515a7fd`;
- internal unkeyed candidate: `f15d49bba7a3263ae23711e553cb978eb0bdcb0902b17072596d1cd626f5dd73`.

The frozen plan validates all 12 unique jobs and the 990-second aggregate cap.

## Results

The runner finished on 2026-08-30 and verified all 12 records. Eight records
completed optimally and four fixed-time diagnostics remained explicitly incomplete.
No run failed or was killed. `results/summary.json` has SHA-256
`90c6e059734d7a34032317823cfa6f412c92f091b6f2946fb3086af2fe626716`.

| Workload    | Keyed (s) | Bypass (s) | Wall reduction |
| ----------- | --------: | ---------: | -------------: |
| 36 optimal  |    23.314 |     18.404 |         21.06% |
| 115 all     |   135.907 |    113.342 |         16.60% |
| 238 all     |    60.680 |     49.831 |         17.88% |
| 238 optimal |    30.744 |     24.767 |         19.44% |

Every completed pair has the same problem, mode, status, validated outcome,
preferred key, full solutions and canonical layout-key set. The candidate returns
49 layouts for 115 and one for each other workload, matching the keyed reference.
Retained-state counts are identical except for 30 extra states out of 2.21 million
on 115. Raw structural decisions rise by 0.03-0.15%, consistent with deferring a
small amount of equivalence removal to the propagated state cache.

The two capped pairs also preserve their incomplete outcomes and partial result
identities. They do not establish completion-time gains:

- 36 all finds the same two layouts. Candidate first-witness time falls from
  23.245 to 18.227 seconds and it retains 27.1% more states within the minute while
  consuming 13.9% less process CPU time.
- Hard-10 optimal finds no witness on either side. The candidate retains 20.4%
  more states and traverses 17.4% more decisions, but sampled peak working set rises
  26.2%, from 1.61 to 2.04 GB, and process CPU time rises 2.2%.

Hotspot counters confirm the intended mechanism. In hard 10, internal marked-link
graph calls fall from 1,769,940 to 198 and legal-decision time falls from 609.2 to
316.5 accounted seconds. More downstream state canonicalization and propagation
consume part of that saving because the bypass admits extra raw decisions.

## Decision and next measurement

The screen supports the hypothesis across one acyclic and two cyclic completed
problems, both find-optimal and find-all. The 16.6-21.1% wall reductions are large
enough to repeat, but one timing sample per pair is not enough for production.

Run two more interleaved samples per variant on the four completed workloads. The
combined three samples must keep exact outputs equal and favor the bypass median on
each workload. If they do, remove the Cargo feature gate and make the internal DFS
bypass permanent. Keep root planning and adaptive-frontier decisions keyed. Do not
resume scheduler experiments during this promotion check. Experiment 26 runs this
check, promotes the bypass, and records the current hard-run memory attribution.
