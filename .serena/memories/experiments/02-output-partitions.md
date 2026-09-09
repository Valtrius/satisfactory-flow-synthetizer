# First-output producer partitions

Decision: keep complete first-output-source roots and explicit root/profile ledger.
Each requested output has exactly one incoming belt. Enumerating every possible producer is disjoint and exhaustive; impossible choices are proved UNSAT. Single worker stays unsplit. Cancellation leaves unfinished parents incomplete.

Evidence: target/astra-partitions-screen-20260908; benchmarks/partitions.json, partition-binaries.json. All 48 records verified. Two repeats at 32 workers.
Custom/Astra: 36 All min N 61.712/52.882; 115 15.495/22.532; 238 5.148/27.394; 258 All min N/L incomplete180/70.519; 10 One min N/L both incomplete180.
Separate old/new Astra pairs: 36 66.783/51.460; 115 58.482/20.445; 238 28.002/31.492 (regression, runs27.804–35.179).
258 had no completed independent reference yet; its two exact witnesses agreed only across Astra runs. Initial 24 tie failure was not retested/erased.
Retained snapshot target/astra-before-direct-flow-20260908; runner 8cb6248770aeaed5f28e54a192ec4a3f8d6a1c5b1e542f0dd5add41d026699ef.
