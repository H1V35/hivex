# hivex

Versioned project knowledge for people and agents. Search declared Markdown sources and open their
complete evidence without a model call, service or private agent memory.

## Install and use

Use Bun 1.4.2, Git 2.45 or newer, and a committed checkout:

```sh
bun install
bun src/cli.ts search "versioned sources" --collection hivex
bun src/cli.ts read docs/adr/0001-versioned-project-knowledge.md --ref <commit-from-search>
```

Git 2.45 introduced the no-lazy-fetch control used to keep source reads local, including in
partial clones. See the [Git 2.45 reference](https://git-scm.com/docs/git/2.45.0).

Commit `bun.lock` when dependencies change. Use `bun ci` for a frozen installation in CI or a fresh
checkout. Bun's native configuration applies the seven-day release age and explicit lifecycle-script
allowlist. See the [installation decision](docs/adr/0003-independent-bun-installation.md).

The commands return JSON. Search includes the complete commit ID, configuration hash, source hash,
collection, declared authority and a bounded preview/location. Use the returned `readCursor` to start
near the matching heading for a text match. An explicit identifier starts at the beginning of the selected source so a
matching amendment does not hide the original decision. Omit the cursor to read any source from
the beginning. Pass the same `--ref` and source ID:

```sh
bun src/cli.ts read <source-id> --ref <commit-from-search> --cursor <readCursor> --max-bytes 16384
```

Continue with the returned `continuation` until it is null. Each page fits the byte budget including
its metadata, cursor and newline. Blocks are not silently cut. If the next block cannot fit, the
error reports the required size and line range; the hard output ceiling is 65,536 bytes. Search may
omit a preview to preserve a ranked source within the budget. `truncated` and `evidenceCompleteness`
must be respected: neither a hit nor a complete block establishes answer correctness.

`--ref` defaults to HEAD and reads committed content only, including `hivex.json`. Uncommitted
changes are not silently mixed into a snapshot. Default search budgets are 4,096 bytes/eight results;
read defaults to 16,384 bytes. Accepted limits are 1–20 results and 1,024–65,536 output bytes.

## Adopt another project

Add and commit `hivex.json` in its Git root:

```json
{
  "version": 1,
  "collections": [
    {
      "id": "product",
      "include": ["docs/**/*.md"],
      "exclude": ["docs/archive/**"],
      "aliasPrefix": "ADR"
    },
    { "id": "history", "include": ["docs/archive/**/*.md"], "default": false, "kind": "evidence" }
  ]
}
```

Then run `bun /path/to/hivex/src/cli.ts search "your question" --root /path/to/project`.
Collections must have distinct ownership of each selected passage. `aliasPrefix` associates explicit
references such as `ADR 0006` with numbered filenames such as `0006-policy.md`; it is configuration,
not a Compi-specific behavior. Files must be regular tracked UTF-8 Markdown. Symlinks are refused.
Document bodies are capped at 2 MiB and selected source counts at 2,048; Git subprocesses have bounded
time and output and cannot lazily fetch missing objects from a remote.

Optional YAML frontmatter supports `title`, `status` and `superseded_by` (an array of exact source
document paths in the same snapshot). Known declarations are accepted, proposed, superseded and historical;
unrecognized or absent status remains unknown. Replacement targets must exist and cannot be the
source itself. This is declaration validation, **not full precedence or cycle analysis**. Currentness
remains `not-established`, including for Accepted documents. For mixed ADRs, inspect the decision,
amendments and exceptions before treating a rule as applicable.

## Sections of mixed documents

An `include` entry can select an exact heading anchor instead of a whole-file glob:

```json
{
  "version": 1,
  "collections": [
    { "id": "product", "include": [{ "path": "docs/context.md", "anchor": "identity" }] },
    {
      "id": "legacy",
      "include": [{ "path": "docs/context.md", "anchor": "orchestration" }],
      "kind": "legacy",
      "default": false
    }
  ]
}
```

Selectors address headings outside lists and block quotes, at any heading level. A section includes
its heading and descendants, ending before the next such heading of equal or lower level.
GitHub-style anchors distinguish repeated headings (`policy`, `policy-1`), counting real headings
inside quotes and lists too; headings inside code blocks do not create sections. Selecting a heading
inside a quote or list fails with `UNSUPPORTED_SECTION`: declare the whole document instead to keep
the containing block intact. Paths are exact repository-relative Markdown paths, without
wildcards or fragments. A missing/excluded path or changed anchor fails explicitly. Whole-document,
nested or duplicate selections cannot overlap, even across opt-in collections. Multiple globs within
one collection still form a union. All explicitly selected sections are validated before serving a
query, including opt-in sections; only active sources enter the search index.

Search returns a source ID such as `docs/context.md#identity`, the original document `path` and
`contentHash`, and a `section` with its anchor and original line range. Read that ID to paginate only
the selected section. Use `read docs/context.md` to open the entire document and check surrounding
conditions, exceptions and amendments. That full read has `section: null` and `collection: null`
when the document is declared through sections. Its hash matches each section's document hash.

Frontmatter declarations always have `authority.scope: "document"`: selecting a section does not
establish its individual status or whether it is sufficient evidence. A null continuation means the
selected source is exhausted, not that the rest of the document has been read. Cursors are bound to
the selected source and commit; a section cursor cannot resume another section or the entire file.
Unselected text remains available through the full-document read but is not indexed for search.

## Indexed relationships

Inspect replacement and amendment records associated with a source:

```sh
bun src/cli.ts relations docs/adr/0088-ui-design-system-ratified.md --limit 4 --max-bytes 8192
bun src/cli.ts relations <source-id> --ref <commit-from-response> --cursor <continuation>
```

Configure the optional input alongside `collections` in the committed `hivex.json`:

```json
{
  "relationIndexes": [
    {
      "path": "docs/adr/supersession-index.jsonl",
      "format": "compi-adr-supersession-index"
    }
  ]
}
```

The supported format is the existing ADR supersession JSONL schema v1. Hivex reads the committed
projection without invoking its generator or importing legacy orchestration. The format adapter
works with configured paths; Compi's index is one adoption of it. Search and read do not load these
indexes, so an invalid derived index does not prevent access to the Markdown sources.

Relations always cover the **whole document**, including when the requested source ID names one
section. Results include records with replacements, a status other than `live`, an `unresolved`
type, or a subject reference without an exact selectable location. A `live` status does not hide
a broken anchor. An omitted record is not proof that its rule is current. `coverage: not-configured`
means no index was configured; an empty result with `coverage: configured-indexes` only describes
those indexes. Records retain index order, which is explicitly not a precedence order.

`indexedStatus`, `indexedText` and `indexedHeading` report the generator's interpretation. Text can
be normalized or synthesized and is not labelled as a verbatim quotation. YAML declarations remain
separate under the source's authority metadata. Both effective currentness and index freshness are
`not-established`: this legacy format has no source-cohort hashes, and sharing a Git commit does
not establish freshness or semantic correctness.

Each record has its index path, hash and original line, plus the affected subject and replacements.
References preserve the indexed ADR number, path and anchor alongside the current snapshot's
source hash. A resolved anchor supplies its heading's line range and a `readCursor`; use it with
`read <path>` and the same commit to open evidence. A `missing-anchor` or `contained-heading`
reference has no exact selectable location and offers a cursor from the document's beginning.
Hivex does not silently repair anchors or cut lists/quotes. Reading a referenced heading still does
not establish all conditions or exceptions; the full document remains available.

Every index's schema, record IDs and referenced document paths are validated before a query returns.
IDs are unique within each index; the index path distinguishes equal IDs from separate inputs.
All referenced documents must be declared regular Markdown sources, including opt-in collections.
Unsafe/missing paths, symlinks, malformed data and duplicate IDs fail explicitly. Anchors are
resolved for returned references; unresolved anchors remain visible rather than invalidating access
to the entire index.

The limits are eight index files, 2 MiB per input file, 10,000 total records and 2,048 referenced
documents. Defaults are eight records and 16,384 output bytes. `--limit` accepts 1–20 records and
`--max-bytes` accepts 1,024–65,536 bytes, including metadata and continuation. Records and their
replacement arrays are indivisible: an oversized next record reports its required budget.
Continuation is bound to the requested source ID, commit, configuration and index hashes. It is
distinct from the Markdown reading cursor carried by each reference.

## Documentation scopes

This repository declares the `hivex` collection for its own documentation. An adopting project
owns its collection IDs, include/exclude selectors and opt-in evidence or legacy sources.
Select an opt-in collection explicitly with `--collection`. Exact source IDs remain readable even
when excluded from default search. Collection selection does not revoke source authority.

## Plan extraction inputs

Inspect the complete declared source cohort before making a model call:

```sh
bun src/cli.ts plan --collection hivex --limit 10 --max-bytes 16384
bun src/cli.ts plan --collection hivex --ref <commit-from-plan> --cursor <continuation>
```

`plan` reads committed sources and makes no model call or workspace write. Default selection is the
same as search; opt-in collections require `--collection`. Each unit retains its document hash,
selected section, original lines, collection and declared authority. Its prompt hash is computed
using the same preparation as `extract`. The plan hash binds the entire selected cohort, Git commit,
configuration and processing contract, independently of pagination. It is not a signature or an
accepted graph manifest.

The summary covers all selected sources, including units on later pages. It reports source bytes,
prepared prompt bytes and oversized sources. These are UTF-8 byte counts, not token or billing
estimates; native harness instructions, response tokens, corrections and runtime costs are additional.
`extractable` means the declared input fits the current 32,768-byte source limit. It does not certify
semantic completeness, graph admission or native account availability. Oversized inputs remain in
the plan as `requires-section`; declare complete sections and compare their plans before extraction.
Hivex does not silently trim or partition a decision to fit the limit.

Continue until `continuation` is null, checking the same plan hash and contiguous page ranges.
Changing the source cohort, selection, commit or processing contract invalidates a cursor. You may
increase the page budget without changing the plan. A complete unit that cannot fit reports its
required byte budget instead of dropping authority or replacement references. Defaults are 20 units
and 16,384 output bytes; accepted limits are 1–20 units and 1,024–65,536 bytes, including metadata and
the trailing newline. This planning interface does not execute or resume a rebuild.

## Extract knowledge candidates

`extract` uses **gpt-5.6-luna/max through the existing Codex ChatGPT subscription**. It requires the
admitted `codex-cli 0.153.2` on macOS or Linux. This is an explicit model operation:

```sh
bun src/cli.ts extract docs/adr/0001-versioned-project-knowledge.md --ref <commit> --attempts 3 --deadline-ms 600000
```

The source must be declared in the selected commit's `hivex.json` and contain at most 32,768 UTF-8
bytes. For a larger document, declare and extract a complete heading section. `--codex` selects the
native executable; it defaults to `codex` on PATH. The default deadline is 600,000 milliseconds;
accepted values are 100–900,000. Each invocation, including an interrupted attempt, is recorded.
Missing usage is `null`; reasoning output and cached input are subsets of the reported output/input
counters, not additional tokens to add again.

The JSON response binds claims and within-source relations to the source, commit, configuration,
base prompt, processing policy and schema hashes. `candidateAttempt` identifies the one-based attempt
that produced a candidate; that attempt records its actual prompt hash, including correction feedback.
Evidence quotes must match their original line ranges. Model success and
valid JSON still yield `status: "candidate"` and `accepted: false`. An exhausted or non-retryable
operation returns `status: "failed"`, preserves attempt evidence and exits with code 1. Input errors
fail before a model call. Save the response as verification evidence; it is not authored authority.

The processor checks its configured ChatGPT endpoint, account, model/effort and disabled capabilities,
and records the admitted values and their configuration origins. Native subprocesses receive only
essential home/path/locale/temp and existing sandbox-marker environment fields; unrelated credentials
and proxy settings are not inherited.
It uses an empty temporary workspace, ephemeral threads, no discovered project instructions or agent
memories, and a read-only policy with model network access disabled. Existing MCP configurations are
disabled locally before creating a knowledge thread; global settings remain available for other work.
The protocol stream is bounded to 4 MiB per frame and 32 MiB overall. A timeout requests interruption;
unconfirmed cancellation stops the attempt sequence. SIGINT/SIGTERM also cancel active processing.

These controls are admission checks for the trusted native CLI. Version 0.153.2 does not provide a
filesystem read-root allowlist, so this interface does not claim OS-level read isolation. The remaining
graph rebuild and grounding requirements are recorded in the
[native knowledge decision](docs/adr/0002-native-knowledge-candidates.md).

## Resume candidate ingestion

Ignore `.hivex/` in the adopting repository. `ingest` keeps one local SQLite store there by default;
`--store` selects another local file. It never moves your Markdown or creates a file per source.

```sh
bun src/cli.ts ingest --collection hivex --max-units 1
bun src/cli.ts ingest --max-units 20
bun src/cli.ts ingest --max-units 0
bun src/cli.ts ingest --show docs/adr/0001-versioned-project-knowledge.md
```

The first command freezes the complete plan. Later commands resume its original commit and
collection when those arguments are omitted, even if HEAD advances. Explicit different inputs or
a changed processing contract fail before extraction. Oversized sources must be split into declared
complete sections before starting any source. This is candidate production, not graph admission.

`--max-units` defaults to 20 and accepts 0–2048; zero checks progress without calling a model.
Only pending sources are claimed. `--attempts` accepts 1–3 (default 3); `--deadline-ms` accepts
100–900,000 (default 600,000) per invocation. Each report retains its effective deadline and the
admitted Luna/max profile. Changing these execution limits does not reopen failed or unresolved
sources or reset their attempt history. Two CLI processes
can process distinct sources; no transaction stays open during a model call. Each attempt is
checkpointed before invocation and after its report is available. Finished sources are reused,
failed sources are retained, and an unresolved invocation is never retried automatically.

Progress includes completed, pending, failed and unresolved source counts, plus recorded attempts,
known total tokens and attempts with unknown usage. Known totals are partial when usage is unknown;
reasoning and cached tokens are already included in their parent counters. `candidates-ready`
requires every source to have a retained candidate and still reports `accepted: false`. A partial
run exits successfully but does not establish that all processing has completed; a recorded failure
exits with code 1.

`--show <source-id>` opens a complete retained result or the checkpoint of an unfinished source
without mutating the store. Its default output budget is 16,384 bytes; `--max-bytes` accepts
1,024–8,388,608. A result that does not fit produces an explicit error with the required size, never
a shortened candidate. Inspection and discard cannot be mixed with extraction options.

The database is limited to 128 MiB and reserves space before invoking a model. Plans/results are
bounded to 8 MiB each, and attempt details to 4 MiB per source. An individual report over 1 MiB
retains its outcome, usage and identifying hashes with explicit omitted-detail metadata. Capacity
or persistence failure cannot silently turn an unresolved source into a completed one.

Retain the evidence needed for later admission or auditing before deliberately removing a cohort:

```sh
bun src/cli.ts ingest --discard <exact-plan-hash>
```

Discard empties that same store for reuse and makes no model calls. It rejects a different hash
or any claimed/unresolved source. There is no automatic rotation or pruning of uncertain evidence.
See the [persistence decision](docs/adr/0004-resumable-ingestion-store.md). Full rebuild admission,
historical graph queries and implementation grounding remain required work.

## Development

```sh
bun run --bun typecheck
bun run --bun lint
bun run --bun format:check
bun test ./src
```

The integration tests exercise the public CLI against temporary Git repositories. The implementation
uses mdast/GFM/frontmatter positions, YAML metadata, GitHub-style heading anchors and Bun SQLite FTS5.
Project smoke tests also query every committed collection and validate the referenced documents in
each configured relation index through the CLI, including header-only indexes.
Run the same checks after dependency changes. Hivex owns this suite independently of the checks
required by an adopting project.

[Domain language](docs/CONTEXT.md) · [Initial decision](docs/adr/0001-versioned-project-knowledge.md) ·
[Native knowledge candidates](docs/adr/0002-native-knowledge-candidates.md)
