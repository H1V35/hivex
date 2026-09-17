# Issues, triage and pull requests

Hivex uses GitHub issues and pull requests in `H1V35/hivex`, with the `gh` CLI. Work is issue-first, with a coherent branch and PR for each change. Tracker/PR text and commit messages are Spanish; code, branches and repository Markdown are English. Commits and PR titles use an emoji followed by a Conventional Commit type/scope and a Spanish description.

## Define and select work

Read the relevant issue, discussion and dependencies. Reuse an existing issue when its scope covers the request or finding; create a separate one when it is independently actionable or cannot fit the current scope. Read-only retrieval does not require a new issue.

Describe the intended outcome, boundaries and useful acceptance evidence. Split work into independently verifiable changes when that helps execution. Do not require a separate spec, a long user-story list or decision-map hierarchy for a task already defined.

Apply the [triage labels](../guidelines/triage-labels.md) consistently. Work is selectable when its scope is ready and actual blockers are resolved. Labels do not authorize external actions or select the model. Ask only for decisions or information that cannot be recovered from existing evidence.

## Implement, review and integrate

Start from the current integration branch and keep changes coherent. Use the project's commit conventions; Conventional Commits provide a useful default. Prefer existing checks and native Git/tracker capabilities to an additional orchestration layer.

Complete the relevant verification and independent review, then open a clear PR linked to its issue. Explain the resulting behavior, the validation actually completed and material limits. Follow CI to an actual result; a queued or skipped check is not a pass.

The owner reserves the final merge decision for each PR. Agents prepare reviewed, passing PRs and stop before integration unless the owner explicitly approves that specific PR. Do not infer authority from a general implementation GO, readiness labels or access to the owner's GitHub credentials.

`main` requires a pull request, an up-to-date successful `quality` check from GitHub Actions and resolved review conversations. Force pushes and deletion are blocked. A separate update restriction permits only the owner's account to integrate through a PR; it does not bypass the integrity requirements. Do not require a second person's approval on owner-authored PRs: GitHub does not allow self-approval. Review evidence and the owner's final merge decision remain separate. These account-level controls cannot distinguish the owner from tools using the owner's credentials, so agents must also honor the per-PR authorization policy.

Preserve shared history and clean up the local/remote branches and worktrees belonging to finished work. Implementation changes must be integrated; completed research, evidence or prototypes must have their useful results and required history retained before cleanup. Keep unfinished or uncertain work intact, regardless of branch age or whether it has a PR. Do not discard unmerged work without an established disposition and the applicable owner authorization.

Record durable decisions in the appropriate Markdown authority and export changed Hivex knowledge with its sources. A tracker discussion is useful evidence but does not replace maintained project documentation.
