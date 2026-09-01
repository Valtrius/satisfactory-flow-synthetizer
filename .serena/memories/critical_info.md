# critical_info

- Product behavior / UX / result semantics: `mem:product/requirements`.
- Shared terms (N, L, scopes, proven_optimal, …): `mem:product/glossary`
  — read when terminology is ambiguous or when writing user-facing / cross-solver text.
- Custom solver / parallelism / benchmarks: read `mem:solver/core` first.
- Full current decisions/tables: `mem:solver/status` — only when you need hashes/evidence detail.
- Launch or interpret a screen: `mem:solver/benchmarking`, then `mem:solver/workflow`.
- In-flight experiment: `mem:solver/active` if present; else idle → use core/status.
- Historical why/outcome: `mem:solver/experiments/index`, then read exactly one
  numbered experiment memory under that topic (never bulk-read the topic).
- Proof/identity: `mem:solver/contracts`. Flags/code map: `mem:solver/controls`.
- Non-solver tasks: do not read `solver/*` (product/* is fine when relevant).
- Conventional commits; `npm run format` before commit; never infer benchmark
  completion from a running job.
- Custom-solver knowledge lives only in Serena memories (no docs corpus).
