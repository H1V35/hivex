---
title: Review extraction fidelity against complete versioned sources
status: accepted
date: 2026-09-08
---

# Review extraction fidelity against complete versioned sources

Before admission, review whether the extracted claims and relationships preserve their source's
meaning. Literal citation checks establish provenance, not semantic fidelity. Use the admitted
Luna/max native route with a separate structured review of the complete source and extraction.

The review assesses every supplied claim and relationship exactly once, identifies omissions and
states whether the supplied context is sufficient. It considers negation, conditions, exceptions,
scope, proposals and authored order. A source that needs another document or surrounding sections
remains unresolved until that context is reviewed. Neither IDs nor serialization order imply
precedence.

Hivex derives the result from the complete validated assessment. Missing assessments, invented
citations, contradictory fields, omissions, distorted claims and insufficient context cannot produce
a successful review. Empty extraction requires an explicit, evidenced no-knowledge assessment;
silence is not evidence of completeness. This assessment still does not admit an empty graph.

Bind the result to the exact graph hash, source revision and content, prompt/schema, admitted native
profile and measured usage. Preserve failed invocation accounting. Reject stale or oversized input
before requesting the model; never truncate a source to make the review fit. The prepare operation
exposes the exact bounded request without a model invocation. Normal execution returns one result
on stdout and creates no per-review files.

This is a source-fidelity check. It does not establish cross-source consistency, effective authority,
graph admission or implementation grounding. Those remain required under
[Compi #1631](https://github.com/H1V35/compi/issues/1631). Resumable cohort review and retention of
admission evidence must integrate the same checks rather than infer them from extraction success.
