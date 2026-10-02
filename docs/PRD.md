---
title: Hivex purpose and product scope
status: accepted
created_at: 2026-10-01
tags: [product, scope]
updated_at: 2026-10-02
---

# Hivex product brief

This document records the owner's agreed product requirements. It owns Hivex's purpose, users, intended outcomes and scope. ADRs own consequential choices and their reasons; the [guide](guide.md) owns the installed command contract. The bundled project PRD is a starting template, not Hivex's product authority.

## Purpose and problem

Projects need a consistent way to work and retain influential decisions that code cannot explain: intent, domain language, constraints, trade-offs, conditions and exceptions. Those decisions must remain in the repository, accessible to later people and agents rather than depending on private conversation or vendor memory.

Reading every document for each task consumes context and tokens, while maintaining a second inferred interpretation can cost more than the retrieval it supports. Hivex exists to give projects a reusable workflow and focused access to reliable Markdown so implementing agents can recover settled decisions, avoid contradictions and work autonomously.

## Users and their needs

- **Agents implementing and reviewing project work** are the primary users. They need predictable CLI responses, exact source evidence, relevant dependencies and exceptions, explicit coverage, and guidance that works independently of their host model.
- **Project owners and maintainers** define intent and decisions, curate knowledge and evaluate changes. They need readable Markdown, useful history and inspectable retrieval without a separate knowledge service.
- **People working in an adopting project** need the same documentation map, conventions and applicable standards. They can read sources in an editor or use the CLI to locate passages and connections; the current interface is terminal and Markdown.

## Vision and intended outcomes

Make repository knowledge economical to find, trustworthy to inspect and practical to maintain alongside code. Reusable skills and templates establish a consistent foundation while allowing each project to retain its own product meaning, stack, scope and owner decisions.

An agent applies settled decisions autonomously. When context is insufficient, it expands through available documentation and tools before asking the owner about a consequential question that remains unresolved. Hivex assists that reasoning rather than approving it.

Success is assessed against the evidence needed for actual tasks: recover the relevant authority and its live conditions, follow indirect prerequisites and exceptions, distinguish history and partial replacement, and keep acceptance separate from implementation, verification and permission. Measure evidence recovered, context read, query latency and maintenance effort together. Reduced bytes or passing structural checks alone do not demonstrate correct implementation reasoning.

## Scope and non-goals

The accepted product scope includes:

- A project foundation covering purpose, domain language, decisions, guidelines, procedures and a concise agent entrypoint, completed from actual project evidence.
- Independent workflow capabilities for knowledge retrieval, design, documentation, implementation, review and Git, with applicable language templates. These are useful capabilities, not a compulsory sequence for every task.
- A project-local CLI for deterministic source discovery, lexical passage search, exact reading, authored relationship navigation and structural validation, with versions, bounds, continuation and historical scope visible.
- One current Markdown authority per topic and scope, reusable explicit relationships, preservation of useful history, and maintenance of affected knowledge alongside a change.

Hivex does not own the adopting project's product decisions, execution permissions or host-model configuration. It does not orchestrate development sessions, replace the responsible implementer/reviewer, infer semantic authority or guarantee that authored knowledge is complete and contradiction-free. Documentation records knowledge code cannot explain; it is not a parallel implementation manual.

The inferred graph and internal model-execution runtime are retired. Current retrieval makes no model calls and maintains no persistent knowledge database. The [runtime decision](adr/0010-practical-knowledge-assistance.md) owns the chosen retrieval/storage mechanisms and distribution constraints.

## Constraints and open decisions

Repository Markdown and Git retain durable knowledge. Compatible existing documentation layouts remain usable; recommended conventions and templates guide adoption without overwriting useful project-owned files or reclassifying historical decisions silently. Relationship syntax is deterministic, while authors and reviewers remain responsible for meaning and applicability.

The verified native distribution currently supports macOS ARM64. Its human interface is CLI JSON and readable Markdown. Additional platforms, presentation changes or retrieval accelerators require a useful need and appropriate verification; this brief grants no implementation or publication authorization for them.

The source workflow is implemented and behavior-tested. Version 0.8.0 was published on 2026-10-02; the [release record](https://github.com/H1V35/hivex/releases/tag/v0.8.0) identifies the exact verified artifact and its npm distribution. Installing a new version does not silently replace an adopting project's owner policy, and Compi migration is deferred until the owner chooses the next steps. Publication and adoption follow their own reviewed scope.

The [dated validation record](archive/validation/source-workflow-2026-10-01.md) provides bounded evidence-exposure cases and local measurements. Evaluation of complete agent tasks across projects and models remains distinct from those checks; no universal token-saving or semantic-quality claim is made.

## Relationships

- Depends on [Versioned source authority](adr/0001-versioned-project-knowledge.md): retrieval must identify inspectable sources and preserve their conditions and versions.
- Depends on [Selective documentary history](adr/0011-shared-knowledge-and-selective-history.md): replaced reasoning remains accessible without becoming current authority.
- Depends on [Project foundation](adr/0012-project-foundation-and-workflow.md): shared capabilities and conventions support consistent work without a mandatory orchestration pipeline.
