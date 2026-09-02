# solver/active

Prepared, not launched: experiment 48 independent zero-destination and negative-unit
RREF shortcuts. User approved tests; no candidate promoted. Main arithmetic unchanged.
Shared exact-oracle and heartbeat-test repair validated. Both isolated release suites
and strict Clippy configurations pass; 15 exact CLI smokes, 41 tooling tests and
32-job PlanOnly pass. Source/binaries frozen.

`mem:solver/experiments/48-rref-elimination-shortcuts` has identities and validation.
Launch:
`scripts/start-benchmark-screen.ps1 -JobManifest benchmarks/custom/rref-elimination-screening.json -VariantBinaryMap target/parallelism-ladder/rref-elimination-variants-20260902/variants.json -OutputDirectory target/parallelism-ladder/rref-elimination-20260902 -CancellationGraceSeconds 15`

32 jobs, 16 adjacent matched pairs, all complete-work reference cohorts.
115 all/238 optimal/258 minimum_links on fixed CCDs, hard36 optimal unrestricted.
Two AB/BA pairs per case/candidate; hotspots OFF. No combined variant.
54m40 search+cleanup, 5m20 reserved startup/verification, <=1h total.
Commit tests/patches/manifest with related memories, then launch and record PID/time.
End turn after launch; Windows completion/failure dialog plus durable status files.
No scheduler change or push. Prior experiment 47 result docs committed da2c89f.
