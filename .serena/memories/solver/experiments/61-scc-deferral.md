# 61. Isolated SCC deferral vs production after experiment 60

Date: 2026-09-05. State: analyzed; rejected. Related: `mem:solver/experiments/29-deferred-state-canonicalization`, `mem:solver/experiments/57-post54-cost-profile`, `mem:solver/experiments/60-remaining-port-under-l`.

## Question and hypothesis

After experiments 58 and 60, experiment 57 still ranked SCC keys at 7–11% of hotspot-on accounted time (smaller than leftover state-key canonaut). Can DFS skip exact SCC-key canonicalization until a cheap invariant fingerprint repeats, without changing exact layouts or proofs?

Hypothesis: most open-SCC visits pay for a full canonical key that does not hit. First visit of a fingerprint solves algebra on raw ports and stores the summary under the invariant. A repeat promotes the saved snapshot and the current region to authoritative exact keys; later visits use the exact cache. A fingerprint never proves equality. Collision can force extra promotion but cannot cause a false cache hit. Fingerprint inequality can miss reuse but cannot prune search.

## Change and comparison

Isolated candidate against `6bb0763` (production after experiment 60). No new runtime flag. Candidate `SearchFeatures` used `SccCacheMode::Deferred`. Exact-first SCC cache remained test-only (`EXACT_SCC_CACHE`). Public witness protocol and SCC algebra unchanged.

Mechanism:

- Cheap fingerprint hashes remaining profile, discard count, sorted link descriptors, region node-type/incident descriptors, and known port values by isomorphism-invariant owners plus port index. Raw node IDs do not enter the digest.
- First visit: identity port maps, `summarize_open_scc`, apply deductions, store snapshot + raw summary. No exact key.
- Repeat: canonicalize the saved snapshot, relabel the stored summary into the winning joint labeling, insert the exact cache, then canonicalize the current region and look up or solve as today.
- Disabled cache still always solves. Sharing/donation unchanged and remain off in production p1.

Diagnostics: `deferred_scc_visits`, `deferred_scc_promotions`. Deterministic cache-byte accounting includes deferred SCC snapshots.

Before: production `6bb0763`. After: this candidate. Isolated builds `C:/Users/jakez/.codex/tmp/sf61/{before,after}` with `CARGO_TARGET_DIR` `sf61bt`/`sf61at`. Frozen `profile_case` SHA256: before `ED652ECF857CF57DB4ED8271A1DB60D95B020123FA96488648972003F3165EE3`, after `BF191906DCF0B22B9B21DE8E0CE229EB1318569EA6079CC731020F84142D3F64`. Manifest `benchmarks/custom/scc-deferral-screen.json`. Variant map `benchmarks/custom/scc-deferral-variants.json`. Plan-only accepted 30 jobs. Screen `target/parallelism-ladder/scc-deferral-ab-20260905`. 30 hotspot-off jobs, three AB/BA pairs, p1. 258 omitted for the one-hour budget. Search 1740s + cleanup 1800s = 3540s, `MaxScheduledSeconds` 3600. Affinity: 115/238 CCD32 `ffff0000`, CCD96 `ffff`, 36 all-CPU empty.

Prelaunch correctness (not speedup): solver-core `--all-targets` 195 lib tests passed, 2 ignored; outer reference, parallelism, examples passed; strict solver-core clippy `-D warnings` passed; 41 analyzer tests passed.

## Results

Official `BENCHMARK-FINISHED.txt` 2026-09-05T10:19:18+02:00. Runner PID 41524. Frozen `results/binaries.json` matches the launch hashes. Summary SHA256 `676AB89C4FF72956B54AA077FE7DA22999747717624AE1372502DF914193B594`. Analyzer `failures: []`. 30/30 completed `optimal`. All 5 paired cells `all_completed: true`.

Paired candidate/reference (median change, bootstrap 95%):

| Cell | Wall | Wall 95% | CPU | First-valid |
| 115 all CCD32 | −7.15% | −8.62 to −6.17 | −5.80% | −1.44% (CI includes 0) |
| 115 all CCD96 | −5.16% | −7.02 to −3.71 | −5.79% | −3.76% |
| 238 opt CCD32 | −8.85% | −8.86 to −8.50 | −8.75% | −8.85% |
| 238 opt CCD96 | −8.48% | −9.04 to −0.04 | −7.86% | −8.47% |
| 36 opt 32w | **+4.93%** | **+2.27 to +14.0** | −0.73% (CI includes 0) | +4.93% |

36 wall ratios all > 1 (1.140, 1.023, 1.049). Median walls (s): 115 CCD32 20.69→19.21; 115 CCD96 22.28→21.04; 238 CCD32 5.78→5.27; 238 CCD96 6.44→5.89; 36 9.27→9.57.

Peak sampled WS (MiB): 115 CCD32 196.9→222.7; 115 CCD96 188.2→215.8; 238 CCD32 142.6→189.8; 238 CCD96 160.8→204.5; 36 64.3→68.1.

Deferred SCC activity (after only, identical across repeats): 115 74917 visits / 3344 promotions; 238 23126 / 3079; 36 3088 / 47. 115 `scc_solves`+`scc_cache_hits` = 78514 both sides (76300+2214 after, 72306+6208 before). 36 `scc_solves` 3091 and `scc_cache_hits` 44 both sides. 115 CCD32 `canonicalization_time_ns` ~48.05s→36.84s; 36 canon time ~22.6s→22.0s (no material skip).

## Correctness and limitations

Identity held on all 15 before/after pairs: status, layout counts, exact `layout_keys`, and `raw_structural_decisions`/`states`/`deferred_state_*` match. Layouts: 115 all 49 at N=7 L=10; 238 N=8 L=13 one layout; 36 N=9 L=11 one layout. Analyzer `failures: []`. Nested timers must not be treated as CPU. Do not compare hotspot-on walls with this OFF screen. Fingerprint is not a proof of SCC-key equality.

36 is constructor-dominated (experiment 57 SCC share ~1.5% of accounted). Almost no SCC reuse (47 promotions), so snapshot/fingerprint overhead is extra cost without skipped labels.

## Decision and next step

Rejected. One completed cell (36) regresses with bootstrap CI excluding 0, matching the experiment 50 / labeling experiment 58 bar. 115/238 wall wins and identity are real but insufficient. Solver-core source restored to `6bb0763`. Candidate preserved as `benchmarks/custom/variants/scc-deferral.patch` plus the screen/variant-map JSON. Analyzer `deferred_scc_*` median columns remain (harmless zeros on production runs). Results recorded in `4adddc2`. No push.

Parked with the other rejected kernel/cache variants: do not enable production SCC deferral. Next authorized work: hotspot-on cost-mix of production after 58+60. Remaining large nested share is leftover state-key canonaut. Keep parked: templates, sharing, donation, checked64, fraction-free RREF, weighted quotients, duplicate-row removal, rollback-aware Bareiss, early reachability, labeling, over-L-before-prepare, complete-L-before-solve. Do not enable `work_stealing`.

Follow-up (2026-09-05, untested): a production “cyclic problems only” switch is the wrong predicate. `analyze_affected_dynamic_sccs` already filters `region.cyclic`; deferral would only run on those regions. 36 still did 3091 cyclic-region SCC solves (47 promotions), so intermediate search states form cycles even when the optional acyclic constructor owns wall (experiment 57: `acyclic_construct` 6.25–7.92s of 12.19s). Case labels such as acyclic36 are not solver inputs (`mem:solver/contracts`). A constructor-hit or “snapshot only after a fingerprint repeats” gate could target 36’s unused-snapshot overhead; that is not measured and is not a cyclicity/case policy.
