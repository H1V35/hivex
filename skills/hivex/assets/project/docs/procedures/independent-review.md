---
title: "Independent cross-vendor review"
status: draft
created_at: 2026-10-03
tags: [workflow, review, agents]
---

# Independent cross-vendor review

Run the independent review defined in [engineering](../guidelines/engineering.md#independent-review) before integrating a pull request. The reviewer uses a different model vendor than the implementing agent. The initial profile is `gpt-6.1-sol` with `xhigh` effort, read-only through Codex, following the `hivex-review` skill; when the project records another profile in engineering, replace the model, effort and command below accordingly.

## Prerequisites

- The branch is pushed and its issue (and pull request, when open) states the intended outcome. The reviewer reads intent from those sources; the implementer does not summarise it.
- `codex`, `jq` and an authenticated `gh` are available, and Codex is authenticated.
- `/.reviews/` stays ignored; `init` adds it to `.gitignore` for local review traces.

## Run the review

Set the reviewed revision and the integration branch, capture the raw issue and pull request (the read-only sandbox has no network access) and run Codex with the fixed prompt. `--output-schema` keeps every finding field, `--json` keeps the full trace of commands and outputs, and `-o` writes the report. An empty capture stops the run.

```bash
ISSUE=<issue> PR=<pr> BASE=origin/main SHA=$(git rev-parse HEAD)
OUT=.reviews/$PR && mkdir -p "$OUT"
gh issue view "$ISSUE" --json title,body,comments > "$OUT/issue.json"
gh pr view "$PR" --json title,body,comments,reviews > "$OUT/pr.json"
test -s "$OUT/issue.json" && test -s "$OUT/pr.json" &&
  codex exec -m gpt-6.1-sol -c model_reasoning_effort=xhigh -s read-only --json \
    --output-schema docs/procedures/independent-review.schema.json \
    -o "$OUT/$SHA.json" - > "$OUT/$SHA.jsonl" <<EOF
You are the independent reviewer for issue #$ISSUE (pull request #$PR) in this repository.
Follow .agents/skills/hivex-review/SKILL.md. Read-only: do not edit files, commit, push or comment
on GitHub.

Reviewed revision: $SHA, compared with its merge base: git diff $BASE...$SHA.
Read the intended outcome yourself from the unedited tracker captures $OUT/issue.json and
$OUT/pr.json, together with the diff and the repository's Markdown authorities.

Report every actionable finding with severity, location, affected scenario, evidence and a concrete
fix. Record the verification you ran and any material limit of the review. Do not restate the diff
or add praise.
EOF
```

Omit the pull-request capture and references when none is open yet.

## Review later changes as a delta

Resume the same Codex session so the reviewer keeps its own context; do not restate its findings. Refresh the tracker captures first if the issue or pull request changed.

```bash
PREV_SHA=$SHA
THREAD=$(jq -r 'select(.type=="thread.started").thread_id' "$OUT/$PREV_SHA.jsonl")
SHA=$(git rev-parse HEAD)
echo "The branch now points at $SHA. Review git diff $PREV_SHA..$SHA as a delta under the same
instructions and state whether each earlier finding is resolved." |
  codex exec resume "${THREAD:?no Codex thread for $PREV_SHA}" -m gpt-6.1-sol \
    -c model_reasoning_effort=xhigh -c 'sandbox_mode="read-only"' --json \
    --output-schema docs/procedures/independent-review.schema.json \
    -o "$OUT/$SHA.json" - > "$OUT/$SHA.jsonl"
```

## Verify and record

- Confirm the effective profile in the session rollout under `~/.codex/sessions/`: every `turn_context` must show the recorded model, effort and a `read-only` sandbox.
- Apply substantive findings or record a source-backed disposition for each false one.
- Record the review on the pull request: verdict, reviewed revision, findings with their disposition, and limits. The report is the durable record; `.reviews/` is ignored local evidence and may be deleted once the pull request is merged.

## Relationships

- Implements [Engineering](../guidelines/engineering.md#independent-review): Operates the cross-vendor reviewer profile recorded there.
