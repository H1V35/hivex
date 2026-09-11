# Optional Markdown convention

Use the project's existing Markdown as it is. Hivex must not require this convention, frontmatter,
numbered ADRs or a particular directory structure. Recommend these practices when adopting Hivex or
writing documentation, without repeatedly proposing a migration of an established project.

These practices draw on Compi's authority map and documentation layout and the domain-modeling
skill. They preserve the useful documentation discipline, not Compi's retired machinery.

## Give knowledge one authoritative home

Keep shared decisions at project or monorepo level. Package-specific or module-specific knowledge
can stay with that package or module. Link shared rules instead of copying them into each area.
Folders and files appear only when there is useful content to put in them.

A useful catalogue distinguishes:

- An authority map explaining where each kind of knowledge belongs.
- A domain glossary defining the project's terms, with avoided synonyms where useful.
- Decisions and their reasons, often kept as ADRs.
- Guidelines containing design invariants and lessons worth applying again.
- Process documents explaining the working method.
- Procedures for concrete operational or recovery tasks.
- Research and evidence identified as dated support, not automatically current decisions.

For example, a project may use `docs/README.md`, `docs/CONTEXT.md`, `docs/adr/`,
`docs/guidelines/`, `docs/process/`, `docs/procedures/` and `docs/research/`. These are examples,
not required paths. Multiple bounded contexts may have their own glossary and decisions with a
small context map linking them; a monorepo need not duplicate one shared product glossary.

## Write decisions for the next reader

State what was decided and why. Include the scope, conditions and exceptions that change how the
rule is applied. Link the source or decision it depends on or replaces. A short paragraph can be
enough; optional sections should carry information rather than serve as boxes to fill in.

For a consequential architectural trade-off, an ADR might be:

```markdown
# Remove private cached data when access is revoked

Cached private data is removed immediately when access is revoked. The normal cache lifetime
still applies while access remains valid. This prevents stale local access after a permission
change. See the cache policy for the general retention rule.
```

A title, truthful status and decision date can improve cataloguing. Tags are optional. If a rule is
replaced, identify the replacement and whether the change is partial. Keep the historical reasoning
readable; do not silently rewrite the past. A missing status means uncertainty to resolve from the
content, not permission to assume acceptance.

Use an ADR when the choice is consequential, reflects a real trade-off and would be surprising
without its rationale. Do not create one for every routine edit or dependency. Sequential names
such as `0001-short-decision.md` are convenient if the project adopts that convention.

## Compact an ADR without losing its history

When replaced text obscures a decision's current meaning, preserve that history in a clearly marked
Markdown archive and keep the active document focused on applicable rules, reasons, dependencies
and exceptions. A wholly superseded ADR can remain as a short pointer to its replacement and archive.
Preserve referenced anchors or update their links. Do not archive a live exception merely because it
is old, or change what an earlier decision meant while shortening its current presentation.

Compi uses `docs/archive/adr/` for this purpose; other layouts remain valid. Historical evidence should
be available for focused consultation without being loaded into every model context. Check the
installed CLI's source-selection capabilities before moving a referenced source: the accepted
selective-history extension is not yet implemented by Hivex 0.1.0. Compaction must not silently make
necessary evidence inaccessible. The human or implementing agent maintains these documents.

## Keep the glossary focused

Define each project-specific concept briefly and use that term consistently. A glossary explains
what a concept is; it is not an implementation manual, task plan or collection of general programming
terms. Group related concepts when it helps and link context-specific definitions instead of copying.

Code and executable contracts explain mechanics. Markdown preserves the intent, constraints,
decisions and reasons that code cannot explain. Update that knowledge alongside the change rather
than leaving the only explanation in a conversation or private agent memory.
