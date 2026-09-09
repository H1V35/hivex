---
title: Practical knowledge assistance with bounded work
status: accepted
date: 2026-09-09
---

# Practical knowledge assistance with bounded work

The owner approved a new contract after a grill in [Hivex #17](https://github.com/H1V35/hivex/issues/17).
The previous implementation spent disproportionate effort certifying intermediate model output.
Hivex must provide a portable second brain that helps agents act autonomously, reduce hallucinations
and contradictions, and recover settled decisions before asking the owner again.

Markdown remains documentary authority. Documents can live at monorepo, package or module level and
use the adopting project's own organization and format. Recommend clear domain language, decisions
with their reasons, explicit conditions and replacements, and one authoritative home per fact.
Compi's documentation convention is useful guidance, not an admission requirement.

Keep a semantic graph of meaningful decisions and relationships, including implicit cross-document
connections. A document-summary index alone does not satisfy the contract. Recover the relevant
transitive dependencies within an explicit context budget. Explain applicability when the sources
support it; otherwise identify uncertainty and the remaining evidence needed.

The implementing agent consults Hivex before a coherent feature or behavior change. The principal
reviewer consults it with the task and diff during review, checks its findings and keeps responsibility
for accepting the implementation. A demonstrated contradiction must be corrected or resolved by an
approved decision change. Hivex neither conducts every aspect of code review nor rewrites Markdown
on its own. Genuine unanswered decisions go to the owner with sources, impact and a recommendation.

An update splits oversized Markdown into source-bound units with original line provenance, processes
bounded rounds, and checkpoints progress so resumption never discards completed ingestion. Reuse
retained extraction results when their source and processing context still match; do not require the
whole corpus or a large document to fit in one invocation. Caches optimize work and carry no authority.

An update processes a bounded batch and makes one additional knowledge check against its documents
and affected relationships. Do not review every node separately or every possible source pair.
Do not automatically revise and retry until the model produces green output. Usable knowledge remains
available when another part is pending or uncertain, with those limits visible to the caller.
A wrong derived interpretation can be corrected against its source without changing doctrine.

Consultations and reviews detect added, changed and removed Markdown and update affected knowledge
within their work budget. Reuse unchanged knowledge. Working documents need no commit to be readable,
but their working state and exact contents must be identifiable. Results refer to the document and
implementation versions actually considered; subsequent changes are not silently covered.

A work budget spans the entire requested operation, its phases and any resumption or attempts.
Expose actual consumption and unknown usage, preserve progress at a limit and distinguish initial
indexing, maintenance, consultation and review. Small empirical checks establish useful defaults;
a cheap model does not justify unnecessary invocations or context.

Luna/max is the owner's selected knowledge model and the profile validated for this release, without
silent fallback. Keep model selection and invocation localized so another user can configure a
supported model later. Do not build a provider framework speculatively. The principal agent may use
a different model without moving project knowledge into its vendor's private memory.

Use TypeScript/Bun and practical domain-driven modules with small interfaces. There is no obligation
to retain the old code, Opus graph, gold suite or machinery as the architecture or acceptance target.
Historical evidence keeps its actual result and limits. This decision supersedes the mandatory
candidate/fidelity/pair-comparison/admission ceremony in ADRs 0004–0009 for the replacement workflow;
those ADRs describe the earlier implementation and remain historical records.

The portable skill is part of each functional delivery. It teaches the actual CLI, the agreed
workflow, good documentation practice and cost/uncertainty handling without requiring Compi's
private tools or skills. The first slice in #47 supplies explicit initial updates and task context;
#48 adds automatic incremental maintenance and interpretation repair; #19 supplies diff-review assistance. These are
implementation stages, not claims that the entire new contract is already shipped.

Validate with bounded real Compi cases covering a conflict, a valid exception, indirect dependency,
insufficient evidence and a document change, with explicit expected outcomes and measured cost.
Also use a differently organized Markdown project and an agent exercising the skill. Tests exercise
the public CLI and native protocol seam, not incidental representations.

Early use in Compi is desirable. Complete closure additionally requires recovering useful historical
knowledge and retiring the old active machinery and consumers after replacement is verified.
Keep Git history and necessary external evidence; a legacy evidence document can suffice. Do not
rebuild another fleet inside that archive or treat a successful package build as completed adoption.

## Incremental consultation delivery (#48)

A consultation maintains at most one bounded pending batch before answering, prioritizing matching
fragments and known affected neighbors. Update, check and answer share one work budget; context-limit
increases do not reset an unfinished work item. Remaining corpus coverage is reported, and explicit
updates can advance further rounds. Repeating a completed consultation reuses its result while its
supplied context remains unchanged.

Repair revisits selected source units with an explicit correction reason, without editing Markdown.
It replaces their derived decisions and relationships and preserves prior attempts. Check findings
carry source/version/range scope so an unrelated consultation does not inherit a local uncertainty.
Removed sources remain identifiable when a dependency can no longer be verified.
