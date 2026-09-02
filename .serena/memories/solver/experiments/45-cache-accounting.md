# 45. Allocation-free cache memory accounting

Date: 2026-09-02. State: implemented, validated, candidate; benchmark pending.
Authorized additional independent experiments during the user's approximately 8h AFK.
Related primary run: `mem:solver/experiments/44-topology-variable-confirmation`.

## Hypothesis and implementation

search.rs rational_payload_bytes allocates numerator/denominator byte vectors only
to count their serialized lengths. Deferred snapshots and SCC deduction accounting
invoke it. Remove these temporary allocations without changing the reported size.
This targets bookkeeping CPU cost, not a memory-usage promotion gate.

signed_payload_bytes computes the minimal signed little-endian length from BigInt
magnitude bits. Zero occupies one byte. A negative power of two at a byte boundary
already fits its sign bit; other values need one additional sign bit.
The Rational helper adds numerator/denominator lengths with the existing saturation.
No state keys, pruning, arithmetic, cache contents, scheduler or proof rules change.
Whether these allocations matter to whole wall time is unmeasured.

## Correctness and validation

The existing search.rs test module compares against to_signed_bytes_le().len():
all signed 16-bit values, both signs around powers through 2^4096, and deterministic
signed byte arrays through 1024 bytes. Total 90,310 oracle comparisons pass.
Existing cache-accounting monotonicity and solver differential tests also pass.

- 221 solver-core all-target tests with bench-internals pass; two ignored.
- 309 workspace tests pass; five ignored.
- Strict all-target solver-core Clippy passes with and without bench-internals.
- Release profile_case builds using the same VS2019/znver4 environment.
- Twenty tiny before/variables/accounting executable smokes match exact results
  for optimal/minimum_links on the controlled CPU sets and unrestricted CPU.
- Initial test compilation required an explicit BigInt annotation. Initial Clippy
  flagged test literal separators/truncating cast; corrected without allowances.

Logs: target/exp45-oracle.log, exp45-tests.log, exp45-workspace-tests.log,
exp45-clippy-feature.log, exp45-clippy-default.log, exp45-build.log.
No standalone microbench speed claim.

## Benchmark isolation

The accounting executable contains complete variables and rational inequality rows.
Compare it against the unchanged frozen variables executable, not the original
before binary, to isolate this change. Ordinary hotspot recording stays off.

48 paired jobs on the two CCDs: 115 all and 238 optimal each have five pairs per
CCD; 258 minimum_links has two exploratory pairs per CCD. That last sample count
cannot justify a distribution/promotion by itself. Primary variable confirmation
jobs use separate case aliases and Comparison labels.

Frozen root: target/parallelism-ladder/topology-final-variants-20260902/accounting.
profile_case.exe SHA256:
253b8245c9106211ee1a0a44f006bffc246138b0cb59ef2e895bfb4f1b6b1573.
Only search.rs differs semantically from the frozen variables source.
Exact source hashes/build metadata accompany the binary.

Candidate is active in current source, not promoted. Committed candidate in `331b87a`; not promoted. No push.
