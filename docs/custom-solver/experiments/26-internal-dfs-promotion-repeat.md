# 26 - Internal DFS promotion repeat

Date: 2026-08-30. State: repeat run launched; results pending.
Related: [screening results](25-internal-dfs-marked-bypass.md).

Run: ignored `target/parallelism-ladder/unkeyed-dfs-repeat-20260830/`.
Manifest: `benchmarks/custom/unkeyed-dfs-repeat.json`.

## Goal and gate

Repeat only the four experiment 25 workloads that completed. Two new samples per
variant combine with the screening sample to give three measurements for 36 optimal,
115 all, and 238 all/optimal. The 16 new jobs have 1,500 seconds of aggregate caps.
The runner randomizes their frozen schedule.

Promote the candidate only if all records verify, every completed pair preserves
the exact result fields, and the combined three-sample median favors the bypass on
each workload. Keep root planning and adaptive-frontier decisions keyed. Scheduler
experiments remain paused.

The run reuses experiment 25's frozen binaries. Their required SHA-256 values are:

- keyed reference: `743ecf14efeda83b708651c2b33042a89d1ce58b555fd3ee4328939e0515a7fd`;
- internal unkeyed candidate: `f15d49bba7a3263ae23711e553cb978eb0bdcb0902b17072596d1cd626f5dd73`.

## Hard-run memory attribution

The high working set is primarily the sum of worker-local exact memoization caches.
Each dispatched root constructs a `SearchContext` with its own state-status map and
open-SCC summary map. Production p1 can keep 32 such roots active concurrently;
sharing remains disabled.

The experiment 25 hard-10 diagnostic supports this attribution:

| Variant | Largest local cache | State entries | Process peak |
| ------- | ------------------: | ------------: | -----------: |
| Keyed   |             54.0 MB |        24,039 |      1.61 GB |
| Bypass  |             65.6 MB |        30,988 |      2.04 GB |

Thirty-two times the candidate's largest local cache is about 2.10 GB, close to the
sampled 2.04 GB process peak. This is not exact accounting. The local counter combines
state and SCC payload lower bounds, reports the largest local cache rather than their
simultaneous sum, and excludes hash-table buckets, spare capacity, allocator retention,
thread stacks and temporary canonicalization buffers.

State keys are the likely largest retained item because every unique partial state
stores its full canonical byte identity plus a status. SCC summaries add exact keys,
endpoint deductions and arbitrary-precision rational payloads. Saved solutions are not
the cause on hard 10 because neither variant finds a witness.

If memory work resumes, first split live state-cache and SCC-cache bytes per worker and
report their aggregate concurrent totals. Then test smaller exact state storage or a
bounded completed-state policy. Never replace exact keys with unchecked hashes, and
retain in-progress entries needed for cycle and proof accounting.
