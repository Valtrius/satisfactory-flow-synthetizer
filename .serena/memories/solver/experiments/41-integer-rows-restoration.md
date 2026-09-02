# 41. Integer inequality candidate restoration

Date: 2026-09-02. State: restored and committed in `5d8d924`.
Basis: `mem:solver/experiments/43-root258-affinity-results`.
User approved the benchmark policy and final variable-discovery recommendation.

Restored only crates/solver-core/src/canonical.rs to the parent of cd46fae.
No later canonical.rs changes existed to preserve. The original integer-row
experiment and frozen binaries remain available. No history rewrite or docs folder.

Reason: all controlled root comparisons cost 0.60-2.30% wall and 1.07-2.01% CPU
for root 23, despite the earlier isolated calculation improvement. Whole results
were mixed/adverse. No input-, N-, CPU- or memory-based production gate is added.
Complete propagation variables remain a candidate.

Validation in the resulting tree, also containing the independent accounting
candidate: 221 feature-enabled solver-core tests pass, two ignored; 309 workspace
tests pass, five ignored. Both strict solver-core Clippy configurations pass.
Canonical code matches the already tested rational source in the frozen variables
variant. Accounting candidate is separate: `mem:solver/experiments/45-cache-accounting`.
Related benchmark work: `mem:solver/experiments/44-topology-variable-confirmation`.
No speed claim for this restoration turn. No push.
