---
name: hivex-review
description: Independently review a change or pull request for intended scope, correctness and project standards, with relevant decision dependencies and exceptions. Use for implementation review or a review requested against a commit, branch or diff.
---

# Review the actual change

Establish the comparison base, reviewed revision and intended outcome. Read the relevant issue or specification and affected project authorities. Review the diff and the context necessary to understand its behavior; reuse prior findings and verification that still apply.

Use one independent reviewer by default. If you are already assigned as that reviewer, perform the review directly; do not spawn another review pass. Its model and reasoning effort must match the implementation agent's profile. Pass the profile explicitly when defaults differ and verify the effective profile through the available tools. Do not substitute the knowledge model for this review. If independent review cannot be performed, say what remains unreviewed.

## Check the whole result

Assess the intended scope, behavior, regressions and the standards relevant to this repository. Focus on concrete problems and simpler solutions that materially improve correctness or maintenance. Do not manufacture findings from stylistic preferences already handled by the configured tools.

Use Hivex's local retrieval and source reading for decisions, dependencies, exceptions and replacements. Ask for model-assisted knowledge review only when it contributes information the review needs. Confirm its findings against the actual sources and code; it is support for the principal review, not a replacement.

Evaluate whether the checks protect meaningful behavior and whether their evidence covers the change. Do not require TDD, a test for every edit, or implementation-shaped assertions. Distinguish an unverified outcome from a demonstrated defect.

## Report and follow through

Report actionable findings with their location, impact and supporting evidence. Explain technical details clearly when needed to understand the problem. State a clean result when there are no justified findings, together with material verification limits.

A later change invalidates only the affected conclusions. Review that delta instead of repeating the entire review without cause. Add another independent pass when concrete risk or findings justify it, preserving the required model/effort profile.

Resolve adverse findings by evidence or correction, not repeated attempts to obtain approval. A review result does not grant merge or publication permission; existing owner authorization and repository requirements still govern those actions.
