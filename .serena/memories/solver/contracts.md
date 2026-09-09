# Solver contracts and code map

- crates/solver-core: exact cvc5 search; problem.rs normalization, lower_bound.rs sound arithmetic certificates, profile.rs exact port accounting, encoding.rs QF_LRA, cardinality.rs Boolean counters, process.rs owned backend sessions, portfolio.rs independent proof races, diagnostics.rs root traces.
- solver-api: rational problems, three scopes, proof/result/progress contracts.
- solver-validation: independent full topology equations, capacity/positive flow/reachability/global and SCC uniqueness; exact layout-v1 identity and deterministic benchmark witness.
- solver-reference: independent exhaustive small-case oracle, retained for full result comparisons.
- synthetizer-app: production runner and graph presentation. src-tauri: jobs, IPC, history. frontend: queue, graphs and UI.

History schema 4 stores no solver type. JSON schemas through version 2 and relational schema 3 migrate transactionally, preserving entries, selection and graph edits. Strip imported engine prefixes from layout cache keys. Unknown or corrupt schemas fail without deleting user data. Equivalent scope tags are accepted when reading; new writes use one_min_nl, all_min_nl and all_min_n. UI history shows proved minimum L with a route icon.

Exact search normalizes every rate and capacity with one positive scale, preserving terminal mappings. Enumerate N then exact L then all feasible profiles. Unary belt multiplicities, sorted same-type operator flow, source/destination port counts, direct one-input flow equalities and decreasing selected reachability paths preserve all physical graphs, including cycles. Reject nonunique steady states; unexpected validation/extraction failures stop the worker.

With multiple workers, first-output producer choices partition each profile completely. Exhausted roots alone discharge the ledger. First optimum stops/joins siblings without claiming same-group exhaustion. Enumeration exhausts the entire requested scope.

Portfolio races independent sparse arithmetic and Boolean count formulations, each with its own ledger. Total workers are split; sparse gets the odd extra, one worker uses Boolean. Join both searches and processes before terminal events. Return one proof owner, never sum ledgers. User cancellation remains incomplete even if internal success races it.

Keep full canonical graph-set equality, independent exact witnesses, objective/proof/completion and worker invariance tests. Counts alone are insufficient. Benchmark canonicalization runs after solve timing; never select an optimal tie just to compare bytes.

Windows x64 installer and portable ZIP bundle cvc5 1.3.4 beside the app, with notices in licenses/cvc5. src-tauri/cvc5-package.json pins the official archive and SHA256. npm run dev/build prepares it through scripts/prepare-cvc5.ps1; generated binaries/resources are ignored by Git. Lookup: SOLVER_CVC5 override, beside executable, PATH, Windows user installation. No downloads at solve time.

npm run package:release assembles installer/ZIP/checksums in target/release/bundle/distribution. npm run verify:release extracts both with 7-Zip, compares application/backend/notices hashes, and checks a production-API exact solve with external lookup disabled; a missing-backend control must fail. It does not install the app or open user history.
SOLVER_DIAGNOSTICS=1 emits solver.root records. Identity=(branch,N,L,root), branch 0 sparse/1 Boolean; solver.portfolio_proof_owner identifies the final ledger. Keep incumbents separate from enumeration counts.
