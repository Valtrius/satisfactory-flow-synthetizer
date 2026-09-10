# Static Boolean partition promotion

Approved and integrated in commit 6852be9 after the 10 September confirmation. The production source matches the frozen pairs-boolean candidate from cd703e9; descending parent order remains. Boolean All min N/L only, two or more outputs, fewer live first-output roots than branch workers. Each second-output producer gives a disjoint child; all children must exhaust before their profile counts. Keep independent proof ownership, exact witness validation, scope/cancellation contracts and second_source diagnostics. Portfolio allocation stays 50/50.

Evidence: target/optimization-promotion-20260910/frozen and /analysis; repository record benchmarks/optimization/results-promotion-20260910.md. Five suites, 56 timing runs/28 pairs, workers 8/16/32, 1 h 15 m. All completed; five verifiers and nine separate diagnostic audits passed. 745 frozen hashes verified; full saved enumeration objects match across six exact scopes.

Six pairs at 32 workers, All min N/L: 258 55.598 -> 12.955 s (-76.50% median paired change, two layouts); 97 431.490 -> 212.744 s (-50.71%, 813 layouts). Every hard pair wins. Retain weakest 97 pair: 371.520 -> 231.256 s, -37.75%. Largest easy guard median penalty 1.990 s, single-pair 2.104 s (36), acceptable under the user's approximately 10-second ceiling. First witness 258 2.320 -> 4.335 s; terminal completion takes priority.

Next compare adaptive refinement and a 250 ms witness grace against this combined baseline at 8/16/32 workers, within three hours including cleanup and verification. Adaptive and sparse startup delay remain experimental. Case 10 All min N/L was not retested here; previous 600 s caps remain unresolved. See `mem:solver/benchmarking` for current limits. Do not pool cross-session medians.
