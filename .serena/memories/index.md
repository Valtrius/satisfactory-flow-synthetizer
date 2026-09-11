# Project memory index

Current baseline: solver-core, an exact cvc5 sparse/Boolean portfolio with descending first-output roots and hybrid static/adaptive All min N/L refinement. solver-reference remains the independent exhaustive test oracle. Scheduling experiments are paused after the user accepted the hybrid's measured tradeoff.

- Product and exact result terminology: `mem:product`.
- Code, proof, cancellation and history contracts: `mem:solver/contracts`.
- Benchmark evidence and run limits: `mem:solver/benchmarking`.
- Experiment decisions, one record per experiment: `mem:experiments/index`.
- Accepted hybrid and adverse samples: `mem:experiments/13-hybrid-final`.

Release workflow: docs/release.md. Conventional commits; npm run format before committing. Preserve unrelated work. Launch an authorized benchmark with durable status and notification, then end the turn.

Browser work: docs/web-backends.md records the agreed scope, verifier-only Wasm and incremental cvc5 modules, host adapters, shared job projection, IndexedDB history and the unresolved Canonaut wasm32 port. Browser solving remains disabled. Browser release requires exact single-solution and all minimum-N,L solving; a viewer alone is not the release target. No input-only sharing. Browser history rejects stale-tab writes and does not resume searches.
