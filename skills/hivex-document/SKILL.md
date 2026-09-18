---
name: hivex-document
description: Establish or maintain project purpose, domain language, decisions, guidelines and procedures, including Hivex adoption and documentation migration.
---

# Give knowledge an authoritative home

Read the documentation map, relevant sources and current owner decisions. Update the document that owns the topic and scope; create another only for a distinct purpose. Markdown records intent and reasons; Hivex retrieves derived knowledge.

For adoption, use the installed `hivex init` when available and read its report. Complete missing meaning from evidence and the owner, keeping unresolved content provisional. Do not invent vision, requirements or vocabulary, or index empty scaffolding. The [foundation templates](../hivex/assets/project/docs/README.md) are starting material.

When adopting or updating the foundation, establish its [knowledge-before-merge requirements](../hivex/assets/project/docs/guidelines/engineering.md#knowledge-before-merge) in the project's existing engineering or knowledge authority, linked from its Git procedure. `init` preserves existing files; package installation alone does not update their policy. Preserve explicit owner decisions and surface a genuine conflict instead of silently replacing them.

Prefer migration to the standard when reasonably possible. Preserve useful content, links, history and monorepo/package/module authority. Reuse shared rules rather than duplicating them; keep product, stack and host details in the adopting project. Inspect source selection before moving documents and use `hivex snapshot relocate` before normal incremental maintenance.

## Keep documents useful

- PRD: purpose, users, vision, outcomes and scope.
- CONTEXT: concise domain definitions and avoided synonyms where useful. Check vocabulary against rules and code; omit generic programming terms.
- ADR: a consequential decision and its rationale, with scope, alternatives, dependencies and exceptions where they affect application.
- Guideline: maintained rules for ongoing work.
- Procedure: steps and verification for an operation, with prerequisites or recovery when relevant.

Preserve conditions, exceptions and partial replacements. Keep proposed, current and historical decisions distinct. When replaced detail obscures the current authority, archive it with provenance and links while preserving referenced anchors or updating their callers.

Use a short [AGENTS.md entrypoint](../hivex/assets/project/AGENTS.md): orientation, development/verification guidance and indispensable project constraints. Explain when linked authorities matter; do not duplicate them or require every document on every task.

Check affected links and source selection. Maintain changed knowledge and export its snapshot using Hivex's installed capabilities, preserving work, history and accounting.
