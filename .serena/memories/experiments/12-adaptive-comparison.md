# Rejected global adaptive replacements

Both immediate adaptive and 250 ms witness-grace replacements were rejected against 6852be9. Unlike the promoted hybrid, these remove static splitting everywhere. Retain only the specific conclusions from their own matched baselines.

Completed 2026-09-10: 56/56 timing runs, 28 pairs, ten suites, 51 m 27 s; no caps/failures. All 1,485 hashes and ten verifiers rechecked; full canonical keys and saved objects agreed across five All min N/L scopes. Fourteen diagnostic qualification probes included two deliberate incomplete cancellations.

Case 258 immediate adaptive: -24.74% at 8 workers (59.87 -> 45.09 s), -50.89% at 16 (60.91 -> 29.88 s), +38.07% at 32 (14.16 -> 19.55 s). Grace: -18.78%, -50.92%, +36.02% against separate paired baselines. Case 97 at 32: immediate +44.56% (215.43 -> 311.45 s), grace +45.87% (201.98 -> 293.86 s). All four adverse 97 pairs added about 90–100 s. Easy gains <=3 s did not justify this replacement.

Evidence IDs adaptive-comparison-campaign/analysis/preparation and benchmarks/optimization/results-adaptive-20260910.md. Grace was not paired directly against immediate adaptive. The final hybrid in `mem:experiments/13-hybrid-final` preserves static splitting and has now been promoted; no follow-up queue remains pending.
