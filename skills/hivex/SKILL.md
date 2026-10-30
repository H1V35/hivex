---
name: hivex
description: Adopt HiveX's project foundation, recover relevant Markdown decisions and explicit dependencies or exceptions, and maintain reusable source knowledge.
---

# Project knowledge and foundation

Markdown records project intent and decisions. Use the documentation map and explicit relationships to find the sources relevant to the task. The CLI retrieves authored evidence without inferring knowledge or invoking a model; the responsible agent assesses meaning and implementation.

## Use the installed interface

Read the brief project entrypoint and source configuration. Use `hivex --help` for the command contract when the interface is unfamiliar. Do not invent commands. Model configuration belongs to the host harness, not HiveX retrieval.

`hivex init` copies these skills with their references, scripts, foundation assets and language templates, so their links resolve within `.agents/skills/`. Links into HiveX's own `docs/` resolve only in the HiveX repository; for command details and migration guidance, read the guide of the project's pinned release, `https://github.com/H1V35/hivex/blob/vX.Y.Z/docs/guide.md`, where `X.Y.Z` is the `version` in `hivex.json`. Do not substitute the adopting project's README for the release's command contract. A `versionWarning` in a result means the installed CLI differs from the pinned release; its `install` field gives the command that installs the pinned one.

For adoption, `init` prepares missing documents and configuration without model calls. Use `hivex-document` to complete actual project meaning and preserve useful history, and `hivex-git` for tracker work. The five workflow skills and knowledge skill are independent capabilities, not compulsory phases. Apply relevant standards from the installed [language-template catalogue](templates/README.md); project-specific language rules belong in the project.

## Recover sources before deciding

Choose the current authority from the task route in the documentation map. Use focused `search`, `relations` and `read` when they recover missing evidence; they are deterministic and make no model calls. A known authority can be read directly, and already verified current context can be reused without another query. `relations <document>` preserves authored direction and resolves destination ranges; inspect its coverage, warnings and continuation. Read the exact passage and source version, including conditions, exceptions and replacements. A search preview, empty relationship result or accepted label does not establish applicability.

Follow authored dependencies and qualifications, including indirect relationships, until the task has enough evidence. Track visited document versions and cited ranges so cycles or repeated links do not trigger repeated reading. Read archive material only when it explains a relevant history, replacement or unresolved condition. Reuse current context rather than repeating queries per file or reading every document.

Use `context.complete` and its range to expand incomplete search results and reuse complete returned context. Before applying a match, recover its complete relevant section and governing document context; read the whole authority when their boundaries are unclear. Follow applicable incoming and outgoing qualifications. Check the proposed behavior against the rule's scope, conditions, exceptions and replacements using identifiable source passages. Finish needed continuations and resolve consequential gaps under the [shared context-reading method](references/markdown.md#recover-enough-context-to-apply-a-rule); a fragment or self-reported understanding is insufficient evidence.

Apply settled decisions autonomously. Expand with available documentation and tools when the first evidence is insufficient. Ask the owner only after those routes cannot resolve a consequential question, with sources, impact and a recommendation.

## Maintain the Markdown base

Before writing, check the map and search current and relevant historical knowledge for the existing authority; read its scope and replacements. Update that authority for the same topic instead of creating a parallel source. State useful relationships as ordinary Markdown links with their scope and reason, following [the Markdown convention](references/markdown.md#make-relationships-explicit). Keep one current home per topic; archive replaced text with dates, provenance and links rather than deleting it or retiring a live exception by age.

Maintain affected knowledge alongside the code change, then use `check --source <document>` for affected authorities and callers, and review current rules and implementation together before integration. `check` validates structure and selected references; it does not certify semantic truth, detect undeclared doctrine or grant permission. Distinguish accepted choices from implemented or verified behavior using the packaged decision/implementation catalogue; neither acceptance nor a state label grants a new permission. Repair real contradictions and easy actionable defects; record an evidenced disposition for a false claim. Close review against a defined revision and review later changes as a delta. Do not require complete graph ingestion or globally zero graph warnings for source-grounded work. The project's engineering policy and existing owner authorization govern integration.

## Preserve retired execution evidence

Version 0.8 retires inferred graph/model commands. Source retrieval makes no model calls and never opens the old graph or execution store. During adoption, preserve useful knowledge in Markdown and retain necessary historical snapshots, SQLite work, failed candidates, receipts, uncertainty and consumption before retiring graph integration. Do not reset or relabel them to clear a warning. The pinned release's guide owns migration and the historical command reference.
