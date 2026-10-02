---
title: "Architectural and domain decisions"
status: draft
created_at: 2026-09-13
updated_at: 2026-10-02
tags: [adr, documentation]
---

# Architectural and domain decisions

Add a numbered Markdown decision when its rationale, alternatives, scope or exceptions will help future work. A short statement of the context, decision and reason can be sufficient; add detail when it matters.

Before writing, consult the authority map and search current and relevant historical decisions for the topic. Read likely matches and their replacement links. Update the existing authority for the same scope; create a decision only for a distinct responsibility, then link it from the map and affected callers.

Preserve the distinction between proposed, current and replaced decisions. When changing a decision, record what changed and link its predecessor and replacement. Keep useful history without turning the current document into an unbounded transcript.

<a id="decision-and-delivery-states"></a>

## Decision and implementation states

Use the exact lowercase state literals below in maintained metadata. Normalize legacy labels during adoption while preserving their scope and qualifications in the body; immutable historical bodies retain their original wording. Documentation inside `docs/` has frontmatter with `title`, truthful `status`, `created_at` and topic `tags`, optional `updated_at` after changes, and `archived_at` on archived records. Decision status, implementation, verification and permission remain separate. Existing source layouts stay readable while adopting this convention.

| Decision state | Meaning |
|---|---|
| `draft` | Incomplete material; it does not establish a settled decision. |
| `proposed` | A choice under consideration, awaiting its applicable acceptance. |
| `accepted` | The choice is approved for its stated scope and conditions; it does not establish implementation or grant an implementation/merge/deployment authorization. |
| `rejected` | The proposal was not adopted; keep useful reasons and the chosen alternative. |
| `superseded` | A linked replacement governs the identified scope; explicitly retain any live part of a partial replacement. |
| `historical` | A dated record of prior reasoning or behavior; read its scope and current replacement before applying it. Archival location alone does not retire a live condition. |

Use optional `implementation` metadata when a decision describes executable consequences. It records the implementation of that stated scope independently of acceptance:

| Implementation state | Meaning |
|---|---|
| `not-started` | Implementation has not started for this capability. |
| `in-progress` | Work is partial; name what is implemented and what remains. |
| `implemented` | The named behavior exists in the stated code revision; verification is a separate claim. |
| `removed` | The named implementation previously existed and has been withdrawn; record the removal revision and any surviving scope or guarantees. Archive placement is not evidence of removal. |

For example, an accepted target architecture can have `implementation: not-started` and still require an implementation GO. An implemented feature may remain unverified. Verification belongs in the body with its evidence, revision and scope; it is not another implementation literal. Omit the field for indexes, glossaries and documentary guidance where it adds no meaning. Missing state or evidence remains unknown, not an inferred acceptance or completion. Normalize legacy `delivery` during adoption without dropping its qualifications or verification evidence.

Maintain the decision, applicable relationships, implementation scope and verification evidence alongside the code change. Review them together before integration to prevent stale intent, invented rules and contradictory implementation. Preserve prior decisions and evidence when scope changes.
