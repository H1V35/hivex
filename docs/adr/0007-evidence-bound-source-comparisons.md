---
title: Evidence-bound relationships between complete sources
status: accepted
date: 2026-09-08
---

# Evidence-bound relationships between complete sources

Compare a selected pair of complete versioned sources to propose relationships between their claims. Use the admitted native Luna/max route and retain both source texts, their candidate statements and source-local relationships as input. The same source cannot occupy both sides; both need extracted claims. Missing or oversized input fails before a model invocation rather than becoming an empty or truncated comparison.

Every supplied claim needs one assessment, even when unrelated to the other source. Relationships connect claims from different supplied sources and carry literal evidence from both endpoints. Both endpoint assessments must reference the relationship. Conditions, exceptions and the affected claim scope remain explicit. A partial exception must not become whole-claim or whole-document supersession. Dates, status labels and identifier order alone do not establish precedence.

The model receives short, source-qualified claim identifiers and source aliases. The driver binds them deterministically to the original graph IDs and source identities, rejecting references outside the supplied pair. Preserve the source bindings and hashes of both the normalized model output and the expanded comparison. This saves repeated opaque hashes in model requests and responses without discarding external provenance. Pair serialization is canonical for reproducibility and caching; it is not authored order. Original source positions remain available for documentary order.

Derive the result from the full validated assessment, rather than a model's global verdict. Reported contradictions, unknown scope, incomplete coverage, unresolved claims and insufficient context prevent success. Keep valid negative findings available with their evidence. Malformed output or invented citations fail explicitly and preserve invocation accounting. A comparison does not decide doctrine or invent a resolution on the owner's behalf.

The prepare operation shows the exact bounded request without invoking the model. Execution returns one result on stdout, with no per-comparison files or graph mutation. The current request limit is 256 KiB, covering both complete sources and their extracted context.

Context sufficiency concerns the selected pair. A faithfully preserved reference or a claim unrelated to the other source does not require supplying its external document or prototype merely to verify the assertion's external basis. Keep the reference and its limits explicit. Missing context still blocks when it prevents judging candidate fidelity or an actual relationship within the supplied pair.

An explicit authored amendment can resolve a difference by replacing or extending part of an earlier rule. Represent that affected scope with `supersedes` or `exception-to`, preserving the prior claim, unchanged parts, conditions and exceptions. Reserve `contradicts` for incompatibility that remains unresolved; do not add a redundant contradiction solely because the former and amended rules differ. Authored evidence must establish that resolution; dates, IDs and status labels alone remain insufficient.

Consistent scope repeated in a statement and its conditions or exceptions is not an ambiguity by itself. A fidelity objection must identify changed meaning or conflicting applicability in the actual supplied claim; it cannot invent metadata that the claim does not contain.

These clarifications address two observed comparison failures in [Hivex #19](https://github.com/H1V35/hivex/issues/19). They change the comparison request hash. Earlier assessments and consumption remain in their complete archives and cannot be silently relabelled as results of the clarified request. Graph admission retains its existing requirement to reject unresolved contradictions, incomplete coverage and insufficient context.

Quotes remain byte-for-byte source evidence, including newlines. Prefer separate single-line quotes when spanning Markdown lines. Invalid model output is retained with its hash as untrusted diagnostics, never as a partial comparison. Inspection verifies that hash and its invalid-output status, including previous attempts after recovery. Existing result/history bounds apply; older discarded text cannot be reconstructed from its error message or treated as an assessment.

This operation covers the selected pair only and always returns `accepted: false`. It does not establish graph-wide relationship coverage, effective authority, admission or code grounding. Those remain separate requirements of the graph-admission workflow, including any applicable historical composition and atomic-evaluation obligations.

## Explicit supporting context for a selected pair (#41)

A caller may attach at most two existing graph sources as supporting documentary context to a selected primary pair. A version-1 JSON mapping contains only source IDs, never source text, and must fit 128 KiB. Primary/supporting IDs and pairs are normalized in sorted order. Unknown IDs, duplicate pairs or sources, primary sources used as their own context, and unused configured pairs are rejected before comparison. A standalone comparison accepts context for that pair only; a cohort accepts entries for pairs in its selected plan.

Supporting sources resolve from the same verified graph and source snapshot. Supply their complete Markdown, authority and original positions as `x1`/`x2`, without their extracted claims. The TWO primary sources remain the only claim targets and relationship endpoints. Literal supporting quotes may justify scoped precedence; every relationship still needs evidence from both primary endpoints. Supporting material establishes no automatic exception or doctrinal resolution. Existing context, contradiction, scope and admission checks remain in force.

Without supporting context, the existing request bytes, model schema and global comparison contract remain unchanged. Contextual pairs use the existing per-unit schema override and bind their complete prompt, supporting descriptors and citation aliases. Selection exports embed normalized IDs and source hashes; admission and historical reuse reconstruct this selection from its original graph, never a mutable external configuration path. Only pairs with identical complete inputs reuse results. Changing context on the same graph requires the current full export, `--from/--reuse` and zero units before execution. Earlier adverse results and consumption remain in that archive. Unchanged results and their recovery history are retained; uncertain invocations still prevent replacement.

`update` attaches context only to an existing verified managed candidate and matching ingestion store. It rejects a missing/stale candidate before calls or mutations. A context change requires the explicit comparison transition first; update verifies that cohort before adopting the new normalized mapping into its checkpoint. Omission on resume uses that frozen mapping. It never rereads an omitted configuration path or performs an implicit contextual reassessment.

## Plan comparisons from authored Markdown links

Discover candidate source pairs deterministically from parsed Markdown links in the same graph snapshot. Inline and reference-style links retain their original link evidence; reference definitions retain their own original document positions, including definitions outside a selected section. Only links located inside the selected section participate. CommonMark identifier normalization and first-definition precedence come from the existing Markdown parser ecosystem.

A link suggests a comparison, not a semantic relationship or precedence. Collapse both directions into one source pair while retaining each authored reason. Resolve relative paths within the repository and target headings within the selected graph sources. Missing targets, unsupported paths, missing anchors and pairs without claims remain explicit. External destinations and non-Markdown assets are counted without being opened. Code examples, images and raw HTML are not scanned for Markdown relationships.

The plan records its graph hash, policy, provenance, selected pairs, unresolved links and the number of possible pairs. It covers authored Markdown links only; absent links do not prove independence, and this policy does not establish full semantic-neighborhood coverage or graph admission.

Bound discovery to 10,000 link definitions/links per document, 10,000 inspected links per plan, 32,768 pairs and an 8 MiB working/output budget. A requested smaller output budget either returns the complete plan or fails explicitly. Planning makes no model calls, modifies no graph and creates no per-plan files. A caller may retain the complete result when it is needed for later review.

## Retain and resume the selected comparison cohort

Execute the authored-link selection through the same comparison contract, with one local `.hivex/comparisons.sqlite` working store. Bind it to the graph, the complete selection hash and every pair's prompt and processing contract. Recompute and validate the selection before execution or inspection. An empty selection or unresolved link plan cannot become a successful cohort.

Source-fidelity and pair-comparison cohorts share the atomic claim, capacity reservation and retention implementation. Their database application identities remain distinct; opening one as the other fails. The existing source-review format remains compatible. A comparison cohort supports at most 2,048 pairs and a 1 MiB processing plan; the authored discovery plan retains its separate 8 MiB limit. The database is bounded to 128 MiB, each complete result to 8 MiB, and each active/new claim reserves 16 MiB before a model request. Source texts are loaded once per command; complete pair prompts are prepared before opening the store, without retaining all prompts in memory.

Resume pending pairs without repeating retained results. Stop the current invocation loop after a negative or failed result. A later command may process other pending pairs, but does not retry or approve the negative result. Unknown outcomes remain unresolved. Inspection/export revalidates coverage, original citations, source bindings, expanded/normalized comparison hashes and completed native invocation evidence. A complete export must fit the requested budget; it is never truncated.

Retirement requires the exact processing-plan hash and rejects unresolved invocations. Preserve needed evidence before retiring: it clears the retained comparisons transactionally and allows reuse of the same bounded file. There is no automatic rotation, per-pair file tree or implicit retry. This working cohort still covers only the selected authored-link pairs; completing it does not establish full semantic coverage, graph admission or implementation grounding.

## Optional lexical neighbors

Authored links alone miss relationships that an author never linked. `--neighbors 1..8` adds a bounded deterministic selection over the graph's extracted statement text, conditions, exceptions and quoted evidence. It does not infer a semantic relationship or remove unresolved authored links. `--neighbors 0` is the default and preserves the existing authored-link plan byte for byte.

Build one in-memory FTS5 index for the command. Use SQLite's own `unicode61` vocabulary and document frequencies, avoiding a second tokenizer for neighbor selection. For each source, select up to 32 terms that occur in at least two sources, ordered by increasing document frequency and then term. Rank matching other sources with BM25, breaking ties by source ID, and retain at most the requested number of outgoing candidates. Incoming selections may give a source more neighbors; canonical source pairs are deduplicated. Sources with no shared terms remain counted explicitly. See [SQLite's FTS5 vocabulary reference](https://www.sqlite.org/fts5.html#the_fts5vocab_virtual_table_module).

A lexical reason records the initiating source, target, query terms, position and score. Preserve any authored reasons for the same pair. The expanded policy and neighbor count participate in the selection hash, so a retained cohort rejects different selection settings. Use the same setting for execution and inspection; retirement still uses only its exact processing-plan hash. The comparison request itself remains unchanged: the model assesses both complete sources, not search snippets.

The FTS5 working database is limited to 128 MiB and is disposed at command completion; it creates no persistent index or query files. Existing pair/plan/cohort limits still apply to the combined result. This is a retrieval heuristic, not full semantic coverage: common vocabulary may create irrelevant candidates and paraphrases without shared terms may be missed. Record those limits when interpreting subsequent comparison, admission and grounding evidence.

## Reuse compatible comparisons

Under [Hivex #18](https://github.com/H1V35/hivex/issues/18), an explicit transition can reuse a retained comparison when the pair remains selected and its complete model request, source descriptors, claim identities and processing contract remain identical. The request includes both complete sources, their authority, original positions, claims, conditions, exceptions and local relationships. A shared document hash or coincident pair ID is insufficient.

Selection decides which pairs to assess; its global hash, retrieval ranks and neighbor count are not model inputs. Recompute the new selection and retain its complete reasons. A changed selection may reuse an unchanged pair, but never supplies evidence for a newly selected pair. Preserve the old graph and complete comparison export, including its selection, before replacing the working cohort.

Keep the original comparison, graph/source snapshot, prompt, native invocation, verdict and usage unchanged. Its association binds the current graph, source snapshot and selection, the original selection hash and the complete original receipt. The old export owns the original selection's full provenance. Repeated transitions retain the same original receipt and selection identity without nesting associations. Every inspection and admission revalidates current selection, complete pair inputs, source/claim bindings, literal citations and original/normalized output hashes.

Use the existing bounded comparison store and atomic cohort replacement. Validate the complete old archive against the old graph and the actual retained rows before mutation. Uncertain starts, unconfirmed interruptions, failed cleanup and active claims block replacement. A known preflight failure before any accepted turn remains a failure. Reuse never retries an adverse assessment or changes its verdict; changed/new pairs remain pending within the caller's execution budget. Removed pairs remain in the caller-owned archive. Existing capacity, output and explicit retention limits apply.

The shared [assessment transition binding](0006-source-fidelity-review.md#reuse-unchanged-source-evidence) also applies to comparisons. Repeating a completed transition requires its exact recorded archive; a separately initialized destination is rejected. Use the store that still owns the old cohort, not a new store initialized with the destination plan.

This implements pair reuse only; ingestion across changed plans, safe assessment retries and the whole update/resume command remain separate requirements of #18.

Pair comparisons also support the shared [explicit safe-recovery contract](0006-source-fidelity-review.md#explicit-recovery-of-safe-assessment-failures). Use the selected pair ID and unchanged neighbor settings. Retry only eligible invocation failures; completed adverse comparisons remain blockers. Full earlier receipts, unknown consumption and the three-attempt total budget survive reuse across compatible graphs.
