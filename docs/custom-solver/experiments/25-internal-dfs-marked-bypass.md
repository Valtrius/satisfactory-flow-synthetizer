# 25 - Internal DFS marked-child bypass

Date: 2026-08-29. State: launched; awaiting analysis.
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
