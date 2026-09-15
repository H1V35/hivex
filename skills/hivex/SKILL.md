---
name: hivex
description: Adopt Hivex's project foundation, retrieve relevant project decisions and their dependencies or exceptions, and maintain reusable Markdown knowledge. Use when Hivex is requested or configured for a project.
---

# Project knowledge and foundation

Markdown records project intent and decisions. Hivex retrieves derived knowledge and its relationships for the agent responsible for design, implementation or review. Recover settled decisions before asking the owner to decide them again.

## Use the installed capabilities

Read the project's brief agent entrypoint and source configuration. Use `hivex --help` when the installed interface is unfamiliar; in a Bun project, `bun hivex` resolves the installed CLI. The package is `@h1v35/hivex`, not the unrelated unscoped package. Do not invent commands or silently change the knowledge model.

For adoption, use `init` when the installed version provides it. It prepares missing documents and configuration without model calls. Use `hivex-document` to complete project meaning and migrate existing documentation to the standard when appropriate, and `hivex-git` for the tracker and labels. The five workflow skills are independent capabilities, not compulsory phases.

## Retrieve before deciding

Start with focused `search`, `neighbors` and `read`. Read the evidence and follow relevant dependencies, exceptions and replacements, including indirect ones. Reuse current context instead of repeating a query for every file.

A preview, accepted label or isolated warning does not establish the full meaning. Check scope, conditions, versions and later decisions. If the sources settle the matter, apply it; otherwise explain the actual unresolved decision and recommend a course of action.

Use `ask` for model-assisted interpretation when it adds value. It may maintain one relevant pending batch before answering under the same work budget. Explicit `--source` selection can focus current or historical evidence. Ordinary queries do not require ingesting all declared history.

## Maintain the derived knowledge

The principal agent maintains the Markdown. Detect new, changed or removed sources before relying on related graph entries, and use incremental `update` when needed. Large documents are processed in bounded rounds; preserve partial progress and the same work's consumed budget. Do not resend the whole corpus when only a later round remains.

When sources move, use `snapshot relocate <old> <new>` before updating. It preserves identities, relationships, evidence versions and prior work without model calls. Only an identical source with matching known versions at a previously unknown destination reuses coverage; changes, mixed or missing versions and consolidation remain pending for normal maintenance. Do not relocate unrelated knowledge to hide a warning.

Repair a demonstrated interpretation error with `update --repair-range <document>:<start>-<end> --reason <correction>` for complete affected decisions of a current source, or `--repair <document>` when the whole document needs revisiting. A `RELATIONSHIP_LOSS` result retains the previous graph and needs inspection, not automatic retries. Correct the interpretation after checking the source, its dependencies and replacements. Do not repair correct knowledge merely because a warning omits context, or change doctrine to satisfy the model. Keep unresolved limitations explicit.

## Support review

The principal reviewer directs implementation review and uses the implementing agent's model and effort. Hivex's knowledge model is separate. Use local evidence first; run `review "<task>" --base <git-ref>` only when model-assisted support adds useful information.

Confirm any finding against the actual code and documentary versions. A lack of model findings is not implementation approval. Save a review report outside the reviewed project or in an ignored location when needed; `review --check <report>` verifies its documentary and implementation freshness without a model call.

## Share progress and preserve accounting

After useful knowledge changes, `snapshot export` writes `.hivex/graph.json` for Git alongside its sources. Local SQLite, attempts, locks and caches remain ignored. A fresh clone reuses a matching snapshot; existing local state takes precedence until an explicit `snapshot import`, which refuses unfinished work.

Budgets cover all phases and attempts of a work item. Resume that work with its retained accounting; a larger total limit is not a fresh allowance. Existing owner authorization remains valid. Report actual completed usage and unknown consumption honestly, including failed or interrupted attempts.

Use `recover` for interrupted execution, not ordinary retrieval. An uncertainty acknowledgement preserves uncertainty and cost; it does not certify a remote outcome or authorize an automatic retry. Use `prune` for obsolete completed work/caches when appropriate, preserving unfinished work and required evidence. Do not wipe state, restart counters or repeat semantic checks merely to obtain approval.

If a result reports a context limit, inspect the required evidence and restore it or adjust the bound within scope before resuming the same work. Keep native execution permissions and timeouts appropriate to the task. For document layout and source conventions, consult [Markdown guidance](references/markdown.md) when establishing or reorganizing knowledge.
