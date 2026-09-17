# Documentation map

- [Domain language](CONTEXT.md): documents, decisions, relationships, evidence and bounded work.
- [Engineering guidelines](guidelines/engineering.md): development, verification and knowledge maintenance.
- [Triage labels](guidelines/triage-labels.md) and [tracker procedure](procedures/issue-tracker.md): work selection, Git and review.
- [CI procedure](procedures/self-hosted-runner.md): hosted execution, native verification and runner retirement.
- [Release procedure](procedures/releasing.md): prepare, inspect and publish the exact native npm archive.
- [Practical knowledge assistance](adr/0010-practical-knowledge-assistance.md): the current approved contract, staged delivery, autonomy, semantic relationships, uncertainty and cost.
- [Shared knowledge and selective history](adr/0011-shared-knowledge-and-selective-history.md): accepted extension for compact ADRs, bounded historical retrieval and a Git-versioned knowledge snapshot.
- [Project foundation and workflow](adr/0012-project-foundation-and-workflow.md): initialization, the five shared capabilities, triage and efficient agent instructions.
- [Recommended Markdown convention](../skills/hivex/references/markdown.md): the default adoption structure and writing practices, with preservation of existing project knowledge.
- [CLI guide](guide.md) and [agent skill](../skills/hivex/SKILL.md): the interface actually available.

Earlier decisions remain in `adr/` as history. ADRs 0004–0009 describe the replaced experimental cohort/admission workflow; ADR 0010 supersedes its mandatory ceremony. Historical evidence is scoped to its original revision and is not a current acceptance result. An adopting project retains its own Markdown at monorepo, package or module level; Hivex does not own that source tree.

The [domain-module and execution decision](adr/0013-domain-modules-and-execution-integrations.md) defines ownership, replaceable integrations and model/profile independence.
