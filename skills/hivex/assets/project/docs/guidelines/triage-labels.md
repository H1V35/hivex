# Triage labels

Labels should help people and agents select or act on work. Keep their meanings distinct and the catalogue small.

## Readiness

| Label             | Meaning                                           |
| ----------------- | ------------------------------------------------- |
| `needs-triage`    | The issue needs evaluation.                       |
| `needs-info`      | Essential information is missing.                 |
| `ready-for-agent` | The work is defined enough for an agent to begin. |
| `ready-for-human` | The work requires human implementation.           |
| `wontfix`         | The issue will not be actioned.                   |

Keep the readiness state coherent. Readiness does not promise that every later decision is settled, select a model or override an unresolved dependency.

## Participation, dependencies and risk

`AFK` means the stated work can finish without further owner participation; `HITL` means owner participation is expected before completion. Use these when they help coordination. Neither grants permission or replaces readiness.

Use native issue dependencies for actual blockers and `blocked` when a visible blocked marker helps selection. `deferred` means the work is decided but intentionally not scheduled. Avoid starting blocked or deferred work merely because its scope is defined.

`high-risk` directs review attention to consequential changes. It does not choose a vendor, model or fixed number of reviews. Security severity labels (`severity: critical`, `severity: high`, `severity: moderate`, `severity: low`) describe the finding's severity, which is distinct from implementation risk.

## Type and scope

Use useful work types such as `bug`, `enhancement`, `documentation`, `refactor`, `testing`, `architecture`, `security`, `performance` or `ci`. `epic` groups coherent child work; `research`, `prototype` and `decision` identify work that resolves uncertainty. These names describe the work and do not depend on a particular skill.

Use area or component labels that match this project. Preserve useful existing classification when adopting the standard; do not import another product's area catalogue or add labels that repeat the title. Follow the [tracker procedure](../procedures/issue-tracker.md).
