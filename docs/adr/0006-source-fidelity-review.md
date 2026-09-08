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
scope, proposals and authored order. Context sufficiency concerns fidelity to the supplied source. A represented link or prerequisite to
consult another document does not by itself require that document in this phase; assess whether the
reference is faithfully preserved, without inventing the target content. Missing context remains
unresolved when it prevents assessing that fidelity. Cross-source comparison determines relationships
and applicable exception scope; source fidelity must not silently take over that separate decision. Neither IDs nor serialization order imply
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

## Resumable cohort review

Review a graph cohort through the same source-fidelity contract, retaining results in one local
SQLite file, normally `.hivex/reviews.sqlite`, excluded from Git. This working store is separate
from candidate ingestion: it binds reviews to a complete graph hash and review processing contract.
It is not an accepted graph or documentary authority. Do not create a file for each source or call.

Prepare every complete source request before opening the store or invoking the model. The plan is
limited to 1 MiB and 2,048 sources, each retained result to 8 MiB, and the database to 128 MiB. Reserve
16 MiB of database capacity for each claimed or newly requested source, including unresolved claims.
Use local storage and a SQLite rollback
journal; no network database is introduced. Data pages freed by retirement may be reused by the next
cohort rather than growing a new file.

Claim a pending source atomically before calling the model, outside the database transaction, and
retain its complete result only for the recorded claimant. Resume pending sources without repeating
finished work. An interrupted invocation remains unresolved and cannot be automatically retried or
discarded. Retained negative assessments remain negative; re-running the command cannot turn them
into approval. Failed calls retain reported consumption, and unknown consumption remains explicit.

Inspection and complete export are read-only. They revalidate stored assessments, their checksums,
source citations and coverage before returning them; a complete result must fit the caller's budget.
The caller exports or preserves needed evidence before explicitly retiring a cohort with its exact
plan hash. Retirement rejects unresolved claims and clears retained reviews transactionally. The
same file can then serve a new cohort; no automatic rotation or retention by wall-clock age occurs.

Ordinary cohort resume requires one exact graph. Reuse across graph snapshots requires the explicit
operation below; a different graph or processing contract is otherwise rejected. Full cohort source
fidelity still does not establish cross-source consistency, effective authority, graph admission or
implementation grounding.

For evidenced omissions or distortions with sufficient context, the
[ingestion revision operation](0004-resumable-ingestion-store.md#correct-adverse-fidelity-findings)
can produce a replacement candidate while preserving this negative review. Revision does not change
its verdict, erase usage or admit the replacement. Unresolved context requires resolving the missing
inputs, not repeatedly requesting a more favorable assessment.

## Reuse unchanged source evidence

A change to one source changes the graph hash without necessarily changing another source's fidelity
inputs. Under [Hivex #18](https://github.com/H1V35/hivex/issues/18), reuse a retained assessment only
when the complete source-review request is identical except for that graph-hash marker. This includes
the original Markdown, source identity, authority, section/line information, claims, relationships,
their order and the review instructions. The admitted model, native policy and schema must remain
compatible. A matching document hash alone is insufficient.

Preserve the original review, graph and prompt identities, verdict, invocation report and usage.
Add an explicit association with the current graph, source snapshot and prepared prompt; its integrity
hash binds the complete original result. Admission and every subsequent read revalidate both the
association and the original request's semantic inputs. Repeated associations retain that same original
result rather than nesting copies of earlier associations. Reuse makes no model call and does not
turn an adverse or insufficient assessment into a successful one.

Before replacing a working review cohort, the caller preserves its previous candidate graph and a
complete review export. Validate the archive against the old plan and actual retained rows; reject
missing or altered results and any unresolved claimant. Replace the cohort transactionally in the same
bounded SQLite file, reusing compatible results and leaving changed/new sources pending. Removed
sources remain in the caller's archive. Existing plan, result and database size limits still apply.
The archive's lifecycle is caller-owned; this operation does not create an automatic history tree.

A repeated transition to the already-current plan preserves progress. Ordinary execution then handles
only pending sources within its explicit budget. Retained negative results continue to block admission;
no automatic retry fishes for approval. Source comparison reuse and ingestion across changed source
plans remain separate requirements of the complete update workflow.
