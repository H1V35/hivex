---
title: Domain modules and replaceable execution integrations
status: accepted
date: 2026-09-17
---

# Domain modules and replaceable execution integrations

The owner approved [#101](https://github.com/H1V35/hivex/issues/101) after completing the Rust migration. Organize Hivex as a modular monolith around its language and invariants. Directory names alone do not establish a domain boundary; each capability owns meaningful behavior and exposes a small interface. Keep one product without mandatory hexagonal layers or a second orchestration runtime.

Documents own source discovery, parsing and source coordinates. Knowledge owns the derived graph, retrieval, ingestion, relationships, warnings, repairs and portable snapshots. Work owns progress, attempts, budgets, state transitions and recovery. Consultation and review compose those capabilities for their respective tasks. CLI parsing and process exit/output belong at the entry boundary. Concrete SQLite persistence remains beside the behavior it serves; no repository interface is required for every entity.

The executing integration, model provider, model name and reasoning/options are separate choices. The current default is Codex with OpenAI `gpt-6-luna` and effort `max`, replacing `gpt-5.6-luna` at the owner's request on 2026-09-23 in [#122](https://github.com/H1V35/hivex/issues/122). The domain does not require those names or Codex's app-server, account, flags, thread or turn protocol. Integrations validate their requested effective profiles and return execution results, honest usage and lifecycle evidence. Unsupported choices fail explicitly; no fallback silently changes model or provider. A synthetic integration demonstrates substitution without claiming another vendor is production supported.

Work budgets span all phases and attempts. Resumption and configuration changes cannot create an implicit fresh allowance. Execution/cache identity includes the integration and result-affecting profile. The owner clarified in #122 that GPT-6 Luna replaces GPT-5.6 Luna/max completely: resuming unfinished work must not depend on running the former model. The integration declares that specific replacement; work retains its ID, progress, historical attempts and consumed budget while recording the transition to the new profile. Running/uncertain invocations and failed checks retain their normal recovery/retry safeguards. Historical model caches and completed answers keep their original identity and are not reused as GPT-6 Luna results. Existing knowledge is not reingested merely because execution configuration changes. Unrelated profile changes still report an explicit mismatch for pending work.

Internal Work operations own mutation of its v1 representation. Typed states and phases protect invariants while boundary serialization preserves absent optional fields, retained errors and field ordering where compatibility requires it. A domain refactor does not authorize a data migration or reset.

Tests protect observable rules and meaningful boundaries. Keep focused unit tests for invariants, integration contract tests for execution, and CLI checks where process/filesystem/public output behavior matters. Consolidate repeated assertions and fixtures when another test retains the same guarantee; do not remove coverage to make a refactor pass or pursue an arbitrary count. The test audit and before/after evidence belong to #101; permanent documentation records the policy, not a duplicate catalogue of code.
