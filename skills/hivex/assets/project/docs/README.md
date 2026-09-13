# Documentation map

Markdown records project intent, terminology, rules and reasons. Hivex retrieves this knowledge and its relationships; its graph is derived assistance. The responsible agent checks applicability against the sources and the owner's current decisions.

- [PRD](PRD.md): purpose, problem, vision, users and scope. Draft sections are open work, not accepted product decisions.
- [CONTEXT](CONTEXT.md): the project's agreed domain language.
- [ADRs](adr/README.md): decisions whose reasons, alternatives or exceptions matter to future work.
- [Engineering](guidelines/engineering.md): shared implementation and review principles.
- [Triage](guidelines/triage-labels.md): the meaning and use of issue labels.
- [Issue tracker](procedures/issue-tracker.md): how work enters and moves through Git and review.

Guidelines define ongoing rules; procedures explain how to perform an operation. Folder placement alone does not grant or remove authority. Read the scope, status, conditions and later amendments of each document.

Keep documents at the monorepo, package or module they describe. Link shared rules instead of copying them into every scope. Prefer this structure when adopting Hivex, preserving and completing useful existing documents and updating their links.

Use `docs/archive/` when replaced detail needs preserving outside a concise current document. Leave an explicit replacement/history link in the current authority; historical material does not silently regain force. Hivex's history selection keeps this material available for focused retrieval.

Maintain the owning document as part of a change. Do not leave durable decisions only in chats, tracker comments, model output or vendor-private memory.
