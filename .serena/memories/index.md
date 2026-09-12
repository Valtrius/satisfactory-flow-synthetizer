# Project memory index

Current baseline: solver-core, an exact cvc5 sparse/Boolean portfolio with descending first-output roots and hybrid static/adaptive All min N/L refinement. solver-reference remains the independent exhaustive test oracle. Scheduling experiments are paused after the user accepted the hybrid's measured tradeoff.

- Product and exact result terminology: `mem:product`.
- Code, proof, cancellation and history contracts: `mem:solver/contracts`.
- Benchmark evidence and run limits: `mem:solver/benchmarking`.
- Experiment decisions, one record per experiment: `mem:experiments/index`.
- Accepted hybrid and adverse samples: `mem:experiments/13-hybrid-final`.

Release workflow: docs/release.md. Conventional commits; npm run format before committing. Preserve unrelated work. Launch an authorized benchmark with durable status and notification, then end the turn.

Browser work: docs/web-backends.md records the agreed scope, backend modules, host adapters, shared projection, IndexedDB history and selected-solution sharing. Shares omit coordinates, solve mode and proofs, and run bounded exact verification in a terminable worker. Sharing uses self-contained long links only, with no backend storage or link-management UI. Step 03 extracts solver-core's portable ExactPlanner/LeafDriver and qualifies the vendored Canonaut patch. The separate solver-portable-tests module runs native/reference/Wasm comparisons; it is not an application backend. Step 04 is single-worker browser job integration. Browser Find remains disabled. Exact single-solution and complete minimum-N,L solving are required for release; retain all-min-N too. No input-only sharing or search resumption.
