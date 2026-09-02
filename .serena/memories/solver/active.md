# solver/active

Prepared, not launched (2026-09-02). Permanent RREF promotion docs c106d4c,
diagnostic source + related memories 1006e6f. No push or scheduler change.

## Next launch

`scripts/start-benchmark-screen.ps1 -JobManifest benchmarks/custom/rref-arithmetic-profile.json -VariantBinaryMap target/parallelism-ladder/rref-arithmetic-variants-20260902/variants.json -OutputDirectory target/parallelism-ladder/rref-arithmetic-20260902 -CancellationGraceSeconds 15`

14 jobs; search+cleanup allowance 2490s (41m30), leaving 18m30 within the one-hour cap.
Both variants hotspots ON; p1/16 workers. 115 all/238 optimal/258 minL reference controls;
hard36 all/hard10 optimal 120s stress. CCD96/CCD32 fixed per case, never mixed.
No paired performance claim: instrumentation adds work.

## Evidence and resume

Source, binary identities, validation and exact case/cap details:
`mem:solver/experiments/47-rref-arithmetic-profile`.
Plan check: `target/parallelism-ladder/rref-arithmetic-plan-20260902`.
222 core + 331 workspace tests, strict Clippy both configs, 41 tooling tests,
nine exact CLI smokes across all three modes, format and release build pass.

On launch record actual time/PID below. End turn; Windows dialog and
`BENCHMARK-STATUS.txt` / FINISHED or FAILED markers notify completion.
Then recheck frozen hashes, full exact outputs/proofs and diagnostic consistency.
Interpret local phases and weighted operand patterns, not instrumented speedup.
No results yet; next arithmetic optimization depends on those measurements.
