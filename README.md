# Hivex

Project decisions, dependencies and exceptions for the agent responsible for implementation and
review. Markdown remains authority; Hivex supplies context so agents can act autonomously without
reopening settled decisions.

Hivex is a TypeScript/Bun CLI. The current knowledge profile is Luna/max through native Codex and the
user's ChatGPT subscription, without silent fallback. The implementing agent may use another model.
Model invocation is localized for future configuration; multiple providers are not yet validated.

## Current delivery

This release-in-development supplies initial updates and task consultation (#47), plus automatic
incremental maintenance and interpretation repair (#48), and task/diff review assistance (#19).
Publication (#21) prepares early use; introduction in Compi is a separate step. It is not complete Compi adoption or legacy retirement.

No installed command approves an implementation. The principal reviewer verifies findings, tests and
the actual source evidence. See the [approved product decision](docs/adr/0010-practical-knowledge-assistance.md).

## Install the CLI and skill

Requires Bun 1.4.2. Once the release is available from npm:

```sh
bun add --dev --exact @h1v35/hivex@0.1.0
bun hivex --help
```

Copy `node_modules/@h1v35/hivex/skills/hivex` into the skill directory used by your agent. For an
agent that discovers project skills in `.agents/skills`, use `.agents/skills/hivex`. Keep the CLI and
skill at the same release; upgrade the copied skill when upgrading the package. The skill and its
Markdown guide are portable and do not require Compi's private tools or other installed skills.

The current knowledge profile needs an authenticated Codex CLI session with the selected Luna/max
model available. Native invocation checks that profile and stops rather than silently falling back.
Document discovery and version checks work without a model. See the CLI help for bounded model work.

## Run

Use Bun 1.4.2. In this checkout:

```sh
bun install
bun hivex --help
bun hivex sources --root /path/to/project
bun hivex update --root /path/to/project --max-calls 0
```

The scoped package name is `@h1v35/hivex`, with command `hivex` and MIT license. Publication and registry
installation are tracked separately; do not fetch the unrelated unscoped npm package.

Documents need no Git repository or commit. They can live at monorepo, package or module level and
use their project's own Markdown format. An optional `hivex.json` selects relative globs:

```json
{
  "include": ["docs/**/*.md", "packages/**/*.md", "src/**/decisions/*.md"],
  "exclude": ["docs/generated/**"],
  "history": ["docs/archive/**/*.md"]
}
```

Without configuration, Hivex selects Markdown files under the project. It skips dependencies,
its own cache, Git metadata and private dot directories; explicitly named documentation directories
can be selected. `history` declares additional Markdown that remains readable evidence while staying
out of ordinary update and consultation ingestion. Use `--source <document>` with `ask` or `review`
to select it, or let a known relationship bring back the bounded ranges it requires. Historical
metadata and evidence carry `historical: true`; an extracted decision from that source remains
`historical`, even when the transport suggests another status. An `exclude` glob wins over `history`.
Symlinks are not followed and protected directories and scope escapes remain rejected. The previous
experimental `collections` configuration is rejected with a migration message rather than silently
reinterpreted.

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

`sources` returns document metadata without their full text and a snapshot-bound continuation when
more records remain. Resume with `--cursor`. `read` returns original text, its version and line ranges;
use `--from`, `--to` and `--max-bytes` for a bounded passage. Continuation and omitted content remain
explicit. A working version is not evidence of approval.

Search covers both extracted decisions and original Markdown, so terminology omitted from a summary
remains discoverable. When needed, select a known document with `--source` in a consultation rather
than reopening a settled question with the owner. Historical sources are not searched into an ordinary
consultation merely because they are available; a focused source or a known dependency is required.
If a necessary historical source is excluded or unavailable, the result names the missing evidence.

Search and neighbor traversal are deterministic and make no model calls. Neighbor traversal includes
indirect connections within `--limit` and lists decisions it could not expand. Stale knowledge is not
presented as current evidence. `status` reports available knowledge and documents requiring attention.

`ask` first detects added, changed and removed Markdown. It updates at most one bounded batch,
prioritizing matching fragments, relevant documents and their known dependencies, then asks Luna/max over
the available decisions and original Markdown. Its default budget is three calls for the complete
update/check/answer operation. If the budget ends before the answer, repeat the same task with an
authorized higher total: the work, progress and consumption are retained. An unchanged task reuses
its answer. Use explicit `update` to advance remaining corpus batches; pending coverage stays visible.

Changed sources bring their known incoming and outgoing neighbors into comparison, including
relationships supported by a third document. Deleted sources are removed from pending ingestion;
`unavailableDocuments` identifies dependencies that can no longer be verified. Source-local check
findings remain scoped, so unrelated consultations can use their valid knowledge.
It explains applicability and uncertainty. The evidence text in the result is read from the cited
source ranges, not copied from a model-generated quotation. Large sources are supplied as relevant units within the context limit; `omittedUnits` reports
unread portions so a partial answer is not mistaken for complete coverage. Identical retained consultations are
reused. A partial result remains useful within its declared limits.

Hivex does not compact, move or rewrite Markdown. Authors preserve the original historical text,
dates, links and anchors; Hivex exposes the selected source version and line ranges while keeping
historical applicability and conditions visible.

## Update and repair knowledge

```sh
bun hivex update --root /path/to/project --max-calls 2
```

An update splits large Markdown into line-preserving units of at most 8 KiB, preferring Markdown
boundaries. Each round selects at most four units and 16 KiB of target text, with up to 8 KiB of
relevant existing evidence, then performs one additional check. Original document IDs and line
numbers survive splitting. Earlier rounds remain queryable while `pendingUnits` and `pendingDocuments`
show unfinished coverage. Sources up to 32 MiB can be split, within a 64 MiB loaded-corpus limit;
narrow the selected paths if that limit is reached. A line too large to fit is explicitly reported as unread, never silently cut.

Sources declared by `history` are not part of an ordinary update. A focused `ask` or `review` adds
only its selected bounded units to the same resumable work; a known dependency can add the ranges
needed to verify it. The work budget, graph knowledge and cache survive resumption.

Each extraction and check is checkpointed. Resuming continues the same work and never repeats its
completed rounds. Successful structured model results are cached in the same store by the complete
request, schema and model profile; an identical request can be reused without a call, even when
reconstructing earlier knowledge. Changed context invalidates that cache entry. Context discovery considers authored links, lexical
matches and recent decisions; `relationshipCoverage` states that this is bounded, not exhaustive. Cache hits are
reported separately from calls and tokens; this is an optimization, not documentary authority.

To correct derived knowledge against unchanged Markdown, use:

```sh
bun hivex update --root /path/to/project --repair docs/cache.md --reason "The source specifies seven days, not indefinite retention."
```

Repair replaces the affected unit's interpretations and revisits its relationships without editing
Markdown. Its reason guides comparison with the source; it does not create new authority. Repeating
the same completed repair reuses its work. A genuine unresolved documentary conflict still needs a
decision by the responsible person.

The default explicit-update work budget is two invocation attempts and 131,072 input bytes. `--max-calls` and
`--max-input-bytes` set totals for the complete work, including extraction, check and resumption.
A zero-call update reports pending documents without invoking the model. An exhausted work item
retains its progress; repeating the command does not reset its counter. An authorized larger total
can complete the remaining stage without repeating completed extraction.

A failed or unfinished invocation is not retried automatically by increasing the budget. Inspect its
reported outcome and usage first. `--retry-failed` can explicitly resume a safely ended failure within
the same work budget; uncertain invocations remain blocked. A completed adverse check is not an
invocation failure and is never retried by this flag. Uncertain or pending knowledge does not become a blanket pass.
The project-local `.hivex/knowledge.sqlite` stores working knowledge and execution accounting;
no source Markdown is rewritten. Preserve it when work evidence is needed. Storage is bounded at
64 MiB; do not delete an active store to hide unfinished calls or reset a work budget.

`recover` inspects abandoned work without invoking the model or killing processes. Live owners or
native processes remain protected. If local processes ended but remote delivery is uncertain,
`recover --acknowledge-uncertain` records an explicit acknowledgement; original reports and unknown
usage remain visible. Recovery itself never retries: a subsequent `--retry-failed` uses the retained
work budget. Do not treat acknowledgement as proof that the earlier remote turn completed.

`prune` releases space occupied by old completed work and cached results, retaining the graph and all
unfinished work, attempts and budgets. It keeps the newest eight completed works and 64 cached
results by default; `--keep-completed` and `--keep-caches` change those counts. Pruned answers can
require a new model call when requested again. Export evidence before pruning if historical reports
are needed; pruning is explicit, never an automatic budget reset.

Native operations accept `--codex` and `--deadline-ms`; the default deadline is 30 minutes. Consultation
context defaults to 65,536 bytes and can be bounded with `--max-context-bytes`. Limits are reported,
not met by silently cutting a rule or pretending omitted evidence was reviewed. Input-byte and call
budgets limit work; reported token usage is actual consumption, including known failed attempts.

## Share knowledge through Git

```sh
bun hivex snapshot export --root /path/to/project
bun hivex snapshot import --root /path/to/project
```

`snapshot export` writes `.hivex/graph.json` atomically as stable, readable JSON. Commit that file
alongside the Markdown it describes to share decisions, relationships, source versions, evidence,
available provenance and coverage. It exports the graph, not work records, process identities,
budgets or cached model answers. Both snapshot operations make zero model calls.

A fresh clone can use `search`, `neighbors` and `status` directly from the shared snapshot without
creating a local database. Its first update reuses matching ingestion units and starts local work
accounting. If a local graph already exists, it takes precedence: use `snapshot import` to adopt a
new shared version. Import refuses while local work is unfinished and never resets attempts or
budgets. Complete or recover that work through its normal lifecycle first.

The snapshot response identifies current, stale and unavailable source versions, pending units and
warnings. A changed or absent source is not silently current; matching sources remain reusable.
Partial and uncertain knowledge can be shared with those states retained. Freshness is not proof
that a model interpretation is correct: the cited Markdown remains authority.

Keep only the shared graph under version control, for example:

```gitignore
/.hivex/*
!/.hivex/graph.json
```

Read-only queries do not rewrite the snapshot. Export intentionally when reusable knowledge changes,
not on every consultation. Invalid snapshots or symbolic-link paths fail without replacing local
knowledge. Existing local stores continue to work without a shared file.

## Agent skill and Markdown practice

The [portable Hivex skill](skills/hivex/SKILL.md) teaches consultation before implementation, support to
the principal reviewer, documentation maintenance, uncertainty and budget handling. It uses the
installed CLI's actual interface and does not require Compi's private tools or other skills.

The [optional Markdown convention](skills/hivex/references/markdown.md) describes authority maps,
glossaries, ADRs, guidelines, process and procedures. Recommend it when useful; existing layouts,
metadata conventions and writing styles remain valid. Create only the documents a project needs.

## Development

```sh
bun run typecheck
bun run lint
bun run format:check
bun run test
```

Tests use the public CLI and a simulated native transport. Real Luna evaluations are bounded and
reported separately; simulated token usage is not a consumption measurement. Development is
issue-first, with coherent PRs, independent Standards/Spec review and CI on the final commit.
See the [engineering workflow](docs/engineering.md).

Earlier candidate/fidelity/comparison/admission protocols and their tests are retired from the active
CLI. Their code remains in Git history and historical evidence keeps its original results. They do
not impose a requirement to reproduce an Opus graph or exhaustively replay an old gold suite.

## Support an implementation review

From the Git project root, supply the task and the base revision. Hivex captures the working change,
including untracked files, and provides findings tied to code and Markdown versions:

```sh
hivex review "Change cache behavior" --base main --max-calls 3 > /tmp/hivex-review.json
hivex review --check /tmp/hivex-review.json
```

Use the same task and base to resume or reuse retained work. Update, knowledge check and review share
one budget, including expansion of a partial report. Context is bounded; large changes must be narrowed or split into coherent reviews. Larger existing
text files contribute diff excerpts with original line numbers and explicit omissions; their full-file
versions still detect later changes.
The principal reviewer verifies conflicts and exceptions and resolves supported contradictions before
closing the change. A `ready` result means assistance is available, never that the implementation is
approved. A saved report can be checked without a model; changed code or documents make it stale.
Keep reports outside the project or in an ignored path so they do not become part of the change.
