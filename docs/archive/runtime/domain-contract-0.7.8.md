---
title: Domain modules and replaceable execution integrations
status: accepted
created_at: 2026-09-17
tags: [history, architecture, runtime]
archived_at: 2026-10-01
source_path: docs/adr/0013-domain-modules-and-execution-integrations.md
source_revision: 180ea2750e0da9253494b02e7bbbb25938720758
updated_at: 2026-10-02
---


# Domain modules and replaceable execution integrations

This decision records the Rust domain boundaries and the compatibility contracts for Hivex's supported runtime. The model-assisted CLI and derived graph remain optional capabilities. [ADR 0014](../../adr/0014-reliable-markdown-and-explicit-relationships.md) makes reliable Markdown and authored relationships the primary retrieval path; this decision preserves the current runtime and data contracts without making graph use mandatory or removing supported commands.

## Module ownership

Documents own source discovery, parsing and source coordinates. Knowledge owns derived graph operations, retrieval, ingestion, relationships, warnings, repairs and portable snapshots. Work owns progress, attempts, budgets, state transitions and recovery. Consultation and review compose these capabilities. CLI parsing and process exit/output belong at the entry boundary. Concrete SQLite persistence stays beside the behavior it serves; a repository interface is not required for every entity.

Keep execution contracts independent of vendors. The execution integration, model provider, model and result-affecting options are separate choices. An integration validates the effective profile, returns execution results, usage and lifecycle evidence, and fails unsupported choices without silent fallback. Knowledge-profile configuration remains separate from the implementing agent's and code reviewer's profiles.

## Data and execution compatibility

The Rust CLI continues to read existing SQLite state and shared snapshot v1 data. Supported maintenance preserves source coordinates and versions, decision and relationship identities, citations, evidence, native quality, work identity and plans, attempts, results, receipts, caches, budgets and unknown consumption. A domain refactor or optional graph change does not by itself authorize migration, reingestion, relabeling or reset of existing data.

Work budgets span operation phases, attempts and resumption. A profile change cannot grant a new allowance or silently change the work's operation, sources or arguments. Explicit profile resumption keeps the same work identity and scope, records the transition and integration request identity, and preserves coverage, prior results and receipts. Running or uncertain calls retain their recovery requirements; failed work retains explicit retry. Selecting a profile alone neither invokes a model nor makes an old result current under a different profile. A changed candidate uses the supported normal check; an unchanged adverse check is not repeated for a green result.

The exact profile-transition behavior, including the implemented default migration, is in the [optional graph and model-assisted CLI reference](graph-cli-0.7.8.md#execution-profiles). The work/data runtime contract and optional model-assisted operations are in [ADR 0010](../../adr/0010-practical-knowledge-assistance.md). The implementation's build, compatibility and package checks are in the [Guide's Development section](../../guide.md#development).

Keep tests at observable boundaries: domain invariants, persisted v1 data, failures/recovery, and public CLI or integration behavior. Consolidate or retire coverage only when its meaningful guarantee is preserved; test counts and language statistics are not acceptance criteria.

## Relationships

- Depends on [Product runtime contract](../../adr/0010-practical-knowledge-assistance.md): optional model work preserves its accounting and data guarantees.
- Extends [Reliable Markdown](../../adr/0014-reliable-markdown-and-explicit-relationships.md): records runtime and execution boundaries without requiring graph use.

## History and verification

The Rust CLI is verified under the [development procedure](../../guide.md#development). Historical rationale and delivery dates remain in the [original ADR 0013 record](../adr/0013-domain-modules-and-execution-integrations.md).

<a id="explicit-profile-continuity-148"></a>
## Explicit profile continuity

Profile resumption is an optional supported CLI operation. The original operation, source versions and arguments must match; the selected profile must be supported by its integration. Preserve the same work ID, plan, pending candidate, attempts, results, receipts, consumption and limits. Record each transition and opaque integration request identity so retained input can be checked without guessing serialization. Coverage remains attributed to the same work across profile-key changes; refuse ambiguous attribution. Keep decision/relationship IDs, evidence and native quality unchanged, and do not relabel completed historical results.

An unchanged adverse check is not retried by changing profile. A source-backed candidate correction receives its normal check under the selected profile; an evidenced local disposition may inspect the exact historical check without invoking the former model. Profile changes alone do not mutate Markdown, reingest knowledge or reset the budget.
