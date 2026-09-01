# Memory Maintenance

## Discovery Model

- Core principle: progressive discovery through references, building a graph of memories.
- Initially, agents are provided with the list of all memories (names only).
- Agents should read `mem:critical_info` as the top-level entry point (graph root).
  This memory should contain references to other memories covering major project domains.
  The referenced memories shall, in turn, contain references to even more specific memories, and so on.
  The depth of the graph shall depend on the project complexity.
- Custom solver history is entirely under `solver/*` memories. Prefer
  `mem:solver/core` over bulk-reading `solver/experiments/*` or `mem:solver/status`.
- Use topics/folders to group related memories in order to make the content structure explicit.
- Memory references must use a mem: prefix inside backticks, e.g. `mem:solver/core`.
  The surrounding text should clearly indicate when to read the memory/which content to expect.
- Memories themselves should not contain information about when to read them; this is the responsibility of the referring memory.

## Style

Dense agent notes, not prose docs. Prefer invariants, terse bullets.
Avoid obvious context, rationale, and examples unless they prevent likely mistakes.
Experiment memories may hold full evidence tables; routing memories stay short.

## Add/update threshold

For Custom solver work, follow `mem:solver/workflow`: every meaningful experiment
change updates the numbered experiment memory plus index/status/core/active as needed.
Otherwise: add or update memories only with stable, non-obvious project conventions.
Do not add: quick-read facts; generic language/framework knowledge; one-off task notes;
volatile line-level details unrelated to recorded experiments.

## Maintenance Actions

- Renaming memories: References are updated automatically if handled via Serena's memory rename tool.
- Checking for stale memories (e.g. after deletion): Call `serena memories check` for a report.
