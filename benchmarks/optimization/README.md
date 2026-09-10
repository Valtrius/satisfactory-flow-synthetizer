# Solver optimization records

Production includes descending first-output producer order, static Boolean
second-output partitions when roots underfill the branch workers, and adaptive
refinement otherwise for All min N/L. The hybrid was promoted after explicit
acceptance of its measured performance tradeoffs. Scheduling work is paused.

| Record                                                         | Decision                                               |
| -------------------------------------------------------------- | ------------------------------------------------------ |
| [Ordering and recovered campaign](results-20260910.md)         | Descending order retained; `cd703e9`                   |
| [Static partition confirmation](results-promotion-20260910.md) | Static Boolean refinement retained; `6852be9`          |
| [Global adaptive comparison](results-adaptive-20260910.md)     | Global adaptive and 250 ms grace replacements rejected |
| [Hybrid promotion](results-hybrid-20260910.md)                 | Tested hybrid accepted; case-97 uncertainty preserved  |

The full [Serena experiment index](../../.serena/memories/experiments/index.md)
also covers the initial encoding and portfolio decisions. Read only the relevant
record. The [benchmark guide](../README.md) describes the current reusable tools
and exact comparison rules.

## Reproduce recorded work

[Evidence locations](../evidence.json) map stable IDs to local frozen campaigns,
source snapshots and analysis. Raw files keep their exact paths and schema keys;
names inside archived artifacts are evidence, not current product terminology.

Completed manifests, machine-specific binary maps, source templates and sweep
generators were removed from the working tree. They remain in Git at `2dccf50`;
extract that revision into a separate directory to reproduce those recipes.
Use each record's solver source revision: discovery `2d4e0d6`, static promotion
`cd703e9`, global adaptive and hybrid comparisons `6852be9`. The rejected eager
partition patch is also retained at `a9c114d`. Frozen campaigns are unchanged.

The hybrid promotion imports the exact tested nine solver source files from
`target/optimization-hybrid-prep-20260910/variants/hybrid-boolean/solver-source/solver-core/src`.
Its contracts are now maintained in solver-core. Only the diagnostic test harness
adds an isolated child process so ordinary Cargo tests enable their own tracing.

No further scheduling sweep or new backend experiment is queued. After the pause,
a new request can target an expensive exact SMT obligation with reproducible input
and whole-solver validation.
