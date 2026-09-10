# Partition promotion confirmation

[HTML report with every hard-case pair](https://copyparty.jakez.eu/agent-files/FduJOz2sb.html?k=DzyI0jZgZLLxI6aK).

Promoted static Boolean second-output partitions for All min N/L after user approval.
Keep the descending first-output order already committed in `cd703e9`.
The combined policy retains the hard-case improvements, with measured easy-case
penalties well below the user's roughly 10-second tolerance. At this promotion, the integrated solver source matched the tested frozen candidate.
The subsequent hybrid promotion retains this static path.

Integration validation: all nine solver source files match the frozen candidate;
50 solver/application tests passed, including full reference enumeration and
cancellation contracts. Strict Clippy passed for all targets of the five solver
and application crates. Formatting passed before the promotion commit.

## Completed campaign and verification

The campaign in `target/optimization-promotion-20260910/frozen` finished in
1 h 15 m 03 s. All five suites finished: 56 timing jobs, 28 matched pairs,
10 request/scope/worker groups, workers 8/16/32. Every timing solve completed;
none reached its deadline. The reserved session was 2 h 54 m 20 s, with a
three-hour controller limit.

Fresh analysis in `target/optimization-promotion-20260910/analysis`:

- Verified all 745 frozen artifact hashes and reran all five frozen result
  verifiers, with no failures. Saved raw-result SHA-256 hashes separately.
- Compared completed results across eight exact problem/scope combinations,
  including six enumeration scopes with identical full layout keys and saved
  solution objects. One min N/L accepts any validated equal optimum.
- Reaudited all nine diagnostic qualification probes: eight complete results
  and one deliberately cancelled refinement. Proof-owner ledgers agree; the
  cancellation remains incomplete. These probes are separate from timing data.
- The runners preserve their exact baseline and candidate solver sources.
  Both variants use revision `cd703e9d1e1ee928584771b8efb26f7c79162ae0` and the
  same frozen cvc5 1.3.4 backend. The candidate adds static Boolean refinement.

Witness validation was performed by the benchmark runner; this review checked
its recorded validation evidence, full result equality and proof bookkeeping.
It did not rerun an independent solver on the hard cases.

## Six-pair confirmation

Times are medians of complete terminal wall time. Percentage changes are the
median of the matched pair ratios, not the ratio of the two displayed medians.

| All min N/L, 32 workers | Reverse order | Reverse + partitions | Paired change | Paired median saving | Layouts |
| ----------------------- | ------------: | -------------------: | ------------: | -------------------: | ------: |
| 258                     |      55.598 s |             12.955 s |       -76.50% |             42.593 s |       2 |
| 97                      |     431.490 s |            212.744 s |       -50.71% |            218.619 s |     813 |

Both candidates win all six pairs. The within-campaign bootstrap 95% intervals
for paired percentage changes are [-77.77%, -76.29%] and [-51.19%, -43.76%].
Six repeats on one machine do not establish a universal speedup.

Retain case 97's weakest pair: baseline 371.520 s, candidate 231.256 s,
37.75% less time and 140.264 s saved. It was the final scheduled pair (repeat 2).
Its cause is unmeasured; do not discard it or attribute it to thermal effects.

First validated witness arrives later: 258 goes from 2.320 to 4.335 s;
97 from 1.705 to 2.116 s. Exact terminal completion remains the priority.

## Scope and regression guards

Each row has two matched pairs. These are integration guards, not confirmation
of small timing differences. Positive deltas mean additional time.

| Request         | Workers | Reverse order | Reverse + partitions | Median paired delta |
| --------------- | ------: | ------------: | -------------------: | ------------------: |
| 10, One min N/L |       8 |      18.231 s |             18.004 s |            -0.227 s |
| 10, One min N/L |      16 |      21.858 s |             21.677 s |            -0.181 s |
| 97, One min N/L |      16 |       1.548 s |              1.642 s |            +0.094 s |
| 36, All min N/L |      16 |       3.304 s |              4.010 s |            +0.706 s |
| 24, All min N/L |      32 |       0.916 s |              1.798 s |            +0.882 s |
| 36, All min N/L |      32 |       2.892 s |              4.882 s |            +1.990 s |
| 65, All min N/L |      32 |       0.189 s |              0.461 s |            +0.273 s |
| 36, All min N   |      32 |       4.897 s |              4.905 s |            +0.008 s |

The largest individual guard penalty is 2.104 s, on 36 All min N/L at 32 workers.
The large percentage increases on subsecond cases should not outweigh savings
of 43 and 219 seconds on the confirmed hard cases. The refinement is inactive
outside All min N/L; small differences in those scopes are not evidence of an
algorithmic benefit or penalty from partitioning.

## Final decision

Static Boolean second-output partitions were promoted in `6852be9`, retaining
descending order from `cd703e9`. This static path is preserved by the subsequently
[promoted hybrid](results-hybrid-20260910.md). The
[global adaptive comparison](results-adaptive-20260910.md) was completed and
rejected both replacements. Those completed experiments supersede the proposed
follow-up queues from this confirmation; no new queue is pending.
