# Project memory index

Current baseline: solver-core, an exact cvc5 sparse/Boolean portfolio with descending first-output roots and hybrid static/adaptive All min N/L refinement. solver-reference remains the independent exhaustive test oracle. Scheduling experiments are paused after the user accepted the hybrid's measured tradeoff.

- Product and exact result terminology: `mem:product`.
- Code, proof, cancellation and history contracts: `mem:solver/contracts`.
- Benchmark evidence and run limits: `mem:solver/benchmarking`.
- Experiment decisions, one record per experiment: `mem:experiments/index`.
- Accepted hybrid and adverse samples: `mem:experiments/13-hybrid-final`.

Release workflow: docs/release.md. Conventional commits; npm run format before committing. Preserve unrelated work. Launch an authorized benchmark with durable status and notification, then end the turn.

Browser architecture and commands: docs/web-backends.md. Historical phase evidence: docs/web-backend-history.md. Publication/offline behavior: docs/web-release.md. Production uses a Rust proof coordinator, independent Rust/cvc5 compute workers and IndexedDB result collections. Automatic follows the browser's logical processor report; one complete independent branch owns proof. Durable bounded checkpoints precede acknowledgements, and the page owns cancellation and sealing. Collection summaries/sort indexes are reused across pages; graphs use stable source indices. Sharing is self-contained, selected-solution-only and proofless after independent verification. Initial offline installation acquires a coherent document before mounting; later upgrades wait for old tabs to close without clients.claim or skipWaiting. Reload never resumes search or clears history. The manual Pages workflow must actually run before claiming a public deployment.
