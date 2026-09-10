# Final hybrid partition comparison

This is the last scheduling experiment before the requested pause. Production
remains revision `6852be9`: descending first-output order and static Boolean
second-output partitions for All min N/L. The completed
[adaptive comparison](results-adaptive-20260910.md) rejected both global adaptive
replacements: they helped case 258 at 8/16 workers but increased case 97's exact
32-worker completion by about 90–100 seconds per pair.

## Candidate and proof

`hybrid-boolean` is built in an isolated preparation directory from the same
revision as `baseline`. At each Boolean All min N/L group with at least two
outputs and more than one branch worker:

- Fewer live first-output roots than branch workers: retain the exact production
  static splitting code and interleaved child order.
- At least as many live roots as branch workers: use immediate adaptive
  refinement. Parents start unsplit; after the first validated current-group
  optimum witness, children may fill spare slots once unstarted parents are gone.

The choice uses the unsplit root count, never the case name or input rates.
Sparse allocation, descending parent order, encoding, diagnostics and the other
result scopes retain their production definitions. Static children form a
complete disjoint cover. Adaptive completion selects either the parent or all
its children, never both; cancellation joins owned workers and keeps incomplete
enumeration explicit.

The adaptive ledger tests and contracts run against the hybrid at 16 total
workers, including the eight-slot Boolean concurrency bound. Release probes
compare full exact results at 8/16/32, require actual adaptive children at 8/16,
require static children at 32, audit proof ownership, and deliberately cancel
both paths. Qualification durations are excluded from benchmark analysis.

## Final timing queue

| Scope                              | Total workers | Paired repeats | Solve cap | Jobs | Search and cleanup allowance |
| ---------------------------------- | ------------- | -------------: | --------: | ---: | ---------------------------: |
| 258 All min N/L                    | 8             |              2 |      90 s |    4 |                        420 s |
| 258 All min N/L                    | 16            |              6 |      90 s |   12 |                      1,260 s |
| 258 All min N/L                    | 32            |              2 |      60 s |    4 |                        300 s |
| 24/36/65 All min N/L; 36 All min N | 32            |         2 each |      15 s |   16 |                        480 s |
| 97 All min N/L                     | 32/8/16       |         2 each |     600 s |   12 |                      7,380 s |
| Setup and verification reserve     | Seven suites  |              — |         — |    — |                        840 s |
| Total                              | 8/16/32       |             24 |         — |   48 |      **10,680 s (2 h 58 m)** |

The controller enforces a three-hour session limit including cleanup and
verification, retains incomplete samples, runs suites sequentially and displays
one completion/failure dialog with sound. Timing diagnostics are disabled.
Every pair uses identical settings with balanced adjacent execution order.
The six repeats at 16 workers confirm the largest potential gain; the other
worker budgets screen for harmful tradeoffs. Case 97 at 8/16 fills the missing
evidence from the preceding batch.

## Reproduction and evidence

Preparation passed: 50 baseline tests, 54 hybrid tests, strict Clippy for both
variants, 42 Python harness tests and all 14 release qualification probes.
The twelve completed probes matched full canonical sets and saved witnesses;
both deliberate cancellations stayed incomplete. Case 258 exercised adaptive
children at 8/16 workers and retained all 100 static children at 32 with no
adaptive records. These observations establish qualification, not a speedup.
All 94 prepared variant artifacts and the recorded recipe hashes matched.
The final frozen identity receipt is
`target/optimization-hybrid-20260910/PRELAUNCH-VALIDATION.json`.

Commands prepare fresh directories and must not overwrite completed evidence:

```powershell
python scripts/prepare-optimization.py --output target/optimization-hybrid-prep-20260910 --revision 6852be9 --variants baseline hybrid-boolean
python scripts/make-optimization-campaign.py --hybrid --output target/optimization-hybrid-20260910/matrix
python scripts/validate-optimization-followup.py --hybrid --binaries target/optimization-hybrid-prep-20260910/variant-binaries.json --output target/optimization-hybrid-20260910/qualification
pwsh -NoProfile -File scripts/freeze-optimization-campaign.ps1 -MatrixDirectory target/optimization-hybrid-20260910/matrix -VariantBinaryMap target/optimization-hybrid-prep-20260910/variant-binaries.json -OutputDirectory target/optimization-hybrid-20260910/frozen -ValidationEvidence target/optimization-hybrid-20260910/qualification
pwsh -NoProfile -File scripts/start-optimization-campaign.ps1 -PreparedCampaign target/optimization-hybrid-20260910/frozen
```

Validate source/binary/backend identity, tests, strict Clippy, release proof
evidence and frozen hashes before launch. End the agent turn once launched.
Read `CAMPAIGN-STATUS.txt` and `suite-outcomes.json` in the frozen directory for
actual completion. Preparation does not imply a benchmark result.

Judge exact terminal completion against each candidate's own matched baseline,
retaining all adverse pairs and caps. Reasonable easy-case penalties up to about
ten seconds can be accepted for substantial hard-case savings. Full canonical
result sets and witnesses must match. One min N/L permits any exact optimum;
All min N/L and All min N still require their complete specified sets.

After this batch, analyze and recommend whether to keep the hybrid, then pause.
Do not launch another sweep or promote the candidate automatically. Further
SMT proof or encoding work requires a new request.
