# solver/active

Focused experiment 46 confirmation prepared and plan-validated; not launched.
Read `mem:solver/experiments/46-rref-order-confirmation`.
Prior failed screen: `mem:solver/experiments/46-rref-order-results`.
No new solver code; RREF a79ecf7 remains active/unpromoted.

## Launch

Manifest benchmarks/custom/rref-order-confirmation-screening.json.
Map target/parallelism-ladder/rref-order-variants-20260902/variants.json.
Run target/parallelism-ladder/rref-order-confirmation-20260902.
Use scripts/start-benchmark-screen.ps1 with -CancellationGraceSeconds 15.
20 jobs, ten adjacent pairs, exact order balance. 3000s search + 300s cleanup =
55 minutes. Five minutes reserved within the user's hour for startup/verification.
Completion/failure dialog required; end turn after launching.

## Scope

accounting/rref: 115 all CCD96 two pairs, 238 optimal CCD32 four pairs,
258 minimum_links CCD96 two pairs. before/variables: 258 minimum_links CCD32
two pairs with 400s cap instead of 320s. All workers16, capacity1200, p1, hotspots off.
Unrestricted accounting/36 repeats omitted to fit the budget.
All frozen binaries/source hashes and the full plan verified. No new build/test.
No performance result. No production scheduling/affinity change or push.

## After completion

Check root BENCHMARK-FINISHED.txt or BENCHMARK-FAILED.txt. Reverify frozen hashes,
all exact outputs, proofs and structural work. Reference-cohort caps are failures,
not stress data. Keep the old failed 320s run intact. Do not pool placements or
silently merge sessions. Interpret two-pair cells as limited evidence.
