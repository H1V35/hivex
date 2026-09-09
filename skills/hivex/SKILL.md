---
name: hivex
description: Retrieve project decisions and their dependencies and exceptions with Hivex, support implementation and review, and maintain useful Markdown knowledge. Use when Hivex is requested or configured for a project.
---

# Hivex

Hivex gives the implementing and reviewing agents project context. Markdown is documentary
authority; the graph is derived assistance. The principal agent remains responsible for the work.
Recover settled decisions before asking the owner to decide them again.

## Start with the installed interface

Read the project's agent instructions and source configuration. Run `hivex --help` when the installed
interface is unfamiliar. In a Bun project, `bun hivex` resolves the installed CLI. The package is
`@h1v35/hivex`; do not fetch the unrelated unscoped package.

Use the capabilities advertised by that version. Do not invent commands, silently switch models or
pretend a planned capability exists. Hivex's current validated knowledge profile is Luna/max through
native Codex; the principal agent's model is independent of that choice.

## Before implementation

For a coherent feature or behavior change, ask about the intended task and recover the relevant
decisions. Read their evidence and follow relevant dependencies, exceptions and replacements,
including indirect relationships. A search preview or accepted label alone does not settle scope.

Use Hivex's source/version references. Keep conditions with their rules; a partial exception does
not revoke an entire document. If the evidence answers the question, apply it without asking the
owner again. Ask only when information is missing, sources cannot resolve a real ambiguity or a
new decision requires the owner's involvement. Present the sources, impact and your recommendation.

Queries should be focused. Reuse context that remains current rather than asking again per file or
implementation step. Respect declared incomplete exploration and unavailable evidence.

## During review

The principal reviewer directs the review. Where the installed CLI supports task/diff assistance,
use it to identify relevant decisions and possible conflicts; it need not receive a predetermined
list of suspicions. Check its findings against the actual implementation and Markdown. Tests, lint
and general code review still address their own concerns.

Resolve a demonstrated contradiction before closing the change, by correcting the implementation
or recording an approved decision change. An uncertain finding needs investigation of the affected
point, not an automatic rerun of the whole process. Do not present a partial result as complete.
If this installed version lacks review assistance, report that limit rather than fabricating it.

## Maintain knowledge

The implementing agent maintains the documents as part of the change. Hivex may identify affected
documents or suggest a correction, but does not rewrite project decisions on its own. Correct an
incorrect graph interpretation against its source; do not alter doctrine to satisfy the model.

Keep documents at their monorepo, package or module authority. Follow the project's existing layout
and format. When establishing or improving documentation, use the optional
[Markdown convention](references/markdown.md); it is guidance, not an installation prerequisite.

Detect new, changed or removed documents before relying on the graph. Use the installed update
workflow within the same work budget. Working documents may be queried without a commit, but that
state is not approval; preserve the exact versions used by a review. Pending or uncertain knowledge
limits the conclusions that depend on it, without making unrelated context unusable.

`ask` updates at most one relevant pending batch before answering, under the same total call/input
budget. Its default is three calls. Repeat the same task to resume; changing its budget changes the
total ceiling, never the consumed count. Pending corpus coverage is explicit; use `update` when more
rounds are needed, rather than issuing repeated identical questions to force indexing.

Repair a demonstrably wrong interpretation with `update --repair <document> --reason <correction>`.
Check the Markdown first. This revises derived knowledge and its relationships without changing the
document or authorizing new policy. Do not use it to suppress a genuine contradiction. Review source-local
warnings in their stated scope and inspect `unavailableDocuments` for changed or removed dependencies.

## Control consumption

Large Markdown is ingested in bounded rounds. Inspect pending units and coverage, and resume the
same work; do not wipe the store or resend the whole corpus when only a later round remains.

A budget covers the complete work item, its phases and attempts. Resume retained progress with its
original accounting; do not start a fresh counter to bypass an exhausted limit. Separate initial
indexing, maintenance, consultation and review costs and report actual usage and unknown consumption.
Deterministic reads do not require another model call, although the caller consumes context tokens.

Do not automatically retry semantic disagreements or keep correcting until the model says green.
After a crash, use `recover` to inspect retained work. It must not alter live owners or processes.
An explicit `--acknowledge-uncertain` preserves uncertainty and cost; it does not certify a remote
outcome or authorize an automatic retry. Resume only the intended work with its existing budget.
Use `prune` for obsolete completed work and cached responses when needed; keep unfinished work and
export evidence that must outlive cache retention.

Preserve useful results and their limits. At a budget boundary, explain what remains and obtain an
authorized extension before spending more. Existing user authorization remains valid; the skill does
not require another permission question for already authorized work.
