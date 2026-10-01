---
title: "Documentation map"
status: draft
created_at: 2026-09-13
updated_at: 2026-10-01
---

# Documentation map

Markdown records project intent, terminology, rules and reasons. The map and authored relationships guide focused retrieval; Hivex retrieves current source evidence without an inferred graph or model invocation. The responsible agent checks applicability against the sources and the owner's current decisions.

- [PRD](PRD.md): purpose, problem, vision, users and scope. Draft sections are open work, not accepted product decisions.
- [CONTEXT](CONTEXT.md): the project's agreed domain language.
- [ADRs](adr/README.md): decisions whose reasons, alternatives or exceptions matter to future work.
- [Engineering](guidelines/engineering.md): shared implementation and review principles.
- [Triage](guidelines/triage-labels.md): the meaning and use of issue labels.
- [Issue tracker](procedures/issue-tracker.md): how work enters and moves through Git and review.

Guidelines define ongoing rules; procedures explain how to perform an operation. Folder placement alone does not grant or remove authority. Read the scope, status, conditions and later amendments of each document.

Keep documents at the monorepo, package or module they describe. Link shared rules instead of copying them into every scope. Prefer this structure when adopting Hivex, preserving and completing useful existing documents and updating their links.

Use `docs/archive/` when replaced detail needs preserving outside a concise current document. Leave an explicit replacement/history link in the current authority; historical material does not silently regain force. Declare archive paths in the source configuration so history stays available for focused reading without ordinary ingestion.

Maintain the owning document as part of a change. Do not leave durable decisions only in chats, tracker comments, model output or vendor-private memory.

## Task routes and explicit relationships

Use product/ADR sources for a behavior decision, the glossary for language, engineering for implementation/review, and tracker procedures for Git work. Follow relevant dependencies, exceptions and replacements from those sources; do not read every document by default. Add project-specific routes when they help readers find the correct authority.

Declare formal relationships in one exact `## Relationships` block, using one unindented hyphen bullet (`- `) per entry with a plain relative Markdown link and a plain-text reason after `: `. The closed literals are `Depends on`, `Exception to`, `Supersedes`, `Implements` and `Extends`; aliases and inverse labels are not authored types. Keep conditions with the rule and explain the affected scope in the reason. The installed Hivex Markdown reference owns the complete shared contract.

Use `relations <document>` to retrieve outgoing and incoming declarations deterministically, then `read` for their cited ranges. Follow relevant indirect links by querying the destination document. Inspect coverage, warnings and continuation instead of interpreting a partial or empty result as absence of knowledge. Ordinary links remain useful navigation outside the formal block. Use `check --source <document>` for affected authorities/callers before integration. Authors and reviewers judge meaning, uniqueness of authority and permission; the CLI checks syntax, selected targets and anchors.
