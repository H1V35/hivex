---
title: "HiveX guide"
status: accepted
created_at: 2026-09-17
updated_at: 2026-10-30
tags: [cli, retrieval, adoption]
---

# HiveX guide

HiveX is an AI-first CLI for agents and people. The principal agent reasons and implements; HiveX supplies reusable workflow skills and deterministic retrieval of repository Markdown. No command invokes a model or requires a persistent knowledge database.

## Install the CLI

HiveX is one executable with the six skills, the project foundation and the language templates built in; it needs no Node.js, Bun, npm, model credentials or Codex process. Each release `vX.Y.Z` on [GitHub Releases](https://github.com/H1V35/hivex/releases) provides prebuilt archives for macOS, Linux and Windows: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu` and `x86_64-pc-windows-msvc`. Each archive, `hivex-vX.Y.Z-<target>.tar.gz` (`.zip` on Windows), contains the executable, `LICENSE` and `THIRD-PARTY-NOTICES.txt`, and has a `.sha256` checksum file beside it. Download yours from the Releases page or with the GitHub CLI, check it and put `hivex` (`hivex.exe` on Windows) on your `PATH`:

```sh
gh release download v0.14.3 --repo H1V35/hivex --pattern "*aarch64-apple-darwin.tar.gz*"
shasum -a 256 -c hivex-v0.14.3-aarch64-apple-darwin.tar.gz.sha256
tar -xzf hivex-v0.14.3-aarch64-apple-darwin.tar.gz
```

The macOS executables are not notarized, and a browser download is quarantined; remove that attribute with `xattr -d com.apple.quarantine hivex` after checking the archive, or download with `gh` or Cargo instead. Alternatively, build and install a release with stable Rust; `--force` replaces an installed version:

```sh
cargo install --git https://github.com/H1V35/hivex --tag v0.14.3 --locked hivex --bin hivex --force
```

Then run `hivex init` in the project root. Versions up to 0.11.1 were installed from the npm package `@h1v35/hivex`, now deprecated; existing projects [migrate from npm](#migrate-from-npm).

## Pin the release

`hivex.json` can pin the release a project uses with `version`, in `MAJOR.MINOR.PATCH` form:

```json
{
  "version": "0.14.3",
  "include": ["docs/**/*.md"]
}
```

`init` writes the installed version when it creates `hivex.json`, and fails with `VERSION_MISMATCH` (details `installed` and `pinned`) before writing anything when the project pins another release. `update` adds the pin to an existing `hivex.json` or moves it forward to the installed release, reported as `hivex.json` in `updated`; it fails with `VERSION_MISMATCH` rather than move the pin back. All other commands keep working and add `"versionWarning":{"installed":…,"pinned":…,"install":…}` to their result, where `install` is the command that installs the pinned release, `cargo install --git https://github.com/H1V35/hivex --tag vX.Y.Z --locked hivex --bin hivex --force`. A missing pin imposes nothing; an invalid one fails with `INVALID_CONFIG`.

Commit `hivex.json` so collaborators and agents use the same release. The guide for a pinned release is `https://github.com/H1V35/hivex/blob/vX.Y.Z/docs/guide.md`; `hivex --help` lists the installed command contract.

## Initialize a project

`init [--root <project>]` prepares missing foundation documents, a source configuration [pinned](#pin-the-release) to the installed release, a brief `AGENTS.md` and the six skills built into the executable. It copies each skill completely into `.agents/skills/<name>/`: its `SKILL.md`, `references/` and `assets/`, including the project foundation under `hivex/assets/project/` and the language templates under `hivex/templates/`, so the skills' relative links keep working. It writes the same copy into `.claude/skills/<name>/`, because Claude Code loads project skills only from `.claude/skills`. The project commits both, so every clone and worktree has the skills on macOS, Linux and Windows without a further step. It does not create `CLAUDE.md`: Claude Code 2.1.277 and later reads `AGENTS.md` natively when no `CLAUDE.md`, `.claude/CLAUDE.md` or `CLAUDE.local.md` exists in the working directory or above it. Where that support is [unavailable](https://code.claude.com/docs/en/memory#when-agents-md-support-is-unavailable), such as older versions, the first session after upgrading or Amazon Bedrock and disabled-telemetry sessions before 2.1.281, keep a project-owned `CLAUDE.md` containing `@AGENTS.md`; `init` preserves an existing `CLAUDE.md`. It creates missing foundation and skill files and never changes existing ones, preserving configuration, custom skills and historical runtime state; the one exception is the [skills section](#the-skills-section-of-agentsmd) of `AGENTS.md`. The executable is self-contained, so a standalone copy works anywhere. Unsafe destination paths, symlinked destinations and symlink parents fail before writes. Newly created Markdown under `docs/` retains its template title and provisional status and gets `created_at` for its UTC creation day; source-template update/archive dates are not copied. Agent entrypoints remain plain Markdown. Existing document dates and contents are preserved. Version 0.10.0 adds the cross-vendor independent review procedure with its report schema (initially `gpt-6.1-sol` with `xhigh` effort through Codex) and the self-hosted runner pool procedure for private repositories; other files under `docs/`, such as that schema, are copied unchanged. Version 0.10.1 stops creating `CLAUDE.md` while preserving an existing one, and appends the `/.hivex/` and `/.reviews/` ignore rules only when they are absent as exact lines after the file's last negation, so rules already present mid-file are not duplicated. Version 0.11.0 reports write failures as `INIT_WRITE_FAILED` instead of `READ_FAILED` and determines the UTC creation day without an external process.

Initialization is a starting point. The responsible agent completes the project's purpose, vocabulary and scope from evidence and owner decisions, maps current authorities and preserves useful history. The [Markdown foundation](../skills/hivex/references/markdown.md) and [language-template catalogue](../skills/hivex/templates/README.md) guide adoption. Installing a release does not replace an existing project's owner policy.

New installations ignore `.hivex/` as retained local state and `.reviews/` for local review traces; `init` appends whichever of these final rules is missing, matching the file's line endings. Commit the skill copies in `.agents/skills/` and `.claude/skills/`. Earlier releases linked `.claude/skills/<name>` to the copy in `.agents/skills/` on each machine and ignored the links with `/.claude/skills/hivex*` and, for extension skills, `/.claude/skills/<name>`, so new clones and worktrees lacked the skills in Claude Code. [`update`](#update-a-project) replaces the links with copies and removes those exact rules wherever they appear, unless an extension declares one; `init` leaves both, so a kept link never becomes visible to Git ([#244](https://github.com/H1V35/hivex/issues/244)); a link an earlier version committed becomes a directory the project commits in its place. Previously tracked graph files remain tracked until the project deliberately preserves and retires them; `init` neither deletes nor untracks them. Existing nested ignore files are preserved.

### The skills section of `AGENTS.md`

`AGENTS.md` carries a section, between `<!-- hivex:skills -->` and `<!-- /hivex:skills -->`, stating which HiveX skill each kind of work loads: `hivex-design` with the project's design guide and the platform's interface guidelines for anything people see, `hivex-implement`, `hivex-document`, `hivex-review` and `hivex-git` for their work, `hivex` for every task's sources, and delegated work following the section. `init` writes it into a new entrypoint, including an extension's, and adds it to an existing one that lacks it, in the file's line endings: before a final `## Relationships` section, which stays last, or else at the end. `update` replaces what lies between the markers with the installed release's section. The rest of the file stays the project's. A lone or misordered marker fails with `INVALID_AGENTS` before any write; a file that is not UTF-8 text is left unchanged. Version 0.14.0 adds the section ([#230](https://github.com/H1V35/hivex/issues/230)).

`init` also installs the [ponytail](https://github.com/DietrichGebert/ponytail) plugin (MIT) in each host whose CLI is on `PATH`. It first runs `<host> plugin list --json`; when `ponytail@ponytail` is listed it changes nothing, because installing again would replace Codex's copy and re-enable a plugin the owner disabled. Otherwise it runs `claude plugin marketplace add DietrichGebert/ponytail` and `claude plugin install ponytail@ponytail`, or `codex plugin marketplace add DietrichGebert/ponytail` and `codex plugin add ponytail@ponytail`. Each host stops at its first failing step. The report's `plugins` entries are `current` when ponytail is installed (already or now), `missing` when the CLI is not installed and `failed` when a step fails; file results are unaffected and the exit code stays 0. The plugin hooks need Node.js; Codex asks to trust them in `/hooks`, and running apps load updates after a restart. Contract tests replace both CLIs with recording stand-ins and otherwise run with an empty `PATH`, so they never touch real installations.

## Update a project

`update [--root <project>]` brings what HiveX manages to the installed release and never creates or changes project files other than the pin in `hivex.json` and the [skills section](#the-skills-section-of-agentsmd) of `AGENTS.md`. It makes the skill files in `.agents/skills/` and `.claude/skills/` identical to the installed release, reporting changed files in `updated` and missing ones in `created`, restores any missing managed ignore rule as `init` does, removes the former link rules, makes the skills section of an existing `AGENTS.md` current, reporting the file in `updated` when it changes, and [moves the pin forward](#pin-the-release). A former npm link at `.agents/skills/<name>`, pointing into `node_modules`, is replaced with a copy: the directory is reported in `updated` and its files in `created`. Claude's former link at `.claude/skills/<name>` to the copy in `.agents/skills/<name>`, a symlink or, on Windows, a directory junction, is replaced the same way; a link that points elsewhere is the project's and is reported in `preserved`. It then brings the ponytail plugin to its latest release with idempotent steps: `claude plugin marketplace add DietrichGebert/ponytail`, `claude plugin marketplace update ponytail`, `claude plugin install ponytail@ponytail` and `claude plugin update ponytail@ponytail`; `codex plugin marketplace add DietrichGebert/ponytail`, `codex plugin marketplace upgrade ponytail` and `codex plugin add ponytail@ponytail`. Its report has the same shape as `init`, with `"command":"update"` and `current` for each host whose steps all succeeded. After installing a new release, run `hivex update` and commit the changed skill files in both directories, `.gitignore` and `hivex.json`.

## Extensions

An extension adds skills and project files to the bundled foundation for a particular kind of project or person, without forking the CLI. It is a Git repository whose `vX.Y.Z` tags are releases:

- `hivex-extension.json`: a JSON object. Optional `replaces` lists bundled skills the extension removes, for example `["hivex-git"]`; optional `ignore` lists `.gitignore` rules its projects need, such as `["/work/"]`, appended and kept like HiveX's own managed rules, which always come last. Negated rules (`!…`) are refused, so an extension cannot re-include HiveX's local state.
- `skills/<name>/…`: complete skills. A skill with the name of a bundled skill replaces it entirely.
- `assets/project/…`: project files. A file at the same path as a bundled foundation file replaces it; others are added. Markdown under `docs/` receives the creation date like bundled files. A replacing file keeps the headings or explicit anchors that bundled documents and skills link to; verify an extension by running `hivex init --extension <path>` in an empty folder and `hivex check` there.

Install an extension in a new project with `hivex init --extension <repository>`, where `<repository>` is `OWNER/REPO` on GitHub, a Git URL (including `user@host:path`), an absolute path or a path starting with `./` or `../`, resolved from the project root. HiveX needs `git`; private repositories need Git credentials, for example `gh auth setup-git`. `init` pins the newest release in `hivex.json`:

```json
{"version": "0.14.3", "extensions": [{"repository": "OWNER/REPO", "version": "1.2.0"}]}
```

A project that already has `hivex.json` lists extensions there instead; `init --extension` refuses to change an existing configuration. `init` installs each pinned release, so every machine gets the same files, and `update` moves each extension to its newest release, rewrites its skills and records the new versions. Project files keep the rule of every project file: created when missing, never changed afterwards; an extension's `.gitignore` only starts a missing one, to which the managed rules are appended. HiveX fetches exactly the `refs/tags/vX.Y.Z` tag, without line-ending conversion, validates it before caching it in the ignored `.hivex/extensions/` (a real directory, never a link) under a name derived from the repository, and keeps only the pinned release of each repository there. Extension files keep their Unix execute permission; symbolic links inside an extension are skipped, and linked source directories or names that could leave the tree make it invalid.

`.agents/.hivex-managed.json` records the skill files HiveX installed in both directories, and Claude's links in projects an earlier release prepared. `update` removes those a newer release no longer provides, listing them under `removed`; in projects initialized before the inventory existed, only the bundled skill files and links that are present count as managed, so other skills, even with a `hivex` prefix, are kept. `init` removes nothing and keeps recording what an earlier release installed. An inventory listing paths outside `.agents/skills/<skill>/` or `.claude/skills/<skill>` fails with `INVALID_CONFIG`.

The init and update reports list the installed `extensions`. A missing `git`, an unreachable repository or a repository without release tags fails with `EXTENSION_UNAVAILABLE`; an invalid manifest, or one that replaces an unknown skill, fails with `INVALID_EXTENSION`. Nothing is written in either case.

## Select sources

Documents need no Git repository or committed revision. Keep them at their real project, package or module scope. Optional `hivex.json` selects relative Markdown globs:

```json
{
  "include": ["docs/**/*.md", "packages/**/*.md"],
  "exclude": ["docs/generated/**"],
  "archive": ["docs/archive/**/*.md"]
}
```

Without configuration, HiveX selects Markdown under the project. Dependencies, Git metadata, generated directories (`vendor`, `dist`, `build`, Rust `target`), private dot directories and retained state are skipped. Explicit include/archive patterns can name generated documentation directories when that scope is intended. Explicit documentation directories can be selected; symlinks, protected directories and scope escapes are rejected. `exclude` wins over `archive`. The old `history` configuration field remains compatible; specifying both `history` and `archive` is invalid. Legacy `collections` configuration requires migration. Archive membership marks history explicitly rather than changing what its original status meant.

## Recover context

Start from the documentation map and the authority for the task:

```sh
hivex sources --limit 20
hivex search "cache access revocation"
hivex relations docs/policy.md --direction both
hivex read docs/policy.md --from 20 --to 45
```

All operations use current working-copy sources without model calls or graph/SQLite-state access. Every command prints one JSON value: its result on stdout, or `{"error": {...}}` on stderr with exit code 1. Invalid arguments, including unknown options, missing option values and non-UTF-8 arguments, return `INVALID_ARGUMENT`. Version 0.11.0 unifies that code; earlier versions returned `READ_FAILED` for some unknown options and aborted on non-UTF-8 arguments. Read scope, conditions, exceptions and replacements. A preview, search match, empty navigation result or accepted label does not establish applicability. Follow relevant indirect relationships by querying and reading the next document. Reuse visited versions/ranges to avoid reading cycles; read a known authority directly instead of invoking every command by ceremony. Apply settled decisions autonomously; expand with available documentation and tools, then ask the owner only if they cannot resolve a consequential question.

### Search source passages

`search <query>` uses deterministic SQLite FTS5/BM25 in memory over the original nonoverlapping 32-line lexical windows. It tokenizes Unicode terms after lowercase/NFKC normalization and searches with OR semantics. Keeping those ranking units avoids penalizing useful terms merely because their section is longer. Titles are not copied into every window. Ranking is lexical relevance, not semantic approval.

For each match, the shared Markdown parser identifies the sections touched by its window, including child subsections through the next same-or-higher heading. The smallest contiguous range covering those sections and the original match is returned when it fits 8192 UTF-8 bytes. It may include several adjacent sections. Larger context retains the original complete lexical window with coordinates for further reading. Expansion never narrows away text from the ranked match. Identical expanded ranges are returned once with their best original score; distinct larger-window loci remain available. Returned context ranges can still overlap, so reuse already read text. Fenced code and quoted/list headings do not create section boundaries. No related document or whole corpus is expanded automatically.

Before applying a match, read its complete relevant section and governing document context, or the whole authority when their boundaries are unclear. Follow relevant dependencies, incoming exceptions and replacements, and compare the proposed behavior with those rules under the [shared context-reading method](../skills/hivex/references/markdown.md#recover-enough-context-to-apply-a-rule). Reuse already inspected current passages; context expansion does not recover every document-wide condition or certify comprehension.

Each `matches` entry retains `document`, `version`, `historical`, `lineStart`, `lineEnd` and exact `text`, reproducible with `read`. Its `context` adds `lineStart`, `lineEnd` and `complete` for the structural range. False means read that range when needed; true confirms only that its source text is present, not that all applicable rules are known. No unbounded heading label is copied into each match. Ordinary searches exclude archives. Use `--historical` to include selected history or repeated `--source <document>` to restrict the search to specified selected documents, including archives. Invalid or excluded explicit sources are errors.

`--limit` defaults to 6, maximum 64. `--max-bytes` defaults to 16384, maximum 65536, and bounds the complete serialized UTF-8 JSON. Passages are never cut to fit. Follow `continuation` with `--cursor` and identical sources/options; changed versions or options return `INVALID_CURSOR`. Cursors from the prior unexpanded search contract are also invalid. If the next complete passage cannot fit, `OUTPUT_LIMIT` cites its document/range for focused `read`. Indexing more than 32768 nonblank passages returns `SEARCH_LIMIT`; narrow source selection instead of accepting silent omissions. Source-discovery warnings mark `status` and `coverage` as `partial`. The shared parser also bounds each document to 32768 heading/explicit-anchor records; excess returns `ANCHOR_LIMIT` for narrower source selection. A complete empty lexical result proves only absence of matching terms within the declared scope.

The shared metadata convention requires topic `tags` in documentation under `docs/`. Their frontmatter text is already searchable, so `search "data-retention"` can find that keyword even when it occurs only in tags. Tags neither select authority nor imply a relationship; this is ordinary lexical search, without a tag-only filter or a separate index.

### Navigate authored relationships

`relations <document>` uses an exact selected document ID, without a source fragment. `--direction outgoing|incoming|both` defaults to `both`. Incoming navigation scans ordinary sources and the queried document; other archived declarations are excluded. Historical destinations remain reachable, and explicitly querying an archived document reads its own declarations.

Results preserve `kind`, exact `literal`, authored `from`/`to` and `reason`. `navigation` is `outgoing`, `incoming` or `self`. References carry source versions, historical flags, optional destination anchors and one-based ranges. Version 0.11.0 gives an explicit anchor on the line right after a heading the nearest following heading's section, as the relationship contract specifies; earlier versions ended it with the preceding heading's section. Read the destination range, then query the next document to follow relevant indirect dependencies. No recursion or relationship inference runs automatically.

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

## Migrate from npm

Versions up to 0.11.1 were distributed as the npm package `@h1v35/hivex`, deprecated after that release, and `init` linked `.agents/skills/<name>` into `node_modules`. To move a project to a release:

1. [Install the CLI](#install-the-cli) from a release archive or with Cargo.
2. Run `hivex update` in the project. It replaces each skill link into `node_modules`, and Claude's skill links, with copies and pins the installed version in `hivex.json`.
3. Remove the dependency with `npm uninstall @h1v35/hivex` (`bun remove @h1v35/hivex` in Bun projects).

Replace `npx hivex` and `bun hivex` in scripts and project instructions with `hivex`, then commit the copied skills, `hivex.json` and the dependency change.

<a id="update-and-repair-knowledge"></a>
## Migrate from the inferred graph runtime

Version 0.8.0 retires `update`, `ask`, `neighbors`, model-assisted `review`, `warnings`, `snapshot`, `recover`, `prune` and graph `status`. They return `COMMAND_RETIRED`, without opening or changing legacy data. Since 2026-10-19 the `update` name is reused for [project updates](#update-a-project), which never open graph data; the other names stay retired. `search` now returns source passages rather than extracted decisions. Model/profile/budget options are no longer accepted by source commands. Use the responsible agent and workflow skills for interpretation and review.

Before removing a project's old shared graph from ordinary tracking, preserve its useful decisions in Markdown and retain needed execution evidence. Keep original SQLite, snapshots, failed candidates, receipts, unknown consumption and consumed budgets unchanged; do not relabel pending work as completed. The [0.7.8 command record](archive/runtime/graph-cli-0.7.8.md) and [domain contract](archive/runtime/domain-contract-0.7.8.md) describe historical behavior. Use a preserved pinned legacy package only for a concrete historical investigation; the current CLI does not resume those operations or silently replay them.

The former #152 check-context optimization is superseded by runtime retirement, not completed graph maintenance. Its checkpoint and evidence remain preserved. Source-only operations create no persistent index or query artifacts. SQLite remains solely an ephemeral FTS implementation; existing execution databases remain historical files.

## Development

Use stable Rust and Git 2.45 or newer. The [engineering guideline](guidelines/engineering.md) and [Rust quality standard](guidelines/rust-quality.md) own review, checks and module boundaries.

```sh
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo build --locked --release --bin hivex
HIVEX_TEST_BINARY=target/release/hivex cargo test --locked
```

`HIVEX_TEST_BINARY` runs the public CLI contracts against the given executable (`target/release/hivex.exe` on Windows). Contract deadlines assume an optimized build, as users install it. The Quality workflow runs these steps on `macos-15`, `ubuntu-latest` and `windows-latest`; a final `quality` job, which the `main` ruleset requires, passes only when every platform passed. Contract tests that create symbolic links run only on Unix. No synthetic model server or model test suite remains.

The development tool has one command, `cargo run --locked --bin hivex-dev -- notices <target> <output>`, which writes the third-party license notices for a target's normal and build dependencies, excluding development-only analyzers. The release workflow ships them in each archive; follow the [release procedure](procedures/releasing.md) to publish a release.

## Relationships

- Implements [Reliable Markdown workflow](adr/0014-reliable-markdown-and-explicit-relationships.md#decision): focused reading and authored navigation apply the source-first policy.
