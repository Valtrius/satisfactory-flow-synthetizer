# Project memory index

Current baseline: solver-core, an exact cvc5 sparse/Boolean portfolio with descending first-output roots and hybrid static/adaptive All min N/L refinement. solver-reference remains the independent exhaustive test oracle. Scheduling experiments are paused after the user accepted the hybrid's measured tradeoff.

- Product and exact result terminology: `mem:product`.
- Code, proof, cancellation and history contracts: `mem:solver/contracts`.
- Benchmark evidence and run limits: `mem:solver/benchmarking`.
- Experiment decisions, one record per experiment: `mem:experiments/index`.
- Accepted hybrid and adverse samples: `mem:experiments/13-hybrid-final`.

Release workflow: docs/release.md. Conventional commits; npm run format before committing. Preserve unrelated work. Launch an authorized benchmark with durable status and notification, then end the turn.

Browser work: docs/web-backends.md records the agreed scope, backend modules, host adapters, shared projection, IndexedDB history and selected-solution sharing. Sharing uses self-contained selected-solution long links only, with no backend storage or link-management UI. Shares omit coordinates, solve mode and proofs, and use bounded exact verification in a terminable worker. Step 03 is committed as 0d7cbe2 and supplies the portable ExactPlanner/LeafDriver and qualified Canonaut port. Step 04 adds the production solver-browser bridge, page-owned browserJobs registry, hard worker cancellation and live/history projection. Find now runs all three exact scopes locally. Accepted witnesses and Rust-projected interruption packets reach the page before the next blocking check. Search assets load only for solving; the small share verifier stays separate. solver-portable-tests remains test-only. Step 05 is parallel workers and paged collections; Step 06 is offline caching and release delivery. No input-only sharing, automatic search resumption or public deployment.
