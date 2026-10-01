---
title: "Engineering"
status: draft
created_at: 2026-09-13
updated_at: 2026-10-01
---

# Engineering

## Understand the change

Use the project's Markdown and Hivex's relevant source passages and authored relationships to recover settled context. Apply the scope, conditions, exceptions and replacements. Ask the owner only when evidence cannot resolve a meaningful decision; give a clear recommendation.

Choose the workflow the task needs. Implement a defined change directly, use a grill for open decisions, and use focused research or a disposable prototype when it answers a real question. Additional specs and tickets should help define or divide work. They are not mandatory stages for every task.

## Design and implementation

Prefer domain-driven design: group behavior by meaningful domain responsibilities, use the agreed language and keep interfaces small. Do not impose hexagonal architecture or speculative layers. Respect existing accepted project choices and raise genuine conflicts before replacing them.

Prefer an existing library, standard facility or small direct implementation when it solves the problem. Create files and abstractions for meaningful responsibilities, not to satisfy arbitrary structure. Keep code self-explanatory; documentation records intent and reasons that code cannot explain.

Delegate suitable bounded subtasks when authorized and useful. Keep responsibility for the integrated result. Model and effort choices belong to the task or project configuration, not to issue labels.

## Names

Prefer kebab-case for authored files and directories. Preserve conventional Markdown entrypoints and names required by a framework, tool or language; apply idiomatic language conventions where they differ.

## Tests and verification

Choose checks for value and risk. Prioritize critical flows, stable rules and demonstrated regressions. TDD is optional and reserved for critical flows whose behavior is sufficiently defined; use exploration first when product assumptions remain open.

Test observable behavior at useful interfaces with independent expected results. Avoid tests that freeze internal helpers, arbitrary structure or the implementation's own calculation. Investigate failures against the intended behavior before deciding whether the code or the test needs correction.

Run the relevant checks for the affected surfaces. Documentation and configuration need appropriate reference, format or behavior checks, not a test for every edit. Record what was actually verified and any material limits; do not turn unavailable or interrupted checks into passes.

## Independent review

Use one independent review by default, covering scope, correctness and project standards. Add another when concrete risk or findings justify it. The reviewer uses the implementing agent's model and reasoning effort. Hivex retrieval does not choose or invoke a model.

The principal reviewer verifies possible conflicts with relevant decisions, dependencies and exceptions. Hivex assists that review. Start with local recovery of evidence; the reviewer reasons about actual applicability. Structural validation does not approve an implementation.

Resolve findings on their merits. Do not rerun reviewers or modify doctrine merely to obtain approval. Apply existing owner authorization and the repository's Git procedure to completion.

## Maintain knowledge and communicate clearly

Before writing, consult the map and search for the existing authority, including relevant history and replacements. Update that authority for the same scope; use new documents only for distinct purposes. Preserve source versions, relevant history and honest uncertainty. Retain necessary historical execution evidence, failed results and consumed budgets when retiring old integration; source retrieval must not reset or reopen it.

Use familiar language and concrete explanations. Briefly explain a technical term when the owner needs it to understand or decide; do not explain every term or change the requested level of detail.

## Knowledge before merge

Before integration, maintain Markdown that adds durable value in the authority that owns the topic. Record behavior, constraints, verification and deferred capabilities without duplicating implementation details. Link explicit dependencies, exceptions and replacements under the shared formal Relationships contract; follow relevant links when reviewing a change. Run `check --source <document>` for affected authorities and callers, inspect coverage/findings and preserve current source versions. The structural checker does not certify meaning or permission.

Review the affected authorities, their current conditions and history links, and the changed implementation at a defined revision. Resolve demonstrated contradictions, missing decisions needed by supported behavior, broken references and actionable defects. Treat a model finding as a claim to verify against those sources and code. Keep follow-up finite and scoped to the evidence that changed; do not repeatedly certify the whole corpus or chase a globally green result. A real current defect or unresolved policy decision that affects supported behavior blocks integration. When evidence cannot resolve a consequential question, preserve it as unresolved and ask the owner with the sources, impact and a recommendation.

Start with the documentation map and relevant Markdown authorities, using deterministic search and following explicit links as the task requires. Hivex's CLI reads authored Markdown and invokes no model. Missing source coverage or unread continuation limits the affected conclusions; expand with documentation and available tools before asking the owner. Historical graph status does not certify the current base or block unrelated work. Preserve necessary old sources, quality marks, uncertainty, failed results, receipts and consumption; do not erase or relabel history to clear a warning. Correct current defects on their merits.

Keep the source and implementation revisions reviewed identifiable. Close the review when its concrete findings are resolved or an owner decision is needed. A bounded evidence update may resolve a concrete finding; a changed source or implementation requires review of the affected scope again. This policy complements independent implementation review, relevant behavioral checks and the project's existing merge authorization in the [tracker procedure](../procedures/issue-tracker.md).

Keep decision approval separate from implementation and verification under the project decision/delivery catalogue. Reconcile affected authorities alongside code changes to prevent invented rules and contradictions.
