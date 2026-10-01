---
title: Practical project knowledge with bounded work
status: accepted
updated: 2026-10-01
---

# Practical project knowledge with bounded work

Hivex is an AI-first CLI for people and agents. It supplies reusable workflow capabilities and project knowledge, helping agents recover settled decisions, avoid contradictions and reduce repeated source reading. It does not become a session orchestrator or the principal implementation reviewer.

Markdown remains authoritative, wherever a project, package or module keeps it. Read relevant conditions, exceptions and replacements before acting. Apply settled decisions autonomously; use available documentation and tools before asking the owner about a question that remains unresolved. [ADR 0014](0014-reliable-markdown-and-explicit-relationships.md) makes reliable sources and explicit relationships the primary retrieval path and the inferred graph optional.

## Runtime and compatibility

The Rust CLI and Cargo development toolchain replace the TypeScript/Bun runtime. Use cohesive domain modules, stable Rust, rustfmt and Clippy; TypeScript lint rules do not prescribe Rust architecture. The retired runtime and migration stages remain in [the original decision history](../archive/adr/0010-practical-knowledge-assistance.md#rust-migration-80).

Keep compatible SQLite and shared snapshot v1 data, document versions, useful CLI guarantees, retained answers, caches, work identities, attempts, results, budgets and recovery evidence. Do not silently convert unknown legacy metadata into approval or reset work to migrate a runtime. Fixed compatibility fixtures preserve executable contracts; the old experimental Opus/cohort graph is not an acceptance target.

## Optional model-assisted knowledge

An explicit graph update processes bounded, source-preserving units with one normal check per batch and checkpointed progress. Reuse exact matching knowledge when its processing context permits it; a text hash alone does not certify scope or meaning. Maintain prior supported relationships or justify their changed meaning against sources. Report current findings and limitations without treating every incidental detail as a missing decision.

A work budget spans its phases, attempts and resumption. Expose actual and unknown usage; limits preserve progress. A cheaper model does not justify unnecessary invocations or context. Keep profile selection local and explicit, with no silent fallback. The knowledge profile and the principal agent/reviewer profile are separate choices.

Corrections and evidenced local dispositions retain the original candidate, failed checks, receipts and consumption. A changed candidate requires the applicable normal check; an unchanged adverse check is not repeated for green output. These mechanics describe the optional CLI and do not reinstate a global graph gate. Current interface and recovery conditions belong in the [graph CLI reference](../reference/graph-cli.md).

## Distribution and verification

The package is `@h1v35/hivex`, its command is `hivex`, and its license is MIT. Retain the six workflow/knowledge skills and optional language templates. The first verified native package is macOS ARM64, with a direct executable and no JavaScript launcher, runtime dependency or installation hook. Further targets require their own build, behavior and artifact verification.

Cargo and the Rust development utility prepare and verify the exact npm archive using system `tar`. npm remains the distribution channel and `package.json` remains metadata. Registry installation/publication is a separate release step; development changes do not authorize publication. Follow the [release procedure](../procedures/releasing.md).

Verify observable behavior, failures, source/version changes and recovery at useful boundaries. Tests are selected for value rather than count. Future retrieval alternatives must demonstrate correct decisions and evidence, including a conflict, exception, indirect dependency, insufficient evidence and a source change, with expected outcomes and measured total cost.

## Relationships

- Depends on [versioned authority](0001-versioned-project-knowledge.md), [selective history](0011-shared-knowledge-and-selective-history.md) and [execution/data continuity](0013-domain-modules-and-execution-integrations.md).
- Extended by [the project foundation](0012-project-foundation-and-workflow.md); the capabilities are independent rather than compulsory phases.
- Partially superseded by [ADR 0014](0014-reliable-markdown-and-explicit-relationships.md): inferred relationships no longer define the required knowledge base or integration gate.
- Replaces the [experimental cohort/admission workflow](../archive/adr/0004-resumable-ingestion-store.md), whose original rules and evidence remain historical.
- Historical delivery stages, Bun/Rust migration amendments, warning mechanisms and the #150 rationale remain in [the full record](../archive/adr/0010-practical-knowledge-assistance.md).
