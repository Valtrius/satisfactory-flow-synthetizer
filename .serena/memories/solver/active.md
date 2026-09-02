# solver/active

Experiment 46 ready to launch; no run started yet.
Read `mem:solver/experiments/46-rref-order`.
Promotion notes 6812173. RREF candidate a79ecf7, active but not promoted.
Variables (520b352) and accounting (331b87a) are permanent.

## Launch

Manifest benchmarks/custom/rref-order-hour-screening.json.
Map target/parallelism-ladder/rref-order-variants-20260902/variants.json.
Output target/parallelism-ladder/rref-order-hour-20260902.
Use scripts/start-benchmark-screen.ps1 with -CancellationGraceSeconds 15.
40 jobs, 20 adjacent pairs. 2520s search + 600s cleanup = 52 minutes.
User limit one hour, eight minutes reserved for startup/verification.
Completion/failure dialog and durable status enabled.

## Validated

221 solver-core and 330 workspace tests, both strict Clippy builds, 41 tooling
tests, release build and 12 frozen exact executable smokes pass.
Frozen 40-job plan passes with exactly balanced AB/BA order.
RREF executable SHA256:
a431c5ce82fe435c75831a7d4403e84e9c5c6e53353ba04bcde05a40e00678d7.
70 frozen source files, only canonical.rs differs from accounting.
No performance result. Candidate a79ecf7; no push.

## After launch/completion

Check root BENCHMARK-FINISHED.txt or BENCHMARK-FAILED.txt, not just job status.
Reverify frozen hashes, all scheduled outcomes, complete solution objects, proofs
and structural work before interpreting paired timings. Preserve caps/failures.
Four RREF pairs on 115/238 and two on 36 are screening, not universal proof.
The other two-pair cells are regression spot checks. Do not pool placements.
No RREF/258 or accounting/unrestricted258 run in this time-limited batch.
No builds/tests while timing; end turn after launch.
