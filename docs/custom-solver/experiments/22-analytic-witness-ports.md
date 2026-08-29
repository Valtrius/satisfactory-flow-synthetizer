# 22 - Analytic full-witness port minimization

Date: 2026-08-29. State: implemented candidate; validation passed; replay pending.
Related: [experiment 21 results](21-results.md).

## Hypothesis

For fixed terminal, discard and node labels, the lexicographically minimum
symmetric physical-port labels can be derived directly. The witness search need
not enumerate an independent factorial permutation for every splitter and merger.

## Exact reduction

Every physical producer port is used at most once, so global link order has fixed
producer blocks. Within one splitter block, producer bytes and successive port
numbers are invariant. Ordering its outgoing links by encoded consumer base and
then exact flow minimizes the block. Encoded protocol tags are output 0, node 1
and discard 2.

After splitter blocks are fixed, assigning each merger's input ranks in global
producer order minimizes the first byte position at which any merger permutation
can differ. Parallel splitter-to-merger links have equal consumer bases; their
exact flows order them, after which merger ranks follow the same occurrence order.
Unused ports in structurally accepted malformed graphs do not enter witness bytes
and receive the remaining ranks deterministically.

Production still exhausts every equal-rate terminal, anonymous discard and
same-type node permutation. On the profiled 238 root this should reduce witness
leaves from 80,621,568 to 144 without changing a byte of the winning key.

## Implementation and corrected failure

`WitnessEncoder::complete_port_ranks` fills splitter and merger ranks at each
remaining leaf. `IncidenceGraph::witness_labeling_groups` excludes only symmetric
port groups; all internal state, SCC and marked-object canonicalization remains on
the existing `canonaut` path.

The first candidate sorted consumer bases by Rust enum order. The exhaustive outer
matrix rejected `3 -> 2` with a splitter, merger and discard because enum order puts
discard before node while the stable byte protocol does the reverse. No benchmark
used that candidate. Sorting by explicit protocol tags fixes the mismatch.

## Validation before replay

- The new protocol/flow regression covers a discard beside a node consumer and
  unequal parallel splitter-to-merger flows.
- The independent exhaustive reference agrees on the existing tiny label/port
  variants and on all 60 outer-solver cases through two physical nodes.
- Solver-core passes 183 tests with two manual benchmarks ignored, the outer
  differential, four parallelism integrations and 26 example tests.
- Strict all-target Clippy passes with `bench-internals`.
- Two timer-based cancellation tests now use the previously supplied hard-10
  workload because the old seven-node witness completes before their timers.

## Frozen replay

Re-run the six-job `benchmarks/custom/root238-screening.json` screen from a clean
committed candidate. Compare exact requests and witness objects with experiment 21,
ordinary best/all timings separately, and diagnostic witness branch/leaf counts.
The replay must exhaust the selected root with no kill, failure or open activity.

No speedup is claimed until that run finishes and frozen verification passes.
