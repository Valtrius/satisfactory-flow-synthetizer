# 55. Completed-state sharing that keeps deferred owners

Date: 2026-09-04. State: planned, not implemented. Related: `mem:solver/experiments/53-post51-cost-profile`, `mem:solver/experiments/52-tail-scheduling-results`, `mem:solver/experiments/29-deferred-state-canonicalization`.

## Question

Can concurrent roots publish completed canonical states for reuse while the owner still defers the first visit to a cheap invariant bucket?

## Why not this screen

Experiment53 traces show overlapping long similar roots on 238/258, so live private-cache DFS donation (experiment52) is the wrong shape. Existing `shared`/`p12` sharing forces exact keys and would give back experiment29’s deferred win. Experiment52 also dropped 115 all-mode layouts because `DonationPool` joins only `best_witness`.

User authorized both experiment53 recommendations. Combining sharing with Bareiss-forward would confound the 2h A/B. Experiment54 is the isolated calculation screen. This scheduler candidate stays out of that binary.

## Required design, when started

- Owners keep deferred canonicalization on first visit.
- Only completed authoritative keys are published.
- Lookup must not wait on an in-flight owner.
- Join must merge helper `witnesses` maps, not only `best_witness`.
- Do not enable public `work_stealing` without that sharing.
- Exact full results/proofs on 115 all-mode especially.
- Separate hotspot-off paired screen after experiment54 analysis.

## Decision

Not started. Experiment54 is analyzed and recommended for promotion separately. This scheduler candidate still has no source. Start only after 54 is committed or explicitly kept, and keep it a separate hotspot-off screen. 115 all-mode identity remains the hard gate.
