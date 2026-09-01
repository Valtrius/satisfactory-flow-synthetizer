# 24 - Canonicalization purpose profile

Date: 2026-08-29. State: completed and verified.
Related: experiment 23 results (`mem:solver/experiments/23-results`).

Run: ignored `target/parallelism-ladder/canonical-purpose-20260829/`.

## Question

Which exact identity consumes the legal-decision and graph-canonicalization time:
state keys, finalist open-port keys, marked-child keys or SCC summary keys?

This is a measurement change. It does not remove or reuse any key and does not
change DFS decisions, ordering, propagation, caching or proof accounting.

## Instrumentation

The existing graph-canonicalization recorder now attributes calls and elapsed time
to state, open-port, marked-link and other key purposes. The existing SCC timer also
counts calls. Purpose totals overlap the existing top-level search buckets and must
not be added to them as independent CPU time.

Recording stays disabled outside hotspot diagnostics. The ordinary path adds no new
clock read or recorder lookup: it classifies the graph call at the existing recorder
site. The SCC call count uses its existing timer record.

## Screen

Manifest: `benchmarks/custom/canonical-purpose-screening.json`.

- 115 all and 238 all are expected completed controls.
- Hard 36 all and hard 10 optimal stop after 60 seconds.
- Every job uses production p1, 32 workers and hotspot recording.
- Four jobs have 390 seconds of aggregate search caps.

Completed outcomes must match experiment 23's full saved solutions, preferred key,
canonical layout keys, proof and structural counters. Capped jobs remain incomplete
and can show cost shares only. Corpus cyclic/acyclic labels do not enter solver
policy.

## Decision rule

Optimize the largest repeated purpose only if the change retains its exact key
protocol. Candidate methods include sharing immutable incidence construction across
several keys from one parent or retaining already generated raw decisions. A parent
state key cannot substitute for an individualized open-port or marked-child key.

If marked-child keys dominate, first test reuse within `keyed_decisions_for`. If
open-port keys dominate, build the unmarked incidence graph once for all finalists.
If state keys dominate independently, profile incidence build versus labeling and
semantic encoding before changing the state cache identity.

## Validation before launch

- 183 solver-core tests pass, two manual benchmarks ignored.
- The exhaustive outer reference integration and four parallelism integrations pass.
- Strict all-target solver-core Clippy passes with `bench-internals`.
- A release tiny all solve is optimal and validates both layouts. Its 84 state,
  120 open-port, 171 marked-link and two other calls sum to all 377 graph calls.
- The release `profile_case` example builds. Its prelaunch SHA-256 is
  `b02ebc4e392324df3a74b5a701ceb1805932ea480e792c3035aaf5bd756e5415`.
- The frozen plan validates four unique jobs and the 390-second aggregate cap.

## Results

The frozen analyzer verifies all four records. The 115 and 238 controls complete
optimally; hard 36 and hard 10 end at their declared 60-second caps. No job fails
or is killed. Completed preferred keys, full solution objects, layout-key sets,
proofs and structural counters equal experiment 23. The run consumes 5.49 process
wall minutes and 102.16 process CPU minutes. `summary.json` SHA-256 is
`1a7d6dcf819559d246f58726cdce0874412fd8489aad25e84f74f7010ce81789`.

| Case                    |    State | Open port | Marked child |      SCC | Open + marked / legal |
| ----------------------- | -------: | --------: | -----------: | -------: | --------------------: |
| 115 all, complete       | 754.17 s |  289.23 s |     381.49 s |  85.17 s |                94.46% |
| 238 all, complete       | 263.14 s |  121.38 s |     185.78 s |  43.76 s |                95.29% |
| hard 36 all, capped     | 389.46 s |  179.49 s |     398.68 s |   6.21 s |                94.57% |
| hard 10 optimal, capped | 516.96 s |  332.66 s |     308.42 s | 200.28 s |                96.22% |

Times are aggregate worker time and overlap the top-level search buckets. Marked
children are the largest legal-decision purpose on 115, 238 and hard 36. Open ports
are slightly larger on hard 10. State keys remain the largest single graph purpose
on 115, 238 and hard 10.

Marked-child call counts barely exceed traversed decisions: 0.107% on 115, 0.045%
on 238, 0.044% on hard 36 and 0.285% on hard 10. The key therefore removes few
physical candidates in these runs, although it also supplies invariant child order
and stable root-augmentation identities. That second role prevents unconditional
removal.

## Decision

1. Retain the diagnostic counters.
2. Test an isolated internal-DFS variant that keeps marked keys for root and adaptive
   frontier planning, but enumerates already symmetry-filtered raw decisions inside
   each dispatched root. The propagated child state key/cache still proves repeated
   child equivalence.
3. Require exhaustive reference, parallel-root, cancellation and full result-object
   comparisons before considering production use.
4. If the variant fails or gives little wall-time benefit, batch finalist open-port
   keys over one immutable incidence graph. This preserves every key byte but can
   save only the repeated incidence-build portion.
