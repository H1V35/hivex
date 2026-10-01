---
title: Source-bound graph snapshots before admission
status: accepted
date: 2026-09-08
---

> Historical record of the document at commit `1e3768f65f901522a63e00bde51069020049e123`, before the Markdown-first decision on 2026-10-01. Original decisions, dates and wording are retained; relative links were adjusted for this location. This archive is not current policy. See the [current authority](../../adr/0005-source-bound-graph-snapshots.md) and [the current approach](../../adr/0014-reliable-markdown-and-explicit-relationships.md).

# Source-bound graph snapshots before admission

Build graph snapshots from a complete, verified ingestion cohort. Keep source declarations, claims, relationships and extraction evidence distinct. Claims and edges have content-derived identities; serialization order and model-local IDs never establish precedence. Preserve literal citations and their original source revision. Neither a successful extraction nor assembly establishes currentness or admits a graph.

Assembly is deterministic and model-free. Its default response is a bounded summary; an explicit export emits the complete candidate snapshot for inspection or later admission. It does not replace the accepted graph. Candidate snapshots carry a versioned format and an integrity hash; readers reject altered, unsupported or incomplete data before presenting it as graph content.

Freshness compares declared source and processing inputs, independently of unrelated code commits. A source or configuration change remains visible as stale evidence. Historical provenance is not rewritten to make a candidate look current. A later admission operation must bind its checks to the exact snapshot and preserve the previous accepted graph on failure.

Full reconstruction with Luna/max, semantic evaluation, admission and implementation grounding remain requirements of the graph-admission workflow. This decision defines the projection they operate on; it does not declare those gates delivered.
