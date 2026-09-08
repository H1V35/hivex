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
