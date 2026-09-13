---
name: hivex-git
description: Manage issue triage, execution tickets, branches, commits, pull requests, CI, merge conflicts and completed-work cleanup under project conventions and existing authorization.
---

# Keep work and Git coherent

Read the project's tracker/Git conventions, relevant issue and working state. Preserve unrelated work. Reuse an issue when its scope fits; read-only retrieval needs no new ticket. Define the outcome, boundaries and useful acceptance evidence, splitting only for independent results or actual dependencies.

## Triage

Follow the project's guide or the [foundation triage guide](../hivex/assets/project/docs/guidelines/triage-labels.md). Recover relevant prior decisions, including why work was rejected, before reopening the question. Record durable reasons in their existing Markdown authority instead of another knowledge database.

Use the [common label catalogue](assets/labels.json) when adopting or aligning labels. Preserve useful project areas and transfer issue classification before retiring aliases. `epic`, `research`, `prototype` and `decision` describe work independently of a skill; a generic task label can be retired once its useful meaning is retained. Readiness, participation, dependencies and risk are distinct; labels grant no permissions and select no model.

Use native blocking/sub-issue relationships when helpful. A label alone does not establish readiness, and defined work needs no decision-map hierarchy.

## Publish and integrate

Start a coherent branch from the current integration branch. Follow the project's commit format and language, with Conventional Commits as a useful default. Check the diff and applicable verification; link the PR to its issue and explain the resulting behavior, validation and material limits. Use structured arguments or a body file for multiline text and follow the project's draft convention.

Follow CI to an actual result and confirm the independent review is current and matches the implementation agent's model/effort. Reuse valid verification. Resolve conflicts from both changes' intent and check affected behavior. Preserve shared history; avoid direct integration-branch pushes and force pushes.

Merge when required checks/review pass and existing owner authorization covers it. Do not ask for the same permission again. If a tool blocks the action, explain the actual blocker and prepare the reviewable result before asking for what is missing. Update the appropriate checkout safely after integration.

## Clean up completed work

Remove local/remote branches and worktrees belonging to completed work. Establish integration for implementation branches. Research, evidence and prototypes may have another end state: preserve useful results and required history before retiring them.

Keep unfinished, uncertain or unrelated work. Age, no PR or no open issue does not prove completion. Do not discard unmerged work to bypass a deletion refusal; establish integration, preservation of needed results/history, or explicit authorization to discard it. Preserve required evidence before cleaning execution state.
