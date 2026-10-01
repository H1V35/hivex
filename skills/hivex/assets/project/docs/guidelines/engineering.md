# Engineering

## Understand the change

Use the project's Markdown and Hivex's relevant decisions, neighbors and sources to recover settled context. Apply the scope, conditions, exceptions and replacements. Ask the owner only when evidence cannot resolve a meaningful decision; give a clear recommendation.

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

Use one independent review by default, covering scope, correctness and project standards. Add another when concrete risk or findings justify it. The reviewer uses the implementing agent's model and reasoning effort; do not substitute the knowledge model for that review.

The principal reviewer verifies possible conflicts with relevant decisions, dependencies and exceptions. Hivex assists that review. Start with local recovery of evidence; use model-assisted review when it adds value, rather than by ceremony. A model's lack of findings is not approval of an implementation.

Resolve findings on their merits. Do not rerun reviewers or modify doctrine merely to obtain approval. Apply existing owner authorization and the repository's Git procedure to completion.

## Maintain knowledge and communicate clearly

Before writing, consult the map and search for the existing authority, including relevant history and replacements. Update that authority for the same scope; use new documents only for distinct purposes. Preserve source versions, relevant history and honest uncertainty. When optional graph work is used, preserve its incremental progress and cumulative budget rather than reingesting the corpus. Existing work history and consumed budgets must not be reset when changing the retrieval approach.

Use familiar language and concrete explanations. Briefly explain a technical term when the owner needs it to understand or decide; do not explain every term or change the requested level of detail.

## Knowledge before merge

Before integration, maintain Markdown that adds durable value in the authority that owns the topic. Record behavior, constraints, verification and deferred capabilities without duplicating implementation details. Link explicit dependencies, exceptions and replacements in ordinary Markdown, with their scope; follow relevant links when reviewing a change.

Review the affected authorities, their current conditions and history links, and the changed implementation at a defined revision. Resolve demonstrated contradictions, missing decisions needed by supported behavior, broken references and actionable defects. Treat a model finding as a claim to verify against those sources and code. Keep follow-up finite and scoped to the evidence that changed; do not repeatedly certify the whole corpus or chase a globally green result. A real current defect or unresolved policy decision that affects supported behavior blocks integration. When evidence cannot resolve a consequential question, preserve it as unresolved and ask the owner with the sources, impact and a recommendation.

Start with the documentation map and relevant Markdown authorities, using deterministic search and following explicit links as the task requires. Hivex's inferred graph and model-assisted commands are optional derived assistance. A stale graph, pending check or unrelated warning alone does not block integration; use the Markdown authority when graph evidence is incomplete or uncertain. If graph or local work state is maintained, preserve its original sources, quality marks, uncertainty, failed results, receipts and consumption. Never reset accounting, erase evidence or relabel historical/native results as success to clear a warning. Correct current defects on their merits; do not dismiss them as historical because a graph also contains old evidence.

Keep the source and implementation revisions reviewed identifiable. Close the review when its concrete findings are resolved or an owner decision is needed. A bounded evidence update may resolve a concrete finding; a changed source or implementation requires review of the affected scope again. This policy complements independent implementation review, relevant behavioral checks and the project's existing merge authorization in the [tracker procedure](../procedures/issue-tracker.md).

Keep decision approval separate from implementation and verification under the project decision/delivery catalogue. Reconcile affected authorities alongside code changes to prevent invented rules and contradictions.
