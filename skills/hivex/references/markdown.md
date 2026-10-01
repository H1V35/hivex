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

The authored relationship contract v1 is a single root `## Relationships` block per document. Its entries use one exact literal, one plain inline Markdown target and a nonempty plain-text explanation on one unindented hyphen bullet line (`- `). Blank lines are allowed; unknown labels, aliases, bold labels, free prose, multiple links, duplicate declarations or another Relationships block are diagnostics, not inferred relationships. Ordinary prose and links outside the block remain ordinary Markdown.

| Exact literal | Authored direction |
|---|---|
| `Depends on` | Dependent decision → prerequisite |
| `Exception to` | Scoped exception → base rule |
| `Supersedes` | Current replacement → prior decision; name any live part of a partial replacement |
| `Implements` | Procedure or rule → policy it carries out |
| `Extends` | Added scope → base decision; extension does not imply a prerequisite |

Use only relevant entries, not every type in each document:

```markdown
## Relationships

- Depends on [Retention policy](../guidelines/retention.md#work-records): unfinished attempts must survive cleanup.
- Exception to [Default cleanup](../procedures/cleanup.md#completed-work): referenced replacement work remains protected.
```

`Replaces`, `Extended by` and `Applied by` are not authored aliases. Express replacement with `Supersedes` and put `Extends`/`Implements` in the authority that owns the extension or procedure. Incoming navigation computes the inverse without storing a duplicate statement. Preserve the meaning and condition when normalizing existing prose; do not turn an extension into a dependency.

Targets are selected relative Markdown sources, optionally with an exact heading or explicit ID fragment; same-document `#anchor` links are valid. The CLI resolves the target range and current source version. It rejects missing or ambiguous anchors, out-of-project/excluded sources and malformed formal entries instead of guessing. A relationship source is the document; its declaration coordinates identify the entry. An anchor destination does not invent a section-specific source entity.

Heading IDs lowercase the rendered heading text, remove ASCII punctuation except hyphen/underscore, and replace whitespace with hyphens. Repeated IDs receive `-1`, `-2` and subsequent available suffixes in document order. For a stable ID independent of heading text, use `<a id="identifier"></a>`; IDs are exact and must not collide with another explicit or generated ID. Fenced code and quoted headings/anchors are not targets. A heading range ends before the next heading of the same or higher level; an explicit ID starts at its marker and includes the nearest following heading's section (or the preceding section if no heading follows). This supported anchor convention is deterministic; do not assume every external Markdown renderer uses the same slug algorithm.

HTML comments and raw-text elements such as `script`, `style` and `textarea` do not create anchors. Navigation validates at most 2048 declarations across its scanned sources; excess returns `RELATION_LIMIT`, without truncating validation or reporting a complete result.

`relations <document>` returns direct authored connections and incoming connections from ordinary sources, with the declared `from`/`to` unchanged. It reads historical declarations only when that source is queried explicitly; historical destinations remain reachable. Pagination binds its cursor to the source snapshot and query options, counts the complete serialized UTF-8 response and never cuts a relation. Explicit coverage limits remain visible. Navigation does not infer relationships, recurse automatically or certify semantic truth.

Authors and reviewers judge meaning, reason and conditions. A deterministic reader/checker verifies syntax, direction encoding, selected targets and anchors. Follow relevant links, including indirect dependencies, by querying the next document and reading the cited destination range. Update relationship entries and callers when a target or scope changes.

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

## Relationships

- Implements [Selective history](../../../docs/adr/0011-shared-knowledge-and-selective-history.md): authors preserve dates and provenance while keeping live authorities concise.
