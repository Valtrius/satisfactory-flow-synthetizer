# 16 - Diagnostics, prefix outcomes and next steps

Date: 2026-08-28. [Whole results and verification](16-post-calculation-results.md),
[protocol](16-post-calculation-screen.md). Evidence is under
`target/parallelism-ladder/post-calculation-20260828/`.
Diagnostics are single 120 s samples, separate from uninstrumented timing repeats.
Both traces have zero dropped records. Nested timer sums are not CPU shares.

## Hard-36 optional constructor

P1 runs ten `acyclic_construct` calls totaling 32.832 s. Its winning N=9/L=11
group ends at 35.697 s, but L=12 starts at 64.090 s. Two constructor calls within
that gap take 3.890 and 22.476 s, 26.366 s together. The solver already knows a
solution exists at N=9. P14 runs eight calls totaling 6.359 s and no constructor
calls after the winning group because its remaining-group path bypasses them.

Source inspection in `solver.rs` confirms that sequential enumeration attempts
the optional existence helper before each next group, even after `winning_node`
is set. Exact enumeration still has to search all profiles in those groups.
This is a specific avoidable-work hypothesis, not a measured 26.366 s speedup.
Changing it may affect when additional partial witnesses become visible.

Next candidate: skip this helper only when enumerating and the winning N is
already established. Keep pre-witness and optimal behavior unchanged. Use runtime
`winning_node`, never the corpus's acyclic/cyclic label. Verify complete exact
solution maps, preferred witness, cancellation/proof accounting and event semantics.
Then compare isolated before/after whole all and retain optimal/all regressions.
Do not make it permanent until completed-work evidence supports it.

## Worker scheduling

In p1, L=12 uses a 32-worker budget. At cancellation only one root remains active,
in partial graph labeling, with no full-witness canonicalization active. The last
4.617 s has exactly one active root. This establishes a DFS occupancy tail, but
not its uncapped duration or the benefit of subdividing it. Root teardown after
cancellation is only 0.011 s; shutdown is not the current bottleneck.

P14 starts L=12 through L=15 at about 35.55 s with eight workers per group.
L=15 finishes at 103.131 s; the remaining three groups retain eight workers each.
At cancellation 24 roots remain active, eight each in L=12/L=13/L=14. The fixed
allocation does not reassign the freed eight workers to those groups. L=12 keeps
all eight roots active for 84.373 s. Teardown is 0.199 s.

P14 performs more concurrent work, including later groups, and returns group
results after joining them. Its visible partial result count is not a throughput
measure. Worker allocations also alter the planned frontiers, so root ordinals
across p1/p14 do not identify equal subtrees.

Full-witness spans total 23.495 s in p1 and 122.358 s in p14, with overlap in p14.
The traces visit different work and cannot isolate a canonicalization regression.
P14 processes 104,580,154 witness leaves versus p1's 26,210,304 at this cap.

After the helper trial, compare p1, p12 and p123 on the completed fixed N=9/L=12
group. P12 isolates shared-cache cost; p123 adds the existing bounded donation
pool. Donation currently requires sharing, so comparing only p1/p123 would mix
two changes. The fixed-work benchmark API first needs access to these flags.
Keep 32 workers and exact group proof/results fixed. Do not design a new pool or
promote donation from this one capped tail. Defer broader cross-group rebalancing
until this cheaper experiment establishes whether intra-group donation pays.

## Hard-10 prefix discovery

Six exact certificates, best/all twice each, one worker and 30 s caps. All 24
hard jobs remain incomplete with zero witnesses and 0/1 roots exhausted.
The two tiny controls exhaust with empty sets in 0.647 and 0.670 ms.

| Depth / pick | Hard search wall range, seconds | Process CPU range, seconds | Sampled peak range, MiB |
| ------------ | ------------------------------- | -------------------------- | ----------------------- |
| 4 / 0        | 30.007-30.017                   | 29.797-29.906              | 56.5-56.8               |
| 4 / 2        | 30.009-30.019                   | 29.641-29.906              | 37.7-38.2               |
| 6 / 0        | 30.015-30.020                   | 29.844-29.938              | 56.9-57.4               |
| 6 / 2        | 30.004-30.016                   | 29.891-29.969              | 33.1-33.7               |
| 8 / 0        | 30.010-30.020                   | 29.875-29.953              | 57.4-57.6               |
| 8 / 2        | 30.005-30.015                   | 29.813-29.938              | 33.4-34.3               |

Each row contains four processes. Preparation takes only 2.0-5.5 ms and is outside
search wall time. One-core CPU use is expected for these serial subtrees. The
maximum search cap overshoot is 20.4 ms. Exact scope and certificate checks pass,
but the screen found no completed hard control. It establishes no calculation gain.

Next discovery should try deeper paths, initially depths 10/12 within the current
limit, one all-mode sample per recipe and 5-10 s caps. Refine capped paths; retain
every outcome. Freeze a useful completed certificate before best/all repeats or
another before/after matrix. If depth 12 still fails, extend only the benchmark
selection mechanism with an explicit budget. Do not rerun the unchanged 24-job
grid, sum nested prefixes as disjoint proof, or request full serial hard baselines.

## Order and limits

Test the helper change first, discover completed prefixes cheaply, then isolate
sharing/donation on fixed work. Retain real optimal and full-enumeration checks.
Lower CPU/memory is useful evidence about cost, but completion remains the target.
No new problem cases, scheduler default or longer whole-search cap is needed yet.
