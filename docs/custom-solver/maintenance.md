# Maintaining the experiment history

This is a continuing part of Custom solver work, as requested by the user.
Keep the records in the repository so another agent does not need chat history.

## On each meaningful change

1. Read [current status](status.md) and the relevant experiment before editing.
2. Add a numbered record under `experiments/` for a new hypothesis or intervention.
   Use [the template](experiments/template.md); combine closely related retries.
3. Record hypothesis, implementation, exact comparison, evidence, validation,
   outcome, limitations and next decision. Include failed/abandoned experiments.
4. Update [the index](experiments/README.md) and [current status](status.md).
   Update [controls](controls.md) or [benchmarking](benchmarking.md) if behavior changes.
5. Record commit hash and publication state separately from implementation and
   validation. Do not imply an uncommitted candidate is disabled in the working tree.

## Before launching a benchmark

Record manifest, source/binary identity, worker counts, mode, node/time caps,
instrumentation, repeats, expected result class and the question being tested.
Label the record "launched; awaiting analysis". Freeze inputs and tools, provide
a completion signal, then end the turn. Avoid builds/tests during timing runs.

## When results arrive

Verify schedule completeness, executable identity, exact problem and result
identities before interpreting timing. Record kills and verification failures.
Give sample count and median/range where repeated; label single samples.
Separate full solve, first witness, bounded proof, timeout and cancellation tail.
Record what the evidence cannot establish. A corrected hypothesis should retain
its original observation and explain why the conclusion changed.

## Keep the files usable

- Aim for roughly 40-120 lines per file. Split by experiment or concern, not by
  arbitrary line count. Keep the entry point and current status short.
- Historical findings stay in the dated record; current choices live in status.
  Link follow-ups instead of copying old recommendations as if they were current.
- Keep essential numbers, failures and conclusions in Markdown. Raw `target/`
  evidence is local/ignored and may be absent in another checkout.
- Link evidence directories and exact result/summary filenames. Never claim a
  missing archive was reverified. Do not invent hashes, timings or causal claims.
- Preserve experiment records when code is removed or a proposal is rejected.
- Check links, headings and Markdown formatting. Documentation-only work during
  benchmarks should not trigger builds, tests, reruns or source changes.

This workflow concerns repository documentation. It does not require changing
Codex's separate memory files or creating an automation.
