# Hivex

Project decisions, dependencies and exceptions for the agent responsible for implementation and
review. Markdown remains authority; Hivex supplies context so agents can act autonomously without
reopening settled decisions.

Hivex is a TypeScript/Bun CLI. The current knowledge profile is Luna/max through native Codex and the
user's ChatGPT subscription, without silent fallback. The implementing agent may use another model.
Model invocation is localized for future configuration; multiple providers are not yet validated.

## Current delivery

This release-in-development supplies explicit initial updates and task consultation (#47).
Automatic incremental maintenance (#48), task/diff review assistance (#19), and publication/adoption
(#21) are subsequent deliveries. It is not complete Compi adoption or legacy retirement.

No installed command approves an implementation. The principal reviewer verifies findings, tests and
the actual source evidence. See the [approved product decision](docs/adr/0010-practical-knowledge-assistance.md).

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
  "exclude": ["docs/archive/**"]
}
```

Without configuration, Hivex selects Markdown files under the project. It skips dependencies,
its own cache, Git metadata and private dot directories; explicitly named documentation directories
can be selected. Symlinks are not followed. The previous experimental `collections` configuration
is rejected with a migration message rather than silently reinterpreted.

## Recover context

```sh
bun hivex sources --root /path/to/project --limit 20
bun hivex read docs/policy.md --root /path/to/project
bun hivex search "cache access revocation" --root /path/to/project
bun hivex neighbors <decision-id> --root /path/to/project --limit 24
bun hivex ask "How should private cached data behave when access is revoked?" --root /path/to/project
```

`sources` returns document metadata without their full text and a snapshot-bound continuation when
more records remain. Resume with `--cursor`. `read` returns original text, its version and line ranges;
use `--from`, `--to` and `--max-bytes` for a bounded passage. Continuation and omitted content remain
explicit. A working version is not evidence of approval.

Search covers both extracted decisions and original Markdown, so terminology omitted from a summary
remains discoverable. When needed, select a known document with `--source` in a consultation rather
than reopening a settled question with the owner.

Search and neighbor traversal are deterministic and make no model calls. Neighbor traversal includes
indirect connections within `--limit` and lists decisions it could not expand. Stale knowledge is not
presented as current evidence. `status` reports available knowledge and documents requiring attention.

`ask` makes a bounded Luna/max consultation over the selected decisions and original Markdown.
It explains applicability and uncertainty. The evidence text in the result is read from the cited
source ranges, not copied from a model-generated quotation. Large sources are supplied as relevant units within the context limit; `omittedUnits` reports
unread portions so a partial answer is not mistaken for complete coverage. Identical retained consultations are
reused. A partial result remains useful within its declared limits.

## Update deliberately

```sh
bun hivex update --root /path/to/project --max-calls 2
```

An update splits large Markdown into line-preserving units of at most 8 KiB, preferring Markdown
boundaries. Each round selects at most four units and 16 KiB of target text, with up to 8 KiB of
relevant existing evidence, then performs one additional check. Original document IDs and line
numbers survive splitting. Earlier rounds remain queryable while `pendingUnits` and `pendingDocuments`
show unfinished coverage. Sources up to 32 MiB can be split, within a 64 MiB loaded-corpus limit;
narrow the selected paths if that limit is reached. A line too large to fit is explicitly reported as unread, never silently cut.

Each extraction and check is checkpointed. Resuming continues the same work and never repeats its
completed rounds. Successful structured model results are cached in the same store by the complete
request, schema and model profile; an identical request can be reused without a call, even when
reconstructing earlier knowledge. Changed context invalidates that cache entry. Context discovery considers authored links, lexical
matches and recent decisions; `relationshipCoverage` states that this is bounded, not exhaustive. Cache hits are
reported separately from calls and tokens; this is an optimization, not documentary authority.

The default work budget is two invocation attempts and 131,072 input bytes. `--max-calls` and
`--max-input-bytes` set totals for the complete work, including extraction, check and resumption.
A zero-call update reports pending documents without invoking the model. An exhausted work item
retains its progress; repeating the command does not reset its counter. An authorized larger total
can complete the remaining stage without repeating completed extraction.

A failed or unfinished invocation is not retried automatically by increasing the budget. Inspect its
reported outcome and usage first. `--retry-failed` can explicitly resume a safely ended failure within
the same work budget; uncertain invocations remain blocked. A completed adverse check is not an
invocation failure and is never retried by this flag. Uncertain or pending knowledge does not become a blanket pass.
The single project-local `.hivex/knowledge.sqlite` stores derived knowledge and work accounting;
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
