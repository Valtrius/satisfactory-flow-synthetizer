# Release highlights

- Exact flow synthesis uses the cvc5 solver throughout the app. The Windows x64
  installer and portable ZIP include cvc5 and its license notices, so no separate
  solver installation is needed.
- Result choices use consistent labels: One min N/L, All min N/L and All min N.
  Requests, queued jobs and imported history retain their exact scope.
- Minimum-link enumeration combines static and adaptive partitions while
  preserving exact proof ownership, validated layouts and cancellation behavior.
  A cancelled or capped search remains incomplete even if it found all layouts
  seen in a completed run.
- History migrates saved entries without recording a solver type. The history
  card shows the proved minimum operator-belt count when available.

Performance depends on the request and worker budget. The final matched benchmark
on one Windows machine improved case 258 at 16 workers by about 41% across six
pairs, while 32-worker hard-case performance was effectively preserved. Case 97
was mixed at 8/16 workers, including one 600-second timeout. This tradeoff was
accepted for promotion; these are not claims of universal speedups or comparisons
against a released version. See the
[benchmark record](../../benchmarks/optimization/results-hybrid-20260910.md).
