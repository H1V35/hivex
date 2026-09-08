# Documentation map

- [Domain language](CONTEXT.md): terms used by the product and its source configuration.
- [Engineering workflow](engineering.md): development, verification and durable knowledge.
- [Versioned sources](adr/0001-versioned-project-knowledge.md): the product's authority model.
- [Native knowledge candidates](adr/0002-native-knowledge-candidates.md): model invocation and
  candidate evidence; rebuilding and grounding remain separate acceptance requirements.
- [Independent Bun installation](adr/0003-independent-bun-installation.md): runtime and package installation.
- [Resumable ingestion](adr/0004-resumable-ingestion-store.md): bounded candidate persistence and interruption handling.
- [Graph snapshots](adr/0005-source-bound-graph-snapshots.md): source-bound projections, integrity and freshness before admission.
- [Source fidelity](adr/0006-source-fidelity-review.md): complete extraction assessments before semantic admission.
- [Source comparisons](adr/0007-evidence-bound-source-comparisons.md): scoped relationships with evidence from both sources.

- [Reviewed graph admission](adr/0008-reviewed-graph-admission.md): complete evidence, scoped relationships and historical inspection.
- [Implementation claim grounding](adr/0009-implementation-claim-grounding.md): exact code snapshots, scoped precedence and reusable review evidence.

The [CLI guide](../README.md) describes current executable interfaces. Historical evidence under
`evidence/` records a bounded experiment at its cited revision, not a standing quality verdict.
An adopting project owns its product documentation and source configuration outside this repository.
