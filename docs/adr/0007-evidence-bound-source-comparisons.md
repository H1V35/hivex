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

## Plan comparisons from authored Markdown links

Discover candidate source pairs deterministically from parsed Markdown links in the same graph
snapshot. Inline and reference-style links retain their original link evidence; reference definitions
retain their own original document positions, including definitions outside a selected section.
Only links located inside the selected section participate. CommonMark identifier normalization and
first-definition precedence come from the existing Markdown parser ecosystem.

A link suggests a comparison, not a semantic relationship or precedence. Collapse both directions
into one source pair while retaining each authored reason. Resolve relative paths within the
repository and target headings within the selected graph sources. Missing targets, unsupported paths,
missing anchors and pairs without claims remain explicit. External destinations and non-Markdown
assets are counted without being opened. Code examples, images and raw HTML are not scanned for
Markdown relationships.

The plan records its graph hash, policy, provenance, selected pairs, unresolved links and the number
of possible pairs. It covers authored Markdown links only; absent links do not prove independence,
and this policy does not establish full semantic-neighborhood coverage or graph admission.

Bound discovery to 10,000 link definitions/links per document, 10,000 inspected links per plan,
32,768 pairs and an 8 MiB working/output budget. A requested smaller output budget either returns
the complete plan or fails explicitly. Planning makes no model calls, modifies no graph and creates
no per-plan files. A caller may retain the complete result when it is needed for later review.

## Retain and resume the selected comparison cohort

Execute the authored-link selection through the same comparison contract, with one local
`.hivex/comparisons.sqlite` working store. Bind it to the graph, the complete selection hash and every
pair's prompt and processing contract. Recompute and validate the selection before execution or
inspection. An empty selection or unresolved link plan cannot become a successful cohort.

Source-fidelity and pair-comparison cohorts share the atomic claim, capacity reservation and retention
implementation. Their database application identities remain distinct; opening one as the other fails.
The existing source-review format remains compatible. A comparison cohort supports at most 2,048
pairs and a 1 MiB processing plan; the authored discovery plan retains its separate 8 MiB limit.
The database is bounded to 128 MiB, each complete result to 8 MiB, and each active/new claim reserves
16 MiB before a model request. Source texts are loaded once per command; complete pair prompts are
prepared before opening the store, without retaining all prompts in memory.

Resume pending pairs without repeating retained results. Stop the current invocation loop after a
negative or failed result. A later command may process other pending pairs, but does not retry or
approve the negative result. Unknown outcomes remain unresolved. Inspection/export revalidates
coverage, original citations, source bindings, expanded/normalized comparison hashes and completed
native invocation evidence. A complete export must fit the requested budget; it is never truncated.

Retirement requires the exact processing-plan hash and rejects unresolved invocations. Preserve needed
evidence before retiring: it clears the retained comparisons transactionally and allows reuse of the
same bounded file. There is no automatic rotation, per-pair file tree or implicit retry. This working
cohort still covers only the selected authored-link pairs; completing it does not establish full
semantic coverage, graph admission or implementation grounding.
