# Hivex guide

A project foundation and reusable knowledge for autonomous agents. Hivex provides recommended Markdown, focused workflow skills and retrieval of decisions, dependencies and exceptions. Project Markdown remains authority and the responsible agent directs the work.

Hivex is a native Rust CLI organized by domain capability. The default knowledge profile is `gpt-6-luna` with effort `max` through Codex and the user's ChatGPT subscription. Execution integration, model and options are separate choices; only the Codex integration is currently shipped. The implementing agent may use another model. There is no silent model or provider fallback.

## Project foundation

Hivex combines incremental knowledge, local retrieval, optional model assistance and a small adoption workflow. Its five general skills cover design, documentation, implementation, independent review and Git/triage. Use the capabilities a task needs rather than a compulsory sequence.

No installed command approves an implementation. The principal reviewer checks findings, relevant tests and source evidence. The [knowledge decision](adr/0010-practical-knowledge-assistance.md) and [foundation decision](adr/0012-project-foundation-and-workflow.md) define these responsibilities.

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

The foundation includes a short `AGENTS.md`, a `CLAUDE.md` that imports it, documentation map, draft PRD and glossary, ADR directory, engineering and triage guidelines, and a tracker procedure. Language-specific standards are packaged separately under [`templates/`](../templates/README.md); the adopting agent applies the matching template to the project. `init` copies only the language-neutral foundation. The principal agent completes the project's actual purpose, vision and language from evidence and owner decisions. Draft headings do not stand in for those decisions.

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

```sh
npx hivex sources --limit 20
npx hivex read docs/policy.md
npx hivex read docs/archive/old-policy.md
npx hivex search "cache access revocation"
npx hivex neighbors <decision-id> --limit 24
npx hivex ask "What did the old cache policy require?" --source docs/archive/old-policy.md
npx hivex ask "How should private cached data behave when access is revoked?"
```

`sources` returns document metadata without their full text and a snapshot-bound continuation when more records remain. Resume with `--cursor`. `read` returns original text, its version and line ranges; use `--from`, `--to` and `--max-bytes` for a bounded passage. Continuation and omitted content remain explicit. A working version is not evidence of approval.

Search covers both extracted decisions and original Markdown, so terminology omitted from a summary remains discoverable. When needed, select a known document with `--source` in a consultation rather than reopening a settled question with the owner. Historical sources are not searched into an ordinary consultation merely because they are available; a focused source or a known dependency is required. If a necessary historical source is excluded or unavailable, the result names the missing evidence. An unconsulted historical or unavailable local Markdown link is reported for the responsible agent to assess with `read` or `--source`; merely linking to an archive does not ingest it automatically.

Search and neighbor traversal are deterministic and make no model calls. Neighbor traversal includes indirect connections within `--limit` and lists decisions it could not expand. Stale knowledge is not presented as current evidence. `status` reports available knowledge and documents requiring attention.

`ask` first detects added, changed and removed Markdown. It updates at most one bounded batch, prioritizing matching fragments, relevant documents and their known dependencies, then asks Luna/max over the available decisions and original Markdown. Its default budget is three calls for the complete update/check/answer operation. If the budget ends before the answer, repeat the same task with an authorized higher total: the work, progress and consumption are retained. An unchanged task reuses its answer. Use explicit `update` to advance remaining corpus batches; pending coverage stays visible.

Changed sources bring their known incoming and outgoing neighbors into comparison, including relationships supported by a third document. Deleted sources are removed from pending ingestion; `unavailableDocuments` identifies dependencies that can no longer be verified. Source-local check findings remain scoped, so unrelated consultations can use their valid knowledge. It explains applicability and uncertainty. The evidence text in the result is read from the cited source ranges, not copied from a model-generated quotation. Large sources are supplied as relevant units within the context limit; `omittedUnits` reports unread portions so a partial answer is not mistaken for complete coverage. Identical retained consultations are reused. A partial result remains useful within its declared limits.

Hivex does not compact, move or rewrite Markdown. Authors preserve the original historical text, dates, links and anchors; Hivex exposes the selected source version and line ranges while keeping historical applicability and conditions visible.

## Update and repair knowledge

```sh
npx hivex update --max-calls 2
```

An update splits large Markdown into line-preserving units of at most 8 KiB, preferring Markdown boundaries. Each round selects at most four units and 16 KiB of target text, then performs one additional check. Optional neighbor discovery is limited to 18 decisions and 8 KiB of evidence. Current endpoints of affected relationships and their evidence are required context, so those discovery limits do not silently remove them. If the complete packet exceeds `--max-context-bytes`, Hivex reports a context limit before starting the extraction. Original document IDs and line numbers survive splitting. Earlier rounds remain queryable while `pendingUnits` and `pendingDocuments` show unfinished coverage. Sources up to 32 MiB can be split, within a 64 MiB loaded-corpus limit; narrow the selected paths if that limit is reached. A line too large to fit is explicitly reported as unread, never silently cut.

Sources declared by `archive` are not part of an ordinary update. A focused `ask` or `review` adds only its selected bounded units to the same resumable work; a known dependency can add the ranges needed to verify it. The work budget, graph knowledge and cache survive resumption. Changing an unrelated historical document does not start a fresh consultation budget. Current documents take priority when the loaded-corpus limit is reached; narrow the configured scope to access sources outside the reported coverage.

Each extraction and check is checkpointed. Resuming continues the same work and never repeats its completed rounds. Successful structured model results are cached in the same store by the complete request, schema and model profile; an identical request can be reused without a call, even when reconstructing earlier knowledge. Changed context invalidates that cache entry. Context discovery considers authored links, lexical matches and recent decisions; `relationshipCoverage` states that this is bounded, not exhaustive. Cache hits are reported separately from calls and tokens; this is an optimization, not documentary authority.

To correct derived knowledge against unchanged Markdown, use:

```sh
npx hivex update --repair docs/cache.md --reason "The source specifies seven days, not indefinite retention."
```

Use `--repair-range docs/cache.md:20-35` instead of `--repair docs/cache.md` to revisit only the decisions overlapping those one-based lines and their affected relationships. The range expands to include each affected decision’s complete source passage, including overlapping passages; other knowledge in the same ingestion block stays intact. Repeat the option for multiple ranges. Explicit range repairs pack as many complete ranges as fit in the 16 KiB target-text budget, without the ordinary update's four-unit cap. Context and work budgets still apply before every call. A complete repair range must fit that same 16 KiB round limit; oversized ranges fail before a model call. The source must already be fully ingested at its current version; finish a normal update first if it changed. Whole-document repair remains available when needed. Retained work keeps its original planned ranges and budget, and repeating a completed range request reuses it even when an earlier CLI processed a broader block.

Repair replaces the affected unit's interpretations and revisits its relationships without editing Markdown. Its reason guides comparison with the source; it does not create new authority. Repeating the same completed repair reuses its work. Add `--source <document>` to an update or repair when a known supporting authority must be supplied explicitly, for example after a connection was lost. Its complete Markdown and current decisions become context; the option does not add that document to the ingestion targets. The same context budget applies, and the selection and its versions identify the work. A genuine unresolved documentary conflict still needs a decision by the responsible person.

Repair packets include the current interpretations of the target passages, not just their identifiers in previous relationships. These interpretations are context, not authority. The replacement must retain correct knowledge and supported dependencies, rewiring changed endpoints; correcting a model interpretation is not a source-authored policy supersession. A repair reason may cover several batches, but each round applies only its own target ranges. Unchanged supplied decisions can be referenced by their exact IDs; invalid references remain rejected. This context counts toward the existing limit rather than being silently omitted.

New materialized checks separate the proposed knowledge from the extraction-only list of interpretations being replaced. Prior decisions remain comparison evidence and explicitly indicate whether they still exist in the candidate. Checks also include retained relationships from earlier rounds of the same work between supplied endpoints with current, fully supplied citations, so accepted rounds are not mistaken for missing work. Unrelated graph edges are not copied into every check. Removed interpretations must not be mistaken for current defects; actual missing replacements or faulty retained endpoints still require findings. The relationship-loss and invalid-target guards remain unchanged, and a changed check request cannot reuse an old check receipt.

If a retained, staged candidate has a real decision error, use the same update arguments with `--retry-failed --correct candidate-correction.json`. This avoids another extraction while still requiring a fresh model check before admission. The JSON object contains `workId`, `checkInputHash` (from `candidateResolutionContext`), a source-backed `reason`, versioned `evidence` citations, and `decisions`: complete replacement extraction decisions with their existing local IDs. Replacements must stay in their original document and the pending target ranges; additions, deletions and relationship edits are unsupported. Unknown fields and stale checks, evidence or graph state are rejected. The correction and previous pending candidate are retained alongside the untouched attempts and usage. A correction is reviewed input, not Markdown authority or native extraction output. Its new check sees the materialized result and uses the original work's total budgets. With no remaining calls it stays pending; increasing the total resumes only the check. Replaying the same correction is idempotent, including after completion. `--correct` cannot be combined with `--resolve`, which is reserved for false findings without candidate changes.

New work checks the materialized decisions and relationships, including local validation discards. If it would lose relationships whose endpoints and evidence are still current, the previous graph remains available while the same check justifies their removal or identifies supported replacements. An unjustified loss returns `failed` with `RELATIONSHIP_LOSS`, preserving the previous graph and the attempted result. Current findings that affect a replacement or its endpoints still block it; inherited uncertainty remains visible but does not by itself veto an explicitly justified replacement. Inspect the evidence before a different repair; there is no automatic semantic retry. After correcting a local admission defect, `update` with the same arguments plus `--retry-failed --max-calls 0` can reassess that retained check without model calls. It requires the exact candidate/request and current evidence; graph divergence or a changed request rejects reuse. `work.retainedCheckAssessment` reports the local outcome while the original attempt and consumption remain intact. This does not rerun an adverse model check. Older unfinished work retains its original model request and budgets. When a repair explicitly references a supplied, current decision, Hivex retains that decision and shows its definition to the check. If an older release discarded that endpoint, a resumed repair can reuse the extraction and check the corrected candidate once within the same total allowance. Hivex first verifies the retained request against the older candidate; it does not repeat a check of an unchanged candidate. With `--max-calls 0`, a changed candidate remains pending without a model call.

Warning prevention uses that same normal check to review new uncertainties and expired closures. It may dismiss descriptive observations or revalidate expired closures that are untargeted and non-validation only when complete in-scope documents and the prior closure's complete evidence are in context. Missing evidence leaves the warning pending; the update may still finish with limitations. Findings or local validation failures prevent warning closures. Matching text alone is insufficient. No extra call, retry or automatic repair is added.

For a synthetic example, a closure about `docs/policy.md` that also depends on `docs/retention.md` can be revalidated only when both current documents are supplied; if `docs/retention.md` is omitted, it remains pending even when the policy paragraph is unchanged.

For new update work, the response includes `warningChanges` with `new`, `reopened` and `resolved` lists. Each list contains entries with `id`, `message` and `state`; `state` is `active` or `resolved`. Review `warningChanges.new` entries with `state: "active"` and `warningChanges.reopened` against the current sources. `warningChanges` compares recorded closures and warning IDs from when the work began; reopened includes earlier closures whose evidence is now stale. The baseline records closure disposition independently of source coverage freshness, so a current-source closure can exist before that source is ingested. `new` contains IDs absent at the start, and `resolved` contains IDs without a closure at the start that end with one. Resumption keeps the baseline; this reports that work's changes rather than cleaning the complete backlog.

`warningSummary` separates extraction `limitations`, check `findings`, local `validation` errors and unclassified legacy `unknown` warnings; `sources` counts discovery or ingestion limits. The original messages and scopes remain available. A limitation alone does not make a completed update `partial`. `coverage` reports pending source ingestion separately; current coverage is not proof that every interpretation is correct. The graph retrieves meaningful decisions, dependencies and exceptions; source reading supplies details without requiring the graph to reproduce every sentence.

The default explicit-update work budget is two invocation attempts and 131,072 input bytes. `--max-calls` and `--max-input-bytes` set totals for the complete work, including extraction, check and resumption. A zero-call update reports pending documents without invoking the model. An exhausted work item retains its progress; repeating the command does not reset its counter. An authorized larger total can complete the remaining stage without repeating completed extraction.

A failed or unfinished invocation is not retried automatically by increasing the budget. Inspect its reported outcome and usage first. `--retry-failed` can explicitly resume a safely ended failure within the same work budget; uncertain invocations remain blocked. A completed adverse check is not an invocation failure; this flag does not repeat it for an unchanged candidate. Uncertain or pending knowledge does not become a blanket pass. The project-local `.hivex/knowledge.sqlite` stores working knowledge and execution accounting; no source Markdown is rewritten. Preserve it when work evidence is needed. Storage is bounded at 64 MiB; do not delete an active store to hide unfinished calls or reset a work budget.

For a false semantic finding that blocks a retained repair candidate, inspect `pendingCandidateWarnings` separately from the accepted graph's warnings. Reuse the same update arguments with `--retry-failed --max-calls 0 --resolve candidate-resolution.json`. The file binds the review to the work and exact check using `candidateResolutionContext`:

```json
{
  "workId": "<work-id>",
  "checkInputHash": "<retained-check-input-hash>",
  "resolutions": [
    {
      "id": "<pending-candidate-warning-id>",
      "reason": "<source-backed review disposition>",
      "evidence": [
        { "document": "docs/example.md", "lineStart": 10, "lineEnd": 14, "version": "<current-source-hash>" }
      ]
    }
  ]
}
```

This local operation accepts only findings from that check on current canonical candidate decisions. It requires the exact completed check and unchanged candidate, graph and sources; it never starts a model call. Unknown, batch and structural findings cannot be resolved through it. Every protected relationship still requires its original supported replacement or removal. The original findings, native uncertainty, failed attempt and consumption remain intact; the work records the explicit disposition and the admitted graph retains its evidence. A closure is the reviewer's source-backed judgement, not a correctness proof. Do not use it for a real contradiction or missing dependency.

Use `npx hivex warnings` to inspect active warnings and their IDs. After checking the current sources, `npx hivex warnings --resolve resolutions.json` records an explicit resolution without a model call. The file is an array of `{ "id": "<warning-id>", "reason": "<why the warning is resolved>", "evidence": [{ "document": "docs/example.md", "lineStart": 10, "lineEnd": 14, "version": "<current-source-hash>" }] }`. Unknown or repeated IDs and stale or invalid citations reject the whole operation. A resolution requires a reason and current evidence; it is a reviewed explanation, not automatic proof that the knowledge is correct. Findings about implementation or relationships require an explicit repair or correction, and a knowledge check does not turn a failure into a resolution. Keep real contradictions and missing dependencies open for repair.

Resolved warnings remain in the graph with their original message, reason and versioned evidence. When a warning is closed again, `previousResolutions` retains every earlier closure. Use `warnings --all` to inspect active and resolved history and `snapshot export` to share it in Git alongside the Markdown. Status counts active warnings and current resolutions separately. Retrieval shows active warnings relevant to the query. A changed or unavailable resolution source reactivates its warning; matching text does not silently preserve a closure. Handle new or reactivated warnings in the same incremental work when the complete context is available; otherwise leave them pending. Do not reopen settled questions or scan the whole corpus to chase zero warnings. Resolving a warning does not promote decision or relationship quality, rewrite sources, or change work history and budgets.

`recover` inspects abandoned work without invoking the model or killing processes. Live owners or native processes remain protected. If local processes ended but remote delivery is uncertain, `recover --acknowledge-uncertain` records an explicit acknowledgement; original reports and unknown usage remain visible. Recovery itself never retries: a subsequent `--retry-failed` uses the retained work budget. Do not treat acknowledgement as proof that the earlier remote turn completed.

`prune` releases space occupied by old completed work and cached results, retaining the graph and all unfinished work, attempts and budgets. It keeps the newest eight completed works and 64 cached results by default; `--keep-completed` and `--keep-caches` change those counts. Pruned answers can require a new model call when requested again. Export evidence before pruning if historical reports are needed; pruning is explicit, never an automatic budget reset.

Native operations accept `--integration codex`, `--model`, `--effort`, `--model-provider openai`, `--codex` and `--deadline-ms`; the default deadline is 30 minutes. Consultation context defaults to 65,536 bytes and can be bounded with `--max-context-bytes`. Limits are reported, not met by silently cutting a rule or pretending omitted evidence was reviewed. Input-byte and call budgets limit work; reported token usage is actual consumption, including known failed attempts.

### Execution profiles

Integration and model selection are independent. `--integration` selects the connection; `--model` and `--effort` select a profile supported by that connection. Codex currently admits OpenAI through ChatGPT, so other `--model-provider` values and uninstalled integrations fail explicitly. Provider-specific options and authentication belong to their integration. A future integration can use different model options without requiring a reasoning level named `max`.

```sh
npx hivex ask "Which rules apply?" --model <model-listed-by-codex> --effort <supported-effort>
```

Cache identity includes the integration/profile. The default is now `gpt-6-luna` with effort `max`; new invocations never need the former `gpt-5.6-luna`/max default. Unfinished work under that former default moves to GPT-6 Luna when resumed, keeping its ID, progress, attempts and consumed budget. The recorded profile replacement distinguishes historical results from new invocations. Running or uncertain invocations still require the normal inspection/recovery process, and failed work still requires explicit retry. The first subsequent `update`, `ask` or `review` removes the old unversioned model cache; its hash-only keys cannot distinguish models, so this also clears any custom-profile entries from that generation. New versioned cache entries survive later operations. Completed work and its historical answers remain intact, but are not reused as GPT-6 Luna results. Read-only commands do not perform this cleanup. Existing graph knowledge is not reingested merely because the default changes. Other profile changes continue to report `EXECUTION_PROFILE_CHANGED` for unfinished work without making a model call or resetting consumption. Each new work records its requested profile, and execution receipts record an observed effective profile when known.


## Share knowledge through Git

```sh
npx hivex snapshot export
npx hivex snapshot import
```

`snapshot export` writes `.hivex/graph.json` atomically as stable, readable JSON. Commit that file alongside the Markdown it describes to share decisions, relationships, source versions, evidence, available provenance and coverage. It exports the graph, not work records, process identities, budgets or cached model answers. Snapshot operations make zero model calls.

A fresh clone can use `search`, `neighbors` and `status` directly from the shared snapshot without creating a local database. Its first update reuses matching ingestion units and starts local work accounting. If a local graph already exists, it takes precedence: use `snapshot import` to adopt a new shared version. Import refuses while local work is unfinished and never resets attempts or budgets. Complete or recover that work through its normal lifecycle first.

When Markdown moves, explicitly relocate its knowledge before the next update:

```sh
npx hivex snapshot relocate docs/old-guide.md docs/guidelines/guide.md
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

The [Hivex skill](../skills/hivex/SKILL.md) handles local retrieval, incremental knowledge, uncertainty and accounting. The additional capabilities are [design](../skills/hivex-design/SKILL.md), [documentation](../skills/hivex-document/SKILL.md), [implementation](../skills/hivex-implement/SKILL.md), [review](../skills/hivex-review/SKILL.md) and [Git/triage](../skills/hivex-git/SKILL.md).

The [Markdown foundation](../skills/hivex/references/markdown.md) defines the recommended adoption layout, concise agent entrypoint, purpose, glossary, decisions, guidelines and procedures. Knowledge operations continue to accept other Markdown layouts. Preserve a project's useful rules and exceptions when adopting the baseline.

Prefer DDD and meaningful responsibilities, risk/value-based tests with TDD optional only for sufficiently defined critical flows, and one independent review by default. The reviewer matches the implementation agent's model and effort. Use local knowledge before model-assisted interpretation, and explain technical details clearly when they matter to the owner's understanding or decisions.

## Development

The Rust CLI reads the existing SQLite and shared snapshot v1 formats and retains work budgets and attempts. The GPT-6 upgrade retires the old cache generation as described in [Execution profiles](#execution-profiles). The TypeScript runtime was retired after compatibility validation under [#80](https://github.com/H1V35/hivex/issues/80). Its fixed SQL fixtures and synthetic protocol server remain as independent compatibility evidence.

Use the stable Rust toolchain and Git 2.45 or newer for development. The [Rust quality standard](guidelines/rust-quality.md) describes formatting, Clippy, metrics and domain boundaries. Cargo runs all unit and CLI integration tests, including the synthetic Codex server. No TypeScript, JavaScript, Bun or Node.js tooling is required.

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


The native artifact is verified on macOS ARM64, using system ICU for source ordering. Package preparation, archive inspection and publication of the exact approved bytes follow the [release procedure](procedures/releasing.md).

Tests use the public CLI and a simulated native transport. Real Luna evaluations are bounded and reported separately; simulated token usage is not a consumption measurement. Development is issue-first, with coherent PRs, an independent review covering scope/correctness/standards and CI on the final commit. See the [engineering workflow](guidelines/engineering.md).

Earlier candidate/fidelity/comparison/admission protocols and their tests are retired from the active CLI. Their code remains in Git history and historical evidence keeps its original results. They do not impose a requirement to reproduce an Opus graph or exhaustively replay an old gold suite.

## Support an implementation review

From the Git project root, supply the task and the base revision. Hivex captures the working change, including untracked files, and provides findings tied to code and Markdown versions:

```sh
npx hivex review "Change cache behavior" --base main --max-calls 3 > /tmp/hivex-review.json
npx hivex review --check /tmp/hivex-review.json
```

Use the same task and base to resume or reuse retained work. Update, knowledge check and review share one budget, including expansion of a partial report. Context is bounded; large changes must be narrowed or split into coherent reviews. Larger existing text files contribute diff excerpts with original line numbers and explicit omissions; their full-file versions still detect later changes. The principal reviewer verifies conflicts and exceptions and resolves supported contradictions before closing the change. A `ready` result means assistance is available, never that the implementation is approved. A saved report can be checked without a model; changed code or documents make it stale. Keep reports outside the project or in an ignored path so they do not become part of the change.

## Source organization

The [domain decision](adr/0013-domain-modules-and-execution-integrations.md) defines module ownership. `src/documents` handles sources and parsing; `src/knowledge` owns the graph and its derived state; `src/work` owns typed progress, budgets and persistence; consultation and review compose them. `src/execution` defines the integration-neutral contract, and `src/integrations/codex` contains its protocol, admission and process lifecycle. CLI parsing/output and development tools remain separate.

CI runs unit tests in the development build and the complete CLI contracts once against the packaged release executable. `cargo test --locked` remains the convenient local check. The parser matrix belongs beside the parser; integration-specific protocol tests live in `tests/contracts/codex.rs`. Remove a test only when its meaningful guarantee is retained elsewhere or its behavior is retired.
