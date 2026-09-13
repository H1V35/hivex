---
name: hivex-design
description: Resolve open product or architectural decisions, investigate uncertainty, and shape work into a useful specification or execution slices. Use for planning, a grill, design tradeoffs, focused research, or prototypes that answer a question.
---

# Design useful work

Recover the request, existing decisions and relevant project vocabulary first. Use the project's Markdown and Hivex's local retrieval. Inspect facts available in the repository or authoritative sources instead of asking the owner to rediscover them.

If the task is already defined, carry that scope into implementation. Do not start an interview or create planning artifacts merely to satisfy a pipeline.

## Resolve what is open

For a grill, ask one meaningful question at a time, explain the practical tradeoff and give a recommendation. Follow dependencies between decisions and wait for the owner's answer. Reuse prior answers. Close the shared understanding before implementing a design the owner is still deciding.

Research a factual uncertainty using relevant primary sources. Build a small disposable prototype when seeing or exercising a behavior is the cheapest way to resolve a question. Keep either activity bounded to the decision it serves; preserve the useful conclusion and its evidence.

Use the project's language and domain responsibilities. Prefer a small interface that hides meaningful internal complexity. Respect established architecture and identify concrete tradeoffs before introducing another abstraction.

## Make execution clear

Capture the intended outcome, scope, useful acceptance evidence and unresolved decisions at the level the change needs. Reuse an existing issue or spec when it already owns the work. Create extra execution tickets only for independently verifiable slices or dependencies that help coordination.

A cross-layer feature slice should deliver a meaningful result. A mechanical refactor may need a different sequence; choose it from the actual affected code rather than forcing a template. Do not require exhaustive user stories, a decision-map hierarchy or one ticket per agent session.

Record durable decisions through the project's documentation practice, using `hivex-document` when maintaining that knowledge. Use `hivex-git` for tracker publication within the owner's authorization.
