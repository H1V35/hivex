# Native extraction evidence for Compi #1422

These two synthetic observations ran the executable code at
[`e84b265afafcf87e35f5975373782ebc1f1ac6a7`](https://github.com/H1V35/compi/commit/e84b265afafcf87e35f5975373782ebc1f1ac6a7),
with Codex CLI 0.153.2, the existing ChatGPT subscription and gpt-5.6-luna/max.
`manifest.json` identifies the runner and hashes its source, test peer and configuration.
The following evidence commit does not change that executable code.

The input is the complete 43-byte `source.md`, committed as `policy.md` in a temporary
Git repository together with the exact bytes of `source-config.json.txt` as `hivex.json`.
The local input commit is recorded in each response; the source and configuration
hashes allow the input to be recreated without that local Git object.

Run from the recorded runner checkout, after installing its pinned dependencies:

```sh
bun hivex/src/cli.ts extract policy.md --root <temporary-repository> --codex <codex-0.153.2> --attempts 1 --deadline-ms 180000
bun hivex/src/cli.ts extract policy.md --root <temporary-repository> --codex <codex-0.153.2> --attempts 1 --deadline-ms 5000
```

`completed.json` returned one candidate with literal evidence at source line 3,
confirmed cleanup and exit 0. It reported 6,236 input tokens (4,864 cached), 320 output
tokens (250 reasoning) and 6,556 total tokens in 10,875 ms. Subset counters are not
added to input/output again. This is reported usage, not a billing calculation.

`interrupted.json` reached an accepted turn, exceeded its 5,000 ms execution deadline,
confirmed native interruption and process cleanup, returned no candidate and exited 1.
Its total observed duration was 13,588 ms, including admission and cleanup. No usage
notification was available; the result correctly retains `usage: null`, not zero.
Scheduling and model latency vary, so a future 5-second run can finish before the timeout.

Both responses retain the admitted model, effort, account type, configured endpoint,
configuration origins, policy/prompt/schema hashes and process/turn identity. These
observations validate a small real transport path and cancellation. They do not establish
extraction recall, temporal reasoning, graph correctness, cost savings or implementation
grounding. Neither response is accepted authority. Full corpus reconstruction remains
pending the source recovery and admission work in #1544 and #1422–#1427.
