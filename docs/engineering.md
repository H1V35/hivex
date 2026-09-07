---
title: Engineering workflow
status: accepted
---

# Engineering workflow

Hivex is a TypeScript/Bun product. Modules group behavior by domain responsibility and hide internal
details behind small interfaces. Do not add a second development-session orchestrator or require
an adopting project's layout, tracker or product packages. Codex, Git/GitHub and CI coordinate work.

Use an existing issue for an already tracked requirement. Create a branch from the current remote
main, keep each PR to one coherent change and preserve commit history when merging. Never push
directly to main or force-push a shared branch. Apply review findings before acceptance; an invalid
review can be rerun, while an adverse finding must be resolved on its merits. Current explicit owner
authorization governs whether the agent may merge.

New behavior and bug fixes need tests at their observable interface, including conditions and
failure cases. Run the typechecker, lint, formatting and relevant tests before review; CI checks the
exact candidate revision. Dependency changes also require the verified installation and bootstrap
reproduction checks. Do not claim an omitted, interrupted or truncated check passed.

The lint configuration owns executable syntax/complexity constraints: cyclomatic complexity 20,
cognitive complexity 15, at most four parameters, nesting depth three and no nested/chained
ternaries. Refactor around meaningful responsibilities rather than adding tiny wrappers merely to
make a number pass. Changes to those limits require a documented decision.

Documentation records intent, constraints, decisions and reasons that code cannot explain.
Accepted source history and Markdown retain authority; caches, model output and search hits do not.
An accepted status alone does not settle amendments, exceptions or contradictions. Keep unresolved
evidence explicit. Never promote a historical agent's description of an owner ruling without
checking its provenance and applicability.

Use repository decisions and review evidence for durable knowledge, not private agent memory.
Checkpoints identify the exact commit, verified work and remaining work. Choose a context handoff
when the task needs it; Hivex does not impose the retired machinery's fixed token thresholds.
Knowledge-model operations use the admitted Luna/max profile and record actual usage, including
failed or interrupted attempts. Deterministic retrieval and maintenance do not require a model.

Grounding complements tests and code review. A claim that an implementation satisfies documented
decisions requires evidence tied to its exact code/diff and the accepted knowledge manifest.
Missing graph admission, unresolved contradictions or insufficient evidence remain unresolved;
neither silence nor an unrelated earlier PASS establishes acceptance.
