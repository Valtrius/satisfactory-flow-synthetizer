# 23 results - Whole witness-port translation and DFS profile

Date: 2026-08-29. State: verified. Source launch commit: `25a4dc2`.
Related: [protocol](23-whole-translation-and-dfs-profile.md),
[isolated port result](22-results.md).

## Verification

The frozen analyzer verifies all 27 scheduled records. There are 20 completed
reference runs and seven explicitly capped stress or diagnostic runs. No job failed
or was killed. Completed results preserve the normalized problem, mode, preferred
solution, proof status, full solution objects and canonical layout-key sets.

The original and rechecked `summary.json` files have the same SHA-256:
`1a44754aa9a5c46b4b12201929b6fcc591c390b82d41c59deb975d87a5b0718e`.
The run consumed 39.21 process wall minutes and 654.83 process CPU minutes.
Raw evidence is in ignored
`target/parallelism-ladder/witness-port-whole-20260829/`.

## Completed A/B results

Times are two-run medians. Ranges are shown in parentheses.

| Case    | Mode    |                      Before |                       After |         Change |
| ------- | ------- | --------------------------: | --------------------------: | -------------: |
| 115     | all     | 155.152 s (155.108-155.196) | 139.675 s (139.266-140.084) |  9.98% shorter |
| 115     | optimal |     10.271 s (9.800-10.742) |    10.649 s (10.609-10.689) |   3.68% longer |
| 238     | all     |  102.077 s (98.127-106.026) |    58.081 s (52.844-63.318) | 43.10% shorter |
| 238     | optimal |    65.483 s (57.638-73.329) |    31.601 s (31.598-31.603) | 51.74% shorter |
| hard 36 | optimal |    23.056 s (22.266-23.847) |    23.486 s (23.271-23.701) |   1.86% longer |

All A/B pairs perform identical structural work. For example, 238 optimal has
1,036,393 decisions, 286,996 retained states, 69,562 duplicate states and 51,345
SCC solves in every run. Its median process CPU falls only 1.65% while wall time
falls 51.74%. Removing the serial witness-port enumeration raises median CPU use
from 21.1% to 42.6% and makes both after samples stable at 31.6 seconds.

The 238 all median first witness improves from 70.516 to 29.411 seconds. The 115
all first witness is unchanged within 1.1%. The 115 and hard-36 optimal changes are
small and inconsistent with a useful gain. The port optimization remains permanent
because the isolated replay and 238 whole runs show a large exact improvement, and
no completed workload changes search coverage or results.

## Capped hard 36

The before run and both after runs reach the 240-second cap at N=9/L=14 with the
same 12 known layout keys. They do not prove complete enumeration. The two after
runs retain 2.83-2.91 million states versus 2.66 million before, but fixed-time
partial work is sensitive to scheduling and cannot establish completion speed.

## Remaining measured costs

Diagnostic timers are aggregate worker time and top-level buckets do not overlap.
Graph-canonicalization subphases overlap those top-level buckets.

| Case                      | Legal decisions | State key | Propagation | SCC key | Graph canon calls |
| ------------------------- | --------------: | --------: | ----------: | ------: | ----------------: |
| 115 all, complete         |          31.55% |    33.97% |      27.90% |   3.78% |        11,984,541 |
| 238 all, complete         |          37.08% |    30.76% |      24.74% |   5.07% |         4,138,433 |
| hard 36 all, 120 s cap    |          51.70% |    29.91% |      15.98% |   0.64% |        11,894,293 |
| hard 10 optimal, 60 s cap |          33.80% |    28.36% |      24.05% |  11.02% |         4,731,714 |

Witness search is now below 0.01 seconds of aggregate measured time in every
diagnostic. Graph canonicalization accounts for 60.6-78.9% of top-level accounted
time. Canonical labeling alone is 29.6-36.9% of graph-canonicalization time.

The legal-decision bucket currently combines finalist open-port canonicalization
and marked-child canonicalization. It is therefore the first target to split, not
yet a safe target to remove. State keys, open-port orbit selection and marked-child
keys prove different identities. Reusing one as another would change the exact DFS
quotient unless a new derivation and exhaustive differential tests establish their
equivalence.

## Decision

1. Keep analytic witness ports permanent.
2. Keep scheduler work paused. These results concern calculation cost.
3. Add diagnostic-only per-purpose graph-canonicalization calls and time for state,
   open-port, marked-child and SCC keys.
4. Replay completed 238 all plus short hard-36 and hard-10 diagnostics. Optimize
   the dominant purpose by reusing immutable incidence work or generated decisions,
   while retaining the exact key protocols and full reference comparison.
