# Architectural and domain decisions

Add a numbered Markdown decision when its rationale, alternatives, scope or exceptions will help future work. A short statement of the context, decision and reason can be sufficient; add detail when it matters.

Before writing, consult the authority map and search current and relevant historical decisions for the topic. Read likely matches and their replacement links. Update the existing authority for the same scope; create a decision only for a distinct responsibility, then link it from the map and affected callers.

Preserve the distinction between proposed, current and replaced decisions. When changing a decision, record what changed and link its predecessor and replacement. Keep useful history without turning the current document into an unbounded transcript.

## Decision and delivery states

Use these shared meanings for new documentation. Compatible existing project labels remain valid; document their mapping rather than silently rewriting them. State may be expressed in ordinary Markdown or existing metadata, without a required frontmatter schema.

| Decision state | Meaning |
|---|---|
| `draft` | Incomplete material; it does not establish a settled decision. |
| `proposed` | A choice under consideration, awaiting its applicable acceptance. |
| `accepted` | The choice is approved for its stated scope and conditions; it does not establish implementation or grant an implementation/merge/deployment authorization. |
| `rejected` | The proposal was not adopted; keep useful reasons and the chosen alternative. |
| `superseded` | A linked replacement governs the identified scope; explicitly retain any live part of a partial replacement. |
| `historical` | A dated record of prior reasoning or behavior; read its scope and current replacement before applying it. Archival location alone does not retire a live condition. |

Describe delivery independently when a decision has executable consequences:

| Delivery state | Meaning |
|---|---|
| `not-started` | Implementation has not started for this capability. |
| `in-progress` | Work is partial; name what is implemented and what remains. |
| `implemented` | The named behavior exists in the stated code revision; verification is a separate claim. |
| `verified` | The named behavior has supporting checks/evidence at an identified revision and scope; it does not certify every capability or every future revision. |

For example, an accepted target architecture can have delivery `not-started` and still require an implementation GO. An implemented feature may remain unverified. Do not attach delivery state to a purely documentary rule when it adds no meaning. Missing state or evidence remains unknown, not an inferred acceptance or completion.

Maintain the decision, applicable relationships, delivery scope and verification evidence alongside the code change. Review them together before integration to prevent stale intent, invented rules and contradictory implementation. Preserve prior decisions and evidence when scope changes.
