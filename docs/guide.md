---
title: "Hivex guide"
status: accepted
created_at: 2026-09-17
updated_at: 2026-10-03
tags: [cli, retrieval, adoption]
---

# Hivex guide

Hivex is an AI-first CLI for agents and people. The principal agent reasons and implements; Hivex supplies reusable workflow skills and deterministic retrieval of repository Markdown. No command invokes a model or requires a persistent knowledge database.

## Install the CLI and skills

The verified native target is macOS ARM64. Install a pinned project dependency:

```sh
npm install -D -E @h1v35/hivex
npx hivex init
```

In Bun projects use `bun add -d -E @h1v35/hivex`, then `bun hivex init`. The scoped package is `@h1v35/hivex`; do not install the unrelated unscoped package. The executable needs no JavaScript launcher, model credentials or Codex process. npm/Bun are installation clients, not Hivex runtime dependencies.

## Initialize a project

`init [--root <project>]` prepares missing foundation documents, a source configuration, brief `AGENTS.md`, a `CLAUDE.md` import and relative links to six packaged skills in `.agents/skills/` and `.claude/skills/`. It preserves existing files, configuration, custom skills and historical runtime state. Run it from a complete package; copying the executable alone does not install its assets. Unsafe destination paths and symlink parents fail before writes. Newly created Markdown under `docs/` retains its template title and provisional status and gets `created_at` for its UTC creation day; source-template update/archive dates are not copied. Agent entrypoints remain plain Markdown. Existing document dates and contents are preserved.

Initialization is a starting point. The responsible agent completes the project's purpose, vocabulary and scope from evidence and owner decisions, maps current authorities and preserves useful history. The [Markdown foundation](../skills/hivex/references/markdown.md) and [language-template catalogue](../templates/README.md) guide adoption. Installing a package does not replace an existing project's owner policy.

New installations ignore `.hivex/` as retained local state and `.reviews/` for local review traces; `init` appends whichever of these final rules is missing, matching the file's line endings. Previously tracked graph files remain tracked until the project deliberately preserves and retires them; `init` neither deletes nor untracks them. Existing nested ignore files are preserved.

## Select sources

Documents need no Git repository or committed revision. Keep them at their real project, package or module scope. Optional `hivex.json` selects relative Markdown globs:

```json
{
  "include": ["docs/**/*.md", "packages/**/*.md"],
  "exclude": ["docs/generated/**"],
  "archive": ["docs/archive/**/*.md"]
}
```

Without configuration, Hivex selects Markdown under the project. Dependencies, Git metadata, generated directories (`vendor`, `dist`, `build`, Rust `target`), private dot directories and retained state are skipped. Explicit include/archive patterns can name generated documentation directories when that scope is intended. Explicit documentation directories can be selected; symlinks, protected directories and scope escapes are rejected. `exclude` wins over `archive`. The old `history` configuration field remains compatible; specifying both `history` and `archive` is invalid. Legacy `collections` configuration requires migration. Archive membership marks history explicitly rather than changing what its original status meant.

## Recover context

Start from the documentation map and the authority for the task:

```sh
npx hivex sources --limit 20
npx hivex search "cache access revocation"
npx hivex relations docs/policy.md --direction both
npx hivex read docs/policy.md --from 20 --to 45
```

All operations use current working-copy sources without model calls or graph/SQLite-state access. Read scope, conditions, exceptions and replacements. A preview, search match, empty navigation result or accepted label does not establish applicability. Follow relevant indirect relationships by querying and reading the next document. Reuse visited versions/ranges to avoid reading cycles; read a known authority directly instead of invoking every command by ceremony. Apply settled decisions autonomously; expand with available documentation and tools, then ask the owner only if they cannot resolve a consequential question.

### Search source passages

`search <query>` uses deterministic SQLite FTS5/BM25 in memory over the original nonoverlapping 32-line lexical windows. It tokenizes Unicode terms after lowercase/NFKC normalization and searches with OR semantics. Keeping those ranking units avoids penalizing useful terms merely because their section is longer. Titles are not copied into every window. Ranking is lexical relevance, not semantic approval.

For each match, the shared Markdown parser identifies the sections touched by its window, including child subsections through the next same-or-higher heading. The smallest contiguous range covering those sections and the original match is returned when it fits 8192 UTF-8 bytes. It may include several adjacent sections. Larger context retains the original complete lexical window with coordinates for further reading. Expansion never narrows away text from the ranked match. Identical expanded ranges are returned once with their best original score; distinct larger-window loci remain available. Returned context ranges can still overlap, so reuse already read text. Fenced code and quoted/list headings do not create section boundaries. No related document or whole corpus is expanded automatically.

Before applying a match, read its complete relevant section and governing document context, or the whole authority when their boundaries are unclear. Follow relevant dependencies, incoming exceptions and replacements, and compare the proposed behavior with those rules under the [shared context-reading method](../skills/hivex/references/markdown.md#recover-enough-context-to-apply-a-rule). Reuse already inspected current passages; context expansion does not recover every document-wide condition or certify comprehension.

Each `matches` entry retains `document`, `version`, `historical`, `lineStart`, `lineEnd` and exact `text`, reproducible with `read`. Its `context` adds `lineStart`, `lineEnd` and `complete` for the structural range. False means read that range when needed; true confirms only that its source text is present, not that all applicable rules are known. No unbounded heading label is copied into each match. Ordinary searches exclude archives. Use `--historical` to include selected history or repeated `--source <document>` to restrict the search to specified selected documents, including archives. Invalid or excluded explicit sources are errors.

`--limit` defaults to 6, maximum 64. `--max-bytes` defaults to 16384, maximum 65536, and bounds the complete serialized UTF-8 JSON. Passages are never cut to fit. Follow `continuation` with `--cursor` and identical sources/options; changed versions or options return `INVALID_CURSOR`. Cursors from the prior unexpanded search contract are also invalid. If the next complete passage cannot fit, `OUTPUT_LIMIT` cites its document/range for focused `read`. Indexing more than 32768 nonblank passages returns `SEARCH_LIMIT`; narrow source selection instead of accepting silent omissions. Source-discovery warnings mark `status` and `coverage` as `partial`. The shared parser also bounds each document to 32768 heading/explicit-anchor records; excess returns `ANCHOR_LIMIT` for narrower source selection. A complete empty lexical result proves only absence of matching terms within the declared scope.

The shared metadata convention requires topic `tags` in documentation under `docs/`. Their frontmatter text is already searchable, so `search "data-retention"` can find that keyword even when it occurs only in tags. Tags neither select authority nor imply a relationship; this is ordinary lexical search, without a tag-only filter or a separate index.

### Navigate authored relationships

`relations <document>` uses an exact selected document ID, without a source fragment. `--direction outgoing|incoming|both` defaults to `both`. Incoming navigation scans ordinary sources and the queried document; other archived declarations are excluded. Historical destinations remain reachable, and explicitly querying an archived document reads its own declarations.

Results preserve `kind`, exact `literal`, authored `from`/`to` and `reason`. `navigation` is `outgoing`, `incoming` or `self`. References carry source versions, historical flags, optional destination anchors and one-based ranges. Read the destination range, then query the next document to follow relevant indirect dependencies. No recursion or relationship inference runs automatically.

The [authored relationship contract v1](../skills/hivex/references/markdown.md#make-relationships-explicit) owns the five literals and exact `## Relationships` grammar. Invalid declarations, duplicate entries, missing targets and missing/ambiguous anchors are located errors. Ordinary prose links outside the block remain useful navigation.

`--limit` defaults to 20, maximum 2048. `--max-bytes` defaults to 16384, maximum 65536, including complete JSON metadata. A cursor binds versions and query options. Entries are never cut; a nonfitting entry returns `OUTPUT_LIMIT`. Source warnings produce partial coverage. A query validates at most 2048 declarations across its scanned sources; excess returns `RELATION_LIMIT`, not a truncated success. Narrow source selection or query outgoing relations of one document when that bound is reached.

### Read exact sources

`sources` lists source metadata with `--limit`, `--max-bytes` and snapshot-bound `--cursor`. `read <document> --from <line> --to <line>` returns exact source text and version. Read's `--max-bytes` bounds the text payload on complete line boundaries; metadata remains additional. Inspect `truncated` and `continuation`, and continue when needed. Reading an archived document explicitly preserves its historical flag.

## Validate structure

`check` validates outer metadata for all selected Markdown under any `docs/` directory, including nested docs, templates and archived wrappers. It validates body references and formal relationships in ordinary selected sources, including Markdown outside `docs/`. Repeated `--source <document>` restricts the checked authorities/callers and includes a historical body when selected; `--historical` includes all selected historical bodies explicitly. The default checks archived wrapper metadata without reopening frozen body conventions. Historical targets can be validated without adding their declarations to the ordinary backlog. Checks reuse the relationship parser and target/anchor resolver, plus source-coordinate Markdown links; external (including protocol-relative) and non-Markdown links remain outside this scope. Ordinary local links may contain a query, which is ignored when resolving their path/fragment; formal relationship targets retain their stricter query-free grammar. Missing or excluded Markdown targets, escapes and missing/ambiguous anchors are findings. Reserved `## Relationships` blocks must follow the shared grammar; rename a domain section with that heading while preserving its old anchor.

Metadata uses the [shared convention](../skills/hivex/references/markdown.md#document-dates): the eight allowed fields, their order, required fields, YAML types, exact states, real calendar dates and nonempty distinct lowercase kebab-case tags. Only the outer header is parsed; inner historical metadata stays original. Outer YAML is limited to 65536 bytes. `INVALID_METADATA` findings identify the source version, line, column and field when known. The checker validates `source` as nonempty text; it does not fetch that reference or certify its provenance. Dates are checked as calendar dates, without inferring Git history or changes. Legacy sources remain readable through `search` and `read` while their headers are migrated. A Relationships block is never required.

A report contains `checkedDocuments` (the union), `checkedMetadataDocuments`, `checkedReferenceDocuments`, `totalFindings`, source versions, located findings, warnings and explicit coverage. Version 0.9.0 introduces metadata validation and these separate counters. Check continuations use version 2; restart a check rather than resuming a version 1 cursor. `ready` means no findings in its structural scope; `failed` exits with code 1 and JSON on stdout; argument/configuration errors use stderr. Source omissions produce `partial` rather than a certificate of completeness. The checker does not infer relationships, semantic authority, contradiction, implementation or permission.

`--limit` defaults to 20, maximum 2048; `--max-bytes` defaults to 16384, maximum 65536, including the whole serialized JSON. Follow `continuation` with the same options and snapshot; a stale cursor fails. Findings are never cut to fit. A query supports at most 32768 local Markdown references and 2048 findings; excess returns `CHECK_LIMIT` for narrowed validation. Each document supports at most 32768 heading/explicit anchor records; excess returns `ANCHOR_LIMIT` without claiming valid navigation. Query-local reuse retains at most 65536 target anchors across documents; excess also returns `ANCHOR_LIMIT` for narrower source selection.

## Maintain project knowledge

Before writing, inspect the map and search existing current and relevant historical authorities. Update the existing home for the topic/scope. Keep acceptance, implementation, verification and permission distinct under the [state catalogue](../skills/hivex/assets/project/docs/adr/README.md#decision-and-delivery-states). Maintain affected Markdown, authored relationships and callers alongside code changes. Validate affected sources with `check --source`, inspect omissions and continuations, and check supported behavior before integration; independent review judges semantic correctness against a defined source/code revision.

<a id="update-and-repair-knowledge"></a>
## Migrate from the inferred graph runtime

Version 0.8.0 retires `update`, `ask`, `neighbors`, model-assisted `review`, `warnings`, `snapshot`, `recover`, `prune` and graph `status`. They return `COMMAND_RETIRED`, without opening or changing legacy data. `search` now returns source passages rather than extracted decisions. Model/profile/budget options are no longer accepted by source commands. Use the responsible agent and workflow skills for interpretation and review.

Before removing a project's old shared graph from ordinary tracking, preserve its useful decisions in Markdown and retain needed execution evidence. Keep original SQLite, snapshots, failed candidates, receipts, unknown consumption and consumed budgets unchanged; do not relabel pending work as completed. The [0.7.8 command record](archive/runtime/graph-cli-0.7.8.md) and [domain contract](archive/runtime/domain-contract-0.7.8.md) describe historical behavior. Use a preserved pinned legacy package only for a concrete historical investigation; the current CLI does not resume those operations or silently replay them.

The former #152 check-context optimization is superseded by runtime retirement, not completed graph maintenance. Its checkpoint and evidence remain preserved. Source-only operations create no persistent index or query artifacts. SQLite remains solely an ephemeral FTS implementation; existing execution databases remain historical files.

## Development

Use stable Rust and Git 2.45 or newer. The [engineering guideline](guidelines/engineering.md) and [Rust quality standard](guidelines/rust-quality.md) own review, checks and module boundaries.

```sh
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo run --locked --bin hivex-dev -- pack
cargo run --locked --bin hivex-dev -- verify
```

CI runs unit/quality/architecture checks and the complete public CLI suite against the packaged release executable. `HIVEX_TEST_BINARY` selects that executable. Package verification runs with a PATH excluding Bun/Node and confirms initialization, source retrieval and exact preservation of fixed legacy SQLite/snapshot bytes. No synthetic model server or model test suite remains. The Rust development tool and system `tar` prepare the npm-compatible archive; existing artifacts are never overwritten. Follow the [release procedure](procedures/releasing.md) for exact artifact verification, installation and publication.

## Relationships

- Implements [Reliable Markdown workflow](adr/0014-reliable-markdown-and-explicit-relationships.md#decision): focused reading and authored navigation apply the source-first policy.
