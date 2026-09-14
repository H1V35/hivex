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

Update the document that owns an accepted decision, using new documents only for distinct purposes. Preserve source versions, relevant history and honest uncertainty. Keep Hivex's incremental progress and budget rather than reingesting the whole corpus or resetting work after each round.

Use familiar language and concrete explanations. Briefly explain a technical term when the owner needs it to understand or decide; do not explain every term or change the requested level of detail.
