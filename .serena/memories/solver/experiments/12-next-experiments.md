# Next experiments after the hard profiles

Date: 2026-08-28. Original recommendations, implemented as experiment 13 candidates (`mem:solver/experiments/13-calculation-changes`).
Verified results (`mem:solver/experiments/13-calculation-results`) now support all three. The text below
retains the original hypotheses; use current next steps (`mem:solver/experiments/13-whole-results`).
Basis: verified results (`mem:solver/experiments/12-hard-obligation-results`).

## 1. Reject exact-L mismatches before witness canonicalization

`search.rs::evaluate_complete_state` solves a candidate, canonicalizes its full
witness, validates it, then rejects a wrong internal-link count. Full witness
canonicalization only relabels and sorts; it does not change that count.

The completed 36 profile `S2=4,S3=2,M2=3,M3=0` has two witness-validation spans,
2,654,208 canonical leaves and 13.809 s of summed witness canonicalization, yet
returns no witnesses. Given the successful all-mode result and current code path,
these appear to be exact-L rejections after expensive work. The raw rejected
candidates were not saved, so confirm this with focused instrumentation/tests.

After successful topology solving, compare the raw internal-link count with the
requested exact L before exhaustive witness labeling. Retain the independent
validator and count checks for accepted canonical witnesses. Do not reinterpret
other errors as UNSAT, or use expected profile counts to invent a proof.

This is a small, isolated candidate. Measure the complete profile above in both
collection modes, compare exact results and test mismatched-L/cancellation behavior.

## 2. Make full-witness leaves cheaper without changing identity

The 36 p1 tail is still computing a witness key, and about 94% of that operation's
time is inside leaf relabeling/encoding. Changing root scheduling cannot divide
an already-running serial witness call.

Start with cached invariant witness headers/rate encodings and reusable relabeling
buffers. Compare candidate bytes before constructing an owned winning graph where
possible. Preserve the exact public byte protocol, every legal permutation and
the winning graph/key relationship. Do not replace exhaustive byte minimization
with a different Canonaut representative and assume the identity remains equal.

Use saved-witness replay plus relabeling differential tests. The first replay is
one witness from this run's four saved N=9/L=12 results. Follow with whole optimal
and all-mode cases; a faster witness replay alone is not faster solver completion.

## 3. Specialize exact inequality-basis construction

10 spends far more time constructing exact equality/inequality bases than writing
their compact bytes. `primitive_inequality_basis` currently builds two unit rows
per variable and scans the equality basis to reduce each one.

Build a pivot lookup once from the already computed RREF. Derive each variable's
lower/upper row directly from its pivot equation, or keep the unit row for a free
variable. Preserve exact arbitrary-size rational arithmetic, positive-scale
normalization, relation strictness, contradiction handling, sorting and deduplication.
Keep the existing general reduction as a differential test reference.

Test dependent/redundant equations, fixed and free variables, constant constraints,
large rationals and both strict/non-strict bounds. Compare exact resulting rows
and state/SCC keys. Measure separately from witness work before combining changes.

## Scheduling and benchmark policy

Keep p1 as the next hard-work candidate; keep all scheduler flags opt-in. The 10
profile already supplies 32 active workers after partitioning. A new pool is not
the first intervention there. The 36 tail first needs cheaper witness work.
If a DFS tail remains afterwards, test bounded work donation using observed queue
and root state, with parent/child proof accounting unchanged. Do not increase the
frontier indiscriminately: 10 p1 already peaks around 2.1 GiB and remains incomplete.

Use three fresh, randomized uninstrumented repeats per candidate/reference for
selected completed 36 profiles. Keep tiny controls and whole 24/65 optimal/all
regressions to cover SAT witnesses and the real APIs. Empty hard-profile result
sets alone are not enough correctness coverage. Retain one instrumented sample
separately to explain cost changes.

For the expensive 10 profile, keep a bounded diagnostic and seek a complete
partition-prefix workload with an exact frozen identity before claiming a speedup.
The current entry supports N/L/profile selection, not arbitrary root replay.
Caps should remain manageable; do not rerun the six immediately rejected profiles
as costly performance repeats. Existing inputs suffice; request harder ones only
after these completed controls become too short or the remaining hard work completes.
