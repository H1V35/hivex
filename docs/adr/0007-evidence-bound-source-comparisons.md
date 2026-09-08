---
title: Evidence-bound relationships between complete sources
status: accepted
date: 2026-09-08
---

# Evidence-bound relationships between complete sources

Compare a selected pair of complete versioned sources to propose relationships between their claims.
Use the admitted native Luna/max route and retain both source texts, their candidate statements and
source-local relationships as input. The same source cannot occupy both sides; both need extracted
claims. Missing or oversized input fails before a model invocation rather than becoming an empty or
truncated comparison.

Every supplied claim needs one assessment, even when unrelated to the other source. Relationships
connect claims from different supplied sources and carry literal evidence from both endpoints.
Both endpoint assessments must reference the relationship. Conditions, exceptions and the affected
claim scope remain explicit. A partial exception must not become whole-claim or whole-document
supersession. Dates, status labels and identifier order alone do not establish precedence.

The model receives short, source-qualified claim identifiers and source aliases. The driver binds
them deterministically to the original graph IDs and source identities, rejecting references outside
the supplied pair. Preserve the source bindings and hashes of both the normalized model output and
the expanded comparison. This saves repeated opaque hashes in model requests and responses without
discarding external provenance. Pair serialization is canonical for reproducibility and caching;
it is not authored order. Original source positions remain available for documentary order.

Derive the result from the full validated assessment, rather than a model's global verdict. Reported
contradictions, unknown scope, incomplete coverage, unresolved claims and insufficient context prevent
success. Keep valid negative findings available with their evidence. Malformed output or invented
citations fail explicitly and preserve invocation accounting. A comparison does not decide doctrine
or invent a resolution on the owner's behalf.

The prepare operation shows the exact bounded request without invoking the model. Execution returns
one result on stdout, with no per-comparison files or graph mutation. The current request limit is
256 KiB, covering both complete sources and their extracted context.

This operation covers the selected pair only and always returns `accepted: false`. It does not
establish graph-wide relationship coverage, effective authority, admission or code grounding. Those
remain required under [Compi #1631](https://github.com/H1V35/compi/issues/1631), including the historical
composition and atomic-evaluation obligations in #1426/#1427/#1478.
