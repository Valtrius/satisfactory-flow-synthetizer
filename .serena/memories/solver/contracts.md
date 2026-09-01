# Correctness and interpretation contracts

These rules apply to every experiment. Performance changes must not weaken them.
See controls (`mem:solver/controls`) for the current implementation boundaries.

## Objective and result states

- Find one uses `solve_with_observer` and returns one layout after proving minimum
  N and minimum L. Record both first validated witness and terminal result time.
- Find all at minimum L uses `enumerate_minimum_links_with_observer`. It exhausts
  the first satisfiable equal-L group at minimum N, then stops before larger L.
- Find all L uses `enumerate_with_observer` and exhausts every required group at
  minimum N. A SAT group does not establish full-N enumeration.
- A validated incumbent is an upper bound and may remain `bestKnown` in an
  incomplete result. It is not a proof of optimality or a full enumeration event.
- Finite node-bound exhaustion is not global UNSAT. A timeout, helper miss,
  incomplete child or killed process does not discharge a proof obligation.
- Compare canonical layout-key sets, preferred witnesses, objective and full saved
  solutions. Equal layout counts alone cannot establish equivalence.
- Compare results within each mode. Optimal mode's witness must belong to a
  completed enumeration with the same scope and optimum, but need not equal its
  preferred witness.

## Information available to the solver

Benchmark names such as `acyclic36` and user-supplied cyclic/acyclic descriptions
are labels, never scheduler or mathematical inputs. Use only normalized rates,
capacity, current N/L/profile and facts established by the solver at that point.

The optional acyclic constructor uses a necessary denominator condition:
the exact required source denominator must divide `2^S2 * 3^S3`. It uses the GCD
of all input rates and exact scaling, not a hard-coded case or single-input assumption.
Failure skips the helper only; passing does not prove acyclic feasibility.
General exact search still covers cyclic and acyclic possibilities.

All new benchmark cases use max rate 1200 at the user's request. Capacity is still
part of the exact problem identity; do not generalize that it never affects solvability.
Decimal rates are rational strings. Uniform scaling can normalize to the same problem.

## Canonicalization and constructor

- Public full-witness labeling exhaustively minimizes the same encoded bytes.
  Removing color refinement changed branch order, not the set of permutations.
- Partial-state Canonaut labeling is a separate path. Its process-wide kill flag
  is not used, because unrelated concurrent solves could be interrupted.
- Compact internal state/SCC encodings must be injective and compared in full.
  Sparse exact rows and arbitrary-size rationals are not approximate fingerprints.
  Internal sort order can change while public witness identity stays fixed.
- Completed constructor hits/misses may be reused for one N within one solve.
  The helper does not take L, so later groups may reuse its result. Neither a
  cached miss nor an expired attempt proves exhaustive-search UNSAT.
- Cancellation does not retain a completed outcome. Changing N clears the helper
  cache. The five-second deadline is currently removed; its preserved experimental
  patch stores expiries in a separate deferred set, never as completed misses.

## Parallel proof and cancellation

- Static root leaves remain proof-ledger boundaries. Legal frontier refinement
  retains complete leaves and requires a proof for rejected children.
- Shared caches contain completed states only, scoped by exact L and canonical
  state. An in-flight owner is not proof and is not waited on as a cache result.
- Publish a validated witness before a terminal SAT entry. Borrowers and donated
  tasks must preserve witness identity and the complete group enumeration set.
- Donation joins all children before caching a parent result. Failure or
  incompleteness prevents exhaustion. Queue emptiness alone never proves completion.
- Register every required group and fold group/profile/root results consistently.
  Global enumeration success checks all required groups, not a diagnostic counter.
- Join workers and release owned caches before reporting completion. Do not leak
  caches, detach cleanup or relabel unfinished work to shorten reported time.
- A watchdog kill is a failed measurement with no returned solver proof. Keep
  diagnostic sidecars distinct from final solver JSON.

## Measurement claims

Overlapping/nested phase timers are summed elapsed time, not process CPU. Open root
intervals can include waiting and teardown. Last coordinator N/L is not a map of
all concurrent groups. More states, roots or CPU use does not establish faster search.
Report caps, actual return time and cancellation tail separately. See the
benchmark guide (`mem:solver/benchmarking`) before comparing any timings.
