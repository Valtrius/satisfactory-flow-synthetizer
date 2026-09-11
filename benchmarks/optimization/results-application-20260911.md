# Application performance promotion

## Decision

Promote public identity reuse (`0a4f136`), terminal presentation reuse (`1b2721b`),
and integrated history appends (`ab8b913`). Keep the linear duplicate scan and the
existing per-root encoding construction. Solver scheduling and proof ownership
are unchanged.

The original five-way campaign at `1b10261` completed 632 runs. It supported
identity and presentation reuse and the backend history prototype. Indexed
duplicate detection was usually slower on its 2–49-layout fixtures. Encoding
caching reduced parent-process memory but did not establish a wall-time gain.
The experiment branch and its frozen artifacts remain unchanged; none of its
disabled-by-default feature flags or campaign code was merged into production.

## Combined confirmation

Reference: `61b8be60ebcb201f2e3e535f55622fbff2c2c01d`.
Candidate: `ab8b91334ce1d2a1b51432c8b588c57cd48c866a`.

Both revisions were rebuilt in isolated source copies using the same release
profile, toolchain and pinned cvc5. The headless adapter only exposes production
projection and history operations without a window. Its exact changes, source
archives, compiled source copies and binaries are frozen with the results.

All 112 runs and 56 adjacent, order-balanced pairs passed. There were no timeouts
or failed comparisons. Reverification checked 530 frozen files and 562 evidence
files. Enumeration compares full canonical sets and saved witnesses; history
compares every checkpoint with an independent full replacement, not just the
last write.

Four pairs were measured per setting on the Windows 7950X3D. Sixteen-worker
solves use the 96 MiB CCD; the 32-worker case uses the full processor. Replays
are synchronous production operations, not parallel solver measurements.
Each projection run repeats 100 times. Frontend runs use five warmups and ten
measured repetitions; SQLite runs repeat three times with separate temporary
databases. Setup, verification and temporary cleanup are excluded from operation
timing. Native IPC transport and browser rendering are not measured.

Percent changes below are medians of paired ratios. Their bootstrap intervals
are exploratory with four pairs. They need not equal ratios of the separately
reported medians.

### Result presentation

Times are milliseconds per full projection and payload sequence.

| Case            | Live coverage | Reference | Combined | Paired change |
| --------------- | ------------: | --------: | -------: | ------------: |
| 65, 13 layouts  |          100% |     0.885 |    0.604 |        −31.8% |
| 115, 49 layouts |            0% |     1.825 |    1.828 |         +0.4% |
| 115, 49 layouts |           50% |     2.578 |    2.118 |        −17.8% |
| 115, 49 layouts |          100% |     3.371 |    2.422 |        −28.4% |

Case 115's terminal phase at full live coverage fell from 1.690 to 0.722 ms
(paired −57.4%). No-live-delivery performance was effectively unchanged.
These are application-processing gains, not equivalent solver speedups, and the
individual experiments' percentages must not be added together.

### Actual history wire operations

The frontend measurement imports each revision's real `snapshotHistory` and
`diffHistoryOps`. The SQLite replay consumes the resulting JSON operations.
The fixture is the same 49-layout case 115 result in every run.

Times cover the entire checkpoint sequence, including initial and final writes,
in milliseconds. A batch of 100 contains all layouts and therefore cannot benefit
from suffix appends.

| Layouts per batch | Frontend reference | Frontend combined | SQLite reference | SQLite combined | SQLite paired change |
| ----------------: | -----------------: | ----------------: | ---------------: | --------------: | -------------------: |
|                 1 |              31.84 |             62.63 |           346.13 |          117.04 |               −66.2% |
|                10 |               4.81 |              8.52 |            52.57 |           31.96 |               −39.2% |
|               100 |               2.35 |              3.74 |            25.35 |           25.01 |     −1.6%, uncertain |

Detached snapshots prevent in-place UI changes from corrupting the acknowledged
prefix, but cost additional frontend work: paired +96.1%, +77.0% and +58.2%
respectively. On this fixture that is about 0.5–0.6 ms extra per checkpoint on
average. Keep this safety cost visible. Repeated-write savings exceed it, while
single-batch saves have no established speed benefit. The real application's
400 ms debounce and five-second checkpoint limit often coalesce many arrivals;
batch-one results are not a universal saving claim.

Total wire payload fell from 6,087,406 to 482,355 bytes at batch one and from
938,800 to 463,115 bytes at batch ten. Batch 100 remained 461,363 bytes in both
variants. Full replacements remain mandatory for request/form changes, result
corrections, reordered or removed results, and final preferred-proof changes.
Prefix mismatch recovery replaces only affected entries in an atomic retry.

### Solver completion

No solver setting established a wall-time change. Retain the adverse samples.

| Case and scope                  | Workers | Reference seconds | Combined seconds | Paired change | Exploratory 95% interval |
| ------------------------------- | ------: | ----------------: | ---------------: | ------------: | ------------------------ |
| 24, All min N                   |      16 |             1.684 |            1.684 |        −0.03% | −1.50% to +0.43%         |
| 115, All min N                  |      16 |             7.399 |            7.599 |        +3.29% | −1.36% to +9.40%         |
| 258, All min N/L                |      32 |            13.334 |           13.500 |        +0.87% | −2.13% to +2.31%         |
| 115, identical-baseline control |      16 |             7.501 |            7.545 |        −2.01% | −4.27% to +3.23%         |

## Evidence and validation

`benchmarks/evidence.json` resolves `release-audit-opportunities` and
`application-promotion` to their frozen directories. The latter contains
`campaign.json`, `source-*.zip`, `instrumentation-*.diff`, copied source trees,
build logs, `frozen-hashes.json`, and `results/{records,summary,evidence-hashes}.json`.
The compact committed `application-promotion-summary.json` is copied from that
verified summary. Use the matching frozen scripts to inspect or reproduce the
campaign; never overwrite completed results.

The promoted candidate passed 154 frontend tests, 152 Rust tests, 37 Python tests,
Svelte checking, formatting, Clippy, the Windows release build, and both package
verification paths. Coverage includes private/public identity boundaries,
partial presentation delivery, cancellation, prefix recovery, transaction
rollback, immutable persisted snapshots and shared frontend/SQLite wire fixtures.
Subsequent cleanup narrows a development dependency and removes duplicated fixture
cases; it does not change the measured production functions.

This record is not the 0.1.0/0.2.0/1.0.0 comparison. Release notes and SVGs for that
comparison remain postponed. No version, tag or publication is changed here.
