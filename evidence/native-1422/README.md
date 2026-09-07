# Native extraction evidence — 2026-09-07

Two real invocations of the installed `codex-cli 0.153.2`, authenticated through the existing
ChatGPT subscription, processed the same synthetic 42-byte source with Luna/max. Both returned one
constraint with its exact quote on line 3. These are transport/source-fidelity canaries, not a corpus
benchmark or an admitted graph.

| Observation                        | Input | Cached input | Output | Reasoning within output | Total | Wall time    |
| ---------------------------------- | ----: | -----------: | -----: | ----------------------: | ----: | ------------ |
| Initial path                       | 6,234 |            0 |    299 |                     228 | 6,533 | Not captured |
| Endpoint/MCP/cancellation controls | 6,238 |            0 |    300 |                     230 | 6,538 | 23,437 ms    |

The second report also records native thread/turn identity and confirmed cleanup. Its source hash,
configuration hash, prompt/schema hashes and candidate are retained verbatim as JSON. The input
`source-config.json.txt` preserves its original bytes without a final newline; its hash can therefore be
checked against the reports. The original synthetic Git commit is local experiment provenance,
not a commit in Hivex's history.

Both invocations ran during implementation. The adjacent code-hash manifests identify their exact
runner files; those pre-commit source versions were not archived and cannot be reconstructed from
hashes alone. Subsequent changes and the final reviewed commit must be validated separately.
Do not relabel these observations as execution on a later SHA. No corpus source, Opus graph,
extraction cache, user configuration or private agent memory was changed by either invocation.

The [App Server protocol](https://learn.chatgpt.com/docs/app-server) documents structured turn
output and interruption notifications. The generated protocol from the installed 0.153.2 binary
was the version-specific reference: it lacks the read-root access options present in newer web
documentation. These reports certify neither filesystem read isolation nor semantic completeness.
