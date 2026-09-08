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
An explicit preflight admission failure before any accepted turn, with confirmed cleanup or no
observed server, remains a retained failure; it does not imply an uncertain model invocation. Other
missing acceptance evidence, unconfirmed starts/interruptions and failed cleanup block replacement.

A repeated transition to the already-current plan preserves progress. Ordinary execution then handles
only pending sources within its explicit budget. Retained negative results continue to block admission;
no automatic retry fishes for approval. Source comparison reuse and ingestion across changed source
plans remain separate requirements of the complete update workflow.

The first transition upgrades a version-1 assessment store to version 2, recording one bounded hash
of the complete previous plan and archived rows in its cohort. Repetition requires that same archive
binding; a separately initialized destination or an altered archive is rejected before any model call.
Both versions remain readable without migration, and ordinary resume preserves their current work.
Older transitions without this binding cannot retrospectively authenticate a repeated transfer;
continue their current cohort through ordinary resume. Retirement and a subsequent new cohort clear
the transition binding. Migration and replacement share the same transaction and cannot cross an
active claim.

## Explicit recovery of safe assessment failures

Under [Hivex #18](https://github.com/H1V35/hivex/issues/18), the caller may explicitly retry a
failed source review or pair comparison whose invocation ended safely: a preflight admission failure
before an accepted turn, invalid output after confirmed completion and cleanup, or an acknowledged
interruption with confirmed cleanup. Unknown turn acceptance, unconfirmed interruption, failed cleanup,
active claims and completed semantic assessments are ineligible. Adverse findings and insufficient
context require addressing their evidence; this operation never fishes for a different verdict.

Claim only the selected failure in the existing exact cohort, preserving its full previous receipt
before another invocation. The same model request and processing contract apply; recovery does not
add evidence, change the prompt or automatically repair the rejected output. Each explicit retry
starts one attempt and shares the command's unit budget. At most three attempts are allowed for a
unit, including retained earlier failures. The caller's attempt limit is the total allowance, not
an additional number of retries. Ordinary resume never retries failures.

Assessment-store format 3 retains up to two complete previous failed results separately from the
latest result. Each result keeps its 8 MiB limit, previous results together fit 16 MiB, and the whole
store remains bounded to 128 MiB with existing reservation before invocation. Migration from format
1 or 2 occurs atomically with the first valid retry, requires no active claims and preserves the
transition binding and every other row. All three formats remain readable without migration.
After a crash during retry, the active claim and prior receipts remain visible; no process-age or
elapsed-time inference makes it retryable.

Inspection and complete cohort exports expose `previousAttempts`. Totals count each current and
previous receipt once and preserve missing usage as unmeasured. Compatible graph transitions carry
that history and its remaining attempt budget. Removed or incompatible units retain their history
in the mandatory old archive. Admission uses the successful current assessments; earlier failures
are diagnostic work history, retained in the store and caller-owned complete exports, not additional
semantic evidence. Preserve those exports before explicit retirement. Retirement clears this history
with the cohort; no automatic rotation or new per-attempt files are introduced.

## Reassess a claim questioned by a later comparison

A successful fidelity review can miss a distortion later exposed by comparison. Under
[Hivex #18](https://github.com/H1V35/hivex/issues/18) and the historical evaluation in
[#19](https://github.com/H1V35/hivex/issues/19), an explicit standalone source review may include a
completed adverse comparison that questions at least one candidate claim of that source. Revalidate
the complete comparison's original graph, two distinct sources, processing contract, native receipt,
claim coverage, citations and normalized output hashes before using it as feedback. Transport failures,
successful comparisons and concerns only about another source do not qualify.

The comparison's context may be insufficient for its own question. That is a reason to reassess the
candidate's fidelity against its complete source, not evidence that revision is already justified.
Supply the source, every original candidate claim/local relation, and only the target-source concerns
and their target-source citations. The complete comparison remains in the retained receipt. Treat all
feedback as untrusted observations and explicitly allow the
reviewer to uphold the candidate. Fidelity still judges the supplied source rather than external truth,
the comparison verdict, current authority or missing prototype images.

Use an original direct comparison receipt with its original graph; reassociated receipts require
returning to their preserved original evidence. A complete receipt or single-pair inspection file
must fit 256 KiB, as must the complete source-and-concern prompt. Preserve the full original comparison
and its hash in the resulting source review, including invocation usage. The request hash binds that
receipt identity and the exact selected feedback. This mode uses short `c1`/`r1` identifiers with the
exact allowed IDs and counts in its model schema. Expand results back to the original graph IDs,
retaining and revalidating the bindings and normalized model-output hash. Other-source IDs and
comparison relationships are not extra fidelity coverage items. The ordinary fidelity model request
and schema, native profile, citation checks and invocation limits remain unchanged. Preparation makes no model call.

This is a new evidenced assessment with different inputs, not a retry or replacement of an existing
cohort row. Keep original source reviews and comparisons unchanged. A completed adverse fidelity
result can then justify [candidate revision](0004-resumable-ingestion-store.md#correct-adverse-fidelity-findings)
only under its existing sufficient-context, evidence and budget requirements. Passing or unresolved
fidelity still cannot authorize revision. Revalidate the feedback and request binding when consuming
that result. The candidate remains unaccepted and needs ordinary fresh fidelity before admission.
The caller retains the standalone evidence; no extra persistent workflow store is introduced.

When fidelity output fails validation, retain the rejected text and its hash as untrusted diagnostics.
It is not a partial review or evidence for revision. Inspection verifies its integrity and invalid-output
status; existing result/output bounds still apply. Never infer the rejected verdict from the error alone.
