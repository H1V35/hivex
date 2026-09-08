---
title: Admit graph projections with complete review evidence
status: accepted
date: 2026-09-08
---

# Admit graph projections with complete review evidence

Admission compares against current `HEAD`; `--against` belongs only to historical inspection.
Admission combines an exact fresh candidate graph with its complete successful source-fidelity
cohort and complete successful comparison cohort. Every source and selected pair must be covered
once under the recorded processing contract. Pending, interrupted, negative, malformed, stale or
oversized evidence cannot become acceptance. An empty graph is not admissible. A single claim-bearing source requires source fidelity but no
invented cross-source comparison. Explicit unresolved
contradictions and precedence cycles prevent automatic admission.

The admitted snapshot contains the original candidate, retained source assessments, deterministic
comparison selection, retained comparisons and the derived cross-source relationships. Preserve all
claims: a scoped amendment does not delete a whole claim or document. Relationships retain their
conditions, exceptions, affected scope and evidence from both endpoints. A content hash binds the
complete snapshot and serves as the admission manifest identity.

Admission is deterministic and makes no model calls. It exports one complete artifact on request;
normal checking and querying create no files. The caller retains the artifact and commits it with the
project's knowledge configuration so Git preserves its previous versions. Hivex does not create a
new file per claim, comparison, admission attempt or query. The full snapshot must fit 256 MiB and
the requested output budget; failure never yields a partial accepted artifact. The embedded candidate
still must fit 64 MiB, and each embedded assessment retains its original 8 MiB limit. Revalidation
applies those component limits in addition to the outer artifact limit.

Optional authored-link fields are omitted when absent, so a complete exported selection compares
equally with the reconstructed in-memory selection. JSON round-trips must preserve admission without
changing its hashes or requiring model work again.

Reused source assessments retain their complete original result and an explicit association with
this graph. Validate both the current binding and equality of the original source-review inputs,
preserving the original prompt/graph identities and invocation usage. Reuse is not a new model call.

Reused pair comparisons similarly retain their original receipt and selection identity. Validate
their association to the current graph and comparison selection, plus equality of complete pair
inputs and source/claim bindings. Selection changes never extend an assessment beyond its two sources.

Every read revalidates integrity, source evidence and the embedded semantic assessments, rather than
trusting an `accepted` flag or checksum alone. Freshness is evaluated against the requested project
revision. A historical snapshot can be inspected against its original revision; changed documentary
or processing inputs prevent it from serving as current acceptance evidence. Unrelated code changes
do not by themselves make the documentation graph stale.

Admission establishes the recorded source-fidelity and comparison coverage, not exhaustive semantic
consistency across all possible source pairs. Its manifest exposes the exact selection policy and
limits. Grounding must assess applicable decisions with their conditions and amendments against the
exact implementation; admission alone does not approve code or decide unresolved doctrine. Preserve
and evaluate the Decision/amendment regression in Compi #1478 before declaring that cycle complete.
