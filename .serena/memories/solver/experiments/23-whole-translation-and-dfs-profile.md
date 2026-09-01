# 23 - Whole translation and DFS profiling

Date: 2026-08-29. State: completed and verified.
Related: analytic witness-port results (`mem:solver/experiments/22-results`).

Results: whole translation and DFS profile (`mem:solver/experiments/23-results`).

## Questions

1. Does the exact symmetric-port optimization reduce whole best and all completion
   time on medium and difficult cases?
2. Does hard 36 all now complete inside a manageable cap?
3. After witness work is removed, which exact DFS/canonicalization phase should be
   lightened next without changing the searched quotient or proof?

Scheduling remains fixed at production p1 with 32 workers. Corpus labels and known
cyclicity never enter solver policy.

## Comparison

The before binary is rebuilt from `e30c72c`, the direct production ancestor before
analytic ports and after the permanent constructor guard. The after binary is built
from the clean experiment 23 revision containing permanent `f5df873`. Both use the
same toolchain, release profile, manifest, cases and machine.

Manifest: `benchmarks/custom/witness-port-whole-screening.json`.

- 115 and 238 best/all: two ordinary repeats per binary.
- Hard 36 best: two ordinary repeats per binary.
- Hard 36 all: one before and two after jobs; caps may remain incomplete.
- Current after-only all diagnostics for 115, 238 and 36.
- One current hard-10 best diagnostic capped at 60 seconds and N=11.
- Ordinary caps are 60/240 seconds for 115, 150/180 for 238, and 90/240 for 36.

There are 27 serial jobs and 70 minutes of aggregate search caps. Expected runtime
is lower because all but hard-36 all and hard 10 have completed historical controls.
Instrumented jobs stay out of ordinary medians.

## Correctness and interpretation

The verifier must preserve normalized problems, modes, N caps, stage, preferred
witness, full saved solution objects, canonical layout-key sets and proof status.
Incomplete stress jobs remain incomplete evidence; common partial solutions must
agree, and no watchdog kill is accepted.

For DFS work, measure top-level state canonicalization and legal-decision time
separately from overlapping graph-canonicalization sub-buckets. The current exact
path canonicalizes each state, finalist open-port orbits and every marked child.
Possible reuse is only a hypothesis: parent canonical labeling or child structural
information may avoid rebuilding equivalent incidence work, but no identity may be
skipped unless exhaustive reference comparisons prove the same quotient and order.

Do not infer a scheduler conclusion from CPU troughs or capped enumeration. Choose
the next fixed completed root from these results and compare calculation candidates
there before changing production DFS behavior.

## Validation before launch

- The frozen plan validates all 27 unique jobs and the 4,200-second aggregate cap.
- The before binary was freshly built in an isolated `e30c72c` worktree; SHA-256 is
  `909f3ae7b7bb1c7ddfae780c37758c12e9553cf1005e78c36f430bd9c1737d75`.
- The current release binary SHA-256 is
  `99c03267f14446157f3e01c879ba1b5257861fc58c8438c666638d08ff62fe65`.
- Both binaries complete the tiny all-mode smoke with identical outcome,
  validation and full solution objects.
- Experiment 22 already supplies the current source's full solver-core tests,
  exhaustive outer reference matrix and strict Clippy validation. This experiment
  changes only benchmark data and documentation.
