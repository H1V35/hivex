---
name: hivex-implement
description: Implement a defined change, diagnose a bug or refactor code with useful verification and independent review.
---

# Implement the intended outcome

Recover the request, owning issue and relevant project decisions through local Hivex evidence as needed. Apply settled scope, domain language and module boundaries. Ask only when evidence leaves a consequential decision open.

Prefer existing capabilities or a direct solution over speculative layers and dependencies. For a difficult bug, reproduce the failure, use a falsifiable hypothesis and observe what distinguishes causes before changing code; remove temporary diagnostics when finished.

## Verify for value

TDD is optional and reserved for critical flows whose intended behavior is sufficiently defined. Clarify open behavior before freezing assumptions in tests. Even a critical flow does not require TDD if another verification approach is more useful.

Each test must protect an important behavior, meaningful invariant or demonstrated regression. Use observable interfaces and independent expected results; substitute external dependencies when necessary without mocking internal implementation details. Do not add tests merely to mirror helpers, files or structure. Judge failures against the intended contract before deciding whether code or tests need correction.

Run applicable repository checks and report actual results and material limits. Repeat or broaden verification only when changes, failures or unresolved concerns warrant it.

Record durable decisions through `hivex-document`. Obtain an independent `hivex-review` under the project's engineering policy. Resolve findings on their merits, then use `hivex-git` to complete commit, PR and CI work under the project's Git procedure.
