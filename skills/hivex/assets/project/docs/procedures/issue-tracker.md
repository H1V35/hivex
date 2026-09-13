# Issues, triage and pull requests

Use the repository's configured tracker and conventions. The default workflow is issue-first with a coherent branch and pull request for each change. Repository-specific tracker location, language and commands belong in this procedure when established.

## Define and select work

Read the relevant issue, discussion and dependencies. Reuse an existing issue when its scope covers the request or finding; create a separate one when it is independently actionable or cannot fit the current scope. Read-only retrieval does not require a new issue.

Describe the intended outcome, boundaries and useful acceptance evidence. Split work into independently verifiable changes when that helps execution. Do not require a separate spec, a long user-story list or decision-map hierarchy for a task already defined.

Apply the [triage labels](../guidelines/triage-labels.md) consistently. Work is selectable when its scope is ready and actual blockers are resolved. Labels do not authorize external actions or select the model. Ask only for decisions or information that cannot be recovered from existing evidence.

## Implement, review and integrate

Start from the current integration branch and keep changes coherent. Use the project's commit conventions; Conventional Commits provide a useful default. Prefer existing checks and native Git/tracker capabilities to an additional orchestration layer.

Complete the relevant verification and independent review, then open a clear PR linked to its issue. Explain the resulting behavior, the validation actually completed and material limits. Follow CI to an actual result; a queued or skipped check is not a pass.

Merge within the owner's existing authorization after required checks and review. Do not infer authority from readiness or supervision labels. Preserve shared history and clean up the local/remote branches and worktrees belonging to finished work. Implementation changes must be integrated; completed research, evidence or prototypes must have their useful results and required history retained before cleanup. Keep unfinished or uncertain work intact, regardless of branch age or whether it has a PR. Do not discard unmerged work without an established disposition and the applicable owner authorization.

Record durable decisions in the appropriate Markdown authority and export changed Hivex knowledge with its sources. A tracker discussion is useful evidence but does not replace maintained project documentation.
