# Isolated source variants

These patches are benchmark inputs, not production options. They apply to the
working source recorded in [experiment 09](../../../docs/custom-solver/experiments/09-serializer-and-find-all.md).
Use a detached checkout and its own Cargo target directory. Never share build
artifacts between source variants or apply these patches during a timing run.

## Legacy semantic encoding

`legacy-semantic-encoding.patch` changes only the internal semantic row encoder
in `canonical.rs`. It restores dense decimal rational rows while retaining boxed
key storage, the current constructor, diagnostics, public witness bytes and tests.
Unused compact writers become test-only so strict Clippy still applies.

Apply to a copy of the current source with `git apply --check`, then `git apply`.
The comparison's `before` binary uses this patch; `after` uses compact rows.
Both must have identical solver sources except for these encoding hunks.
Neither binary is the historical recovery binary from experiments 06-08.

## Deferred constructor deadline

`constructor-five-second-budget.patch` restores the provisional five-second
attempt limit, separate deferred set and expiry test from experiments 07-08.
It is preserved for a future experiment and is NOT applied to either binary in 09.
No recorded expiry established a benefit. The active constructor again runs until
it completes or receives cancellation, with frequent cancellation checks retained.

The deadline patch is separate from eligibility, per-N reuse and integer subset
arithmetic. Those remain in three solver files and can be reviewed without the
serializer. Do not apply both experimental policies merely to reproduce old timings.
