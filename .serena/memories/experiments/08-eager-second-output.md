# Rejected eager second-output partitions

Rejected and restored in a9c114d. Broad eager splitting across scopes creates disjoint/exhaustive children but hurts hard completion. This differs from the retained Boolean All min N/L-only static path and subsequent hybrid.

Evidence ID output-pairs-screen: 100 verified records, 95 complete; five candidate case-10 caps at 300 s (three timing, two diagnostic). Across 15 scopes: two faster, twelve slower, one lost completion. Three-pair medians: case 10 One min N/L 25.179 s / cap; 258 All min N/L 39.902/16.876 s; 258 One min N/L 2.593/8.598 s; 36 All min N 5.317/8.114 s. Case-36 proof-owner roots rose from 484 to 1052. Completed exact comparisons passed; speed rejected the broad policy.

Full rejected patch/manifests retained at a9c114d and 2dccf50, frozen results via benchmarks/evidence.json. Parent-preserving refinement was later measured in `mem:experiments/12-adaptive-comparison` and `mem:experiments/13-hybrid-final`; it is no longer an untested hypothesis.
