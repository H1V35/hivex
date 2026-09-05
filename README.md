# hivex

Versioned project knowledge for people and agents. Search declared Markdown sources and open their
complete evidence without a model call, service or private agent memory.

## Use in this repository

After the normal dependency installation:

```sh
pnpm hivex search "repository instances compiler" --collection compi --limit 3 --max-bytes 4096
pnpm hivex search "ADR 0006" --collection compi
pnpm hivex read docs/adr/0006-react-compiler-no-manual-memoization.md --ref <commit-from-search>
```

For repeated queries, the pinned runtime can also be invoked directly as
`./node_modules/.bin/bun hivex/src/cli.ts`, without package-manager startup.

The commands return JSON. Search includes the complete commit ID, configuration hash, source hash,
collection, declared authority and a bounded preview/location. Use the returned `readCursor` to start
near the matching heading for a text match. An explicit identifier starts at the beginning so a
matching amendment does not hide the original decision. Omit the cursor to read any source from
the beginning. Pass the same `--ref` and source ID:

```sh
pnpm hivex read <source-id> --ref <commit-from-search> --cursor <readCursor> --max-bytes 16384
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
Collections must have distinct ownership of each matched path. `aliasPrefix` associates explicit
references such as `ADR 0006` with numbered filenames such as `0006-policy.md`; it is configuration,
not a Compi-specific behavior. Files must be regular tracked UTF-8 Markdown. Symlinks are refused.
Source bodies are capped at 2 MiB and selected source counts at 2,048; Git subprocesses have bounded
time and output and cannot lazily fetch missing objects from a remote.

Optional YAML frontmatter supports `title`, `status` and `superseded_by` (an array of exact source
paths in the same snapshot). Known declarations are accepted, proposed, superseded and historical;
unrecognized or absent status remains unknown. Replacement targets must exist and cannot be the
source itself. This is declaration validation, **not full precedence or cycle analysis**. Currentness
remains `not-established`, including for Accepted documents. For mixed ADRs, inspect the decision,
amendments and exceptions before treating a rule as applicable.

## Documentation scopes in Compi

| Collection    | Purpose                                                      | Default search |
| ------------- | ------------------------------------------------------------ | -------------- |
| `compi`       | Product requirements, domain/design decisions and guidelines | Yes            |
| `engineering` | Shared repository rules and mixed domain/workflow documents  | Yes            |
| `hivex`       | Hivex's own language, decisions and usage                    | Yes            |
| `graph`       | Existing decision-graph authority                            | No             |
| `legacy`      | Existing orchestration and operational procedures            | No             |
| `evidence`    | Dated research and frozen historical evidence                | No             |

Select an opt-in scope explicitly with `--collection`. Exact source IDs remain readable even when
excluded from default search. These collections separate retrieval context now; they do not move,
copy, revoke or delete existing documents. Mixed documents remain marked as such until the content
and its consumers are separated safely.

## Development

```sh
pnpm --filter hivex typecheck
pnpm --filter hivex lint
pnpm --filter hivex test
```

The integration tests exercise the public CLI against temporary Git repositories. The implementation
uses mdast/GFM/frontmatter positions, YAML metadata, GitHub-style heading anchors and Bun SQLite FTS5.
Hivex-specific changes run this suite without requiring unrelated app/backend suites; manifest,
lockfile, workflow and uncertain changes retain the complete CI scope.

[Domain language](docs/CONTEXT.md) · [Initial decision](docs/adr/0001-versioned-project-knowledge.md)
