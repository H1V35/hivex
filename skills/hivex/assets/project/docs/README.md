# Documentation map

Markdown records project intent, terminology, rules and reasons. The map and authored relationships guide focused retrieval; Hivex's inferred graph is optional derived assistance. The responsible agent checks applicability against the sources and the owner's current decisions.

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

Declare links with a short reason next to the rule: **Depends on** identifies a prerequisite, **Exception to** qualifies a base rule, **Supersedes** identifies a scoped replacement, and **Applied by** points to its procedure. Keep conditions with the rule. The author/reviewer judges meaning; checking a link only verifies reachability.
