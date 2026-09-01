# solver/workflow

Custom-solver history lives only in Serena memories under `solver/`.
Keep records so another agent does not need chat history.

## On each meaningful change

1. Read `mem:critical_info` → `mem:solver/core` (and `mem:solver/active` if present).
2. Add/update a numbered experiment memory `solver/experiments/NN-slug` (use
   `mem:solver/experiments/template`). Combine closely related retries.
3. Record hypothesis, implementation, exact comparison, evidence, validation,
   outcome, limitations and next decision. Include failed/abandoned experiments.
4. Update `mem:solver/experiments/index` and `mem:solver/status`.
   Update `mem:solver/controls` or `mem:solver/benchmarking` if behavior changes.
5. Refresh `mem:solver/core` and `mem:solver/active` (clear/idle when nothing in flight).
6. Record commit hash and publication state separately from validation.
   Do not imply an uncommitted candidate is disabled in the working tree.

## Before launch

Record manifest, source/binary identity, workers, mode, caps, instrumentation,
repeats, expected result class, and the question under test.
Label “launched; awaiting analysis”. Freeze inputs/tools, provide a completion
signal, then end the turn. No builds/tests during timing runs.

## After results

Verify schedule completeness, executable identity, exact problem and result
identities before interpreting timing.
Separate measured vs hypothesis vs pending vs commit state.
Never invent hashes, timings, or causal claims.

## Keep memories usable

- Prefer dense notes; split by experiment/concern.
- Historical findings stay in the numbered experiment memory; current choices in
  `mem:solver/status` / `mem:solver/core`.
- Raw `target/` evidence is local/ignored and may be absent in another checkout.
- Preserve experiment memories when code is removed or a proposal is rejected.
- Run `serena memories check` after renames/deletes.
- Doc-only work during benchmarks must not trigger builds, tests, or source changes.
