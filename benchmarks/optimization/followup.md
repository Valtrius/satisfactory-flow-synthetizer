# Scheduling and adaptive partitioning follow-up

The 9 September discovery campaign finished all 578 jobs in 4 h 20 m: 502 optimal
results, 76 deadline-capped results, and no exact-result or proof-owner verification
failures. Its 1,814 frozen file hashes matched during analysis. Complete enumeration
sets and saved solution objects agreed across 21 exact problem/scope combinations.
The evidence remains in `target/optimization-campaign-20260909/frozen`; the derived
analysis and result checksums are in the neighboring `analysis` directory.

## Decisions from discovery

- Keep the production baseline at `2d4e0d6`: first proved optimum for **One min N/L**,
  exact **All min N/L** and **All min N**, and the 50/50 sparse/Boolean portfolio.
- Keep `pairs-boolean` for confirmation. Case 258 All min N/L improved from
  40.143 s to 13.255 s, but 13 of 15 completed All min N/L requests became slower.
  The second improved request was scaled258, the same mathematical shape.
- Keep `delay-sparse250` as a secondary finalist: 29/30 pairs improved, with small
  hard-case gains and approximately 5–35 ms savings on the measured small requests.
- Reject the fixed 8/24 and 24/8 allocation changes, eager sparse partitioning,
  and delaying Boolean startup as production defaults. Preserve their evidence.
- Prioritize scheduling: with eight sparse workers, case 10's successful root
  never started within 180 s. Ascending source-owner dispatch occupied every
  worker with earlier roots. The successful source identity is evidence only;
  no candidate selects a case, rate, or known solution identity.

## New isolated candidates

`order-reverse` reverses source-owner order within each profile.
`order-outside-in` alternates the lowest and highest remaining source owners.
Both retain every original root, the same profile order, the existing worker
allocation, and the same proof and completion requirements.

`adaptive-boolean` applies only to Boolean All min N/L with multiple workers and
at least two outputs. Original first-output searches run normally. Only a validated
witness in the current N/L group permits refinement, since all lower N/L obligations
have already been exhausted. When spare slots exist, unfinished parents receive
children partitioned by the second output's unique source owner. Unstarted original
parents have priority; children are distributed across parents in rounds.

Running parents retain their backend sessions. A parent's obligation is discharged
by either its own exhaustion or exhaustion of every disjoint child. The first full
cover owns that proof. Other finished searches remain visible but cannot contribute
again. All searches use the branch's existing worker budget and are joined before
return; user cancellation remains incomplete and retains validated incumbents.

Adaptive root diagnostics add `parent_root`, `child_count`, `proof_committed`, and
`refinement_trigger_s`. The auditor checks the original parent identity, child owner
range, complete selected child covers, trigger ordering, and one disjoint selected
cover. Returned root exhaustion counts include only selected exhausted proof units
from the returned portfolio branch. Partial child groups are not a parent proof.

## Follow-up queue

Generate with `python scripts/make-optimization-campaign.py --followup --output <new-directory>`.

- Six paired repeats at 16 workers for each root order, covering case 10 One min
  N/L, case 258 All min N/L, and case 36 in both enumeration scopes.
- Additional ordering screens at 8, 12, 24 and 32 workers, plus corpus and rational,
  surplus, capacity, cyclic and multi-output coverage.
- Separate root observations near the front of the queue, so the completion cliff
  and adaptive proof covers are available even if the run allowance is reached.
- Adaptive partition screens across the applicable corpus, exact scope guards,
  and worker budgets. This new policy is not a confirmed improvement.
- Six-repeat confirmation of Boolean eager partitioning and sparse startup delay,
  plus four-repeat confirmation at smaller worker budgets.
- Case 10 and ratio97 All min N/L with ten-minute search caps, and separate long
  root diagnostics. Keep incomplete results censored and visible.

The queue runs sequentially. Its deadline sum exceeds eight hours; the existing
controller starts a suite only if its entire allowance fits the remaining eight-hour
run budget. Actual solves often finish far below their caps. A guard pause preserves
the remaining queue and reports it in the completion dialog and durable status files.
No candidate is automatically promoted. Test combinations separately after selecting
individual improvements from completed paired measurements.

## Validation and launch

`scripts/prepare-optimization.py` generates isolated source snapshots, runs strict
Clippy, exact reference and cancellation tests, and builds release executables. New
ledger tests cover partial and duplicate children and competing complete covers.
Live adaptive tests require actual child dispatch and check the branch worker cap.

`scripts/validate-optimization-followup.py` adds release-runner evidence checks:
full-result parity at 12/24 workers, adaptive completion and cancellation, and the
independent Python proof-cover auditor. These are correctness probes; their times
are excluded from benchmark analysis. Freeze their results with
`freeze-optimization-campaign.ps1 -ValidationEvidence <qualification-directory>`.

Preparation paths for this batch:

- Adaptive runner: `target/optimization-prep-20260910`.
- Ordering runners: `target/optimization-orders-prep-20260910`.
- Preserved baseline and confirmation runners: `target/optimization-prep-20260909-v2`.
- Queue, release qualification and frozen results: `target/optimization-campaign-20260910`.

The campaign's `CAMPAIGN-STATUS.txt` and `suite-outcomes.json` are authoritative for
run completion. Review exact sets, proof ownership, censored outcomes and paired
terminal times before proposing a production commit.
