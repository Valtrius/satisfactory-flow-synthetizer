# 22 - Analytic witness-port results

Date: 2026-08-29. State: completed, reverified and permanent.
Protocol: analytic witness ports (`mem:solver/experiments/22-analytic-witness-ports`).

## Integrity

All 6 frozen jobs exhausted the selected root without process failure, watchdog
kill or open activity span. Independent reverification accepts 6/6 and preserves
`summary.json` at SHA-256
`d2b97fc91877470c61f083f0d900edaf1dfbea3128ada107481ec5bb7b78cdf7`.

Cross-run comparison with experiment 21 confirms identical frozen requests, root
plan/selection identities, exhaustion statuses and full solution objects for every
job. The exact solution JSON has SHA-256
`bfdadc1154a0cdd3e375473cb83dead90bcf7c6a35d7daad056868d2e12f3976`
in both runs. Structural decisions, retained states, duplicates and SCC solves are
unchanged at 73,795, 21,779, 3,207 and 8,320 respectively.

## Timings

| Mode | Experiment 21 median | Analytic ports median | Reduction | Speedup |
| ---- | -------------------- | --------------------- | --------- | ------- |
| best | 50.034 s             | 21.002 s              | 58.03%    | 2.38x   |
| all  | 51.665 s             | 20.070 s              | 61.15%    | 2.57x   |

Best ordinary samples range from 20.684 to 21.319 seconds; all ranges from 19.487
to 20.653 seconds. Process CPU falls with wall time and process peak working set
stays approximately 49 MB. This is an isolated selected-root result, not yet a
whole-solve timing claim.

## Calculation effect

The diagnostic work changes as designed:

| Metric                | Exhaustive ports | Analytic ports |
| --------------------- | ---------------- | -------------- |
| Witness leaves        | 80,621,568       | 144            |
| Witness branches      | 338,946,509      | 605            |
| Witness search, all   | 33.654 s         | 0.000239 s     |
| Witness search, best  | 34.403 s         | 0.000255 s     |
| Canonicalization, all | 40.521 s         | 7.036 s        |

The exact leaf reduction is 559,872x. Best and all still perform the same work,
confirming again that retaining one preferred witness does not avoid proving its
minimum canonical key.

## Decision and next measurement

Keep `f5df873` permanently. It preserves the public byte protocol, passes the
independent exhaustive reference matrix and removes the intended factorial work
with a large completed-root speedup.

Before changing another calculation, measure whole 115 and 238 in best/all mode
and hard 36 in best plus capped all mode. These cases distinguish many-witness,
long-root and difficult completion effects. Continue to cap hard 10 and use it for
diagnosis: its known pre-witness basis/labeling cost cannot benefit from this change.
Scheduler experiments remain paused.
