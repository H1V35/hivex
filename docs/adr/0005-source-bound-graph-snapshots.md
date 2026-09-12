---
title: Source-bound graph snapshots before admission
status: accepted
date: 2026-09-08
---

# Source-bound graph snapshots before admission

Build graph snapshots from a complete, verified ingestion cohort. Keep source declarations, claims, relationships and extraction evidence distinct. Claims and edges have content-derived identities; serialization order and model-local IDs never establish precedence. Preserve literal citations and their original source revision. Neither a successful extraction nor assembly establishes currentness or admits a graph.

Assembly is deterministic and model-free. Its default response is a bounded summary; an explicit export emits the complete candidate snapshot for inspection or later admission. It does not replace the accepted graph. Candidate snapshots carry a versioned format and an integrity hash; readers reject altered, unsupported or incomplete data before presenting it as graph content.

Freshness compares declared source and processing inputs, independently of unrelated code commits. A source or configuration change remains visible as stale evidence. Historical provenance is not rewritten to make a candidate look current. A later admission operation must bind its checks to the exact snapshot and preserve the previous accepted graph on failure.

Full reconstruction with Luna/max, semantic evaluation, admission and implementation grounding remain requirements of the graph-admission workflow. This decision defines the projection they operate on; it does not declare those gates delivered.
