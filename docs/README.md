# Documentation map

Hivex supplies a reusable workflow and reliable project knowledge to people and agents. Markdown owns intent and decisions; authored links expose dependencies, exceptions and replacements. Start with the task route below, then follow relevant relationships instead of loading every document. [ADR 0014](adr/0014-reliable-markdown-and-explicit-relationships.md) owns this approach.

## Choose the authority for the task

| Task | Read first | Follow when relevant |
|---|---|---|
| Understand product purpose or retrieval | [Product contract](adr/0010-practical-knowledge-assistance.md) | [Source authority](adr/0001-versioned-project-knowledge.md), [reliable Markdown](adr/0014-reliable-markdown-and-explicit-relationships.md) |
| Find a project decision without a model | [Source-reading guide](guide.md#recover-context) | The result's current passage and its explicit relationships |
| Write, replace or archive knowledge | [Markdown convention](../skills/hivex/references/markdown.md) | [Archival contract](adr/0011-shared-knowledge-and-selective-history.md), [history catalogue](archive/README.md) |
| Implement or review a change | [Engineering policy](guidelines/engineering.md) | [Module/execution boundaries](adr/0013-domain-modules-and-execution-integrations.md), [Rust quality](guidelines/rust-quality.md), [domain language](CONTEXT.md) |
| Adopt Hivex or update workflow skills | [Project foundation](adr/0012-project-foundation-and-workflow.md) | [Initialization](guide.md#initialize-a-project), [language templates](../templates/README.md) |
| Manage an issue, PR or merge | [Tracker procedure](procedures/issue-tracker.md) | [Triage labels](guidelines/triage-labels.md), [knowledge before merge](guidelines/engineering.md#knowledge-before-merge) |
| Run development checks or CI | [Development guide](guide.md#development) | [Rust quality](guidelines/rust-quality.md), [hosted CI](procedures/self-hosted-runner.md) |
| Prepare or publish a native package | [Release procedure](procedures/releasing.md) | [Development checks](guide.md#development), artifact-specific publication approval |
| Migrate the retired graph runtime | [Migration guide](guide.md#migrate-from-the-inferred-graph-runtime) | [Historical command record](archive/runtime/graph-cli-0.7.8.md); preserve evidence and accounting |
| Investigate a replaced behavior | [History catalogue](archive/README.md) | The original dated record and its current replacement |

A document's status alone does not settle its scope. Check conditions, exceptions, later replacements and the exact source version. Expand with available documentation and tools; ask the owner only if they cannot resolve a consequential question. Keep one current authority per topic and put each relationship next to the rule it qualifies.

[Archived sources](archive/README.md) are declared by `hivex.json` and remain available for focused reading. The former cohort/admission workflow and migration stages are history, not current runtime instructions. Existing graph snapshots and local work remain preserved with their actual uncertainty and failures; they do not certify this Markdown base.
