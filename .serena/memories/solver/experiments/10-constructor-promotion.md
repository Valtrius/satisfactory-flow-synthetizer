# 10. Permanent constructor improvements

Date: 2026-08-28. State: promoted in `099cc12`; not pushed.
The user approved a separate constructor commit with its documentation.

## Included

- Necessary acyclic eligibility from normalized input/output rates and profile
  splitter arities. Failure skips only the optional helper, never general search.
- Reuse completed helper hits/misses within one node count and solve. Cancellation
  cannot create a completed miss, and changing N clears the cache.
- Exact integer subset sums in the original mask order, delayed leaf cloning and
  a direct full-mask check for the last output. Frequent cancellation checks remain.
- Crate-local denominator access and solver coordination. The source change is
  limited to `acyclic_incumbent.rs`, `lower_bound.rs` and `solver.rs`.

## Excluded

Compact state/SCC keys are a separate commit. The unproven five-second constructor
deadline stays removed. Scheduler defaults, proof semantics, public result identity
and general cyclic/acyclic coverage are unchanged. Benchmark labels are not inputs.

## Evidence and verification

04 (`mem:solver/experiments/04-witness-constructor-diagnostics`) identified ineligible and repeated helper
work. 07 (`mem:solver/experiments/07-compact-keys-and-constructor`) measured construction at 6.516 s
versus 143.962 s in the earlier recovery run. Those phase observations are not an
isolated whole-solver speedup. No observed deadline expiry explains that improvement.

The constructor-only source passed 199 solver-core library/integration/example tests,
two ignored, and strict all-target Clippy against the committed serializer in a
separate checkout. Tests cover exact ordered subsets, eligibility across scaled and
multiple-source problems, cache reuse/reset, witness validation and cancellation.

09 (`mem:solver/experiments/09-serializer-and-find-all-results`) verified 42 runs without the helper
deadline. Hard optimal p1/32 stayed near 32 s and exact completed/partial result
identities matched earlier results. All three constructor source files still match
that validated source snapshot at promotion. `npm run format` passed before commit.

The same commit includes the accumulated Markdown evidence and handoff updates
through experiment 09 so its references remain available without the chat.
Profiling tools and new hard-work measurements follow separately.
