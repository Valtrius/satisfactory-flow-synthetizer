# 24 - Canonicalization purpose profile

Date: 2026-08-29. State: launched; awaiting analysis.
Related: [experiment 23 results](23-results.md).

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
