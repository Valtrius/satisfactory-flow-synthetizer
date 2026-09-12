# Project memory index

Current baseline: solver-core, an exact cvc5 sparse/Boolean portfolio with descending first-output roots and hybrid static/adaptive All min N/L refinement. solver-reference remains the independent exhaustive test oracle. Scheduling experiments are paused after the user accepted the hybrid's measured tradeoff.

- Product and exact result terminology: `mem:product`.
- Code, proof, cancellation and history contracts: `mem:solver/contracts`.
- Benchmark evidence and run limits: `mem:solver/benchmarking`.
- Experiment decisions, one record per experiment: `mem:experiments/index`.
- Accepted hybrid and adverse samples: `mem:experiments/13-hybrid-final`.

Release workflow: docs/release.md. Conventional commits; npm run format before committing. Preserve unrelated work. Launch an authorized benchmark with durable status and notification, then end the turn.

Browser work: docs/web-backends.md records the agreed scope and phase evidence. Steps 03, 04 and 05 are committed as 0d7cbe2, 00419e5 and 3b7f24b. Step 05 separates a Rust proof coordinator from independent Rust/cvc5 compute workers and stores enumeration graphs in IndexedDB collections. Browser worker selection defaults to all logical processors reported by navigator.hardwareConcurrency; lower choices and the exact client count remain available. The coordinator returns one independent proof owner, never combined branch ledgers. Acknowledgements follow bounded durable graph writes, with retained failed-write overlays and hard cancellation of every worker. Job snapshots contain a collection reference and preferred graph; the table loads summary pages and selection uses stable source indices. All three scopes, graph edits, history reload and off-page selected sharing remain supported. Sharing uses only self-contained selected-solution links and bounded proofless verification, with no backend storage or input-only format. Search assets stay lazy and separate from the small verifier; solver-portable-tests is test-only. Step 06 adds integrity-checked offline caching, waiting upgrades, corresponding-source/relink packaging and a manual GitHub Pages workflow. It never resumes searches automatically or clears IndexedDB during cache upgrades. No public deployment is claimed until that workflow is actually run.
