---
title: Versioned sources before generated knowledge
status: accepted
date: 2026-09-05
---

# Versioned sources before generated knowledge

## Context

The owner authorized a reusable hivex product focused on project knowledge, with Codex, Git/GitHub
and CI handling development coordination. The source-retrieval experiments found that lexical
ranking can locate useful documents while small excerpts still omit conditions. A compact extraction
can preserve facts in descriptions while losing individual relation addressability. See Compi's
[#1597](https://github.com/H1V35/compi/issues/1597), [#1631](https://github.com/H1V35/compi/issues/1631)
and [#1423](https://github.com/H1V35/compi/issues/1423).

## Decision

Hivex begins with versioned source search and expansion. Authored documentation and its accepted
history retain authority. Code, contracts and tests explain executable behavior; documentation
records intent, language, decisions and constraints that execution cannot explain.

Search and read bind to the same immutable revision. An index is disposable derived data; the first
implementation builds it in memory for each search. A read returns complete Markdown blocks or an
explicit continuation/error, rather than silently removing a condition to meet an output budget.
A search preview never certifies that the evidence is sufficient to answer.

Status is a declaration, not a semantic verdict. Hivex exposes declared status and replacement
references, and reports effective currentness as not established. It does not infer that an Accepted
frontmatter value resolves all amendments, exceptions or conflicts. Legacy and evidence collections
are opt-in for retrieval; excluding them from a query does not revoke their authority.

The product uses TypeScript and Bun, with modules grouped by sources, snapshots and retrieval. Its
runtime imports no Compi product or legacy orchestration code. The current monorepo package manager
is transitional; the owner's final Bun package-manager preference remains tracked by #1162.

## Consequences

A project adopts the CLI through a small versioned source configuration. Hivex does not create
private agent memory, a hosted documentation database, or another session orchestrator. Its own
language and decisions live under `hivex/docs/`, separately from Compi's domain documentation.

Physical source moves and removal of old procedures require consumer, link and identity checks.
The accepted Opus graph remains intact until a replacement demonstrates the capabilities it replaces.
Native extraction/grounding, full supersession composition, broader semantic evaluation and historical
graph queries are subsequent capabilities, not claims made by this first source interface.
