> Historical runtime record copied from `docs/guide.md` at Git revision `180ea2750e0da9253494b02e7bbbb25938720758` (0.7.8). The source-only replacement is [the current guide](../../guide.md#migrate-from-the-inferred-graph-runtime). Original behavior and accounting describe that retired version.

# Hivex guide

The primary path is reliable Markdown, explicit relationships and focused source retrieval. See [ADR 0014](../../adr/0014-reliable-markdown-and-explicit-relationships.md). Optional graph/model commands keep their actual compatibility contracts.

## Project foundation

Hivex combines incremental knowledge, local retrieval, optional model assistance and a small adoption workflow. Its five general skills cover design, documentation, implementation, independent review and Git/triage. Use the capabilities a task needs rather than a compulsory sequence.

No installed command approves an implementation. The principal reviewer checks findings, relevant tests and source evidence. The [knowledge decision](../../adr/0010-practical-knowledge-assistance.md) and [foundation decision](../../adr/0012-project-foundation-and-workflow.md) define these responsibilities.

## Install the CLI and skills

The native package targets macOS ARM64. The installed executable does not require Bun, Node.js or Rust; npm (or Bun) is used only to install the package. Other targets need native compatibility validation before distribution.

```sh
npm install -D -E @h1v35/hivex
npx hivex --help
```

The package includes `hivex`, `hivex-design`, `hivex-document`, `hivex-implement`, `hivex-review` and `hivex-git` under `skills/`. Install the directories together in the skill location your agent discovers. Keep them at the same release as the CLI; the relative references between these bundled skills should stay intact.

Run `npx hivex init` after installation. It links the six bundled skills into `.agents/skills` and exposes them through `.claude/skills`, keeping them at the installed CLI release. Existing skill directories or links are preserved. Parent directory symlinks are rejected before any files are written. Other agents can use the same packaged skill directories.

All examples run from the project root. Use `bun hivex` instead of `npx hivex` in a Bun project. For another directory, append `--root /path/to/project`. Install the scoped package first; if npx offers to download the unrelated unscoped `hivex` package, cancel. Automation can use `npx --no-install hivex` to forbid downloading.

The default knowledge profile needs an authenticated Codex CLI session with Luna/max available. The Codex integration also accepts an explicitly selected `--model` and `--effort` advertised by its runtime. Invocation validates the requested configuration and effective profile without silent fallback. Document discovery, source/version checks, snapshot operations and initialization do not require a model.

Codex CLI compatibility is established through the app-server protocol and the effective account, model and isolation settings, rather than an exact CLI version. Compatible tool updates remain usable; the actual CLI version is recorded with each admitted invocation. An incompatible protocol or profile stops execution instead of silently changing the knowledge model or its permissions.

## Initialize a project

```sh
npx hivex init
# Or prepare another existing project directory:
npx hivex init --root /path/to/project
```

Initialization creates missing foundation documents and skill links, a source configuration and Git ignore rules that keep `.hivex/graph.json` shareable while local execution state stays ignored. It reports created, preserved and updated paths, makes no model calls, and does not install dependencies, configure global tools or write to GitHub. Repeating it preserves existing Markdown, configuration, graph and history.

An existing `.hivex/.gitignore` with active patterns can override the root rules, exposing local state or hiding the snapshot. Initialization reports this conflict before writing files; reconcile those nested patterns with the root ignore policy before continuing.

The foundation includes a short `AGENTS.md`, a `CLAUDE.md` that imports it, documentation map, draft PRD and glossary, ADR directory, engineering and triage guidelines, and a tracker procedure. Language-specific standards are packaged separately under [`templates/`](../../../templates/README.md); the adopting agent applies the matching template to the project. `init` copies only the language-neutral foundation. The principal agent completes the project's actual purpose, vision and language from evidence and owner decisions. Draft headings do not stand in for those decisions.

Use the documentation skill to migrate existing material to the standard when reasonably possible, preserving useful content, links, history and monorepo/package/module scope. The CLI does not infer semantic migrations or overwrite existing sources. The Git skill aligns useful labels and tracker conventions within the owner's authorization; project-specific areas remain local.

## Run

From a source checkout with the stable Rust toolchain:

```sh
cargo run --locked -- --help
cargo run --locked -- sources
cargo run --locked -- update --max-calls 0
```

The scoped package name is `@h1v35/hivex`, with command `hivex` and MIT license. Publication and registry installation are tracked separately; do not fetch the unrelated unscoped npm package.

Documents need no Git repository or commit. They can live at monorepo, package or module level and use their project's own Markdown format. An optional `hivex.json` selects relative globs:

```json
{
  "include": ["docs/**/*.md", "packages/**/*.md", "src/**/decisions/*.md"],
  "exclude": ["docs/generated/**"],
  "archive": ["docs/archive/**/*.md"]
}
```

Without configuration, Hivex selects Markdown files under the project. It skips dependencies, its own cache, Git metadata and private dot directories; explicitly named documentation directories can be selected. `archive` declares additional Markdown that remains readable evidence while staying out of ordinary update and consultation ingestion. Use `--source <document>` with `ask` or `review` to select it, or let a known relationship bring back the bounded ranges it requires. Historical metadata and evidence carry `historical: true`; an extracted decision from that source remains `historical`, even when the transport suggests another status. An `exclude` glob wins over `archive`. Symlinks are not followed and protected directories and scope escapes remain rejected. The previous experimental `collections` configuration is rejected with a migration message rather than silently reinterpreted. The former `history` field is still read for existing projects; new configuration uses `archive`. Specifying both is an error. Renaming the field preserves snapshots, retained work and caches.

## Recover context

Start with [the documentation map](../../README.md), choose the task's current authority, then search and read only the passages needed:

```sh
npx hivex sources --limit 20
npx hivex search "cache access revocation"
npx hivex relations docs/policy.md --direction both
npx hivex read docs/policy.md --from 20 --to 45
```

`sources`, `search`, `relations` and `read` make no model calls. Search returns source matches as well as available derived decisions; verify a result against its current Markdown. A preview, status label or graph entry does not settle applicability. Read the complete condition and exception, and follow authored dependencies, qualifications and replacements, including indirect ones.

The [relationship convention](../../../skills/hivex/references/markdown.md#make-relationships-explicit) uses ordinary Markdown links with a reason and scope. Choose the source by the relationship's meaning rather than loading every document. Record the source version and any unread continuation; do not infer a missing decision from a partial excerpt.

Apply settled decisions autonomously. Expand with available documentation and tools when the first passage is insufficient. Ask the owner only after those routes cannot resolve a consequential question, with the evidence, impact and a recommendation.

### Navigate authored relationships

`relations <document>` reads the current selected Markdown directly, without a graph, SQLite store or model. The document is an exact source ID from `sources`; it does not accept a fragment. Use `--direction outgoing`, `incoming` or `both` (default) to choose direct connections. Incoming navigation scans ordinary sources and the queried source; other archived declarations are excluded. Historical destinations remain reachable, and querying an archived source explicitly reads its own declarations.

Each result preserves the authored `kind`, exact `literal`, `from`, `to` and `reason`. `navigation` is `outgoing`, `incoming` or `self`; it never reverses the written statement. References include the document, optional destination anchor, current version, historical flag and one-based `lineStart`/`lineEnd`. Read the destination range with `read`, then query that document to follow relevant indirect dependencies. Navigation does not recursively load the corpus.

The [shared relationship contract](../../../skills/hivex/references/markdown.md#make-relationships-explicit) defines the five literals and one-line entries under exact `## Relationships`. Missing targets, malformed entries, duplicate declarations and missing or ambiguous anchors return errors with source coordinates. Ordinary links outside this block are not formal relationships. An empty result means no declared connections within the reported source coverage, not proof that no relevant decision exists; use the map, search and reading to expand the evidence.

`--limit` defaults to 20 (maximum 2048), and `--max-bytes` defaults to 16384 (maximum 65536). The byte limit covers the complete serialized UTF-8 JSON response; no relation is cut. Follow a non-null `continuation` with `--cursor` and the same query options. Changed sources or options return `INVALID_CURSOR`; a complete next entry that cannot fit returns `OUTPUT_LIMIT`. Source-discovery warnings produce `status: partial` and `coverage: partial`; inspect those warnings before treating navigation as complete. Otherwise coverage is `selected-sources`, not every file in the repository. Structural validation does not certify the meaning of an authored relationship.

A query validates at most 2048 authored declarations across the sources it scans. Exceeding that limit returns `RELATION_LIMIT` with a source line, even if the requested page is small; the CLI never silently drops excess declarations or claims complete coverage. Narrow selected sources or use outgoing navigation for one document when a larger repository reaches this bound.

### Current sources and accessible history

Declare replaced history separately from ordinary source selection:

```json
{
  "include": ["docs/**/*.md"],
  "archive": ["docs/archive/**/*.md"]
}
```

Archived Markdown remains in Git and readable with `read`; it is excluded from ordinary ingestion and model consultation unless selected as evidence or required by a known relationship. Current authority documents link their applicable rules to history and replacements. An archive is evidence for its recorded scope, not an instruction to apply an obsolete command.

Use `read docs/archive/adr/old-decision.md` for an explicit historical question. Source/derived freshness and current/historical selection are distinct from semantic approval. Preserve reported omissions and unavailable evidence. [ADR 0011](../../adr/0011-shared-knowledge-and-selective-history.md) owns archival safety.

### Optional derived assistance

`neighbors`, `ask`, `review` and graph maintenance remain available, but are not the primary workflow or a merge gate. `ask` and model-assisted `review` can run a graph update before answering; they may consume several invocations. Use them only when that cost adds value. They do not replace source review or owner authorization.

The [optional CLI reference](../../reference/graph-cli.md) records budgets, profile continuity, recovery, graph warnings and snapshots. Existing failed work remains failed until its normal lifecycle records another disposition; this documentation change neither resumes it nor resets its consumption.


## Workflow skills and Markdown practice

The [Hivex skill](../../../skills/hivex/SKILL.md) handles local retrieval, incremental knowledge, uncertainty and accounting. The additional capabilities are [design](../../../skills/hivex-design/SKILL.md), [documentation](../../../skills/hivex-document/SKILL.md), [implementation](../../../skills/hivex-implement/SKILL.md), [review](../../../skills/hivex-review/SKILL.md) and [Git/triage](../../../skills/hivex-git/SKILL.md).

The [Markdown foundation](../../../skills/hivex/references/markdown.md) defines the recommended adoption layout, concise agent entrypoint, purpose, glossary, decisions, guidelines and procedures. Knowledge operations continue to accept other Markdown layouts. Preserve a project's useful rules and exceptions when adopting the baseline.

Prefer DDD and meaningful responsibilities, risk/value-based tests with TDD optional only for sufficiently defined critical flows, and one independent review by default. The reviewer matches the implementation agent's model and effort. Use local knowledge before model-assisted interpretation, and explain technical details clearly when they matter to the owner's understanding or decisions.

## Development

The Rust CLI reads the existing SQLite and shared snapshot v1 formats and retains work budgets and attempts. The GPT-6 upgrade retires the old cache generation as described in [Execution profiles](#execution-profiles). The TypeScript runtime was retired after compatibility validation under [#80](https://github.com/H1V35/hivex/issues/80). Its fixed SQL fixtures and synthetic protocol server remain as independent compatibility evidence.

Use the stable Rust toolchain and Git 2.45 or newer for development. The [Rust quality standard](../../guidelines/rust-quality.md) describes formatting, Clippy, metrics and domain boundaries. Cargo runs all unit and CLI integration tests, including the synthetic Codex server. No TypeScript, JavaScript, Bun or Node.js tooling is required.

```sh
cargo fmt --check
cargo check --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --test quality --test architecture
cargo test --locked
cargo run --locked --bin hivex-dev -- pack
cargo run --locked --bin hivex-dev -- verify
```

The CLI suite requires the native binary and has no fallback to another implementation. Cargo builds the test executables automatically. Its SQLite fixtures include retained answers and extraction/check caches with exhausted budgets. Model tests use `tools/test_codex.rs` and consume no real model calls. `HIVEX_TEST_BINARY` selects an already built CLI when testing the packaged executable; `HIVEX_TEST_CODEX_BINARY` can select the synthetic server for isolated native unit tests.

The development-only `hivex-dev` command builds and verifies the npm archive using Cargo and the system `tar`. It refuses to overwrite an existing artifact; provide a different output path for a new verification build. Only `bin/hivex` is distributed. `package.json` contains distribution metadata, not a second development toolchain. npm is needed only by the publisher or by users who choose registry installation.


The native artifact is verified on macOS ARM64, using system ICU for source ordering. Package preparation, archive inspection and publication of the exact approved bytes follow the [release procedure](../../procedures/releasing.md).

Tests use the public CLI and a simulated native transport. Real Luna evaluations are bounded and reported separately; simulated token usage is not a consumption measurement. Development is issue-first, with coherent PRs, an independent review covering scope/correctness/standards and CI on the final commit. See the [engineering workflow](../../guidelines/engineering.md).

Earlier candidate/fidelity/comparison/admission protocols and their tests are retired from the active CLI. Their code remains in Git history and historical evidence keeps its original results. They do not impose a requirement to reproduce an Opus graph or exhaustively replay an old gold suite.

## Source organization

The [domain decision](../../adr/0013-domain-modules-and-execution-integrations.md) defines module ownership. `src/documents` handles sources and parsing; `src/knowledge` owns the graph and its derived state; `src/work` owns typed progress, budgets and persistence; consultation and review compose them. `src/execution` defines the integration-neutral contract, and `src/integrations/codex` contains its protocol, admission and process lifecycle. CLI parsing/output and development tools remain separate.

CI runs unit tests in the development build and the complete CLI contracts once against the packaged release executable. `cargo test --locked` remains the convenient local check. The parser matrix belongs beside the parser; integration-specific protocol tests live in `tests/contracts/codex.rs`. Remove a test only when its meaningful guarantee is retained elsewhere or its behavior is retired.

## Update and repair knowledge

Optional graph maintenance, candidate correction, warning review and recovery are described in the [graph CLI reference](../../reference/graph-cli.md#update-and-repair-knowledge). Maintain source authority and explicit relationships under [the engineering policy](../../guidelines/engineering.md#knowledge-before-merge).

## Execution profiles

Optional invocation profiles and retained-work continuity are described in [the current CLI reference](../../reference/graph-cli.md#execution-profiles). No profile change or new invocation is implied by the Markdown-first workflow.

## Share knowledge through Git

Commit maintained Markdown, explicit relationships and preserved history together. Existing optional graph snapshots remain derived evidence with their original versions and uncertainty; the [snapshot reference](../../reference/graph-cli.md#share-knowledge-through-git) owns import/export safety. This transition preserves the current snapshot and local execution records rather than certifying or relocating stale graph entries.

## Support an implementation review

Read the task's sources and explicit relationships, then independently assess scope, correctness and standards. Optional model-assisted review and zero-call saved-report checks are documented in [the CLI reference](../../reference/graph-cli.md#support-an-implementation-review). Assistance is not implementation approval.

## Relationships

- Implements [Reliable Markdown workflow](../../adr/0014-reliable-markdown-and-explicit-relationships.md#decision): focused reading and authored navigation apply the source-first policy.
