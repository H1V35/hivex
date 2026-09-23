---
title: Native knowledge candidates before graph admission
status: accepted
date: 2026-09-07
---

# Native knowledge candidates before graph admission

## Context

The owner required a Luna/max graph rebuild and implementation grounding before replacing the earlier graph. The earlier Opus projection must remain available until its replacement is admitted. Recovering legacy knowledge precedes source retirement and the full rebuild. The native experiments established a usable subscription transport, but a completed model response does not establish source fidelity, effective authority or correct implementation.

## Decision

The model choice below records the original 2026-09-07 decision. The [GPT-6 Luna replacement](0013-domain-modules-and-execution-integrations.md) supersedes it as of 2026-09-23: all subsequent default knowledge invocations use `gpt-6-luna` with effort `max`, including resumed work.

Knowledge-model operations use the native Codex harness and the existing ChatGPT subscription, with `gpt-5.6-luna` and effort `max`. Retrieval and deterministic validation remain model-free. Each extraction starts from an immutable declared source and produces an explicitly unaccepted candidate. Claims retain literal evidence at original source lines; conditions, exceptions and negation must remain visible. Authored Markdown and Git history retain authority.

The host verifies the admitted CLI protocol, account type, configured first-party endpoint, effective model/effort and unavailable model tools before supplying source text. It does not use private agent memory, project instructions discovered in a working checkout, or a provider fallback. Process-local controls preserve the user's configuration for other work. Native Codex remains the execution harness; Hivex does not create a development-session fleet.

Every invocation has a finite deadline and contributes one recorded attempt. Known usage is retained on failure; missing usage remains unknown. A timeout can retry only after confirmed interruption, within a maximum of three attempts. An unconfirmed start or interruption is a failure, not evidence that no request occurred. Invalid extraction output can be corrected with explicit feedback; an adverse grounded review must not be rerolled to obtain a preferred verdict.

Cleanup confirms termination of the owned process group and reaps the directly spawned server. On Linux, an orphan can remain as a zombie until container init reaps it. A bounded process-state check may confirm that every remaining group member is terminated (`Z` or `X`); a live, unknown or unreadable state cannot confirm cleanup. This does not claim that Hivex has reaped orphan PID entries. See the [Linux process-state reference](https://man7.org/linux/man-pages/man5/proc_pid_stat.5.html).

Candidate production cannot replace an accepted graph. Rebuild admission additionally requires a frozen source cohort and processing contract, provenance, structural checks and semantic evaluation. Old Opus payloads cannot be relabelled as Luna results. Changed sources, parsing or extraction inputs require an explicit cache/gold disposition before the corresponding rebuild.

Implementation grounding binds the reviewed code/diff revision and exact graph manifest. Evidence must resolve applicable conditions, exceptions and partial supersession. Contradictions, insufficient evidence, execution failures and incomplete retrieval cannot produce automatic acceptance. Tests and code review remain necessary alongside grounding.

## Consequences

The initial `extract` interface supplies candidates and failure evidence for the remaining staged delivery. It does not deliver graph assembly, admission, historical queries or the grounding gate. Those remain separate requirements of the staged product. Historical routing and context thresholds are machinery rules rather than defaults for native Codex development.
