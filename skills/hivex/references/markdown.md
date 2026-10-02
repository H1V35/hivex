# Recommended Markdown foundation

Use this foundation when adopting Hivex. Prefer migration to its standard structure when reasonably possible, preserving and completing useful existing documents, their authority, history and links. Hivex accepts other Markdown layouts; the authority map and authored relationships guide source reading. The inferred graph runtime is retired.

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

Prefer kebab-case for authored document and directory names, with numeric prefixes for ADRs where used. Keep conventional entrypoints such as `AGENTS.md`, `README.md`, `CONTEXT.md`, `PRD.md` and `SKILL.md`, and names required by the project's tools or language. Preserve necessary links and history when renaming or retiring files.

For example, a project may use `docs/README.md`, `docs/CONTEXT.md`, `docs/adr/`, `docs/guidelines/`, `docs/procedures/` and `docs/research/`. Keep standing rules and design guidance in `guidelines`, and instructions for carrying out tasks in `procedures`; avoid overlapping directories with indistinct purposes. These are the adoption defaults; an existing project may retain another arrangement where migration would not be appropriate. Multiple bounded contexts may have their own glossary and decisions with a small context map linking them; a monorepo need not duplicate one shared product glossary.

## Document dates

Markdown documentation inside a `docs/` directory, including nested project/module docs and documentation templates, has YAML frontmatter with nonempty `title`, `status`, `created_at` and `tags`. Root entrypoints such as `AGENTS.md`, `CLAUDE.md` and `README.md`, skills and other Markdown outside `docs/` do not require this documentary metadata. Keep metadata required by their own tools, such as skill `name` and `description`.

`title` identifies the document. `status` uses exactly one lowercase literal from the shared catalogue: `draft`, `proposed`, `accepted`, `rejected`, `superseded` or `historical`. Incomplete project templates remain `draft`, maintained approved guidance is `accepted`, and dated evidence or retired runtime material is `historical`. Keep scope, partial replacements and delivery evidence in the body rather than appending them to a status literal. Preserve an original accepted/rejected status in an archived decision when it records the decision at that time; archival scope and provenance distinguish it from present authority. Status never establishes delivery or permission.

`tags` is a nonempty YAML sequence of distinct topic keywords, using lowercase kebab-case. Reuse the project's domain terms and existing equivalent tags; avoid synonyms for the same topic, generic filler and copies of every word in the title. Tags aid lexical discovery, not authority, state or formal relationships. The CLI already searches their source text in frontmatter with ordinary `search`; it neither expands them into inferred edges nor provides a tag-only filter.

Use optional `implementation` for executable consequences, with exactly `not-started`, `in-progress` or `implemented`. It describes the stated scope, not every capability mentioned in the document. Keep verification evidence, revision, partial scope and remaining work in the body. Acceptance, implementation, verification and permission remain separate; neither state field grants an execution GO.

Keep header fields in this order, omitting inapplicable optional fields: `title`, `status`, `implementation`, `created_at`, `updated_at`, `archived_at`, `tags`, `source`. Put other justified provenance fields after them in alphabetical order. Order does not change their meaning. Preserve required tool metadata outside `docs/` under that tool's convention.

Existing Markdown remains readable. During adoption, explicitly map legacy states to the catalogue while preserving their qualifications and original evidence; do not introduce another maintained state such as `Living document`. Preserve immutable historical bodies, including their original labels, under their provenance boundary. Extra metadata is allowed for concrete evidence such as source paths, revisions or date provenance; it must not redefine the standard fields or duplicate a narrative already owned by the body.

`created_at` is an ISO calendar date (`YYYY-MM-DD`) recording the document's creation. Preserve a documented creation date across moves; when it is missing, recover the earliest recorded creation from Git or explicit source provenance and state that limit instead of inventing earlier history. The former `date` key is renamed to `created_at`, and `updated` to `updated_at`.

`updated_at` is optional until a later content change. Maintain it when meaning, links or appended material change after creation; do not reset creation or add an update merely because a template is copied. Existing dates and Git-backed updates survive metadata-only migration.

An archived Markdown record also has `archived_at` for its first archival event. Retain its original `created_at`; later additions or repairs update `updated_at` and leave `archived_at` unchanged. Source path/revision metadata identifies the original record when needed. Fully archived ADRs live only in the archive: repair necessary callers rather than retaining duplicate current-path stubs.

Documentation templates carry their own source dates. `init` dates newly created project files under `docs/` with their UTC creation day and removes template update/archive dates, retaining their title and provisional status. It copies plain `AGENTS.md` and `CLAUDE.md` entrypoints and preserves all existing files byte-for-byte. Apply the same dating convention when an agent creates or maintains documentation. Delivery states and tags remain purpose-specific; dates do not grant authority, establish implementation or imply permission.

## Write decisions for the next reader

State what was decided and why. Include the scope, conditions and exceptions that change how the rule is applied. Link the source or decision it depends on or replaces. A short paragraph can be enough; optional sections should carry information rather than serve as boxes to fill in.

For a consequential architectural trade-off, an ADR might be:

```markdown
---
title: Remove private cached data when access is revoked
status: proposed
created_at: 2026-10-01
tags: [privacy, cache]
---

# Remove private cached data when access is revoked

Cached private data is removed immediately when access is revoked. The normal cache lifetime still applies while access remains valid. This prevents stale local access after a permission change. See the cache policy for the general retention rule.
```

Include the required title, truthful status, creation date and topic tags in documentation frontmatter. If a rule is replaced, identify the replacement and whether the change is partial. Keep the historical reasoning readable; do not silently rewrite the past. A missing status in legacy sources means uncertainty to resolve from the content, not permission to assume acceptance.

Keep decision status separate from delivery and verification. The [shared state catalogue](../assets/project/docs/adr/README.md#decision-and-delivery-states), also prepared by `init`, defines draft, proposed, accepted, rejected, superseded and historical decisions. An accepted choice may be unimplemented or only partly delivered. Record implementation scope and verification evidence independently when they apply; normalize legacy labels without discarding their conditions or inferring delivery.

Use an ADR when the choice is consequential, reflects a real trade-off and would be surprising without its rationale. Do not create one for every routine edit or dependency. Sequential names such as `0001-short-decision.md` are convenient if the project adopts that convention.

## Make relationships explicit

The authored relationship contract v1 is a single root `## Relationships` block per document. That exact heading is reserved for formal document connections; rename a domain-language section with the same heading and preserve its prior anchor when needed. Its entries use one exact literal, one plain inline Markdown target and a nonempty plain-text explanation on one unindented hyphen bullet line (`- `). Blank lines are allowed; unknown labels, aliases, bold labels, free prose, multiple links, duplicate declarations or another Relationships block are diagnostics, not inferred relationships. Ordinary prose and links outside the block remain ordinary Markdown.

Formal relationships live in this block rather than a parallel `related` or `supersedes` frontmatter list. Avoid repeating the same relationship and scope with a reworded reason; structural validation cannot establish whether two differently worded declarations mean the same thing.

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

Targets are selected relative Markdown sources without a query component, optionally with an exact heading or explicit ID fragment (a question mark after `#` is fragment data); same-document `#anchor` links are valid. The CLI resolves the target range and current source version. It rejects missing or ambiguous anchors, out-of-project/excluded sources and malformed formal entries instead of guessing. A relationship source is the document; its declaration coordinates identify the entry. An anchor destination does not invent a section-specific source entity.

Heading IDs lowercase the rendered heading text, remove ASCII punctuation except hyphen/underscore, and replace whitespace with hyphens. Repeated IDs receive `-1`, `-2` and subsequent available suffixes in document order. For a stable ID independent of heading text, use `<a id="identifier"></a>`; IDs are exact and must not collide with another explicit or generated ID. Fenced code and quoted headings/anchors are not targets. A heading range ends before the next heading of the same or higher level; an explicit ID starts at its marker and includes the nearest following heading's section (or the preceding section if no heading follows). This supported anchor convention is deterministic; do not assume every external Markdown renderer uses the same slug algorithm.

HTML comments and raw-text elements such as `script`, `style` and `textarea` do not create anchors. Navigation validates at most 2048 declarations across its scanned sources; excess returns `RELATION_LIMIT`, without truncating validation or reporting a complete result. Documents support at most 32768 heading/explicit anchor records; excess returns `ANCHOR_LIMIT`. A query retains at most 65536 target anchor records in memory; excess is explicit and can be avoided by narrowing source selection.

`relations <document>` returns direct authored connections and incoming connections from ordinary sources, with the declared `from`/`to` unchanged. It reads historical declarations only when that source is queried explicitly; historical destinations remain reachable. Pagination binds its cursor to the source snapshot and query options, counts the complete serialized UTF-8 response and never cuts a relation. Explicit coverage limits remain visible. Navigation does not infer relationships, recurse automatically or certify semantic truth.

Authors and reviewers judge meaning, reason and conditions. A deterministic reader/checker verifies syntax, direction encoding, selected targets and anchors. Follow relevant links, including indirect dependencies, by querying the next document and reading the cited destination range. Reuse visited versions/ranges instead of repeatedly traversing cycles. A known authority can be read directly; queries are capabilities, not mandatory phases. Update relationship entries and callers when a target or scope changes.

When migrating documentation, inventory the project-owned sources, including unselected notes and relevant history. Read each maintained document and the governing context of its destinations to find justified connections, including those expressed without a link. Classify legacy `Related` lists and wikilinks by meaning; normalize useful maintained navigation to relative Markdown links. Indices, citations and provenance stay ordinary links when none of the five relationship types applies. Do not manufacture dependencies, inverse duplicates or empty blocks to satisfy a count. Preserve immutable archive bodies; declare later replacements in their current authority and keep historical notation accessible through provenance. Record the reviewed scope and unresolved gaps in the existing task/review; structural success cannot establish that no declaration is missing.

## Recover enough context to apply a rule

A search passage locates possible evidence; even a complete section does not establish all applicable rules. Use `context.complete` and its coordinates to expand incomplete results; reuse complete returned context. Before applying it, recover the complete relevant section and the document context that governs it, including scope, definitions and qualifications outside the match. Read the whole authority when those boundaries are unclear. A section or anchor range can also omit a document-wide condition. Reuse already inspected context when its source version is still current; another command is unnecessary if the needed context is already present.

Follow relevant authored links in both directions, including indirect prerequisites, exceptions and replacements, and read their applicable text. Inspect warnings, coverage and continuation; finish needed ranges and pages before treating them as inspected. Missing search matches, absent declarations and structural success do not prove there are no other applicable rules. Use the authority map, ordinary links, domain terms and available sources to resolve gaps; consult history when it explains a relevant replacement or qualification.

Before changing behavior, establish which rule applies, its scope and conditions, relevant exceptions or replacements, and how the proposed behavior respects them. Ground that interpretation in identifiable source versions and passages, distinguishing explicit rules from assumptions or unanswered questions. Keep a brief rationale in the task's existing plan or review when it helps assess the change; do not create a separate certificate or repeat every source. The independent reviewer checks this interpretation against the actual sources and implementation, not only a paraphrase.

Expand until the consequential questions for the task are resolved by evidence, not until every repository document has been read. Apply settled decisions autonomously. If reasonable documentation and tool use cannot resolve a consequential gap, ask the owner with the sources, impact and a recommendation. No CLI result or self-reported comprehension can certify semantic completeness.

## Compact an ADR without losing its history

When replaced text obscures a decision's current meaning, preserve that history in a clearly marked Markdown archive and keep the active document focused on applicable rules, reasons, dependencies and exceptions. Remove a fully archived ADR from the current directory and update necessary callers to its archive or current replacement. Preserve referenced anchors or update their links. Do not archive a live exception merely because it is old, or change what an earlier decision meant while shortening its current presentation.

A project may use `docs/archive/adr/` for this purpose; other layouts remain valid. Historical evidence should be available for focused consultation without being loaded into every model context. Declare its relative globs in `hivex.json` under `archive`, then use the installed CLI's `--source` selection or a known relationship for bounded retrieval. Compaction must not silently make necessary evidence inaccessible. The principal agent maintains and migrates the documents. The CLI prepares missing foundation files; source moves require maintained caller links and fresh query snapshots, without deciding product doctrine or silently renaming sources.

## Keep the glossary focused

Define each project-specific concept briefly and use that term consistently. A glossary explains what a concept is; it is not an implementation manual, task plan or collection of general programming terms. Group related concepts when it helps and link context-specific definitions instead of copying.

Code and executable contracts explain mechanics. Markdown preserves the intent, constraints, decisions and reasons that code cannot explain. Update that knowledge alongside the change rather than leaving the only explanation in a conversation or private agent memory.

Before changing behavior, recover the existing authority and follow its relevant dependencies, exceptions and replacements. During the same change, reconcile the resulting code, documentary rules, delivery state and evidence. Before integration, independently review that affected set against a defined revision and repair contradictions or unsupported claims. This maintenance prevents the next agent from implementing against obsolete intent; link and format checks alone cannot detect a hallucinated rule. New evidence reopens only the affected review scope.

## Keep the agent entrypoint small

Use a short `AGENTS.md` with orientation and knowledge pointers, development/verification guidance and project-specific constraints only where needed. Link the relevant authority with enough context to know when it matters. Do not duplicate the PRD, glossary or guidelines, or require every document on every task.

The public [AGENTS.md convention](https://agents.md/) uses ordinary Markdown. Keep agent entrypoints plain and concise; Hivex's documentary metadata applies inside `docs/`, not to these entrypoints.

## Relationships

- Implements [Selective history](../../../docs/adr/0011-shared-knowledge-and-selective-history.md): authors preserve dates and provenance while keeping live authorities concise.

## Validate affected knowledge

Run `check --source <document>` for the affected authorities and callers, or `check` for ordinary selected sources. It validates formal relationship grammar and selected local Markdown targets/anchors without a model. Archived declarations are checked only through explicit source selection or `--historical`; a historical destination remains readable without validating all of its old prose. Unknown or excluded references are located findings, not silently skipped evidence.

Review every finding against its source/scope. An incomplete source load is `partial`; invalid references or declarations produce `failed`, with paginated findings and source-bound continuation. A `ready` report confirms only the stated structural scope; it does not validate required documentary metadata. Authors maintain metadata under the convention above, and legacy source layouts remain readable during migration. It cannot find an undeclared relationship, establish a unique semantic authority, verify implementation/permission or resolve contradictory rules. Authors and independent reviewers perform those checks against the actual task and source/code versions.
