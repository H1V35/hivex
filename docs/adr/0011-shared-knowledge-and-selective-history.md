---
title: Selective documentary history and preserved evidence
status: accepted
created_at: 2026-09-11
tags: [adr, documentation, history]
updated_at: 2026-10-02
---

# Selective documentary history and preserved evidence

The owner accepted two changes while reviewing the project under [Hivex #17](https://github.com/H1V35/hivex/issues/17). Long ADRs mix current rules with replaced text, increasing ingestion cost and ambiguity. Keeping the derived graph only in a local store also makes each clone pay to rebuild knowledge and leaves no shared history of the interpretations used. Markdown remains authoritative; versioning a graph does not make its interpretations correct or its sources current.

## Compact current decisions, preserve accessible history

An author may move replaced decision text into a clearly historical Markdown archive, preserving the original text, dates and provenance. The current ADR retains the applicable decision, reasons, dependencies and exceptions, with links to its history and replacements. A fully archived ADR is removed from the current directory. Update necessary callers to its archive or live replacement instead of keeping a duplicate pointer. Preserve referenced anchors or update their callers; age alone never makes a still-applicable condition obsolete.

`docs/archive/adr/` is one valid convention, not a required layout for every project. Historical sources remain explicitly available for bounded retrieval when needed. They do not enter ordinary search simply because they are accessible. Hivex must respect the project's declared source scope and report unavailable necessary evidence rather than silently omitting a dependency or treating an old rule as current. The author performs documentary compaction; Hivex does not rewrite project decisions automatically.

## Share sources and retain historical execution evidence

Commit maintained Markdown, explicit relationships and source history together. A fresh clone can retrieve them directly, without rebuilding a graph. `archive` globs mark historical sources for explicit reading/search; the `history` field remains a compatible configuration alias and both cannot be specified together. Exclusions still govern source selection.

The shared graph and local SQLite execution store belong to the retired runtime. Preserve necessary snapshots, attempts, caches, budgets and provenance in a bounded private archive outside the active worktree before retiring project integration. Do not erase failure or unknown usage, rewrite results, or reset a work item. Source-only commands never open or mutate those files. Git retains earlier shared snapshots; no active import/export or relocation engine remains. The [historical graph-sharing decision](../archive/runtime/history-contract-0.7.8.md) and [0.7.8 guide](../archive/runtime/guide-0.7.8.md) preserve the old operational contract.

When moving or consolidating Markdown, preserve useful dates and provenance, maintain caller links and anchors, and review affected current meaning. The author performs semantic migration; Hivex does not rewrite decisions automatically. A source move changes query snapshots/cursors and must not make an old excerpt appear current.

## Relationships

- Depends on [Versioned source authority](0001-versioned-project-knowledge.md): historical evidence is distinguished from current decisions.

## Application and qualification

The [Markdown convention](../../skills/hivex/references/markdown.md#compact-an-adr-without-losing-its-history) and [history catalogue](../archive/README.md) apply the archival contract. [ADR 0014](0014-reliable-markdown-and-explicit-relationships.md) retires inferred graph sharing: the Markdown base is committed, while old snapshots and local execution evidence retain their historical meaning.
