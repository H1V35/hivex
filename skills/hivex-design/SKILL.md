---
name: hivex-design
description: Resolve open product or architectural decisions through a grill, focused research or a useful prototype, and define scope or execution slices when needed.
---

# Resolve what matters

Recover the request, relevant Markdown and Hivex's local evidence before asking questions. Reuse settled decisions and inspect facts available in the repository or primary sources. A defined change can proceed to implementation without an interview or another planning artifact.

For a grill, ask one consequential question at a time, explain the tradeoff and recommend a course of action. Follow dependencies between decisions and wait for the owner's answer before implementing a design still under discussion.

Use research to settle factual uncertainty and a small prototype to answer a behavior or design question. Make alternatives meaningfully different; expose the relevant states or interactions. Preserve the conclusion and useful evidence. A disposable presentation does not make validated domain logic disposable or establish production readiness.

## Shape the domain and the work

Check terms and responsibilities against the project's glossary, code, rules and exceptions. Resolve ambiguity in the owning context instead of inventing synonyms. Prefer interfaces that hide useful complexity; question layers that only forward calls. Add an abstraction for a concrete responsibility, not to impose hexagonal architecture.

Capture the outcome, boundaries and useful acceptance evidence in the existing issue or spec. Split only for independently verifiable results or real dependencies. Cross-layer slices should deliver a meaningful behavior; a broad mechanical refactor may instead need introduction, migration and removal in stages.

Use `hivex-document` for durable decisions and `hivex-git` for authorized tracker work. Neither extra tickets nor a fixed number of alternatives, agents or prototypes is required.
