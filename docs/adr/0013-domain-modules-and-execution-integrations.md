---
title: Domain modules and replaceable execution integrations
status: accepted
date: 2026-09-17
updated: 2026-10-01
---

# Domain modules and replaceable execution integrations

This decision records the Rust domain boundaries and the compatibility contracts for Hivex's supported runtime. The model-assisted CLI and derived graph remain optional capabilities. [ADR 0014](0014-reliable-markdown-and-explicit-relationships.md) makes reliable Markdown and authored relationships the primary retrieval path; this decision preserves the current runtime and data contracts without making graph use mandatory or removing supported commands.

## Module ownership

Documents own source discovery, parsing and source coordinates. Knowledge owns derived graph operations, retrieval, ingestion, relationships, warnings, repairs and portable snapshots. Work owns progress, attempts, budgets, state transitions and recovery. Consultation and review compose these capabilities. CLI parsing and process exit/output belong at the entry boundary. Concrete SQLite persistence stays beside the behavior it serves; a repository interface is not required for every entity.

Keep execution contracts independent of vendors. The execution integration, model provider, model and result-affecting options are separate choices. An integration validates the effective profile, returns execution results, usage and lifecycle evidence, and fails unsupported choices without silent fallback. Knowledge-profile configuration remains separate from the implementing agent's and code reviewer's profiles.

## Data and execution compatibility

The Rust CLI continues to read existing SQLite state and shared snapshot v1 data. Supported maintenance preserves source coordinates and versions, decision and relationship identities, citations, evidence, native quality, work identity and plans, attempts, results, receipts, caches, budgets and unknown consumption. A domain refactor or optional graph change does not by itself authorize migration, reingestion, relabeling or reset of existing data.

Work budgets span operation phases, attempts and resumption. A profile change cannot grant a new allowance or silently change the work's operation, sources or arguments. Explicit profile resumption keeps the same work identity and scope, records the transition and integration request identity, and preserves coverage, prior results and receipts. Running or uncertain calls retain their recovery requirements; failed work retains explicit retry. Selecting a profile alone neither invokes a model nor makes an old result current under a different profile. A changed candidate uses the supported normal check; an unchanged adverse check is not repeated for a green result.

The exact profile-transition behavior, including the implemented default migration, is in the [optional graph and model-assisted CLI reference](../reference/graph-cli.md#execution-profiles). The work/data runtime contract and optional model-assisted operations are in [ADR 0010](0010-practical-knowledge-assistance.md). The implementation's build, compatibility and package checks are in the [Guide's Development section](../guide.md#development).

Keep tests at observable boundaries: domain invariants, persisted v1 data, failures/recovery, and public CLI or integration behavior. Consolidate or retire coverage only when its meaningful guarantee is preserved; test counts and language statistics are not acceptance criteria.

## Relationships

- **Preserves** the source-first product scope and optional graph status in [ADR 0014](0014-reliable-markdown-and-explicit-relationships.md).
- **Depends on** [ADR 0010](0010-practical-knowledge-assistance.md) for the current optional model-assisted runtime and its work/data guarantees.
- **Implemented in** the Rust CLI and verified under the [Guide's development procedure](../guide.md#development).
- **Historical rationale and delivery dates** remain in the [original ADR 0013 record](../archive/adr/0013-domain-modules-and-execution-integrations.md).

<a id="explicit-profile-continuity-148"></a>
## Explicit profile continuity

Profile resumption is an optional supported CLI operation. The original operation, source versions and arguments must match; the selected profile must be supported by its integration. Preserve the same work ID, plan, pending candidate, attempts, results, receipts, consumption and limits. Record each transition and opaque integration request identity so retained input can be checked without guessing serialization. Coverage remains attributed to the same work across profile-key changes; refuse ambiguous attribution. Keep decision/relationship IDs, evidence and native quality unchanged, and do not relabel completed historical results.

An unchanged adverse check is not retried by changing profile. A source-backed candidate correction receives its normal check under the selected profile; an evidenced local disposition may inspect the exact historical check without invoking the former model. Profile changes alone do not mutate Markdown, reingest knowledge or reset the budget.
