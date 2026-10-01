# Historical decision records

This archive preserves earlier authorities from Git commit `1e3768f65f901522a63e00bde51069020049e123`. The documents keep their original dates and decisions; relative links were adjusted to keep evidence reachable. Their former accepted labels describe the original decision, not present applicability. The [current map](../README.md) and explicit replacements govern current work.

| Historical record | Why it is outside ordinary current knowledge | Current authority |
|---|---|---|
| [0001: first source interface](adr/0001-versioned-project-knowledge.md) | Initial TypeScript/Bun implementation and staged capabilities were replaced | [Versioned authority](../adr/0001-versioned-project-knowledge.md) |
| [0002: native candidates](adr/0002-native-knowledge-candidates.md) | Candidate/admission delivery and old profile are replaced | [Product contract](../adr/0010-practical-knowledge-assistance.md), [execution continuity](../adr/0013-domain-modules-and-execution-integrations.md) |
| [0003: Bun installation](adr/0003-independent-bun-installation.md) | Hivex development/runtime uses Rust; Bun invocation in adopting Bun projects remains documented | [CLI installation](../guide.md#install-the-cli-and-skills) |
| [0004: cohort ingestion store](adr/0004-resumable-ingestion-store.md) | `ingest` and `.hivex/ingestion.sqlite` cohort formats are retired | [Current runtime/data](../adr/0010-practical-knowledge-assistance.md#runtime-and-compatibility) |
| [0005: candidate snapshots](adr/0005-source-bound-graph-snapshots.md) | Candidate-snapshot/admission format is retired | [Optional snapshot safety](../reference/graph-cli.md#share-knowledge-through-git) |
| [0006: fidelity cohorts](adr/0006-source-fidelity-review.md) | Per-source fidelity and review-cohort machinery is retired | [Optional check/recovery contracts](../reference/graph-cli.md#update-and-repair-knowledge) |
| [0007: pair comparisons](adr/0007-evidence-bound-source-comparisons.md) | Pair-cohort comparison is not the current workflow | [Explicit relationships](../../skills/hivex/references/markdown.md#make-relationships-explicit) |
| [0008: admission manifests](adr/0008-reviewed-graph-admission.md) | Legacy admission is not implementation approval | [Source review before merge](../guidelines/engineering.md#knowledge-before-merge) |
| [0009: grounding interface](adr/0009-implementation-claim-grounding.md) | Its endpoint/schema is replaced | [Implemented optional review](../reference/graph-cli.md#support-an-implementation-review) |
| [0010: earlier product and migration stages](adr/0010-practical-knowledge-assistance.md) | Mandatory inferred graph and delivery-stage narratives obscure the current product contract | [Current product](../adr/0010-practical-knowledge-assistance.md), [new priority](../adr/0014-reliable-markdown-and-explicit-relationships.md) |
| [0012: former foundation amendments](adr/0012-project-foundation-and-workflow.md) | Whole-graph completeness and zero-warning merge gate is superseded | [Current foundation](../adr/0012-project-foundation-and-workflow.md), [current integration criteria](../adr/0014-reliable-markdown-and-explicit-relationships.md#knowledge-before-integration) |
| [0013: architecture/profile decision history](adr/0013-domain-modules-and-execution-integrations.md) | Original delivery dates and old default transition rationale are history; compatibility behavior remains current | [Current boundaries/continuity](../adr/0013-domain-modules-and-execution-integrations.md) |

Read history only for a relevant reason, such as why a rule changed or how a retained receipt was produced. Do not remove current failure, budget, publication or data-integrity safeguards merely because an old implementation also needed them. Existing `.hivex` data and the unfinished #152 branch are not modified or declared successful by this archival operation.
