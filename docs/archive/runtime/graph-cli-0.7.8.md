---
title: "Optional graph and model-assisted CLI reference"
status: historical
created_at: 2026-10-01
updated_at: 2026-10-02
archived_at: 2026-10-01
tags: [history, cli, runtime]
source_path: docs/reference/graph-cli.md
source_revision: 180ea2750e0da9253494b02e7bbbb25938720758
---

# Optional graph and model-assisted CLI reference

These commands are implemented by the current CLI. They are optional derived-knowledge capabilities, not prerequisites for source reading, implementation or integration. The [Markdown-first decision](../../adr/0014-reliable-markdown-and-explicit-relationships.md) owns that workflow and does not declare an unfinished graph successful.

Use a model only for a concrete useful question and an explicit work budget. Preserve existing stores, snapshots, attempts, receipts and consumption. This reference describes the current runtime; it does not promise that graph maintenance converges or that model output approves a change.

## Update and repair knowledge

```sh
npx hivex update --max-calls 2
```

An update splits large Markdown into line-preserving units of at most 8 KiB, preferring Markdown boundaries. Each round selects at most four units and 16 KiB of target text, then performs one additional check. Optional neighbor discovery is limited to 18 decisions and 8 KiB of evidence. Current endpoints of affected relationships and their evidence are required context, so those discovery limits do not silently remove them. If the complete packet exceeds `--max-context-bytes`, Hivex reports a context limit before starting the extraction. Original document IDs and line numbers survive splitting. Earlier rounds remain queryable while `pendingUnits` and `pendingDocuments` show unfinished coverage. Sources up to 32 MiB can be split, within a 64 MiB loaded-corpus limit; narrow the selected paths if that limit is reached. A line too large to fit is explicitly reported as unread, never silently cut.

Sources declared by `archive` are not part of an ordinary update. A focused `ask` or `review` adds only its selected bounded units to the same resumable work; a known dependency can add the ranges needed to verify it. The work budget, graph knowledge and cache survive resumption. Changing an unrelated historical document does not start a fresh consultation budget. Current documents take priority when the loaded-corpus limit is reached; narrow the configured scope to access sources outside the reported coverage.

New ordinary `update` work can group pending units whose exact ranges and text fingerprints match previously checked knowledge into one check without extraction. The group must contain its complete checked decisions, relationship endpoints and evidence, without active in-scope uncertainty. Missing fingerprints, changed lines, dependencies outside the group or an oversized candidate use ordinary extraction. The four-unit/16 KiB target bound still applies to extraction; a reused group is bounded by the complete materialized check packet and remaining input-byte budget. This avoids supplying the same complete document for several small reused checks. `ask`, `review`, explicit repairs and existing unfinished work retain their execution path.

The reused proposal stays staged until the normal check runs against complete current target documents and affected context: unchanged text cannot certify meaning, general conditions or dependencies. Hivex checks the final request size before choosing reuse; if it cannot fit context or remaining input bytes, extraction proceeds ordinarily. A later call limit or interrupted check retains the proposal and accepted graph. Findings remain visible and can require repair; coverage is not semantic approval.

Each extraction and check is checkpointed. Resuming continues the same work and never repeats its completed rounds. Successful structured model results are cached in the same store by the complete request, schema and model profile; an identical request can be reused without a call, even when reconstructing earlier knowledge. Changed context invalidates that cache entry. Context discovery considers authored links, lexical matches and recent decisions; `relationshipCoverage` states that this is bounded, not exhaustive. Cache hits are reported separately from calls and tokens; this is an optimization, not documentary authority.

To correct derived knowledge against unchanged Markdown, use:

```sh
npx hivex update --repair docs/cache.md --reason "The source specifies seven days, not indefinite retention."
```

Use `--repair-range docs/cache.md:20-35` instead of `--repair docs/cache.md` to revisit only the decisions overlapping those one-based lines and their affected relationships. The range expands to include each affected decision’s complete source passage, including overlapping passages; other knowledge in the same ingestion block stays intact. Repeat the option for multiple ranges. Explicit range repairs pack as many complete ranges as fit in the 16 KiB target-text budget, without the ordinary update's four-unit cap. Context and work budgets still apply before every call. A complete repair range must fit that same 16 KiB round limit; oversized ranges fail before a model call. The source must already be fully ingested at its current version; finish a normal update first if it changed. Whole-document repair remains available when needed. Retained work keeps its original planned ranges and budget, and repeating a completed range request reuses it even when an earlier CLI processed a broader block.

Repair replaces the affected unit's interpretations and revisits its relationships without editing Markdown. Its reason guides comparison with the source; it does not create new authority. Repeating the same completed repair reuses its work. Add `--source <document>` to an update or repair when a known supporting authority must be supplied explicitly, for example after a connection was lost. Its complete Markdown and current decisions become context; the option does not add that document to the ingestion targets. The same context budget applies, and the selection and its versions identify the work. A genuine unresolved documentary conflict still needs a decision by the responsible person.

Repair packets include the current interpretations of the target passages, not just their identifiers in previous relationships. These interpretations are context, not authority. The replacement must retain correct knowledge and supported dependencies, rewiring changed endpoints; correcting a model interpretation is not a source-authored policy supersession. A repair reason may cover several batches, but each round applies only its own target ranges. Unchanged supplied decisions can be referenced by their exact IDs; invalid references remain rejected. This context counts toward the existing limit rather than being silently omitted.

New materialized checks separate the proposed knowledge from the extraction-only list of interpretations being replaced. Prior decisions remain comparison evidence and explicitly indicate whether they still exist in the candidate. Checks also include retained relationships from earlier rounds of the same work between supplied endpoints with current, fully supplied citations, so accepted rounds are not mistaken for missing work. Unrelated graph edges are not copied into every check. Removed interpretations must not be mistaken for current defects; actual missing replacements or faulty retained endpoints still require findings. The relationship-loss and invalid-target guards remain unchanged, and a changed check request cannot reuse an old check receipt.

Correct a real retained staged-candidate error with the same update arguments plus `--retry-failed --correct candidate-correction.json`. No re-extraction; a fresh check is required. Bind the JSON to `workId`, `checkInputHash` from `candidateResolutionContext`, a bounded source-backed `reason`, current versioned `evidence`, and `decisions` (`[]` for relationship-only changes).

- `decisions`: complete replacements with existing local IDs and original documents. Ordinary decisions stay within pending target ranges, allowing complete contiguous-unit coverage; restored decisions keep their own validated range. No arbitrary additions.
- Optional `relationships`: complete additions/replacements by local ID. Touch a pending decision; cite current supplied evidence. Other endpoints must be current supplied retained decisions, referenced by canonical ID.
- Optional `removeDecisions`/`removeRelationships`: local pending IDs only, including duplicates to coalesce. Never remove accepted entries.
- Optional `retainedRelationships`: `{ "previousId": "<protected-id>", "replacement": <extraction relationship> }`. Restore an omitted protected dependency between current supplied endpoints outside this extraction; one endpoint must belong to an earlier accepted round of this work. Require unique prior/new IDs, compatible type/direction/authority and complete current supplied evidence for all prior/replacement relationship citations and endpoint ranges. Current prior endpoints retain their IDs.
- Optional `retainedDecisions`: the same record shape with an extraction decision replacing an obsolete protected endpoint. Require the same document and a current source range within an earlier processed unit attributable to this work/original plan (the passage may have shifted from its obsolete coordinates), a new non-colliding local ID, no duplicate current interpretation and a corrected relationship using it. Distinct meanings may share a passage; current accepted decisions outside original targets stay unchanged.

Verify the original receipt before adding missing protected comparison passages, complete contextualized earlier-edge citations or validated restoration ranges from current Markdown to the new packet, within documents/versions already identified. Preserve the old packet exactly; additions were not necessarily supplied to its check. Declared restoration ranges permit proposals, not semantic approval. They never expand replacement targets, alter the plan, overwrite accepted entries or retire earlier warnings. Context/work limits apply before calls.

Reject unknown fields/IDs, repeated IDs, contradictory edits, stale checks/evidence/graph, incomplete coverage, dangling references and unchanged materialized proposals; a different review reason alone cannot authorize a check. New-work/explicitly corrected checks constrain target/mapping IDs in schema and local validation without rewriting historical outputs or automatically retrying invalid ones.

Preserve correction, previous pending candidate, attempts/results/receipts and consumption. Reviewed input remains non-authoritative and is not native output. The fresh check reassesses all protected removals, including those previously preserved; findings/loss guards block missing meaning. Use the original total budget: limits retain the pending correction; raised authorized totals resume only the check. Exact replay stays idempotent after completion. Never combine `--correct` with false-finding `--resolve`. No automatic semantic retry or budget reset. A prepared correction may be amended before its fresh check after verifying the original retained receipt through the saved proposals. Preserve every prior proposal and the original source/version scope, target plan and cumulative budget; refine citations or context only within current supplied sources and validated ranges. Each amendment must change the materialized proposal and remains pending until the fresh check.

New work checks the materialized decisions and relationships, including local validation discards. If it would lose relationships whose source authorities remain available, including authorities with changed hashes, the previous graph remains available while the same check justifies their removal or identifies supported replacements. An unjustified loss returns `failed` with `RELATIONSHIP_LOSS`, preserving the previous graph and the attempted result. Current findings that affect a replacement or its endpoints still block it; inherited uncertainty remains visible but does not by itself veto an explicitly justified replacement. Inspect the evidence before a different repair; there is no automatic semantic retry. After correcting a local admission defect, `update` with the same arguments plus `--retry-failed --max-calls 0` can reassess that retained check without model calls. It requires the exact candidate/request and current evidence; graph divergence or a changed request rejects reuse. `work.retainedCheckAssessment` reports the local outcome while the original attempt and consumption remain intact. This does not rerun an adverse model check. Older unfinished work retains its original model request and budgets. When a repair explicitly references a supplied, current decision, Hivex retains that decision and shows its definition to the check. If an older release discarded that endpoint, a resumed repair can reuse the extraction and check the corrected candidate once within the same total allowance. Hivex first verifies the retained request against the older candidate; it does not repeat a check of an unchanged candidate. With `--max-calls 0`, a changed candidate remains pending without a model call.

When normal ingestion changes a source version, new rounds retain the old decisions and relationships needed by unprocessed units. This includes a relationship spanning two documents: an old endpoint survives even if its own document's units finish first. Old citations keep their original versions and are not presented as current evidence. A source remains pending while that transition still retains stale knowledge; completing the remaining dependency can mark it current without re-extracting its already completed units. Bounded consultations use the complete source plan for this protection, including units outside that consultation's call allowance.

Once the relationship's dependent units have been processed, its removal or replacement must pass the existing check with current supplied Markdown and current replacement endpoints. Earlier current replacements are required context even when lexical discovery would omit them. An unjustified loss keeps the prior graph and fails explicitly; budget and context limits preserve the transition for resumption. Existing retained check packets keep their original identity. This prevents new silent losses; a relationship already lost by an older run still needs a source-based repair.

Warning prevention uses that same normal check to review new uncertainties and expired closures. It may dismiss descriptive observations or revalidate expired closures that are untargeted and non-validation only when complete in-scope documents and the prior closure's complete evidence are in context. Missing evidence leaves the warning pending; the update may still finish with limitations. Findings or local validation failures prevent warning closures. Matching text alone is insufficient. No extra call, retry or automatic repair is added.

For a synthetic example, a closure about `docs/policy.md` that also depends on `docs/retention.md` can be revalidated only when both current documents are supplied; if `docs/retention.md` is omitted, it remains pending even when the policy paragraph is unchanged.

For new update work, the response includes `warningChanges` with `new`, `reopened` and `resolved` lists. Each list contains entries with `id`, `message` and `state`; `state` is `active` or `resolved`. Review `warningChanges.new` entries with `state: "active"` and `warningChanges.reopened` against the current sources. `warningChanges` compares recorded closures and warning IDs from when the work began; reopened includes earlier closures whose evidence is now stale. The baseline records closure disposition independently of source coverage freshness, so a current-source closure can exist before that source is ingested. `new` contains IDs absent at the start, and `resolved` contains IDs without a closure at the start that end with one. Resumption keeps the baseline; this reports that work's changes rather than cleaning the complete backlog.

`warningSummary` separates extraction `limitations`, check `findings`, local `validation` errors and unclassified legacy `unknown` warnings; `sources` counts discovery or ingestion limits. The original messages and scopes remain available. A limitation alone does not make a completed update `partial`. `coverage` reports pending source ingestion separately; current coverage is not proof that every interpretation is correct. The graph retrieves meaningful decisions, dependencies and exceptions; source reading supplies details without requiring the graph to reproduce every sentence.

The default explicit-update work budget is two invocation attempts and 131,072 input bytes. `--max-calls` and `--max-input-bytes` set totals for the complete work, including extraction, check and resumption. A zero-call update reports pending documents without invoking the model. An exhausted work item retains its progress; repeating the command does not reset its counter. An authorized larger total can complete the remaining stage without repeating completed extraction.

A failed or unfinished invocation is not retried automatically by increasing the budget. Inspect its reported outcome and usage first. `--retry-failed` can explicitly resume a safely ended failure within the same work budget; uncertain invocations remain blocked. A completed adverse check is not an invocation failure; this flag does not repeat it for an unchanged candidate. Uncertain or pending knowledge does not become a blanket pass. The project-local `.hivex/knowledge.sqlite` stores working knowledge and execution accounting; no source Markdown is rewritten. Preserve it when work evidence is needed. Storage is bounded at 64 MiB; do not delete an active store to hide unfinished calls or reset a work budget.

For a false semantic finding that blocks a retained repair candidate, inspect `pendingCandidateWarnings` separately from the accepted graph's warnings. Reuse the same update arguments with `--retry-failed --max-calls 0 --resolve candidate-resolution.json`. The file binds the review to the work and exact check using `candidateResolutionContext`:

```json
{
  "workId": "<work-id>",
  "checkInputHash": "<retained-check-input-hash>",
  "resolutions": [
    {
      "id": "<pending-candidate-warning-id>",
      "reason": "<source-backed review disposition>",
      "evidence": [
        { "document": "docs/example.md", "lineStart": 10, "lineEnd": 14, "version": "<current-source-hash>" }
      ]
    }
  ]
}
```

This local operation accepts semantic findings from that exact check on current canonical candidate decisions. Materialized checks also permit findings directed at the batch, a current supplied document or relationship, or a protected previous relationship with an explicit replacement mapping below. A document disposition must cite that document; a batch disposition must cite every pending target range. It requires the exact completed check and unchanged candidate, graph and sources; it never starts a model call. Unknown targets and structural validation failures remain blocking. The original findings, native uncertainty, failed attempt, candidate, receipts, total budget and consumption remain intact. The work records the reviewed input and prior pending candidate, and the admitted graph retains the warning dispositions. Replaying the exact review is idempotent; a different disposition is rejected. A closure is the reviewer's source-backed judgement, not a correctness proof. Do not use it for a real contradiction or missing dependency.

When a materialized check falsely reports a missing previous relationship or omits its replacement mapping, inspect `pendingRelationshipReview` in the failed update's JSON. Repeating the original update arguments without a retry or changed budget reports the retained failure and this context without model calls. The context is available only when the retained check still matches the reconstructed candidate: it supplies canonical previous/candidate relationships, their endpoint definitions, supplied source versions and line numbers, and the untouched native check. Use those IDs rather than local extraction IDs. Keep consumer evidence private.

Add optional `relationshipChanges` to the same review file:

```json
{
  "workId": "<retained-work-id>",
  "checkInputHash": "<exact-check-input-hash>",
  "resolutions": [],
  "relationshipChanges": [
    {
      "previousId": "<protected-previous-relationship-id>",
      "replacements": ["<present-canonical-candidate-relationship-id>"],
      "reason": "Explain how the current endpoints, direction, scope, conditions and citations preserve the previous meaning.",
      "evidence": [
        {"document": "docs/example.md", "lineStart": 10, "lineEnd": 14, "version": "<current-source-hash>"}
      ]
    }
  ]
}
```

`resolutions` may be empty only when supplying mappings alone; a false finding still needs its warning ID, reason and evidence. Each mapping must name a unique protected relationship actually supplied in this check and one or more unique replacements actually present in its materialized or retained relationship context. Replacements preserve the relationship type and endpoint direction/authority; a retained endpoint keeps its ID. Each review citation must be current, versioned and within supplied lines. Evidence must cover the previous relationship and endpoint ranges at current source versions, and the replacement relationship citations and current endpoint ranges. Cite a complete range in one citation; add citations for each supporting range. A relationship finding's closure must cover those same ranges. Review entries allow at most 128 mappings, 128 replacements per mapping and 32 citations, with a non-empty reason bounded to 2,048 UTF-16 code units. Unknown fields, stale identity/evidence, absent replacements, empty replacement arrays in `relationshipChanges` and duplicate IDs are rejected before any model invocation. Changes of type or endpoint authority require source-backed correction and a fresh check, not this false-finding path. For a protected relationship absent from the candidate because its prior meaning is unsupported or is coalesced within a current decision, use separate optional `relationshipRemovals` records with `id`, `reason` and `evidence`. Cite complete prior relationship and endpoint ranges at current supplied source versions, and the current decision passage for coalesced meaning. When present, the field needs 1–128 unique removal IDs, actually absent and not also listed in `relationshipChanges`, with the existing reason/evidence limits. A dependency that remains a distinct relationship needs a present replacement; removal is limited to unsupported prior meaning or meaning fully coalesced within a current decision. An associated false finding requires its own resolution covering those same ranges. Findings affecting the batch, relationship or endpoints and structural failures remain blocking; unrelated warnings retain their scope and visibility. The reviewer judges whether removal is justified; structural validation cannot certify it.

Every protected relationship still needs a valid native disposition, validated reviewed mapping or explicit reviewed removal. Partial coverage, remaining findings affecting the batch, relationship or endpoints, and actual missing relationships keep admission blocked. The CLI checks identity, existence and evidence coverage; the independent reviewer remains responsible for semantic equivalence. It does not infer equivalence from text or promote native quality. Run the original update arguments with `--retry-failed --max-calls 0 --resolve candidate-resolution.json` to admit only the already extracted candidate. This command preserves the original total limits despite `--max-calls 0`, which enforces a local-only operation. `warnings --resolve` alone cannot supply replacement mappings.

Use `npx hivex warnings` to inspect active warnings and their IDs. After checking the current sources, `npx hivex warnings --resolve resolutions.json` records an explicit resolution without a model call. The file is an array of `{ "id": "<warning-id>", "reason": "<why the warning is resolved>", "evidence": [{ "document": "docs/example.md", "lineStart": 10, "lineEnd": 14, "version": "<current-source-hash>" }] }`. Unknown or repeated IDs and stale or invalid citations reject the whole operation. A resolution requires a reason and current evidence; it is a reviewed explanation, not automatic proof that the knowledge is correct. Findings about implementation or relationships require an explicit repair or correction, and a knowledge check does not turn a failure into a resolution. Keep real contradictions and missing dependencies open for repair.

Resolved warnings remain in the graph with their original message, reason and versioned evidence. When a warning is closed again, `previousResolutions` retains every earlier closure. Use `warnings --all` to inspect active and resolved history and `snapshot export` to share it in Git alongside the Markdown. Status counts active warnings and current resolutions separately. Retrieval shows active warnings relevant to the query. A changed or unavailable resolution source reactivates its warning; matching text does not silently preserve a closure. Handle new or reactivated warnings in the same incremental work when the complete context is available; otherwise leave them pending. Do not reopen settled questions or scan the whole corpus to chase zero warnings. Resolving a warning does not promote decision or relationship quality, rewrite sources, or change work history and budgets.

`recover` inspects abandoned work without invoking the model or killing processes. Live owners or native processes remain protected. If local processes ended but remote delivery is uncertain, `recover --acknowledge-uncertain` records an explicit acknowledgement; original reports and unknown usage remain visible. Recovery itself never retries: a subsequent `--retry-failed` uses the retained work budget. Do not treat acknowledgement as proof that the earlier remote turn completed.

A rejected update can be explicitly closed as superseded when a completed update has replaced its pending objectives. Use `recover --supersede supersession.json`; this makes no model call and does not admit the old candidate or turn its failure into success. The review file identifies the original work and last check (`checkInputHash` is reported in `candidateResolutionContext`), the completed replacement, a bounded reason, and current versioned source evidence for every affected document:

```json
{
  "workId": "<failed-update-id>",
  "checkInputHash": "<last-check-input-hash>",
  "replacementWorkId": "<completed-update-id>",
  "reason": "<why the admitted replacement covers the remaining objectives>",
  "evidence": [
    { "document": "docs/example.md", "lineStart": 10, "lineEnd": 14, "version": "<current-source-hash>" }
  ]
}
```

Supersession requires a failed update with a staged candidate and a completed last check. Every invocation in both works must have completed with confirmed acceptance and cleanup; a recovery acknowledgement cannot substitute for this evidence. The replacement must be a completed update with no pending stage. Its uniquely attributable current coverage units must occur in its original plan and cover every remaining or staged target range at the original, still-current source versions. If a later overlapping repair removed that attribution, the work key is ambiguous, or a range/version is missing, the command refuses rather than inferring completion from overall graph coverage. The source-backed review remains responsible for semantic adequacy; supersession does not resolve graph warnings or certify the graph.

The command acquires the update lease and records the disposition atomically. The original failed status, candidate, attempts, results and budgets stay intact with a `supersession` record identifying the replacement and reviewed coverage. Repeating the exact review file is idempotent; a different disposition is rejected. A superseded work cannot resume, retry, change profile, correct or resolve its candidate. It no longer blocks snapshot import or relocation, while running, uncertain and other unfinished work still do. Both the superseded record and its referenced completed replacement are protected from pruning.

`prune` releases space occupied by old completed work and cached results, retaining the graph and all unfinished work, attempts and budgets. In addition to completed replacements protected by supersession, it keeps the newest eight other completed works and 64 cached results by default; `--keep-completed` and `--keep-caches` change those counts. Pruned answers can require a new model call when requested again. Export evidence before pruning if historical reports are needed; pruning is explicit, never an automatic budget reset.

Native operations accept `--integration codex`, `--model`, `--effort`, `--model-provider openai`, `--codex` and `--deadline-ms`; the default deadline is 30 minutes. Consultation context defaults to 65,536 bytes and can be bounded with `--max-context-bytes`. Limits are reported, not met by silently cutting a rule or pretending omitted evidence was reviewed. Input-byte and call budgets limit work; reported token usage is actual consumption, including known failed attempts.

The explicit `--max-context-bytes` range is 1,024–1,073,741,824 bytes, matching the configurable total-input ceiling; its default remains 65,536 bytes. A materialized check can be larger than its extraction packet. If that check pauses for context, resume the same task with a sufficient explicit context limit while its sources, graph and execution profile still match: Hivex retains the extraction and runs the pending check. Increasing context does not raise the retained call or input-byte budget, authorize a retry of an adverse check, or guarantee that the selected model accepts that packet size. The existing work budget still applies before any new invocation.

### Visible progress

`update` (including repairs), `ask` and `review` accept `--progress auto|always|never`.
The default `auto` writes plain, newline-delimited checkpoints to stderr only when
stderr is a terminal. Use `always` for agents, redirected logs and other non-TTY
callers; `never` suppresses progress, but preserves error diagnostics. Stdout stays
the final JSON result. Progress is a human-readable aid, not a stable event protocol.

Checkpoints report work start/resumption, extraction, checking and admission at
the existing transitions. Completed/pending units refer to the retained work plan;
extracted units remain pending until their check and admission finish. Admission
can preserve findings and uncertainty: it does not certify every interpretation.
Cache hits and retained results are distinguished from new reserved model calls.
Resumption keeps the work's existing attempts and total budget.

During model execution a wait line appears every 15 seconds with elapsed time,
without claiming knowledge of internal progress, percentages, ETA or unknown usage.
Elapsed time starts with this CLI invocation; each model wait also reports its own
duration. Limits, failures and handled interruptions retain their actual work state.
An abruptly killed process cannot emit a final checkpoint; inspect it with `recover`.
Progress contains no source text, prompts, model responses or internal reasoning.

Keep stderr visible while saving the JSON, or duplicate stderr in Bash/Zsh:

```sh
npx hivex update --progress always > result.json
npx hivex update --progress always > result.json 2> >(tee progress.log >&2)
```

Redirecting both streams to files hides progress from the terminal. Do not merge
stderr into stdout when a caller expects a single parseable JSON result.

### Execution profiles

Integration and model selection are independent. `--integration` selects the connection; `--model` and `--effort` select a profile supported by that connection. Codex currently admits OpenAI through ChatGPT, so other `--model-provider` values and uninstalled integrations fail explicitly. Provider-specific options and authentication belong to their integration. A future integration can use different model options without requiring a reasoning level named `max`.

```sh
npx hivex ask "Which rules apply?" --model <model-listed-by-codex> --effort <supported-effort>
```

Cache identity includes the integration/profile. The default is now `gpt-6-luna` with effort `max`; new invocations never need the former `gpt-5.6-luna`/max default. Unfinished work under that former default moves to GPT-6 Luna when resumed, keeping its ID, progress, attempts and consumed budget. The recorded profile replacement distinguishes historical results from new invocations. Running or uncertain invocations still require the normal inspection/recovery process, and failed work still requires explicit retry. The first subsequent `update`, `ask` or `review` removes the old unversioned model cache; its hash-only keys cannot distinguish models, so this also clears any custom-profile entries from that generation. New versioned cache entries survive later operations. Completed work and its historical answers remain intact, but are not reused as GPT-6 Luna results. Read-only commands do not perform this cleanup. Existing graph knowledge is not reingested merely because the default changes. Other profile changes report `EXECUTION_PROFILE_CHANGED` unless explicitly resumed as below. Each new work records its requested profile, and execution receipts record an observed effective profile when known.

Use the original `update`, `ask` or `review` arguments with `--resume-with-profile <work-id>` and the supported model/provider/options you choose. The work ID, operation, source versions and arguments must match; the option cannot change scope or relabel completed work. For example, `hivex update --resume-with-profile <work-id> --model <supported-model> --effort <supported-effort> --max-calls 0` records the choice without invocation or lowering the stored total limits. A later resumption keeps those limits unless you explicitly set new totals. Repeating the same choice is idempotent. Source edits, unknown IDs, ambiguous ownership, running or unresolved uncertain invocations are rejected without discarding data.

`profileReplacement` records the previous profile/request identity and current choice; `previousReplacements` keeps earlier transitions. Existing coverage follows the same work atomically, while decision/relationship IDs, citations, quality, raw native results and receipts keep their provenance. Selecting a profile alone does not re-extract accepted knowledge or convert an old answer into a new-model answer. Historical input verification permits correction or evidenced local disposition without requiring the old runtime. It does not authorize repeating an unchanged adverse check; changing the candidate requires the selected profile's normal check within the same budget. Retained failures remain inspectable after context or budget limits. See [the continuity decision](../../adr/0013-domain-modules-and-execution-integrations.md#explicit-profile-continuity-148).

## Share knowledge through Git

```sh
npx hivex snapshot export
npx hivex snapshot import
```

`snapshot export` writes `.hivex/graph.json` atomically as stable, readable JSON. Commit that file alongside the Markdown it describes to share decisions, relationships, source versions, evidence, available provenance and coverage. It exports the graph, not work records, process identities, budgets or cached model answers. Snapshot operations make zero model calls.

A fresh clone can use `search`, `neighbors` and `status` directly from the shared snapshot without creating a local database. Its first update reuses matching ingestion units and starts local work accounting. If a local graph already exists, it takes precedence: use `snapshot import` to adopt a new shared version. Import refuses while local work is unfinished and never resets attempts or budgets. Complete or recover that work through its normal lifecycle first.

When Markdown moves, explicitly relocate its knowledge before the next update:

```sh
npx hivex snapshot relocate docs/old-guide.md docs/guidelines/guide.md
```

The old source must no longer be selected, and the destination must be selected current Markdown. Relocation preserves IDs, relationships, source versions and uncertainty, leaving existing work, cached answers and budgets intact. It makes zero model calls and refuses unfinished local work. An identical move to a destination without prior knowledge reuses ingestion coverage when all retained source evidence has matching, known versions. If content changed, source versions are mixed or missing, or the destination already had knowledge, its coverage becomes pending so the usual update/check can validate the result. Mismatched evidence remains stale until then. Keep the relocation report with the change and export the final graph; do not use relocation to hide unrelated missing evidence.

The snapshot response identifies current, stale and unavailable source versions, pending units and warnings. A changed or absent source is not silently current; matching sources remain reusable. Partial and uncertain knowledge can be shared with those states retained. Freshness is not proof that a model interpretation is correct: the cited Markdown remains authority.

Keep only the shared graph under version control, for example:

```gitignore
/.hivex/*
!/.hivex/graph.json
```

Read-only queries do not rewrite the snapshot. Export intentionally when reusable knowledge changes, not on every consultation. Invalid snapshots or symbolic-link paths fail without replacing local knowledge. Existing local stores continue to work without a shared file.

## Support an implementation review

From the Git project root, supply the task and the base revision. Hivex captures the working change, including untracked files, and provides findings tied to code and Markdown versions:

```sh
npx hivex review "Change cache behavior" --base main --max-calls 3 > /tmp/hivex-review.json
npx hivex review --check /tmp/hivex-review.json
```

Use the same task and base to resume or reuse retained work. Update, knowledge check and review share one budget, including expansion of a partial report. Context is bounded; large changes must be narrowed or split into coherent reviews. Larger existing text files contribute diff excerpts with original line numbers and explicit omissions; their full-file versions still detect later changes. The principal reviewer verifies conflicts and exceptions and resolves supported contradictions before closing the change. A `ready` result means assistance is available, never that the implementation is approved. A saved report can be checked without a model; changed code or documents make it stale. Keep reports outside the project or in an ignored path so they do not become part of the change.
