---
title: Reliable Markdown and explicit relationships before inferred graphs
status: accepted
date: 2026-10-01
---

# Reliable Markdown and explicit relationships before inferred graphs

## Reason

Hivex exists to give projects a reusable workflow and preserve decisions that code cannot explain. Knowledge retrieval should help implementing agents avoid contradictions while spending less context and fewer tokens. Repeated graph checks and repairs consumed more effort than the small source changes they maintained; some findings were false and some mappings were prepared incorrectly. Lower input size alone did not establish a reliable or economical workflow.

The owner approved prioritizing a reliable Markdown base, archived history and explicit relationships. The goal remains autonomous, source-grounded work; maintaining an inferred graph is not itself the outcome.

## Decision

Maintain one current authority for each meaningful topic. The documentation map directs a task to the relevant sources. Authors state dependencies, exceptions and replacements as ordinary Markdown links with their relationship and scope, rather than relying on inferred edges. Follow those links, including indirect dependencies, only as far as the task requires.

The [shared authored relationship contract v1](../../skills/hivex/references/markdown.md#make-relationships-explicit) fixes five literals, their direction and an exact `## Relationships` block. Deterministic CLI navigation resolves selected targets and cited ranges, reports incomplete coverage and rejects malformed or ambiguous declarations. Authors and reviewers remain responsible for semantic correctness; no model guesses the relationship type or reverses its meaning.

Before writing, the agent checks the map and searches existing current and relevant historical knowledge for that topic. Read likely matches and their replacements; update the existing authority for the same scope. Create another document only after establishing a distinct useful responsibility. This prevents parallel sources of truth from diverging.

Use the [shared decision/delivery state catalogue](../../skills/hivex/assets/project/docs/adr/README.md#decision-and-delivery-states). Approval, implementation, verification and permission are separate claims: an accepted future decision is not an implemented feature or an implementation GO. Missing state is unknown; current applicability still depends on scope, conditions and replacements.

Separate replaced material into a declared Markdown archive while preserving original text, dates, provenance and reachable history. Keep applicable guarantees in their current authority; age alone does not retire them. A partially replaced decision keeps its live conditions and identifies the replaced scope.

Agents start with the map, deterministic source search and bounded reading. They expand to explicit dependencies, exceptions, replacements and relevant historical evidence. Apply settled decisions autonomously. Ask the owner only after reasonable available evidence and tools cannot resolve a consequential question, with sources, impact and a recommendation.

## Retire the inferred graph runtime

After validating explicit navigation, the owner approved the remaining five-point plan: complete source retrieval; remove graph/runtime code; close the shared methodology; measure bounded real use cases; prepare release and Compi adoption. Search and runtime retirement are one coherent delivery because both replace the old extracted-decision query contract. Maintenance validation and adoption follow as useful independently verifiable changes.

Version 0.8.0 retires graph extraction, checks, inference, model-assisted consultation/review, repair, execution profiles, work orchestration and active snapshot operations. Source commands discover, search, navigate and read Markdown directly. They never open or change the historical graph stores. SQLite remains useful solely as in-memory FTS5/BM25 lexical search; do not add a persistent index or a second knowledge database without measured need.

Preserve the old package/Git checkpoint and required execution evidence before integration, including snapshots, failed/pending work, attempts, receipts, uncertainty and consumption. A retirement diagnostic replaces removed commands; no current path resumes or silently rewrites legacy data. The #152 candidate is superseded as an optimization of removed runtime, not admitted or called completed maintenance. Preserve its checkpoint and adverse results.

Evaluate retrieval against correct evidence recovered, agent context, query latency and maintenance cost together. The responsible agent follows available documents and tools autonomously before asking the owner. Do not introduce another inference accelerator without evidence that this simpler base falls short.

## Knowledge before integration

Maintain valuable Markdown and its explicit relationships before merging. The reviewer checks the affected authorities, current conditions, exceptions, history links and code together. Repair demonstrated contradictions, missing decisions needed by supported behavior, broken references and easy actionable defects. Do not leave durable decisions only in chats or tracker comments.

Recover settled rules before implementation and reconcile the affected authority during the same change. Keep delivery and evidence aligned with the actual code revision. This shared methodology applies to Hivex and its adopting-project foundation to prevent hallucinated rules and contradictions; installing an updated package does not silently replace an existing project's explicit owner policy.

Close the review against a defined source and implementation revision. Record new evidence as a bounded change to that review. A model finding is a claim to verify, not a reason to repeatedly certify the whole corpus. Current unresolved policy conflicts or implementation defects still block integration; a globally green graph, zero unrelated graph warnings or complete graph ingestion are not prerequisites. This does not authorize ignoring a real problem surfaced by the graph.

Preserve existing graph snapshots, SQLite records, failed candidates, attempts, receipts, native uncertainty and consumption. Do not reset accounting, relabel failures as success, rewrite historical output or erase evidence to complete this transition. Archive and report the #152 optimization as superseded with its unfinished results intact.

## Relationships

- Depends on [Versioned source authority](0001-versioned-project-knowledge.md): exact sources and their conditions govern current work.
- Depends on [Selective archival](0011-shared-knowledge-and-selective-history.md): replaced reasoning remains available without ordinary ingestion.
- Depends on [Project foundation](0012-project-foundation-and-workflow.md): reusable capabilities support the source-reading workflow.
- Implements [Versioned source authority](0001-versioned-project-knowledge.md): the reliable Markdown workflow applies source authority through focused reading and explicit references.
- Supersedes [Prior mandatory graph contract](../archive/adr/0010-practical-knowledge-assistance.md): replaces the mandatory inferred-graph requirement only; runtime/data safety stays applicable.
- Supersedes [Former graph merge gate](../archive/adr/0012-project-foundation-and-workflow.md#knowledge-before-merge-2026-09-18): whole-graph completeness and globally zero warnings are no longer integration prerequisites; applicable source/code defects still block.

## Preserved contracts and application

[Source runtime boundaries and historical data preservation](0013-domain-modules-and-execution-integrations.md) govern retirement. The [source-reading guide](../guide.md#recover-context) and [engineering policy](../guidelines/engineering.md#knowledge-before-merge) apply this decision. Choosing a future retrieval accelerator requires separate evidence.
