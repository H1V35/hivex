# Hivex

A project foundation and reusable knowledge for autonomous agents. Hivex provides recommended Markdown, focused workflow skills and retrieval of decisions, dependencies and exceptions. Project Markdown remains authority and the responsible agent directs the work.

Hivex is a TypeScript/Bun CLI. The current knowledge profile is Luna/max through native Codex and the user's ChatGPT subscription, without silent fallback. The implementing agent may use another model. Model invocation is localized for future configuration; multiple providers are not yet validated.

## Project foundation

Hivex combines incremental knowledge, local retrieval, optional model assistance and a small adoption workflow. Its five general skills cover design, documentation, implementation, independent review and Git/triage. Use the capabilities a task needs rather than a compulsory sequence.

No installed command approves an implementation. The principal reviewer checks findings, relevant tests and source evidence. The [knowledge decision](docs/adr/0010-practical-knowledge-assistance.md) and [foundation decision](docs/adr/0012-project-foundation-and-workflow.md) define these responsibilities.

## Install the CLI and skills

Requires Bun 1.4.2 or newer:

```sh
bun add --dev --exact @h1v35/hivex
bun hivex --help
```

The package includes `hivex`, `hivex-design`, `hivex-document`, `hivex-implement`, `hivex-review` and `hivex-git` under `skills/`. Install the directories together in the skill location your agent discovers. Keep them at the same release as the CLI; the relative references between these bundled skills should stay intact.

For agents using project-local `.agents/skills`, package links avoid copied skills becoming stale. Run from the project root after installation:

```sh
mkdir -p .agents/skills
for skill in hivex hivex-design hivex-document hivex-implement hivex-review hivex-git; do
  target=".agents/skills/$skill"
  if [ ! -e "$target" ] && [ ! -L "$target" ]; then
    ln -s "../../node_modules/@h1v35/hivex/skills/$skill" "$target"
  fi
done
```

Existing skill entries are preserved; inspect them before replacing a custom or older installation. An agent using another discovery location can install the same directories there. The bundled skills do not require the former external general-workflow skill set.

The current knowledge profile needs an authenticated Codex CLI session with Luna/max available. Invocation validates that profile without silent fallback. Document discovery, source/version checks, snapshot operations and initialization do not require a model.

Codex CLI compatibility is established through the app-server protocol and the effective account, model and isolation settings, rather than an exact CLI version. Compatible tool updates remain usable; the actual CLI version is recorded with each admitted invocation. An incompatible protocol or profile stops execution instead of silently changing the knowledge model or its permissions.

## Initialize a project

```sh
bun hivex init
# Or prepare another existing project directory:
bun hivex init --root /path/to/project
```

Initialization creates missing foundation documents, a source configuration and Git ignore rules that keep `.hivex/graph.json` shareable while local execution state stays ignored. It reports created, preserved and updated paths, makes no model calls, and does not install dependencies, configure global tools or write to GitHub. Repeating it preserves existing Markdown, configuration, graph and history.

An existing `.hivex/.gitignore` with active patterns can override the root rules, exposing local state or hiding the snapshot. Initialization reports this conflict before writing files; reconcile those nested patterns with the root ignore policy before continuing.

The foundation includes a short `AGENTS.md`, documentation map, draft PRD and glossary, ADR directory, engineering and triage guidelines, and a tracker procedure. Language-specific standards are packaged separately under [`templates/`](templates/README.md); the adopting agent applies the matching template to the project. `init` copies only the language-neutral foundation. The principal agent completes the project's actual purpose, vision and language from evidence and owner decisions. Draft headings do not stand in for those decisions.

Use the documentation skill to migrate existing material to the standard when reasonably possible, preserving useful content, links, history and monorepo/package/module scope. The CLI does not infer semantic migrations or overwrite existing sources. The Git skill aligns useful labels and tracker conventions within the owner's authorization; project-specific areas remain local.

## Run

CI uses Bun 1.4.2 as the tested baseline; compatible newer runtimes are supported. In this checkout:

```sh
bun install
bun hivex --help
bun hivex sources --root /path/to/project
bun hivex update --root /path/to/project --max-calls 0
```

The scoped package name is `@h1v35/hivex`, with command `hivex` and MIT license. Publication and registry installation are tracked separately; do not fetch the unrelated unscoped npm package.

Documents need no Git repository or commit. They can live at monorepo, package or module level and use their project's own Markdown format. An optional `hivex.json` selects relative globs:

```json
{
  "include": ["docs/**/*.md", "packages/**/*.md", "src/**/decisions/*.md"],
  "exclude": ["docs/generated/**"],
  "history": ["docs/archive/**/*.md"]
}
```

Without configuration, Hivex selects Markdown files under the project. It skips dependencies, its own cache, Git metadata and private dot directories; explicitly named documentation directories can be selected. `history` declares additional Markdown that remains readable evidence while staying out of ordinary update and consultation ingestion. Use `--source <document>` with `ask` or `review` to select it, or let a known relationship bring back the bounded ranges it requires. Historical metadata and evidence carry `historical: true`; an extracted decision from that source remains `historical`, even when the transport suggests another status. An `exclude` glob wins over `history`. Symlinks are not followed and protected directories and scope escapes remain rejected. The previous experimental `collections` configuration is rejected with a migration message rather than silently reinterpreted.

## Recover context

```sh
bun hivex sources --root /path/to/project --limit 20
bun hivex read docs/policy.md --root /path/to/project
bun hivex read docs/archive/old-policy.md --root /path/to/project
bun hivex search "cache access revocation" --root /path/to/project
bun hivex neighbors <decision-id> --root /path/to/project --limit 24
bun hivex ask "What did the old cache policy require?" --source docs/archive/old-policy.md --root /path/to/project
bun hivex ask "How should private cached data behave when access is revoked?" --root /path/to/project
```

`sources` returns document metadata without their full text and a snapshot-bound continuation when more records remain. Resume with `--cursor`. `read` returns original text, its version and line ranges; use `--from`, `--to` and `--max-bytes` for a bounded passage. Continuation and omitted content remain explicit. A working version is not evidence of approval.

Search covers both extracted decisions and original Markdown, so terminology omitted from a summary remains discoverable. When needed, select a known document with `--source` in a consultation rather than reopening a settled question with the owner. Historical sources are not searched into an ordinary consultation merely because they are available; a focused source or a known dependency is required. If a necessary historical source is excluded or unavailable, the result names the missing evidence. An unconsulted historical or unavailable local Markdown link is reported for the responsible agent to assess with `read` or `--source`; merely linking to an archive does not ingest it automatically.

Search and neighbor traversal are deterministic and make no model calls. Neighbor traversal includes indirect connections within `--limit` and lists decisions it could not expand. Stale knowledge is not presented as current evidence. `status` reports available knowledge and documents requiring attention.

`ask` first detects added, changed and removed Markdown. It updates at most one bounded batch, prioritizing matching fragments, relevant documents and their known dependencies, then asks Luna/max over the available decisions and original Markdown. Its default budget is three calls for the complete update/check/answer operation. If the budget ends before the answer, repeat the same task with an authorized higher total: the work, progress and consumption are retained. An unchanged task reuses its answer. Use explicit `update` to advance remaining corpus batches; pending coverage stays visible.

Changed sources bring their known incoming and outgoing neighbors into comparison, including relationships supported by a third document. Deleted sources are removed from pending ingestion; `unavailableDocuments` identifies dependencies that can no longer be verified. Source-local check findings remain scoped, so unrelated consultations can use their valid knowledge. It explains applicability and uncertainty. The evidence text in the result is read from the cited source ranges, not copied from a model-generated quotation. Large sources are supplied as relevant units within the context limit; `omittedUnits` reports unread portions so a partial answer is not mistaken for complete coverage. Identical retained consultations are reused. A partial result remains useful within its declared limits.

Hivex does not compact, move or rewrite Markdown. Authors preserve the original historical text, dates, links and anchors; Hivex exposes the selected source version and line ranges while keeping historical applicability and conditions visible.

## Update and repair knowledge

```sh
bun hivex update --root /path/to/project --max-calls 2
```

An update splits large Markdown into line-preserving units of at most 8 KiB, preferring Markdown boundaries. Each round selects at most four units and 16 KiB of target text, then performs one additional check. Optional neighbor discovery is limited to 18 decisions and 8 KiB of evidence. Current endpoints of affected relationships and their evidence are required context, so those discovery limits do not silently remove them. If the complete packet exceeds `--max-context-bytes`, Hivex reports a context limit before starting the extraction. Original document IDs and line numbers survive splitting. Earlier rounds remain queryable while `pendingUnits` and `pendingDocuments` show unfinished coverage. Sources up to 32 MiB can be split, within a 64 MiB loaded-corpus limit; narrow the selected paths if that limit is reached. A line too large to fit is explicitly reported as unread, never silently cut.

Sources declared by `history` are not part of an ordinary update. A focused `ask` or `review` adds only its selected bounded units to the same resumable work; a known dependency can add the ranges needed to verify it. The work budget, graph knowledge and cache survive resumption. Changing an unrelated historical document does not start a fresh consultation budget. Current documents take priority when the loaded-corpus limit is reached; narrow the configured scope to access sources outside the reported coverage.

Each extraction and check is checkpointed. Resuming continues the same work and never repeats its completed rounds. Successful structured model results are cached in the same store by the complete request, schema and model profile; an identical request can be reused without a call, even when reconstructing earlier knowledge. Changed context invalidates that cache entry. Context discovery considers authored links, lexical matches and recent decisions; `relationshipCoverage` states that this is bounded, not exhaustive. Cache hits are reported separately from calls and tokens; this is an optimization, not documentary authority.

To correct derived knowledge against unchanged Markdown, use:

```sh
bun hivex update --root /path/to/project --repair docs/cache.md --reason "The source specifies seven days, not indefinite retention."
```

Repair replaces the affected unit's interpretations and revisits its relationships without editing Markdown. Its reason guides comparison with the source; it does not create new authority. Repeating the same completed repair reuses its work. Add `--source <document>` to an update or repair when a known supporting authority must be supplied explicitly, for example after a connection was lost. Its complete Markdown and current decisions become context; the option does not add that document to the ingestion targets. The same context budget applies, and the selection and its versions identify the work. A genuine unresolved documentary conflict still needs a decision by the responsible person.

The default explicit-update work budget is two invocation attempts and 131,072 input bytes. `--max-calls` and `--max-input-bytes` set totals for the complete work, including extraction, check and resumption. A zero-call update reports pending documents without invoking the model. An exhausted work item retains its progress; repeating the command does not reset its counter. An authorized larger total can complete the remaining stage without repeating completed extraction.

A failed or unfinished invocation is not retried automatically by increasing the budget. Inspect its reported outcome and usage first. `--retry-failed` can explicitly resume a safely ended failure within the same work budget; uncertain invocations remain blocked. A completed adverse check is not an invocation failure and is never retried by this flag. Uncertain or pending knowledge does not become a blanket pass. The project-local `.hivex/knowledge.sqlite` stores working knowledge and execution accounting; no source Markdown is rewritten. Preserve it when work evidence is needed. Storage is bounded at 64 MiB; do not delete an active store to hide unfinished calls or reset a work budget.

`recover` inspects abandoned work without invoking the model or killing processes. Live owners or native processes remain protected. If local processes ended but remote delivery is uncertain, `recover --acknowledge-uncertain` records an explicit acknowledgement; original reports and unknown usage remain visible. Recovery itself never retries: a subsequent `--retry-failed` uses the retained work budget. Do not treat acknowledgement as proof that the earlier remote turn completed.

`prune` releases space occupied by old completed work and cached results, retaining the graph and all unfinished work, attempts and budgets. It keeps the newest eight completed works and 64 cached results by default; `--keep-completed` and `--keep-caches` change those counts. Pruned answers can require a new model call when requested again. Export evidence before pruning if historical reports are needed; pruning is explicit, never an automatic budget reset.

Native operations accept `--codex` and `--deadline-ms`; the default deadline is 30 minutes. Consultation context defaults to 65,536 bytes and can be bounded with `--max-context-bytes`. Limits are reported, not met by silently cutting a rule or pretending omitted evidence was reviewed. Input-byte and call budgets limit work; reported token usage is actual consumption, including known failed attempts.

## Share knowledge through Git

```sh
bun hivex snapshot export --root /path/to/project
bun hivex snapshot import --root /path/to/project
```

`snapshot export` writes `.hivex/graph.json` atomically as stable, readable JSON. Commit that file alongside the Markdown it describes to share decisions, relationships, source versions, evidence, available provenance and coverage. It exports the graph, not work records, process identities, budgets or cached model answers. Snapshot operations make zero model calls.

A fresh clone can use `search`, `neighbors` and `status` directly from the shared snapshot without creating a local database. Its first update reuses matching ingestion units and starts local work accounting. If a local graph already exists, it takes precedence: use `snapshot import` to adopt a new shared version. Import refuses while local work is unfinished and never resets attempts or budgets. Complete or recover that work through its normal lifecycle first.

When Markdown moves, explicitly relocate its knowledge before the next update:

```sh
bun hivex snapshot relocate docs/old-guide.md docs/guidelines/guide.md --root /path/to/project
```

The old source must no longer be selected, and the destination must be selected current Markdown. Relocation preserves IDs, relationships, source versions and uncertainty, leaving existing work, cached answers and budgets intact. It makes zero model calls and refuses unfinished local work. An identical move to a destination without prior knowledge reuses ingestion coverage when all retained source evidence has matching, known versions. If content changed, source versions are mixed or missing, or the destination already had knowledge, its coverage becomes pending so the usual update/check can validate the result. Mismatched evidence remains stale until then. Keep the relocation report with the change and export the final graph; do not use relocation to hide unrelated missing evidence.

The snapshot response identifies current, stale and unavailable source versions, pending units and warnings. A changed or absent source is not silently current; matching sources remain reusable. Partial and uncertain knowledge can be shared with those states retained. Freshness is not proof that a model interpretation is correct: the cited Markdown remains authority.

Keep only the shared graph under version control, for example:

```gitignore
/.hivex/*
!/.hivex/graph.json
```

Read-only queries do not rewrite the snapshot. Export intentionally when reusable knowledge changes, not on every consultation. Invalid snapshots or symbolic-link paths fail without replacing local knowledge. Existing local stores continue to work without a shared file.

## Workflow skills and Markdown practice

The [Hivex skill](skills/hivex/SKILL.md) handles local retrieval, incremental knowledge, uncertainty and accounting. The additional capabilities are [design](skills/hivex-design/SKILL.md), [documentation](skills/hivex-document/SKILL.md), [implementation](skills/hivex-implement/SKILL.md), [review](skills/hivex-review/SKILL.md) and [Git/triage](skills/hivex-git/SKILL.md).

The [Markdown foundation](skills/hivex/references/markdown.md) defines the recommended adoption layout, concise agent entrypoint, purpose, glossary, decisions, guidelines and procedures. Knowledge operations continue to accept other Markdown layouts. Preserve a project's useful rules and exceptions when adopting the baseline.

Prefer DDD and meaningful responsibilities, risk/value-based tests with TDD optional only for sufficiently defined critical flows, and one independent review by default. The reviewer matches the implementation agent's model and effort. Use local knowledge before model-assisted interpretation, and explain technical details clearly when they matter to the owner's understanding or decisions.

## Development

```sh
bun run typecheck
bun run lint
bun run format:check
bun run test
```

Tests use the public CLI and a simulated native transport. Real Luna evaluations are bounded and reported separately; simulated token usage is not a consumption measurement. Development is issue-first, with coherent PRs, an independent review covering scope/correctness/standards and CI on the final commit. See the [engineering workflow](docs/guidelines/engineering.md).

Earlier candidate/fidelity/comparison/admission protocols and their tests are retired from the active CLI. Their code remains in Git history and historical evidence keeps its original results. They do not impose a requirement to reproduce an Opus graph or exhaustively replay an old gold suite.

## Support an implementation review

From the Git project root, supply the task and the base revision. Hivex captures the working change, including untracked files, and provides findings tied to code and Markdown versions:

```sh
hivex review "Change cache behavior" --base main --max-calls 3 > /tmp/hivex-review.json
hivex review --check /tmp/hivex-review.json
```

Use the same task and base to resume or reuse retained work. Update, knowledge check and review share one budget, including expansion of a partial report. Context is bounded; large changes must be narrowed or split into coherent reviews. Larger existing text files contribute diff excerpts with original line numbers and explicit omissions; their full-file versions still detect later changes. The principal reviewer verifies conflicts and exceptions and resolves supported contradictions before closing the change. A `ready` result means assistance is available, never that the implementation is approved. A saved report can be checked without a model; changed code or documents make it stale. Keep reports outside the project or in an ignored path so they do not become part of the change.
