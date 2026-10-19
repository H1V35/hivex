---
title: "Independent review"
status: draft
created_at: 2026-10-03
updated_at: 2026-10-19
tags: [workflow, review, agents]
---

# Independent review

Run the independent review defined in [engineering](../guidelines/engineering.md#independent-review) before integrating a pull request. The reviewer follows the `hivex-review` skill in a read-only session that never saw the implementation, reads the intended outcome from the tracker itself and returns a structured report.

## Choose the reviewer

Use the first option available, in this order, and record which one ran:

1. **Cross-vendor**: a model from a different vendor than the implementing agent, so the review does not share its blind spots. When Claude implements, review with Codex; when Codex or another OpenAI model implements, review with Claude Code.
2. **Same vendor, isolated**: when no other vendor is available, a new session of the same vendor with no access to the implementation conversation. It still reads only the tracker, the diff and the repository.

Use the reviewer profile recorded in engineering (tool, model and effort). Never fall back silently: the review record states the independence level and the profile that actually ran.

## Prerequisites

- The branch is pushed and its issue and pull request state the intended outcome. Dependency updates opened by a bot may have no issue; omit the issue capture and say so in the prompt.
- The reviewer tool is installed and authenticated: `codex` or `claude`, plus `jq` and an authenticated `gh`.
- `/.reviews/` stays ignored for local review traces.

## Run the review

Capture the tracker first; the read-only sandbox may have no network access. An empty capture stops the run.

```bash
ISSUE=<issue> PR=<pr> BASE=origin/main SHA=$(git rev-parse HEAD)
OUT=.reviews/$PR && mkdir -p "$OUT"
gh issue view "$ISSUE" --json title,body,comments > "$OUT/issue.json"
gh pr view "$PR" --json title,body,comments,reviews > "$OUT/pr.json"
test -s "$OUT/issue.json" && test -s "$OUT/pr.json"
PROMPT="You are the independent reviewer for issue #$ISSUE (pull request #$PR) in this repository.
Follow .agents/skills/hivex-review/SKILL.md. Read-only: do not edit files, commit, push or comment
on GitHub.
Reviewed revision: $SHA, compared with its merge base: git diff $BASE...$SHA.
Read the intended outcome yourself from the unedited tracker captures $OUT/issue.json and
$OUT/pr.json, together with the diff and the repository's Markdown authorities.
Report every actionable finding with severity, location, affected scenario, evidence and a concrete
fix. Record the verification you ran and any material limit of the review. Do not restate the diff
or add praise."
```

With Codex (`-o` writes the report; `--json` keeps the full trace):

```bash
codex exec -m <model> -c model_reasoning_effort=<effort> -s read-only --json \
  --output-schema docs/procedures/independent-review.schema.json \
  -o "$OUT/$SHA.json" - > "$OUT/$SHA.jsonl" <<< "$PROMPT"
```

With Claude Code (`plan` mode cannot change files; the session ID allows a later delta review):

```bash
SESSION=$(uuidgen | tr 'A-Z' 'a-z')
claude -p --model <model> --effort <effort> --permission-mode plan --session-id "$SESSION" \
  --json-schema "$(cat docs/procedures/independent-review.schema.json)" \
  --output-format json "$PROMPT" > "$OUT/$SHA.claude.json"
jq '.structured_output' "$OUT/$SHA.claude.json" > "$OUT/$SHA.json"
echo "$SESSION" > "$OUT/$SHA.session"
```

## Confirm the profile that ran

Check the effective profile before trusting the report:

- Codex: read the session whose file name contains the thread ID, `ls ~/.codex/sessions/*/*/*/rollout-*-$THREAD.jsonl` with `THREAD=$(jq -r 'select(.type=="thread.started").thread_id' "$OUT/$SHA.jsonl")`. Every `turn_context` must show the recorded model, effort and a `read-only` sandbox. Codex may start helper sessions, such as an approvals reviewer with another model, whose files also mention the thread; they are not the review.
- Claude Code: `modelUsage` in the JSON result names the model that answered; confirm it and that the command used `--permission-mode plan`.

## Review later changes as a delta

Resume the same session so the reviewer keeps its own context; refresh the tracker captures first if they changed. Do not restate its findings.

```bash
PREV_SHA=$SHA SHA=$(git rev-parse HEAD)
DELTA="The branch now points at $SHA. Review git diff $PREV_SHA..$SHA as a delta under the same
instructions and state whether each earlier finding is resolved."
# Codex
codex exec resume "$THREAD" -m <model> -c model_reasoning_effort=<effort> \
  -c 'sandbox_mode="read-only"' --json \
  --output-schema docs/procedures/independent-review.schema.json \
  -o "$OUT/$SHA.json" - > "$OUT/$SHA.jsonl" <<< "$DELTA"
# Claude Code
claude -p --resume "$(cat "$OUT/$PREV_SHA.session")" --permission-mode plan \
  --json-schema "$(cat docs/procedures/independent-review.schema.json)" \
  --output-format json "$DELTA" > "$OUT/$SHA.claude.json"
```

## Verify and record

- Apply substantive findings, or record a source-backed disposition for each false one. Add a regression check for a demonstrated defect when it protects meaningful behavior.
- Record the review on the pull request: independence level, tool, model and effort; reviewed revisions; verdict; each finding with its disposition; and the reviewer's limits. Note checks it could not run, such as CI in a sandbox without network, and their actual results.
- The pull request comment is the durable record; `.reviews/` is local evidence that may be deleted once the pull request is merged.
- Review does not grant merge or publication permission.

## Relationships

- Implements [Engineering](../guidelines/engineering.md#independent-review): operates the reviewer profile and independence order recorded there.
