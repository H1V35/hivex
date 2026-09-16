---
name: hivex-review
description: Independently review a change for intended scope, correctness and project standards, including relevant decision dependencies and exceptions.
---

# Review the actual change

Establish the comparison base, reviewed revision and intended outcome from the issue or specification. For a branch review, inspect the diff from its merge base with the relevant integration revision. Read the surrounding code and affected authorities needed to assess behavior.

Assess the change independently under the project's engineering policy and state any limit to that independence.

Assess scope, correctness, regressions and relevant standards. Ground findings in an affected scenario and evidence; omit speculative defects, existing problems outside scope and style preferences handled by configured tools. Identify simpler solutions when they materially improve the result.

Recover decisions, dependencies, exceptions and replacements through Hivex's local evidence. Model-assisted knowledge review is optional support; verify its findings against sources and code. Its silence is not implementation approval.

Check whether verification protects meaningful behavior and covers the change. TDD is optional and reserved for sufficiently defined critical flows; do not require it, a test per edit or assertions shaped around the implementation. Distinguish an unverified outcome from a demonstrated defect.

Report actionable findings with location, impact and evidence, or a clean result with material verification limits. Explain technical details when needed. Review later changes as a delta, reusing conclusions that remain valid. Add another pass only for concrete risk or findings; never repeat reviews to obtain approval. Review does not grant merge or publication permission.
