# 11. Permanent compact exact keys

Date: 2026-08-28. State: promoted in `2716fac`; not pushed.
The user approved a separate compact-key commit with its documentation.

## Included and unchanged

`canonical.rs` retains state/SCC keys as exact-sized boxed bytes and encodes their
semantic systems as sparse exact rows with binary rational values. Full bytes are
compared, not hashes or approximate fingerprints. Arbitrarily large signed values
and wide sparse indices remain exact; public witness and marked-key bytes are unchanged.

Tests reconstruct the prior coefficient stream and cover sparse rows, arbitrary
exact numbers, equality/inequality bases and existing canonical invariants. No
scheduler default, helper deadline or general search pruning changes are included.

## Evidence and verification

[09 results](09-serializer-and-find-all-results.md) isolate row encoding with boxed
storage and constructor code fixed. All seven short-case medians favor compact
encoding. On 24 all/baseline, five samples give 24.763 versus 21.627 s, -12.7%;
CPU falls 15.1% and peak memory 82.2%. Timing ranges overlap. The diagnostic pair
reduces summed encoding time 7.631 to 0.405 s, not a whole-solver 18.8x speedup.

All 42 records verified, including 34 completed results and eight explicitly
incomplete hard enumerations. Completed identities and partial solution objects
match the prior screen. The old cross-bundle regression remains unexplained;
this comparison does not isolate boxed storage or establish universal speedups.

Both encoding variants passed 200 solver-core library/integration/example tests,
two ignored, strict Clippy and release builds. At promotion `canonical.rs` matches
the validated compact source snapshot. `npm run format` passed before commit.
The constructor source was promoted separately; new hard profiling follows separately.
