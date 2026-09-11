# Hivex domain language

Hivex supplies project knowledge to the agents responsible for implementation and review. Markdown
records that knowledge; the graph helps locate and interpret it without becoming authority itself.

## Language

**Document**: A selected Markdown file, wherever its project, package or module keeps it.

**Archived document**: Historical Markdown preserved as evidence of replaced decisions and their
reasons. It remains available for focused retrieval without being presumed current authority.

**Ingestion unit**: A bounded fragment of a document with its original line range. It permits
processing and resumption in rounds without becoming a separate documentary authority.

**Document version**: The exact contents of a document at a point in the work. A working copy is a
version even when it has not been committed; its existence does not establish approval.

**Snapshot**: The selected document versions considered together for a particular work item.

**Decision**: A meaningful project choice or constraint together with its scope, conditions,
exceptions and reasons. Proposals and historical decisions retain their declared state.

**Relationship**: An evidenced connection between decisions, such as a dependency, exception or
replacement. It may cross documents that have no authored link; its interpretation can be uncertain.

**Evidence**: An identifiable passage of a particular document or implementation version that a
reader can inspect. A model's paraphrase is not the passage itself.

**Knowledge graph**: Derived decisions and relationships, with supporting definitions and lessons, that help an agent recover project context.
It may be incomplete or uncertain without making every usable part unavailable.

**Knowledge snapshot**: A portable representation of reusable graph knowledge, its source versions,
evidence, provenance and coverage. It is distinct from a document snapshot and from execution state.

**Applicability**: Whether a decision governs the case being considered after its conditions,
exceptions and replacements have been taken into account.

**Freshness**: Whether derived knowledge still corresponds to the selected document versions.
Freshness does not establish applicability or correctness by itself.

**Knowledge update**: Processing a selected set of document versions into decisions and relationships,
followed by one bounded check of that set and its affected relationships.

**Knowledge check**: Examination of an update against its sources to identify omissions or incorrect
interpretations. It reports issues and uncertainty, not a certificate of global completeness.

**Interpretation repair**: Replacing a wrong derived interpretation by comparing it with unchanged
Markdown. It preserves source authority and does not resolve a genuine policy conflict by itself.

**Context**: The decisions, related evidence and remaining uncertainties relevant to a particular task.

**Implementation version**: The captured change against a particular base together with the exact
contents of the affected files. Later code changes are outside that review.

**Finding**: A possible conflict between an implementation and project knowledge. The principal
reviewer verifies it and retains responsibility for the implementation review.

**Work**: One requested update, consultation or review, including its phases and any resumed work.

**Work budget**: The limits shared by every phase and attempt of one work item. Resuming does not
reset its consumption, and unknown consumption remains visible.

**Recovery**: Incorporating useful historical decisions, reasons and lessons into their appropriate
Markdown authorities while identifying obsolete, duplicate or purely operational material.
