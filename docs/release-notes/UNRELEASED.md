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

- Graph placement no longer leaks between history entries. Connections stay locked to the validated solution.
- Keyboard navigation covers layout selection, menus and segmented controls.
- Closing hides the window immediately, completes solver cleanup, and reports save failures with retry and discard choices. Running work has five-second history checkpoints.
- Result processing reuses completed work, and growing history entries save new layouts without rewriting existing graphs. Corrections and failed append recovery retain a full-write path.

The 0.1.0/0.2.0/1.0.0 release-speed comparison is still pending. The [application measurements](../../benchmarks/optimization/results-application-20260911.md) distinguish projection/checkpoint gains from frontend costs and uncertain solver timings; they are not release-to-release speedups.
