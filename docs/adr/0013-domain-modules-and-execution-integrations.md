---
title: Domain modules for deterministic Markdown retrieval
status: accepted
created_at: 2026-09-17
updated_at: 2026-10-01
---

# Domain modules for deterministic Markdown retrieval

Hivex remains a modular Rust monolith with small concrete interfaces. The owner approved retiring the inferred graph and its execution engine under [ADR 0014](0014-reliable-markdown-and-explicit-relationships.md). The [0.7.8 domain record](../archive/runtime/domain-contract-0.7.8.md) preserves the replaced runtime boundaries and accounting guarantees.

## Module ownership

Documents own source selection, Markdown parsing, exact source coordinates, lexical search and authored relationships. SQLite search belongs beside that retrieval responsibility and runs in memory. Foundation owns safe preparation of missing project files and skill links. CLI modules own arguments, bounded responses, continuation and process diagnostics. Shared error and compatibility utilities support those domains. Documents and foundation do not depend on CLI or each other; the entry boundary composes them.

Keep submodules private and expose only operations/types consumers need. Reuse existing helpers and installed dependencies before adding an abstraction. There is no provider framework, model invocation, graph store, execution orchestrator or persisted query cache in the current runtime.

## Historical data boundary

Current commands do not open, rewrite or delete graph SQLite, snapshots, attempts or caches. Preserve them with their exact identities and actual status before retiring integration; earlier failure and unknown consumption remain historical facts. A preserved legacy package/Git checkpoint supports a concrete historical investigation without retaining the old engine in current code.

Source versions identify exact working-copy text; snapshot identities bind source selection and continuations. A changed source or option invalidates its continuation. Neither a version hash nor a valid relationship grants semantic approval or execution permission.

## Verification

Protect current observable boundaries: source scope and exact ranges, deterministic search/navigation, bounded responses, argument errors, safe initialization and preservation of retained file bytes. Retire tests and dependencies exclusive to removed behavior; test count is not an acceptance goal. The architecture gate follows only current domain declarations.

<a id="explicit-profile-continuity-148"></a>
## Historical profile continuity

The previous profile-resumption contract remains in the [retired domain record](../archive/runtime/domain-contract-0.7.8.md#explicit-profile-continuity-148). The current CLI has no model profile or operation to resume; removing the engine does not relabel or reset its recorded work.

## Relationships

- Implements [Source-first product contract](0010-practical-knowledge-assistance.md#runtime-and-retrieval): runtime domains own deterministic source operations rather than derived interpretation.
- Depends on [Reliable Markdown](0014-reliable-markdown-and-explicit-relationships.md): authored knowledge and scoped review govern the replacement.
