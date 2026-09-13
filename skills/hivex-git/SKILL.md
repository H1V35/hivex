---
name: hivex-git
description: Manage issue triage, execution tickets, branches, commits, pull requests, CI follow-through and merge conflicts under the project's conventions and existing authorization.
---

# Keep work and Git coherent

Read the repository's tracker and Git conventions, relevant issue and current working state. Preserve unrelated work. Use the configured tracker and native Git capabilities; do not introduce an orchestration framework to coordinate ordinary changes.

## Triage and plan execution

Reuse an existing issue when its scope fits. Capture a clear outcome, boundaries and useful acceptance evidence. Split work only when independent results or actual dependencies make it easier to execute. Read-only retrieval does not need a new issue.

Use the project's triage guide. For an adopter without one, the [foundation triage guide](../hivex/assets/project/docs/guidelines/triage-labels.md) defines the baseline. Readiness, expected owner involvement, dependencies and risk are different dimensions; none selects a model or grants permission.

When adopting or aligning labels, use the [common catalogue](assets/labels.json) as the starting set and compare the live catalogue with the documented meaning. Preserve useful project-specific areas and reconcile aliases without losing issue classification. The shared names `epic`, `research`, `prototype` and `decision` describe work independently of a skill. The catalogue also identifies known legacy aliases. If a replacement label already exists, transfer useful issue classification before retiring the alias; do not discard dependencies or change issue scope. Retire a generic task label only when the remaining labels and issue retain its useful meaning.

Use native blocking/sub-issue relationships when they help coordination. Do not create a decision-map hierarchy for already defined work or declare something ready merely because a label is present.

## Commit and open a PR

Start a coherent branch from the current integration branch, following the project's naming conventions. Use its commit format and language; Conventional Commits are a useful default. Check the diff and the relevant verification before publishing the branch.

Keep the PR focused and linked to its owning issue. Lead with the concrete problem and resulting behavior, then the validation and material limits. Use a structured body argument or body file for multiline text. Follow the project's readiness convention for drafts.

Track CI to an actual result. Fix demonstrated problems within scope, and preserve an honest distinction between failure, omission and success. Confirm that the independent review is current and uses the implementation agent's model and effort. Reuse a valid completed review instead of starting another; do not infer approval from an automated knowledge report.

## Integrate and clean up

Merge when the required checks and review pass and the owner's existing authorization covers the action. Do not ask for the same authorization again; if a tool blocks an otherwise authorized action, explain the actual blocker and prepare the concrete result before asking for what is missing.

When resolving conflicts, read both changes and their intent, preserve needed behavior and run the checks affected by the resolution. Do not choose one side blindly or overwrite unrelated work.

Preserve shared history and avoid direct integration-branch pushes or force pushes. After integration, update the appropriate checkout safely.

## Finish the branch lifecycle

Clean up the local and remote branches and worktrees belonging to completed work. For implementation work, establish that the intended changes were integrated. Research, evidence and prototype branches may have a different end state: preserve their useful results and required history in project documentation or an appropriate archive before retiring a completed branch.

Keep unfinished work, including research, evidence and prototypes that have not reached their intended outcome. Branch age, absence of a PR or absence of an open issue is not enough to establish completion. Inspect the associated work and its disposition; leave uncertain or unrelated branches intact instead of treating cleanup as a repository-wide purge.

Do not discard unmerged work merely to bypass a Git deletion refusal. Establish that it is integrated, its needed results/history are preserved, or discarding it is covered by the owner's explicit authorization. Preserve knowledge and required evidence before cleaning local execution state.
