---
title: "Hivex domain language"
status: accepted
created_at: 2026-09-05
updated_at: 2026-10-01
---

# Hivex domain language

Hivex gives agents and people a reusable project foundation and focused access to reliable Markdown. The agent reasons about applicability; the CLI discovers, searches, validates and reads authored sources.

## Language

**Project foundation**: Recommended starting documents, workflow capabilities and knowledge practices, tailored to the adopting project's purpose and decisions.

**Initialization**: Preparing missing foundation files, configuration and skill links while preserving project-owned files.

**Adoption**: Completing the foundation from project evidence, preserving useful knowledge and aligning working practices.

**Authority**: The maintained Markdown home for a topic and scope. Its current applicability depends on conditions, exceptions and replacements, not merely its folder or status.

**Document**: A selected Markdown file at project, package or module scope.

**Archived document**: Historical Markdown retained as evidence of replaced decisions. It is readable and searchable explicitly, without presumed current authority.

**Document version**: The hash of exact source contents, including working copies. A version identifies evidence, not approval.

**Snapshot**: The selected document versions and source-selection configuration for a query. It is an identity, not a persistent knowledge database.

**Passage**: A bounded range of original source lines returned by search. A passage can omit surrounding context; read relevant conditions and relationships before acting.

**Decision**: A project choice or constraint with its scope, conditions, exceptions and reasons. Acceptance, implementation, verification and permission are separate claims.

**Explicit relationship**: An author-declared connection with a fixed literal, Markdown target and scope/reason. Incoming navigation derives the inverse while preserving the written direction.

**Evidence**: An inspectable passage of a particular document or implementation version. A model's summary is not that passage.

**Coverage**: The selected and successfully loaded sources considered by an operation. Omissions, bounds and unread continuation limit conclusions.

**Applicability**: Whether a decision governs the particular case after its conditions, exceptions and replacements are considered.

**Historical execution evidence**: Preserved graph/runtime records, including failed work, attempts, receipts, uncertainty and consumed budgets. The current CLI does not reopen or mutate them.
