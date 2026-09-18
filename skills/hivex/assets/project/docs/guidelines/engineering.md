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

## Knowledge before merge

Before merging to the integration branch, every development must maintain the Markdown that adds durable value and complete its Hivex maintenance. Keep decisions in their existing authority; add a document only for a distinct useful scope. Record behavior, constraints, verification and deferred capabilities without duplicating implementation details.

Require no pending ingestion or checks for selected current sources, no stale or unavailable current dependencies, and no active warnings. Review each finding against its sources and affected code or relationships. Repair actual defects; close warnings only with an evidence-backed disposition. An explicitly deferred capability may receive an evidence-backed disposition without resolving its future design when its authority prohibits implementation or activation until its prerequisites are met. Roadmap inclusion alone does not make it currently supported behavior. Being unrelated to the current PR is insufficient; closing a warning does not implement a deferred capability. Unresolved contradictions, decisions needed for supported behavior and uncertain execution block the merge.

Native quality and historical provenance are separate from current review. Warning closure preserves original scopes and earlier resolutions without promoting records to `checked`. A literal `partial` or an entry in `uncheckedDecisions` therefore needs inspection, not automatic rejection or acceptance. Contrast affected current decisions, relationships and citations with the current Markdown and record the independent reviewer's disposition. Retained historical references and native marks may remain after that review; current defects must not be dismissed as history. Never delete history, relabel quality, suppress sources, reset accounting or repeat an unchanged model check just to obtain a clean status.

Export `.hivex/graph.json` with the maintained Markdown, review their final diff and recheck freshness after later source edits. Report the tool's literal status and the current-source review evidence. Wait if this gate cannot be completed. Hivex is decision support: passing the gate does not replace independent implementation review, applicable behavioral checks or merge authorization under the [tracker procedure](../procedures/issue-tracker.md).
