# 43. Isolated 258 root results

Date: 2026-09-02. State: completed, reverified and analyzed.
Design, source/binary hashes: `mem:solver/experiments/43-root258-affinity`.

## Verification

All 28 jobs exhausted their selected root. No caps, kills, process failures or
validation failures. Finished 2026-09-02 01:13:05 Europe/Paris, after 51.578 process
minutes. These are local proofs, not whole-solve optimality proofs.
Both roots have empty solution sets. They measure dead-subtree proof work, not
time to a witness or successful witness canonicalization.

Frozen verifier passes 28/28 again. Rechecked summary is byte-identical:
`e776daf51c952516dd5f369b6ede3ccd3c55235a568e8567e1c65c98512ff8c4`.
All four executables and 1,156 archive file hashes verify. Requests, seeded schedule,
normalized problems, selected certificates, full exact profile/proof fields and
all 15 structural counters agree within each root across variants and CPUs.
All 31 non-timing hotspot counters agree across the four root-23 diagnostics.

Original p1/32 plan has target 128 and 76 ordered keys in every job.
Canonical JSON plan hash:
`9f136655c1ea5b4994c01063b23bd6c7157579433592b1b9e5c3828a11458020`.
Affinity requested/observed masks agree in all 28 jobs. Application delay is
8.216-26.833ms after process launch. No claim of affinity before first instruction.

Evidence: `target/parallelism-ladder/root258-affinity-20260902`.
Reproduction: run its `analyze-results.py`. It redirects the frozen verifier's
output to `analysis/summary-rechecked.json`, preserving original summary/results.
Independent detailed report: `analysis/verified-report.json`.
An initial extra-file audit rejected the 26 expected diagnostic live snapshots as
extra completed jobs. It was corrected to distinguish scheduled results from
named live snapshots. No benchmark or solver failure occurred.

## Completed search timings

Hotspots OFF. Root 23 has two samples per CPU/variant; root 7 has one.
Negative change means faster. Compare only within the same CPU/root.

| Root | CPU | Reference | Integer rows     | Complete variables | Combined         |
| ---- | --- | --------- | ---------------- | ------------------ | ---------------- |
| 23   | 0   | 103.694s  | 104.316s, +0.60% | 101.935s, -1.70%   | 104.161s, +0.45% |
| 23   | 31  | 118.411s  | 120.606s, +1.85% | 118.065s, -0.29%   | 119.943s, +1.29% |
| 7    | 0   | 98.165s   | 99.944s, +1.81%  | 97.278s, -0.90%    | 100.156s, +2.03% |
| 7    | 31  | 117.137s  | 119.826s, +2.30% | 115.971s, -1.00%   | 117.074s, -0.05% |

Root 23 ranges, CPU 0 / CPU 31:

- Reference: 102.787-104.601 / 118.329-118.493s.
- Integer: 104.220-104.412 / 119.461-121.751s.
- Variables: 101.783-102.086 / 118.005-118.125s.
- Combined: 103.505-104.818 / 119.077-120.808s.

Process CPU changes for integer / variables / combined:

- Root 23 CPU 0: +1.07% / -1.28% / +0.55%.
- Root 23 CPU 31: +2.01% / -0.17% / +0.87%.
- Root 7 CPU 0: +1.93% / -0.93% / +1.01%.
- Root 7 CPU 31: +1.12% / -0.94% / -0.23%.

Variables are modestly better in every matched wall/CPU comparison. Integer is
worse in every comparison. Combined loses most of the variable-only benefit.
Root 7 single samples are corroboration, not a measured distribution.

## Placement and diagnostics

CPU 31 is 14.19-15.82% slower on root 23 and 16.89-19.89% slower on root 7 across
variants. This establishes placement sensitivity on these isolated workloads.
The initial experiment did not identify cache topology or clock behavior, and does
not explain all earlier whole 258 variance. In the subsequent user discussion,
the user confirmed a dual-CCD CPU with one V-Cache CCD. A read-only Windows
GetLogicalProcessorInformationEx query then verified group 0 logical CPUs 0-15
share 96MiB L3, mask `ffff`; CPUs 16-31 share 32MiB L3, mask `ffff0000`. Thus CPU 0/31
cross those cache domains. This does not isolate cache capacity from clock or
other placement effects. Proposed future protocol: `mem:solver/benchmarking`. Do not pool CPUs or compare isolated replay directly with full-run
root times. Concurrent roots, caches and resource contention differ.

Separate root 23 diagnostics, reference to variables:

| Phase               | CPU 0                       | CPU 31                      |
| ------------------- | --------------------------- | --------------------------- |
| Variable collection | 0.9602 to 0.2234s, -76.74%  | 0.9464 to 0.2221s, -76.53%  |
| Sparse preparation  | 5.0990 to 4.3845s, -14.01%  | 4.9708 to 4.2243s, -15.02%  |
| Sparse analysis     | 24.7543 to 23.8445s, -3.68% | 23.3524 to 22.9794s, -1.60% |
| Forward elimination | 14.9675 to 14.8471s         | 13.9288 to 14.1843s         |

556,726 sparse calls and 5,600,464 variable instances agree across diagnostics.
Thus the intended local saving repeats on both CPUs. Do not insert instrumented
wall times into ordinary timing medians.
Root finish is only 0.057-0.063s. Root search owns effectively the entire span.
No open/dropped activity records. Sparse forward elimination is the largest
individual recorded algebra subphase here, around 14-15s. Inequality sort and
equality RREF each take around 7s. Timers are nested; do not sum them or subtract
their sum from process CPU.

CPU 31's longer search is not accompanied by larger canonical/algebra timers in
these diagnostic samples. Further attribution needs a sampled CPU profile or
non-overlapping DFS/cache phase measurements. Cache lookup, fingerprints,
snapshot creation and cache-byte accounting are hypotheses, not established costs.
Source review confirms deferred insertion/promotion recomputes snapshot byte
estimates, but this run did not time those operations.

## Decision and next steps

1. Recommend restoring the previous rational inequality representation. The
   integer-only candidate's isolated microbench benefit did not translate to these
   roots, and prior whole evidence was mixed/adverse. Do not gate by input/CPU/N.
2. Retain complete variables as the sole candidate for a final matched whole-solve
   screen. Prior 115/238 controls and all current root comparisons favor it, but
   the earlier 258 whole regression remains unresolved. Do not promote it from
   empty-root results alone. Include optimal and minimum_links cases with real
   witnesses; 258 minimum_links remains the long completion control.
3. Keep scheduler extras paused. Use placement-aware benchmark diagnostics without
   changing production affinity. Before another broad arithmetic rewrite, sample
   the remaining DFS/cache work on these completed roots. Forward elimination is
   a measured future target, not evidence for an incremental-basis redesign.

No production source edit, restoration, promotion, new benchmark or push this turn.
Tooling/launch is committed in `986e330`. This result and current-state memory
update are uncommitted. Await user decision on the recommended rollback/screen.
