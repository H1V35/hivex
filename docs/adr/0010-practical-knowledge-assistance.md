---
title: Practical project knowledge through deterministic sources
status: accepted
created_at: 2026-09-09
updated_at: 2026-10-02
tags: [adr, retrieval, runtime]
---

# Practical project knowledge through deterministic sources

This decision owns Hivex's retrieval/runtime and distribution choices. The [product brief](../PRD.md) owns purpose, users, intended outcomes and scope. [ADR 0014](0014-reliable-markdown-and-explicit-relationships.md) records why reliable Markdown and authored navigation replaced the inferred graph runtime.

<a id="runtime-and-compatibility"></a>
## Runtime and retrieval

The Rust CLI uses deterministic source discovery, lexical passage search, authored relationship navigation and exact reading. SQLite FTS5/BM25 runs in memory per search and remains useful without a persistent knowledge database. No command invokes a model, requires a provider profile or depends on a shared graph. Keep results bounded and report omitted sources, unread continuation and historical scope honestly. Lexical relevance and valid links do not establish semantic applicability.

Keep lexical ranking units separate from context recovery. Whole-section indexing changed the ordering adversely in a bounded repository sample; retaining the original windows preserves discovery while structural expansion recovers qualifications beyond a window boundary. An expanded range contains its original match and can cover adjacent sections. Limit automatic expansion, expose larger ranges for focused reading and reuse overlapping evidence. Structural completeness of that range does not establish semantic completeness of a task.

Retired graph/model operations return a migration diagnostic rather than executing. Version 0.8.0 deliberately changes the `search` result contract and removes those operations. Preserve historical SQLite/snapshot files, failed candidates, attempts, receipts, uncertainty and consumption unchanged. The [retired command record](../archive/runtime/graph-cli-0.7.8.md) and [original decision](../archive/adr/0010-practical-knowledge-assistance.md) retain their prior meaning; do not carry the old engine into the current package solely to interpret history.

## Distribution and verification

The package is `@h1v35/hivex`, command `hivex`, license MIT. Retain the six workflow/knowledge skills and optional language templates. The verified native target is macOS ARM64, with a direct executable and no JavaScript launcher, runtime installation hook or model integration. Additional targets require their own behavior and artifact verification.

Cargo and the Rust development tool prepare and verify the exact npm archive using system `tar`. Registry installation/publication follows the [release procedure](../procedures/releasing.md); installing Hivex does not replace an adopting project's owner policy.

Validate observable retrieval, failures, source changes, history and preservation at useful boundaries. Measure relevant evidence recovered, context read, latency and maintenance effort together. Include indirect dependency, exception, conflict, insufficient evidence and source-change cases; do not substitute identity with an old model graph or an exhaustive replay requirement.

## Relationships

- Implements [Product requirements](../PRD.md#scope-and-non-goals): the source-only runtime and distribution choices carry out the accepted product scope.
- Depends on [Versioned authority](0001-versioned-project-knowledge.md): exact source evidence governs decisions and their applicability.
- Depends on [Selective history](0011-shared-knowledge-and-selective-history.md): historical reasoning remains available with current scope identified.
- Depends on [Markdown navigation](0014-reliable-markdown-and-explicit-relationships.md): deterministic retrieval and explicit links replace graph inference.
- Supersedes [Retired graph runtime](../archive/runtime/graph-cli-0.7.8.md): removes model-assisted commands while preserving historical evidence and accounting.
