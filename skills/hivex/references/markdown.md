# Recommended Markdown foundation

Use this foundation when adopting Hivex. Prefer migration to its standard structure when reasonably possible, preserving and completing useful existing documents, their authority, history and links. Hivex accepts other Markdown layouts; the authority map and explicit relationships guide source reading. An inferred graph is optional assistance.

The foundation combines a clear authority map, project purpose, domain language and maintainable decisions. The bundled documentation skill guides adoption and completion; no retired planning framework or external skill set is required.

When establishing engineering guidance, read `templates/README.md` in the installed Hivex package and apply only the matching language-specific project template. Those standards belong in the adopting project's documentation, not in the common foundation or general skills. The template catalogue is part of the CLI package, not a directory copied with standalone skills.

## Give knowledge one authoritative home

Keep shared decisions at project or monorepo level. Package-specific or module-specific knowledge can stay with that package or module. Link shared rules instead of copying them into each area. Initialization prepares the core documents and directories. Complete project-specific drafts from evidence and the owner, and add further documents only for useful distinct purposes.

Before writing or creating a document, check the authority map and search existing current and relevant archived sources for the topic and its terminology. Read likely matches and their replacements. Extend the existing authority when it owns the same scope; create a document only after establishing a distinct useful responsibility, then update the map and callers. A failed keyword search alone does not establish that no authority exists.

A useful catalogue distinguishes:

- An authority map explaining where each kind of knowledge belongs.
- A PRD recording purpose, problem, vision, users and scope.
- A domain glossary defining the project's terms, with avoided synonyms where useful.
- Decisions and their reasons, often kept as ADRs.
- Guidelines containing design invariants and lessons worth applying again.
- Procedures for concrete operational or recovery tasks.
- Research and evidence identified as dated support, not automatically current decisions.

Prefer kebab-case for authored document and directory names, with numeric prefixes for ADRs where used. Keep conventional entrypoints such as `AGENTS.md`, `README.md`, `CONTEXT.md`, `PRD.md` and `SKILL.md`, and names required by the project's tools or language. Preserve links and history when renaming existing files.

For example, a project may use `docs/README.md`, `docs/CONTEXT.md`, `docs/adr/`, `docs/guidelines/`, `docs/procedures/` and `docs/research/`. Keep standing rules and design guidance in `guidelines`, and instructions for carrying out tasks in `procedures`; avoid overlapping directories with indistinct purposes. These are the adoption defaults; an existing project may retain another arrangement where migration would not be appropriate. Multiple bounded contexts may have their own glossary and decisions with a small context map linking them; a monorepo need not duplicate one shared product glossary.

## Write decisions for the next reader

State what was decided and why. Include the scope, conditions and exceptions that change how the rule is applied. Link the source or decision it depends on or replaces. A short paragraph can be enough; optional sections should carry information rather than serve as boxes to fill in.

For a consequential architectural trade-off, an ADR might be:

```markdown
# Remove private cached data when access is revoked

Cached private data is removed immediately when access is revoked. The normal cache lifetime still applies while access remains valid. This prevents stale local access after a permission change. See the cache policy for the general retention rule.
```

A title, truthful status and decision date can improve cataloguing. Tags are optional. If a rule is replaced, identify the replacement and whether the change is partial. Keep the historical reasoning readable; do not silently rewrite the past. A missing status means uncertainty to resolve from the content, not permission to assume acceptance.

Keep decision status separate from delivery and verification. The [shared state catalogue](../assets/project/docs/adr/README.md#decision-and-delivery-states), also prepared by `init`, defines draft, proposed, accepted, rejected, superseded and historical decisions. An accepted choice may be unimplemented or only partly delivered. Record implementation scope and verification evidence independently when they apply; preserve compatible project-specific statuses instead of silently reclassifying them.

Use an ADR when the choice is consequential, reflects a real trade-off and would be surprising without its rationale. Do not create one for every routine edit or dependency. Sequential names such as `0001-short-decision.md` are convenient if the project adopts that convention.

## Make relationships explicit

Put the relationship next to the rule that owns it. Use an ordinary Markdown link to the relevant decision or stable section, and a short explanation of why it matters. Keep conditions and exceptions with the rule; no separate relation database or mandatory frontmatter schema is needed.

- **Depends on** points from a rule to its prerequisite. State the condition if it applies only in a particular case.
- **Exception to** points from the scoped exception to the base rule it qualifies.
- **Supersedes** points from the current replacement to preserved history; name any scope that remains applicable.
- **Applied by** points from a policy to the procedure that carries it out. A procedure can link back with **Implements**.

For example:

```markdown
## Relationships

- Depends on [retention policy](../guidelines/retention.md#work-records): unfinished attempts must survive cleanup.
- Exception to [default cleanup](../procedures/cleanup.md#completed-work): referenced replacement work remains protected.
```

These directions describe meaning, not execution order. Do not declare a prerequisite merely because two topics are related. Authors and reviewers judge the relationship; a link checker verifies the target exists but cannot certify its truth.

Use the authority map to choose the initial source, then follow relevant links, including indirect dependencies. Read enough of each target to recover its conditions, exceptions and replacements. A compact map can save repeated reading without claiming the repository is semantically complete. Update both the relationship and its callers when a target or scope changes.

## Compact an ADR without losing its history

When replaced text obscures a decision's current meaning, preserve that history in a clearly marked Markdown archive and keep the active document focused on applicable rules, reasons, dependencies and exceptions. A wholly superseded ADR can remain as a short pointer to its replacement and archive. Preserve referenced anchors or update their links. Do not archive a live exception merely because it is old, or change what an earlier decision meant while shortening its current presentation.

A project may use `docs/archive/adr/` for this purpose; other layouts remain valid. Historical evidence should be available for focused consultation without being loaded into every model context. Declare its relative globs in `hivex.json` under `archive`, then use the installed CLI's `--source` selection or a known relationship for bounded retrieval. Compaction must not silently make necessary evidence inaccessible. The principal agent maintains and migrates the documents. The CLI prepares missing foundation files and offers explicit relocation for optional graph maintenance; it does not decide new product doctrine or silently rename existing sources.

## Keep the glossary focused

Define each project-specific concept briefly and use that term consistently. A glossary explains what a concept is; it is not an implementation manual, task plan or collection of general programming terms. Group related concepts when it helps and link context-specific definitions instead of copying.

Code and executable contracts explain mechanics. Markdown preserves the intent, constraints, decisions and reasons that code cannot explain. Update that knowledge alongside the change rather than leaving the only explanation in a conversation or private agent memory.

Before changing behavior, recover the existing authority and follow its relevant dependencies, exceptions and replacements. During the same change, reconcile the resulting code, documentary rules, delivery state and evidence. Before integration, independently review that affected set against a defined revision and repair contradictions or unsupported claims. This maintenance prevents the next agent from implementing against obsolete intent; link and format checks alone cannot detect a hallucinated rule. New evidence reopens only the affected review scope.

## Keep the agent entrypoint small

Use a short `AGENTS.md` with orientation and knowledge pointers, development/verification guidance and project-specific constraints only where needed. Link the relevant authority with enough context to know when it matters. Do not duplicate the PRD, glossary or guidelines, or require every document on every task.

The public [AGENTS.md convention](https://agents.md/) is ordinary Markdown without required fields. The foundation supplies a consistent starting shape rather than another schema or a line-count gate.
