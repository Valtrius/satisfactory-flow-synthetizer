# Canonaut 1.0.0 portability patch

This is the registry Canonaut 1.0.0 library, Apache-2.0. Upstream source and attribution are retained in LICENSE, NOTICE, README.md and .cargo_vcs_info.json.

Original registry archive SHA-256: `69947fec24aabe6d0faa4311ebffbe7208c3380b66b4d03f9821b2a47dfa83d6`.

The application still uses the same canonical labeling algorithm and layout-v1 serialization. Native golden identities were captured with the unpatched registry dependency before these changes.

Changes:

- Keep KISS RNG arithmetic and seeds explicitly 64-bit on every target. Conversion to a host index occurs only after reducing a random value modulo 17. No constants or random state are truncated.
- Derive word masks, indexing, shifts, one/two-word specialization boundaries, permutation scratch sizes and search bounds from usize::BITS. The 64-bit operations are unchanged; wasm32 uses 32-bit bitsets throughout.
- Disable rand's default entropy features. Its existing seeded SmallRng graph generator needs only small_rng. Canonical labeling uses the separate deterministic KISS generator.
- Exclude the optional wall-clock RNG initializer on wasm32; canonical labeling never calls it.

Qualification lives in crates/solver-portable-tests. It compares pre-patch native golden colored graphs and physical layout keys, storage/port/equal-rate terminal/discard permutations, and word-boundary primitives in native Rust and a real browser Wasm worker. Compilation alone is not the identity acceptance gate.

The curated manifest omits the upstream logo example, C-nauty comparison binary, benchmark target and build helper because their supporting sources are not included. The unused Criterion and C build dependencies are removed. The retained upstream README describes those upstream tools; this local copy supports its library, `memcheck`, `profile`, and native library tests.

Run the retained upstream library tests with `npm run test:vendor`. This uses the vendor's own locked manifest and writes build outputs under the root `target/canonaut-tests`. The crate remains outside the application workspace; the web-backend workflow invokes its native library tests explicitly. Native test-only rand features supply the upstream permutation tests; the production library still uses only seeded `small_rng`, with no C-nauty or JavaScript entropy.

The local test target retains the committed smoke corpus and primitive/graph tests. It omits the 56 optional full-corpus cases whose inputs and generator scripts are absent from this copy. Smoke helpers now fail on missing files instead of returning a successful empty test.
