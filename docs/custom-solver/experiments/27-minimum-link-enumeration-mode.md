# 27. Minimum-link enumeration mode

Date: 2026-08-30

## Request

Add a third search choice between one optimum and every layout at minimum N:
return every layout at minimum N and minimum L. Expose the three choices as a
segmented control and make the mode available to the benchmark runner.

## Implementation

- Shared API: `AllAtMinimumNodesAndMinimumLinks` has a distinct enumeration status
  with proven N, proven L, and completion state.
- Custom: collect every witness in the first satisfiable equal-L group and return
  only after that group is exhausted. The optional constructor remains an incumbent;
  it cannot complete enumeration.
- Reference: exhaust and collect the first satisfiable L group, then return.
- Z3: prove minimum N and L through the existing optimization loop, then enumerate
  again under the proven L cap. The prior UNSAT cap proves that `L <= minimum L`
  is the exact minimum-L set.
- Tauri/frontend: requests, form snapshots, history filters, exports, and persisted
  rows use `solveMode` as the only live field. Version 1 history and UI preferences
  migrate once to version 2 and remove the former boolean. The UI offers `One`,
  `All min L`, and `All L`.
- Benchmarks: `minimum_links` selects the new mode. Existing defaults stay unchanged.

## Verification

- `cargo test --workspace`: 302 passed, three ignored.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `npm run check`: passed with zero diagnostics; all 91 frontend tests passed.
- `npm run format:check`: passed.
- Shared differential test: Custom, Z3, and Reference returned identical layout-key
  sets on three small exact problems. Every solution used the proven minimum L.
- The `profile_tiny` smoke run reported mode `minimum_links`, completed at N=2/L=1,
  and returned one validated layout. A one-job `-PlanOnly` benchmark schedule accepted
  the new mode.

## Decision

Permanent product and benchmark feature. This changes enumeration scope, not solver
correctness or scheduler policy. Performance claims require separate benchmark data.
