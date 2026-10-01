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

Separate replaced material into a declared Markdown archive while preserving original text, dates, provenance and reachable history. Keep applicable guarantees in their current authority; age alone does not retire them. A partially replaced decision keeps its live conditions and identifies the replaced scope.

Agents start with the map, deterministic source search and bounded reading. They expand to explicit dependencies, exceptions, replacements and relevant historical evidence. Apply settled decisions autonomously. Ask the owner only after reasonable available evidence and tools cannot resolve a consequential question, with sources, impact and a recommendation.

The inferred graph and model-assisted commands remain optional capabilities with their actual limits. Stale or uncertain derived records cannot certify current sources or block unrelated source-grounded work by themselves. Reading Markdown remains available when derived knowledge is insufficient. An alternative for reducing retrieval tokens and context is not yet selected; evaluate it against this simpler base with correctness, evidence recovered, agent context, model usage and maintenance cost measured together.

## Knowledge before integration

Maintain valuable Markdown and its explicit relationships before merging. The reviewer checks the affected authorities, current conditions, exceptions, history links and code together. Repair demonstrated contradictions, missing decisions needed by supported behavior, broken references and easy actionable defects. Do not leave durable decisions only in chats or tracker comments.

Close the review against a defined source and implementation revision. Record new evidence as a bounded change to that review. A model finding is a claim to verify, not a reason to repeatedly certify the whole corpus. Current unresolved policy conflicts or implementation defects still block integration; a globally green graph, zero unrelated graph warnings or complete graph ingestion are not prerequisites. This does not authorize ignoring a real problem surfaced by the graph.

Preserve existing graph snapshots, SQLite records, failed candidates, attempts, receipts, native uncertainty and consumption. Do not reset accounting, relabel failures as success, rewrite historical output or erase evidence to complete this transition. The unfinished #152 optimization remains separate from this decision.

## Relationships

- Depends on [versioned source authority](0001-versioned-project-knowledge.md), [selective archival](0011-shared-knowledge-and-selective-history.md) and [the project foundation](0012-project-foundation-and-workflow.md).
- Supersedes the mandatory inferred-graph requirement in [the prior product contract](../archive/adr/0010-practical-knowledge-assistance.md) and the graph-completeness/zero-warning merge gate in [the former foundation policy](../archive/adr/0012-project-foundation-and-workflow.md#knowledge-before-merge-2026-09-18).
- Preserves [runtime, data and execution compatibility](0013-domain-modules-and-execution-integrations.md); no runtime migration or model selection is implied.
- Implemented in the [source-reading guide](../guide.md#recover-context) and [engineering policy](../guidelines/engineering.md#knowledge-before-merge). Choosing the future retrieval accelerator requires separate evidence.
