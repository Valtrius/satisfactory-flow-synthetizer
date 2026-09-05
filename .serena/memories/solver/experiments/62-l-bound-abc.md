# 62. Remaining-port under-L placement and forced remaining-L A/B/C

Date: 2026-09-05. State: analyzed; both candidates rejected.
Related: `mem:solver/experiments/59-l-shortcut-opportunity`, `mem:solver/experiments/60-remaining-port-under-l`.

## Question and hypothesis

Does applying the existing remaining-port under-L prune in prefix replay, planner prepare, and `search_state` (places) improve completed wall, and does a new forced remaining-L lower bound (forced) prune low-L work that experiment 59's already-over-L counter missed?

Hypothesis: places helps high-L `all` groups (115) where under-L already fires but planned/replayed prefixes still pay prepare. Forced helps low-L unsat groups (238 optimal) where under-L hit rate was 0%. 36 is a regression canary. Candidates are isolated against the same production before; they are not combined.

## Change and comparison

Production before: `6bb0763` (experiments 58+60). Hotspot-off p1. Two comparisons share the before binary.

- `under_l_places` / variant `places`: `benchmarks/custom/variants/under-l-places.patch`. Same under-L predicate. Also prune in `prepare_applied_child` (covers replay and adaptive refine) and at `search_state` entry before snapshot. Child apply path still prunes before prepare.
- `forced_remaining_l` / variant `forced`: `benchmarks/custom/variants/forced-remaining-l.patch`. Adds `remaining_operator_link_lower_bound` (leftover node ports minus remaining terminals) and prunes when `current L + min extra > expected L` in `evaluate_applied_child` before prepare. Existing under-L upper bound unchanged. Increments existing `lower_bound_prunes`.

Manifest `benchmarks/custom/l-bound-abc-screen.json`. Variant map `benchmarks/custom/l-bound-abc-variants.json`. Isolated builds `C:/Users/jakez/.codex/tmp/sf62/{before,places,forced}`. Frozen `profile_case` SHA256: before `B3F8A05D0081231684EB74FE031A43A97F2C089555D7F498464F24506D7C5A9C`; places `0166ACE37BAC32B8714EECF9F7ABBEF1B4AE39B26C6CD0993FCA37F8219321C6`; forced `FD2B6B3D43A5BBDD2984B8B6A4D5A291639F2667D528860D133E371942F2E1FD`. Screen `target/parallelism-ladder/l-bound-abc-20260905`. 60 jobs, `MaxScheduledSeconds` 7200. Three AB/BA pairs on 115 all and 238 optimal at both CCDs, 36 optimal all-CPU. 258 omitted. Search 3480s + generator cleanup 900s; watchdog grace additional. Affinity: 115/238 CCD32 `ffff0000`, CCD96 `ffff`, 36 all-CPU empty (observed `ffffffff`, `affinity_before_resume` true).

## Results

Official `BENCHMARK-FINISHED.txt` 2026-09-05T11:57:47+02:00. Frozen `results/binaries.json` matches the launch hashes. Summary SHA256 `119C9FBBB97F4FC4ECCD68B6A531B335C1918DD9011562793329C3A59F0401E1`. Analyzer `failures: []`. 60/60 completed `optimal`. All 10 paired cells `all_completed: true`.

Paired candidate/reference (median change, bootstrap 95%):

**places** (earlier under-L):

| Cell | Wall | Wall 95% | CPU | First-valid |
| 115 all CCD32 | +0.44% | −1.85 to +1.68 | +0.24% | −0.46% (CI excludes 0) |
| 115 all CCD96 | +0.14% | −1.00 to +0.91 | +0.53% | +0.79% (CI includes 0) |
| 238 opt CCD32 | **+1.26%** | **+0.13 to +4.16** | +0.48% | +1.26% |
| 238 opt CCD96 | −0.25% | −1.69 to +3.26 | +0.03% | −0.25% |
| 36 opt 32w | −0.73% | −1.97 to +3.81 | +0.18% | −0.73% |

**forced** (remaining-L lower bound):

| Cell | Wall | Wall 95% | CPU | First-valid |
| 115 all CCD32 | +0.018% | −0.49 to +0.78 | +0.18% | +2.16% (CI excludes 0) |
| 115 all CCD96 | **+0.91%** | **+0.10 to +2.08** | **+0.34%** [+0.16, +0.96] | +1.53% (CI includes 0) |
| 238 opt CCD32 | −0.71% | −0.98 to −0.42 | +0.06% (CI includes 0) | −0.71% |
| 238 opt CCD96 | −1.03% | −3.71 to −0.83 | +0.87% (CI includes 0) | −1.03% |
| 36 opt 32w | −0.95% | −0.98 to +0.29 | +1.24% (CI includes 0) | −0.96% |

Median walls (s): places 115 CCD32 20.315→20.244; 115 CCD96 21.785→21.816; 238 CCD32 5.558→5.692; 238 CCD96 6.338→6.322; 36 9.112→9.052. Forced 115 CCD32 20.147→20.305; 115 CCD96 21.752→22.020; 238 CCD32 5.710→5.675; 238 CCD96 6.279→6.120; 36 9.156→9.095.

Peak sampled WS (MiB, csv median): places 115 CCD32 178.4→174.4; 115 CCD96 172.9→181.0; 238 CCD32 140.9→138.8; 238 CCD96 142.9→143.3; 36 64.5→62.2. Forced 115 CCD32 178.1→174.9; 115 CCD96 178.2→173.9; 238 CCD32 140.3→141.9; 238 CCD96 140.2→143.8; 36 64.5→62.2.

Structural (r1, identical across CCDs where both exist):

- places 115: decisions 2,087,005→2,084,482; states 859,160→858,704; root partitions 812→625; `lower_bound_prunes` 47,475→46,095. Tiny SCC/deferred reductions.
- places 238: all tracked counters identical, including `lower_bound_prunes` 14. Extra prepare/`search_state` checks on a no-hit cell.
- places 36: decisions 1,147,205→1,147,091; roots 1,833→1,819; `lower_bound_prunes` 59,131→59,073.
- forced: decisions, states, SCC, roots, and `capacity_prunes` identical on every cell. `lower_bound_prunes` 115 47,475→59,824; 238 14→15,197; 36 59,131→140,982. `propagation_contradictions` fall by the same deltas (115 742,373→730,024; 238 341,428→326,245; 36 785,285→703,434). The lower bound fires and replaces later propagation contradictions 1:1; it does not shrink search.

## Correctness and limitations

Identity held on all 30 before/candidate pairs: status, layout counts, exact `layout_keys`, preferred witnesses, and full saved solutions. Layouts: 115 all 49 at N=7 L=10; 238 N=8 L=13 one layout; 36 N=9 L=11 one layout. Analyzer `failures: []`. Structural counters may differ where a prune fires; forced keeps decisions/states identical by design of the 1:1 contradiction replacement. Nested timers must not be treated as CPU. Three pairs per cell. 258 untested. places is not complete-L-before-solve; forced is not experiment 59's already-over-L-after-apply counter.

## Decision and next step

Rejected both. places: no completed-wall win with CI excluding 0; 238 CCD32 (the under-L no-hit control) regresses +1.26% with CI excluding 0. Experiment 60's apply-site prune already captured the 115 savings; extra placements do not pay. forced: the missed prune is real (15k hits on 238, 12k on 115, 82k on 36) but only relocates children that already died in propagation, so states/decisions stay flat. 115 CCD96 wall +0.91% and CPU +0.34% both have CIs excluding 0. 238 wall −0.71%/−1.03% is a ~1% skip of those propagation deaths and does not clear the 115 regression. Production solver-core remains `6bb0763`. Candidates preserved as `benchmarks/custom/variants/under-l-places.patch` and `benchmarks/custom/variants/forced-remaining-l.patch`. Results recorded in `4adddc2`. No push.

Do not add earlier under-L placement, forced remaining-L, over-L-before-prepare, or complete-L-before-solve. Keep parked: templates, sharing, donation, checked64, fraction-free RREF, weighted quotients, duplicate-row removal, rollback-aware Bareiss, early reachability, labeling, SCC deferral. Do not enable `work_stealing`. Next authorized work: hotspot-on cost-mix of production after 58+60.
