---
title: Resumable candidate ingestion in a bounded local store
status: accepted
date: 2026-09-08
---

# Resumable candidate ingestion in a bounded local store

## Decision

An ingestion run owns one immutable plan: source commit, collection, configuration, preparation
policy and complete source identities. Omitted selection arguments resume the recorded plan even
after HEAD advances. An explicit different selection fails before a model invocation. Sources
remain in Git and their existing documentation folders; the store is never documentary authority.

Use one local SQLite file, normally `.hivex/ingestion.sqlite`, excluded from Git. Store a single
cohort and its bounded attempt reports and unaccepted candidates, without files per source or turn.
The current format permits at most 2,048 sources, an 8 MiB plan, an 8 MiB result per source and a
128 MiB database. The first delivered store format is version 2, including the full cohort checksum.
Version 3 adds bounded semantic revision history. It reads valid version-2 cohorts intact and upgrades
the format marker on their next mutation; their frozen plans, candidates and reports remain unchanged.
Version 4 adds one bounded transition binding for explicit reuse. It reads valid version-2 and
version-3 cohorts intact and upgrades the format marker only after a validated transition or other
successful mutation; the binding records the complete old plan and rows, not a copy of either inside
each source result.
Experimental version-1 stores are rejected intact before any mutation or model invocation. Do not
silently bless their unchecked metadata by generating a new checksum or relabelling their format.
Reserve capacity before starting work and fail explicitly when capacity runs out.
Use local storage, not a network filesystem. A short-lived SQLite rollback journal belongs to the
same store; it is removed by normal transaction completion or recovered after interruption.

Claim a pending source in a short immediate transaction before invoking the native model. Run the
model outside the transaction and commit the result atomically only for the claimant. Concurrent
CLI processes may work on distinct sources; they cannot reclaim completed or already claimed work.
Record each attempt before invocation and its report when available, including known consumption
on failure. A crash or failed persistence leaves an explicit unresolved invocation. Neither elapsed
time nor a missing process proves that no model request occurred, so such work is never retried
automatically. A completed candidate is still not an admitted graph or a successful grounding.

Explicitly retry a retained failed source only when the previous invocation ended safely: a failed
admission without an accepted turn, an invalid output after confirmed completion/cleanup, or an
acknowledged interruption with confirmed cleanup. Unconfirmed starts, failed cleanup, successful
candidates and unresolved claims cannot be retried through this operation. Repeating ordinary
ingestion never implicitly retries failures.

Retain previous attempt reports and consumption when retrying; do not reset the attempt count or
re-extract completed sources. The caller's attempt limit is a total budget for the current extraction
round, including earlier invocations in that round, with at most three attempts. Atomically claim the failed source
and reserve result capacity before requesting the model. Retry uses the original frozen source;
only an invalid extraction supplies correction feedback, not a failure to start or finish a turn.

## Reuse compatible extraction after a snapshot changes

`ingest --export` is a read-only complete export of the frozen plan, every source row and its retained
result or checkpoint. The caller owns that export. Ordinary ingestion remains frozen and never uses a
new `HEAD` implicitly. An explicit transition uses the same store with
`ingest --reuse <export> --ref <new-commit> --max-units 0`; the destination revision and zero model
budget are required so the transition is a reviewable checkpoint before new extraction begins.

Before replacement, validate the archive against the complete current store, including each row's
state, result, attempts, revision history, checkpoints and usage. A running row, active checkpoint or
uncertain invocation blocks replacement. The transition is immediate and atomic: a failed archive or
capacity check leaves the old cohort byte-for-byte intact. A destination that is already initialized
accepts repetition only when its recorded transition binding matches the same complete old archive;
an altered archive is rejected even when the destination plan is already present.

Reuse compares the complete processing contract and complete source unit: identifier, path, collection,
content hash, section, authority, prompt hash and limits. A changed or new unit becomes pending. A
compatible candidate or safely retained failed result is copied with its attempts, revision history,
usage and budgets unchanged; no failure is retried automatically. The new result carries only a
binding to the current plan and snapshot plus the hash of the original result. The original receipt is
not rewritten, and the old plan is not copied into every result. Store reads and graph assembly both
revalidate the unit/contract relationship and the association; graph receipt hashes still cover the
complete current record. A later `ingest --revise` therefore starts a new extraction origin while
preserving the reused candidate's history and remaining budgets.

## Correct adverse fidelity findings

A structurally valid candidate can omit or distort knowledge. Explicit semantic revision under
[Hivex #18](https://github.com/H1V35/hivex/issues/18) requires the exact retained candidate and its
source-bound graph, plus a validated adverse fidelity result with sufficient context. The review must
retain a completed admitted invocation, literal evidence and complete claim/relation coverage. A
passing review, an unresolved finding, insufficient context or an uncertain invocation cannot trigger
revision. A cohort export may supply the selected source result; it does not approve other rows.
The graph's source snapshot and complete source descriptor, including authority and processing inputs,
must match the frozen ingestion plan. Resolve the model's source from that plan; candidate and receipt
hashes alone cannot establish which documentary revision the invocation may read.

Prepare the complete request before claiming work. It includes the original source, prior candidate,
claim/relation identities and review findings, all treated as untrusted evidence. Source Markdown
remains authority; the model must not invent a rule to satisfy a reviewer. The request is limited to
256 KiB without truncation. A preparation-only command exposes it without mutating state or calling
the model.

Revise only the selected source in the existing cohort. Preserve its earlier candidate, original
result hash, full negative review, request and all attempt reports in the same checkpoint. Each
revision begins a new round of at most three attempts. Three revisions remain the default; an
explicit `ingest --revise --max-revisions 4` (value 1–4) authorizes a fourth and the history reads
all four entries. The source still has an absolute limit of twelve extraction invocations across
its lifetime in this cohort. These limits do not reset after a failure or restart. Check the global
budget before claiming the source and clamp the round before invoking the model, so exhaustion
cannot leave a running claim or create attempt 13. Existing size/reservation limits also apply to
this history. Failed or unchanged replacement output leaves an explicit failed source; an unchanged
candidate is not sent through another fidelity review. A safe explicit retry preserves the revision
request, its origin and prior consumption.
The retained review's usage remains in its original receipt; ingestion attempt totals count extraction
calls only, so copying review evidence does not charge it as a new invocation.

A standalone [fidelity reassessment with comparison feedback](0006-source-fidelity-review.md#reassess-a-claim-questioned-by-a-later-comparison)
may supply that adverse evidence. Revalidate its complete original comparison and prepared request
before revision. The comparison itself never authorizes a candidate change; the resulting fidelity
must independently satisfy the same sufficient-context and concrete omission/distortion requirements.

A successful replacement is still a candidate and needs fresh fidelity review before admission.
Unrelated source candidates remain intact. Reassociation of compatible source reviews uses the explicit
[source-review operation](0006-source-fidelity-review.md#reuse-unchanged-source-evidence); ingestion
revision itself does not relabel existing assessments as current.

## Lifecycle

Read-only retrieval continues to create no artifacts. Ingestion is an explicitly mutating command.
Its store remains until completion/admission evidence has been exported or retained by the caller
and the caller explicitly discards that cohort. Do not silently rotate stores, delete uncertain
attempts or create a new store for every invocation. Reject unrelated, unsupported, altered or
oversized stores before supplying source text to a model. The CLI must expose progress and retained
reports without requiring callers to interpret internal SQLite tables.

This decision governs implementation under [Compi #1422](https://github.com/H1V35/compi/issues/1422)
and [#1631](https://github.com/H1V35/compi/issues/1631). Graph assembly, admission, historical evidence
and implementation grounding remain separate requirements; this store alone does not deliver them.

## References

- [Bun SQLite transactions](https://bun.com/docs/runtime/sqlite#transactions)
- [SQLite transaction isolation](https://www.sqlite.org/lang_transaction.html)
- [SQLite page-count limits](https://www.sqlite.org/pragma.html#pragma_max_page_count)
