# solver/active

IDLE. Negative-unit RREF shortcut is permanent and committed as 6c10b35.
`mem:solver/experiments/48-negative-unit-promotion`.

The commit includes the seven-line arithmetic change, confirmation results and
related Serena documentation. Release tests pass: 223 with bench-internals,
213 default, two ignored each. Strict release Clippy passes both configurations.
npm run format and diff checks pass. No push, no new benchmark.

Zero-destination remains held. Scheduler extras unchanged/paused. No new options,
case/N/cyclicity/affinity/memory gate. Hard36 completion benefit remains unproven;
the results retain all adverse pairs. Future benchmarks <=1h including cleanup.

Base edb4f0b and unrelated changes preserved. This documentation-only follow-up
records the resulting source commit hash and closes the integration handoff.
