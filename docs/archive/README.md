---
title: "Historical records"
status: accepted
created_at: 2026-10-01
tags: [documentation, history]
archived_at: 2026-10-01
updated_at: 2026-10-02
---

# Historical records

Archived documents retain earlier decisions, original body text and source provenance. Their recorded acceptance belongs to their original scope; the [current authority map](../README.md) governs present work. Metadata dates use the [shared date convention](../../skills/hivex/references/markdown.md#document-dates).

## Decision history

The original records at Git revision `1e3768f65f901522a63e00bde51069020049e123` are preserved under `adr/`:

- [0001: versioned source interface](adr/0001-versioned-project-knowledge.md); its current guarantees remain in [the source authority](../adr/0001-versioned-project-knowledge.md).
- [0002: native candidates](adr/0002-native-knowledge-candidates.md), [0003: Bun installation](adr/0003-independent-bun-installation.md), [0004: ingestion store](adr/0004-resumable-ingestion-store.md), [0005: candidate snapshots](adr/0005-source-bound-graph-snapshots.md), [0006: source fidelity](adr/0006-source-fidelity-review.md), [0007: pair comparisons](adr/0007-evidence-bound-source-comparisons.md), [0008: graph admission](adr/0008-reviewed-graph-admission.md) and [0009: grounding](adr/0009-implementation-claim-grounding.md) describe fully retired mechanisms; they have no duplicate files in the current ADR directory.
- [0010: product/runtime history](adr/0010-practical-knowledge-assistance.md), [0012: foundation amendments](adr/0012-project-foundation-and-workflow.md) and [0013: runtime boundaries](adr/0013-domain-modules-and-execution-integrations.md) preserve prior reasoning; live choices remain in their corresponding current authorities.

## Retired runtime and measurements

The 0.7.8 [command reference](runtime/graph-cli-0.7.8.md), [guide](runtime/guide-0.7.8.md), [history contract](runtime/history-contract-0.7.8.md) and [domain contract](runtime/domain-contract-0.7.8.md) preserve the implemented contract at `180ea2750e0da9253494b02e7bbbb25938720758`. They are historical investigation material, not current instructions or permission to replay work.

The [dated source-workflow validation](validation/source-workflow-2026-10-01.md) records bounded observations with their limits. Current source retrieval and migration are described in [the guide](../guide.md).

Creation dates and source revisions identify the original document. `archived_at` records its first archival event; `updated_at` records later changes to the archived record, including appended content or repaired links. Renamed metadata keys and necessary link repairs do not rewrite prior decisions. Existing execution stores, adverse results and consumed budgets remain untouched.
