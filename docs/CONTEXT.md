# Hivex domain language

- **Project** — the body of versioned knowledge adopted by a team and its agents.
- **Snapshot** — an immutable view of a project's sources and their selection rules.
- **Source** — an authored document that carries project knowledge. An index is not a source.
- **Section** — a named part of an authored document, including its subordinate content. It shares the document’s provenance; selecting it does not establish independent authority.
- **Collection** — an explicitly declared scope for finding related sources. It expresses relevance, not permission or revocation.
- **Block** — a complete structural fragment of a source, such as a paragraph, list, table or code example.
- **Authority declaration** — what a source explicitly says about its status or replacement. A declaration alone does not establish which rule applies to a particular case.
- **Indexed relation** — a derived interpretation connecting an affected rule to a replacement or amendment. Its provenance allows inspection; it can be incomplete or stale and is not itself authority.
- **Currentness** — whether a rule applies in the relevant context after considering scope, conditions, exceptions and subsequent decisions.
- **Citation** — the identity and location of evidence within a fixed snapshot.
- **Claim** — an assertion about a project, including its conditions, exceptions and supporting evidence. A claim can be proposed or incorrect.
- **Candidate** — a derived interpretation awaiting admission. Producing or structurally validating it does not make it authority.
- **Ingestion plan** — a complete declared source cohort bound to its snapshot and processing inputs. It measures and identifies prospective work without executing or admitting it.
- **Ingestion store** — bounded local state for one frozen cohort, its claimed sources, attempt reports and unaccepted candidates. It is not documentary authority or an admitted graph.
- **Unresolved invocation** — work whose durable state cannot establish a retained final result. Neither a timeout nor a missing process establishes that no model request occurred.
- **Grounding** — checking an implementation against applicable documented decisions, using evidence from both the implementation and the documents.
- **Continuation** — a position from which reading the same source and snapshot can resume without silently losing content.
