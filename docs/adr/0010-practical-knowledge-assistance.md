---
title: Practical knowledge assistance with bounded work
status: accepted
date: 2026-09-09
---

# Practical knowledge assistance with bounded work

The owner approved a new contract after a grill in [Hivex #17](https://github.com/H1V35/hivex/issues/17). The previous implementation spent disproportionate effort certifying intermediate model output. Hivex must provide a portable second brain that helps agents act autonomously, reduce hallucinations and contradictions, and recover settled decisions before asking the owner again.

Markdown remains documentary authority. Documents can live at monorepo, package or module level and use the project's own organization and format. Recommend clear domain language, decisions with their reasons, explicit conditions and replacements, and one authoritative home per fact. The [recommended Markdown convention](../../skills/hivex/references/markdown.md) is guidance, not an admission requirement.

Keep a semantic graph of meaningful decisions and relationships, including implicit cross-document connections. A document-summary index alone does not satisfy the contract. Recover the relevant transitive dependencies within an explicit context budget. Explain applicability when the sources support it; otherwise identify uncertainty and the remaining evidence needed.

The implementing agent consults Hivex before a coherent feature or behavior change. The principal reviewer consults it with the task and diff during review, checks its findings and keeps responsibility for accepting the implementation. A demonstrated contradiction must be corrected or resolved by an approved decision change. Hivex neither conducts every aspect of code review nor rewrites Markdown on its own. Genuine unanswered decisions go to the owner with sources, impact and a recommendation.

An update splits oversized Markdown into source-bound units with original line provenance, processes bounded rounds, and checkpoints progress so resumption never discards completed ingestion. Reuse retained extraction results when their source and processing context still match; do not require the whole corpus or a large document to fit in one invocation. Caches optimize work and carry no authority.

An update processes a bounded batch and makes one additional knowledge check against its documents and affected relationships. Do not review every node separately or every possible source pair. Do not automatically revise and retry until the model produces green output. Usable knowledge remains available when another part is pending or uncertain, with those limits visible to the caller. A wrong derived interpretation can be corrected against its source without changing doctrine.

Consultations and reviews detect added, changed and removed Markdown and update affected knowledge within their work budget. Reuse unchanged knowledge. Working documents need no commit to be readable, but their working state and exact contents must be identifiable. Results refer to the document and implementation versions actually considered; subsequent changes are not silently covered.

A work budget spans the entire requested operation, its phases and any resumption or attempts. Expose actual consumption and unknown usage, preserve progress at a limit and distinguish initial indexing, maintenance, consultation and review. Small empirical checks establish useful defaults; a cheap model does not justify unnecessary invocations or context.

Luna/max is the owner's selected knowledge model and the profile validated for this release, without silent fallback. Keep model selection and invocation localized so another user can configure a supported model later. Do not build a provider framework speculatively. The principal agent may use a different model without moving project knowledge into its vendor's private memory.

Use TypeScript/Bun and practical domain-driven modules with small interfaces. There is no obligation to retain the old code, Opus graph, gold suite or machinery as the architecture or acceptance target. Historical evidence keeps its actual result and limits. This decision supersedes the mandatory candidate/fidelity/pair-comparison/admission ceremony in ADRs 0004–0009 for the replacement workflow; those ADRs describe the earlier implementation and remain historical records.

The portable skill is part of each functional delivery. It teaches the actual CLI, the agreed workflow, good documentation practice and cost/uncertainty handling without requiring private tools or unrelated skills. The first slice in #47 supplies explicit initial updates and task context; #48 adds automatic incremental maintenance and interpretation repair; #19 supplies diff-review assistance. These are implementation stages, not claims that the entire new contract is already shipped.

Validate with bounded real cases covering a conflict, a valid exception, indirect dependency, insufficient evidence and a document change, with explicit expected outcomes and measured cost. Also use a differently organized Markdown project and an agent exercising the skill. Tests exercise the public CLI and native protocol seam, not incidental representations.

Early use can precede complete legacy recovery. Complete closure additionally requires recovering useful historical knowledge and retiring old active machinery after replacement is verified. Keep Git history and necessary external evidence; a legacy evidence document can suffice. Do not rebuild another fleet inside that archive or treat a successful package build as proof that the workflow is fully validated.

## Rust migration (#80)

After validating adoption in Compi and Waynly, the owner approved replacing the TypeScript/Bun runtime with Rust. This supersedes the language choice above while retaining the product contract and cohesive domain modules. The migration introduces local document, initialization and recovery commands first, then knowledge storage and native execution, and finally npm distribution. The published CLI remains the reference until its replacement is verified; development slices do not claim the unfinished commands work.

Preserve the observable CLI, source coordinates and hashes, snapshot v1 and existing SQLite data without reingestion. Retain identities, relationships, evidence, closures and their history, caches, unfinished or failed work, results and cumulative budgets, including unknown consumption. Compatibility checks use synthetic public fixtures and the simulated native transport. Consumer knowledge stays private and is never migrated in place to make a test pass. Native Codex compatibility remains capability-based, with localized Luna/max selection and no silent fallback.

Use rustfmt and Clippy for Rust. TypeScript lint rules do not prescribe Rust architecture. Remove replaced code and dependencies after compatibility is demonstrated, and prepare and inspect the final npm artifact before requesting the publication approval applicable to that artifact.

## Incremental consultation delivery (#48)

A consultation maintains at most one bounded pending batch before answering, prioritizing matching fragments and known affected neighbors. Update, check and answer share one work budget; context-limit increases do not reset an unfinished work item. Remaining corpus coverage is reported, and explicit updates can advance further rounds. Repeating a completed consultation reuses its result while its supplied context remains unchanged.

Repair revisits selected source units with an explicit correction reason, without editing Markdown. It replaces their derived decisions and relationships and preserves prior attempts. Check findings carry source/version/range scope so an unrelated consultation does not inherit a local uncertainty. Removed sources remain identifiable when a dependency can no longer be verified. A changed known supporting source takes priority over unrelated pending documents. Endpoint updates include the source passages supporting their previous relationships; if that evidence is missing or cannot fit the context limit, retain pending work and report the limitation before spending a model call.

## Localized maintenance and reliable relationships (#83)

The owner reaffirmed that the graph must retrieve decisions, dependencies and exceptions, not reproduce every Markdown detail. Distinguish normal limits of supplied evidence from check findings and local validation failures; preserve legacy warnings without guessing their category. Coverage and interpretation quality are separate.

A repair can select source ranges, expanding to the existing bounded ingestion units and preserving unaffected knowledge. Range repair requires current source coverage; it does not silently certify unchanged-looking fragments of a changed document. New work uses its existing single check on the materialized result. Losing relationships supported by still-current evidence requires an explicit source-grounded justification for removal or replacement; otherwise retain the previous graph and the attempt for inspection. Do not add automatic repair loops or additional model calls to manufacture a clean result. Older unfinished work keeps its checking contract and accounting.

A replacement's admission uses findings from the current check, rather than treating a neighbor's inherited uncertainty as a new rejection. Preserve that uncertainty in the graph and continue to reject current findings affecting the relationship or its endpoints and invalid evidence. A corrected local admission rule may reassess an unchanged retained check without another model invocation; verify candidate identity, evidence freshness and graph continuity, preserving the original failed assessment and consumed budget.

Range repair selects the requested passages and complete overlapping decision citations, rather than replacing an entire physical ingestion block. Preserve unrelated decisions and relationships inside that block. Each complete repair range must fit the existing round byte bound; do not split a decision halfway to fit. Executed or pending work retains its original ranges and accounting, while unexecuted plans may use the more precise scope. A repair completed with broader blocks remains reusable when repeating its original request.

## Warning noise prevention and explicit closure (#93)

Each new work item reuses its one normal bounded check to inspect new uncertainties and revalidate expired closures affected by changed sources. The check may dismiss a descriptive observation or revalidate an untargeted, non-validation closure only when the complete documents in the affected scope and, for a closure, its complete prior evidence are in context. Findings and local validation failures are excluded from that automatic closure path. If any required documentation is missing or cannot fit, keep the affected warning pending. Matching text, including an identical paragraph, is insufficient: current document versions, scope, conditions and relationships remain part of the evidence.

An update reports `warningChanges` as `new`, `reopened` and `resolved` lists of `{id,message,state}`. Its baseline records whether each warning ID has a closure at work start, independently of source coverage freshness; a current-source closure can therefore exist before that source is ingested. `new` contains IDs absent at the start, and `resolved` contains IDs without a closure at the start that end with one. `reopened` includes earlier closures whose evidence is stale, whether they became stale before or during the work, and remains until revalidated. The comparison survives resumption and reports changes seen by that work rather than cleaning the complete backlog. Review new active entries and reopened entries against the current sources. No mandatory model call, retry or automatic semantic repair is added. Deterministic local retrieval and source inspection come first; invoke Luna/max only when it adds useful interpretation.

Closures require an explicit reason and current evidence. Use `warnings --resolve <resolutions.json>` for an evidenced closure and `warnings --all` to inspect active warnings and preserved history; `previousResolutions` retains every earlier closure. A changed or unavailable cited source reactivates the warning. Findings about implementation or relationships require an explicit correction or repair, and a check with findings or local validation failures never turns a failure into a closure. Do not require an answer to a genuine unresolved question, repeat the model to obtain a green result, reopen settled questions or scan the whole corpus to pursue zero warnings.
