---
name: hivex-implement
description: Implement a defined change, diagnose a bug, or refactor code with focused verification and review. Use when work is ready to execute; use design support only for decisions that remain open.
---

# Implement the intended change

Recover the request, the owning issue when present, and relevant project decisions. Use Hivex's local search, neighbors and sources as needed. Carry settled scope forward; raise a question only when the evidence leaves a consequential decision open.

Follow the repository's engineering guideline, domain language and actual module boundaries. Prefer a direct solution, existing capability or standard library before adding layers, dependencies or speculative flexibility. Keep responsibility for the integrated result when delegating suitable bounded subtasks.

For a bug, establish the failing behavior and inspect the relevant path before changing code. Use observations to distinguish causes. Keep a reproduction or regression test when its value justifies its maintenance; do not accumulate broad diagnostics or unrelated fixes.

## Choose useful verification

Prioritize critical flows, stable rules and regressions. Use TDD when the behavior is sufficiently defined and it helps the task, rather than as a universal sequence. Explore or clarify open behavior before locking assumptions into tests.

Test observable results through meaningful interfaces with independent expected values. Avoid tests coupled to private helpers or arbitrary structure. Investigate a failure against the intended contract; do not mechanically preserve a bad test or change a test merely to make it pass.

Run the repository checks that apply to the changed surfaces. Broaden checks when a change, failure or unresolved concern warrants it. Report actual results and material limits without repeating checks that remain valid.

Update the owning Markdown for durable decisions, keeping conditions and exceptions. Use `hivex-document` when that work needs documentation support.

## Finish the work

Use one independent `hivex-review` pass by default. Supply the intended scope, relevant authorities and the implementation agent's actual model and effort so the reviewer matches them. Add another review only for a concrete reason. Resolve findings on their merits.

Use `hivex-git` to complete the authorized commit, PR and CI work. Preserve existing owner authorization and make any remaining dependency or required user action explicit.
