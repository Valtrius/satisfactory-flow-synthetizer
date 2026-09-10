# Static Boolean partition promotion

Promoted in 6852be9 on descending order from cd703e9. Boolean All min N/L with >=2 outputs and fewer live roots than branch workers replaces parents by every second-output producer, interleaved in stable rounds. Every child must exhaust. This exact static path remains part of the current hybrid; see `mem:solver/contracts`.

Evidence IDs partition-promotion-campaign/analysis; repository benchmarks/optimization/results-promotion-20260910.md. Five suites, 56/56 timing runs complete, 28 pairs, workers 8/16/32, about 1 h 15 m. All five verifiers, nine diagnostic audits and 745 hashes passed; full enumeration objects matched across six scopes.

Six matched pairs at 32 workers: 258 All min N/L 55.598 -> 12.955 s (-76.50%, two layouts); 97 431.490 -> 212.744 s (-50.71%, 813). Every hard pair won, including the weakest 97 pair 371.520 -> 231.256 s (-37.75%). Largest easy single-pair penalty was 2.104 s on 36, accepted. First witness on 258 moved 2.320 -> 4.335 s; exact terminal completion takes priority.

Subsequent global adaptive comparison rejected replacement (record 12); hybrid promotion retained this path (record 13). Case 10 full enumeration was outside this confirmation. Do not pool session medians.
