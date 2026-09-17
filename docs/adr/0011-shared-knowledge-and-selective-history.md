---
title: Shared knowledge snapshots and selective documentary history
status: accepted
date: 2026-09-11
---

# Shared knowledge snapshots and selective documentary history

The owner accepted two changes while reviewing the project under [Hivex #17](https://github.com/H1V35/hivex/issues/17). Long ADRs mix current rules with replaced text, increasing ingestion cost and ambiguity. Keeping the derived graph only in a local store also makes each clone pay to rebuild knowledge and leaves no shared history of the interpretations used. Markdown remains authoritative; versioning a graph does not make its interpretations correct or its sources current.

## Compact current decisions, preserve accessible history

An author may move replaced decision text into a clearly historical Markdown archive, preserving the original text, dates and provenance. The current ADR retains the applicable decision, reasons, dependencies and exceptions, with links to its history and replacements. A completely superseded ADR may become a short pointer at its original path. Preserve referenced anchors or update their callers; age alone never makes a still-applicable condition obsolete.

`docs/archive/adr/` is one valid convention, not a required layout for every project. Historical sources remain explicitly available for bounded retrieval when needed. They do not become the normal ingestion backlog or enter every consultation simply because they are accessible. Hivex must respect the project's declared source scope and report unavailable necessary evidence rather than silently omitting a dependency or treating an old rule as current. The author performs documentary compaction; Hivex does not rewrite project decisions automatically.

## Share derived knowledge, keep execution state local

The shared artifact is `.hivex/graph.json`: a portable knowledge snapshot suitable for Git, containing the reusable graph, source versions, evidence, provenance and coverage/uncertainty information. It allows inspection of what changed and reuse in another clone without regenerating an identical model output. Preserve known provenance and identify unavailable legacy metadata; do not invent it.

`.hivex/knowledge.sqlite` remains the local working store for queries, incremental work, attempts, budgets and caches. `.hivex/knowledge.lock` remains local. The shared artifact does not carry process identities, locks, pending invocations or the model-response cache. Loading it must not overwrite unfinished work, erase failed attempts or reset an existing work budget.

Use deterministic structural and source-version checks to distinguish reusable knowledge from stale or unavailable parts. A fresh clone should reuse matching knowledge; changed sources require only the necessary update. A snapshot may retain explicit partial or uncertain coverage. Serializing the same graph should produce a stable diff; normal read-only queries should not dirty the shared file. Snapshot operations must not trigger hidden model calls or source edits.

This extends ADR 0010's portable second-brain contract and supersedes a blanket recommendation to ignore every artifact under `.hivex/`. The CLI supports explicit snapshot export/import and reuses the snapshot in a fresh clone. Configurable `archive` globs keep historical sources available for focused retrieval while excluding them from ordinary ingestion. The former `history` configuration field remains a read-compatible alias; specifying both is invalid. The rename preserves source identities, work accounting and retained caches. A saved snapshot alone does not complete a project's rollout; publication and process retirement follow the project's normal workflow.

Delivery is tracked by [#53](https://github.com/H1V35/hivex/issues/53) (shared graph snapshot) and [#54](https://github.com/H1V35/hivex/issues/54) (selective historical sources), under [the approved extension to #17](https://github.com/H1V35/hivex/issues/17#issuecomment-5631587186). The CLI remains the existing behavioral test boundary for both independent deliveries.

## Source relocation — 2026-09-13 (#64)

An explicit source relocation keeps knowledge usable when documentation changes location. It preserves decision and relationship identities, source versions, uncertainty and existing work history. An identical move can reuse ingestion coverage without a model call. Changed content or consolidation requires the normal update/check at the destination; relocation does not approve that interpretation or make mismatched evidence current. Refuse relocation while local work is unfinished, preserve original work and model answers, and record the source/destination and versions in the operation report. Commit the exported graph with the corresponding source changes so their history remains auditable.
