---
name: hivex
description: Adopt Hivex's project foundation, retrieve relevant project decisions and their dependencies or exceptions, and maintain reusable Markdown knowledge. Use when Hivex is requested or configured for a project.
---

# Project knowledge and foundation

Markdown records project intent and decisions. Hivex retrieves derived knowledge and its relationships for the agent responsible for design, implementation or review. Recover settled decisions before asking the owner to decide them again.

## Use the installed capabilities

Read the project's brief agent entrypoint and source configuration. Use `hivex --help` when the installed interface is unfamiliar; in a Bun project, `bun hivex` resolves the installed CLI. The package is `@h1v35/hivex`, not the unrelated unscoped package. Do not invent commands or silently change the knowledge model.

Manual links below refer to the installed Hivex package's guide. If copied skills cannot resolve those links, read the named section in that package's guide instead (`node_modules/@h1v35/hivex/docs/guide.md` for a project-local installation). Do not substitute the adopting project's README; use the manual matching the CLI release.

For adoption, use `init` when the installed version provides it. It prepares missing documents and configuration without model calls. Use `hivex-document` to complete project meaning and migrate existing documentation to the standard when appropriate, and `hivex-git` for the tracker and labels. The five workflow skills are independent capabilities, not compulsory phases.

## Retrieve before deciding

Start with focused `search`, `neighbors` and `read`. Read the evidence and follow relevant dependencies, exceptions and replacements, including indirect ones. Reuse current context instead of repeating a query for every file.

A preview, accepted label or isolated warning does not establish the full meaning. Check scope, conditions, versions and later decisions. If the sources settle the matter, apply it; otherwise explain the actual unresolved decision and recommend a course of action.

Use `ask` for model-assisted interpretation when it adds value; it can first maintain a relevant pending batch within the same work budget. Explicit `--source` selection can focus current or historical evidence. Ordinary queries do not require ingesting all declared history.

## Maintain the derived knowledge

The principal agent maintains the Markdown. Detect new, changed or removed sources before relying on related graph entries, and use incremental `update` when needed. Large documents are processed in bounded rounds; preserve partial progress and the same work's consumed budget. Do not resend the whole corpus when only a later round remains.

When sources move, use `snapshot relocate <old> <new>` before updating; relocation does not certify changed content. Follow the [snapshot and relocation guide](../../docs/guide.md#share-knowledge-through-git) for coverage and evidence requirements. Do not relocate unrelated knowledge to hide a warning.

Repair a demonstrated interpretation error with `update --repair-range <document>:<start>-<end> --reason <correction>`, or `--repair <document>` when the whole document needs revisiting. Check the source, dependencies and replacements first. Do not repair correct knowledge merely because a warning omits context, or change doctrine to satisfy the model. For range requirements, `RELATIONSHIP_LOSS`, interrupted or limited work, and before `recover` or `prune`, read the [maintenance and recovery guide](../../docs/guide.md#update-and-repair-knowledge); do not retry automatically.

After `update`, review `warningChanges.new` entries with `state: "active"` and `warningChanges.reopened` in that work. Repair demonstrated implementation or relationship findings, or record an evidenced closure with `warnings --resolve`; use `warnings --all` for history. See the [warning guidance](../../docs/guide.md#update-and-repair-knowledge). Do not require closure of a real question or repeat the model for green output.

## Support review

Hivex supplies knowledge support to the principal reviewer; it does not approve the implementation. Use local evidence first; run `review "<task>" --base <git-ref>` only when model-assisted support adds useful information.

Confirm any finding against the actual code and documentary versions. A lack of model findings is not implementation approval. Save a review report outside the reviewed project or in an ignored location when needed; `review --check <report>` verifies its documentary and implementation freshness without a model call.

## Share progress and preserve accounting

After useful knowledge changes, `snapshot export` writes `.hivex/graph.json` for Git alongside the Markdown it describes. Export that snapshot in the same change as the Markdown. Local SQLite, attempts, locks and caches remain ignored. Consult the snapshot guide before importing into existing local state.

Hivex budgets cover all phases and attempts of a work item. Resume that work with its retained accounting; a larger total limit is not a fresh allowance. Preserve the graph, history and honest uncertainty; never wipe state, reset counters or repeat semantic checks merely to obtain approval. Use the CLI's reported consumption, including unknown usage.

For document layout and source conventions, consult [Markdown guidance](references/markdown.md) when establishing or reorganizing knowledge.
