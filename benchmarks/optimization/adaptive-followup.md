# Adaptive comparison after partition promotion

Production baseline: `6852be9`, descending first-output order and static Boolean
second-output partitions for All min N/L. The approved promotion is recorded in
[Partition promotion confirmation](results-promotion-20260910.md).

Compare `baseline`, `adaptive-boolean`, and `adaptive-grace250`, built from that
same revision. Both candidates replace static refinement with the existing
adaptive scheduler. The grace candidate waits at least 250 ms after the first
validated witness in the current N/L group before dispatching children. Parents
retain their running backend sessions; children fill spare Boolean slots after
unstarted parents. Either the parent or a complete child cover owns its proof.

The grace does not delay parent searches or cancellation. Diagnostic records
include `refinement_grace_ms`; qualification and result audits reject children
started before their declared grace elapsed. No timing conclusion comes from
qualification probes.

## Bounded timing queue

| Comparisons, each against production        | Workers | Paired repeats | Solve cap | Jobs | Allowance with 15 s cleanup per job |
| ------------------------------------------- | ------- | -------------: | --------: | ---: | ----------------------------------: |
| 258 All min N/L, both adaptive candidates   | 8/16/32 |              2 |     120 s |   24 |                             3,240 s |
| 24/36/65 All min N/L, both candidates       | 32      |              2 |      15 s |   24 |                               720 s |
| 97 All min N/L, both candidates             | 32      |              2 |     600 s |    8 |                             4,920 s |
| Setup and verification reserves, ten suites | —       |              — |         — |    — |                             1,200 s |
| Total                                       | —       |              — |         — |   56 |                 10,080 s (2 h 48 m) |

Both candidates receive a full-completion attempt on case 97 because their
relative performance can differ from case 258. This uses two long pairs per
candidate in place of the analysis report's proposed four-pair selected finalist.
The total stays below the same three-hour limit. Every pair has identical
settings and adjacent, balanced ordering. Timing diagnostics are disabled.

The controller enforces 10,800 seconds including cleanup and verification, stops
only its owned process tree on a stall, preserves incomplete outcomes, and
displays one completion/failure dialog with a sound. Launch it and end the agent
turn. No candidate is automatically promoted; two repeats are screening evidence.

## Preparation and launch

The recorded local preparation and campaign directories are:

- `target/optimization-adaptive-prep-20260910`
- `target/optimization-adaptive-20260910/matrix`
- `target/optimization-adaptive-20260910/qualification`
- `target/optimization-adaptive-20260910/frozen`

Prelaunch validation passed: 50 baseline tests, 54 tests for each adaptive
candidate, strict Clippy on all prepared variants, 40 Python harness checks,
and 14 release qualification probes. All 1,485 frozen artifact hashes match.
The grace candidate's earliest child in the completed case-258 probe started
251.61 ms after its witness trigger. This checks the delay contract and is not
a benchmark timing result. The full receipt is
`target/optimization-adaptive-20260910/PRELAUNCH-VALIDATION.json`.

Commands below prepare new directories; use fresh paths for subsequent runs.

```powershell
python scripts/prepare-optimization.py --output target/optimization-adaptive-prep-20260910 --revision 6852be9 --variants baseline adaptive-boolean adaptive-grace250
python scripts/make-optimization-campaign.py --adaptive --output target/optimization-adaptive-20260910/matrix
python scripts/validate-optimization-followup.py --adaptive --binaries target/optimization-adaptive-prep-20260910/variant-binaries.json --output target/optimization-adaptive-20260910/qualification
pwsh -NoProfile -File scripts/freeze-optimization-campaign.ps1 -MatrixDirectory target/optimization-adaptive-20260910/matrix -VariantBinaryMap target/optimization-adaptive-prep-20260910/variant-binaries.json -OutputDirectory target/optimization-adaptive-20260910/frozen -ValidationEvidence target/optimization-adaptive-20260910/qualification
pwsh -NoProfile -File scripts/start-optimization-campaign.ps1 -PreparedCampaign target/optimization-adaptive-20260910/frozen
```

Before launch, verify frozen hashes, exact source identity, runner/backend hashes,
full result equality and proof/cancellation qualification. Tests retain the
independent reference oracle. Performance measurements use at least eight total
workers. Reasonable easy-case penalties up to roughly ten seconds can be accepted
for substantial hard-case gains; do not rank policies by counts of small wins.

Analyze full exact terminal outcomes after completion. Preserve caps as incomplete
and retain adverse pairs. Compare each candidate with its own paired production
baseline; do not pool medians across campaigns. Case 10 All min N/L remains a
separate unresolved SMT-proof target and is outside this timing queue.
