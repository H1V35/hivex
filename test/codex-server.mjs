#!/usr/bin/env bun
import { createInterface } from 'node:readline';
import { spawn } from 'node:child_process';
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { once } from 'node:events';

if (
  process.env.HIVEX_TEST_SCENARIO === 'secret-environment' &&
  ['GH_TOKEN', 'DATABASE_URL', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY'].some(
    (name) => process.env[name],
  )
)
  process.exit(19);

if (process.argv.includes('--version')) {
  console.log('codex-cli 0.153.2');
  process.exit(0);
}
if (process.env.HIVEX_TEST_SCENARIO === 'descendant') {
  const descendants = await Promise.all(
    [false, true].map(async (ignoreTerm) => {
      const program = [
        ignoreTerm ? "process.on('SIGTERM', () => {});" : '',
        "console.log('ready');",
        'setInterval(() => {}, 1000);',
      ].join('\n');
      const child = spawn(process.execPath, ['-e', program], {
        stdio: ['ignore', 'pipe', 'ignore'],
      });
      await once(child.stdout, 'data');
      child.stdout.destroy();
      child.unref();
      return child.pid;
    }),
  );
  writeFileSync(process.env.HIVEX_TEST_PID_PATH, JSON.stringify(descendants));
}

const options = Object.fromEntries(
  process.argv.flatMap((arg, index, args) => {
    if (arg !== '-c') return [];
    const setting = args[index + 1];
    const split = setting.indexOf('=');
    return [[setting.slice(0, split), setting.slice(split + 1)]];
  }),
);
const disabled = process.argv.flatMap((arg, index, args) =>
  arg === '--disable' ? [args[index + 1]] : [],
);
function payloadFromPrompt(prompt) {
  const marker = prompt.lastIndexOf('\n\n');
  if (marker < 0) return null;
  try {
    return JSON.parse(prompt.slice(marker + 2));
  } catch {
    return null;
  }
}

function responseForPrompt(prompt) {
  if (process.env.HIVEX_TEST_RESPONSES) {
    const responses = JSON.parse(readFileSync(process.env.HIVEX_TEST_RESPONSES, 'utf8'));
    const packet = payloadFromPrompt(prompt);
    if (responses.capturePackets)
      appendFileSync(process.env.HIVEX_TEST_RESPONSES + '.packets', JSON.stringify(packet) + '\n');
    if (packet?.operation === 'extract' && responses.fromVisibleRules) {
      const decisions = packet.documents
        .filter((doc) => packet.targets.includes(doc.id))
        .flatMap((doc) =>
          doc.lines
            .filter(
              ([number, line]) =>
                /^Rule \d+ requires/.test(line) &&
                (!packet.units ||
                  packet.units.some(
                    (unit) =>
                      unit.document === doc.id &&
                      unit.lineStart <= number &&
                      unit.lineEnd >= number,
                  )),
            )
            .map(([number, line]) => ({
              ...responses.extract.decisions[0],
              id: 'c' + number,
              document: doc.id,
              text: line.split('. ')[0] + '.',
              lineStart: number,
              lineEnd: number,
            })),
        );
      return { decisions, relationships: [], uncertainties: [] };
    }
    if (packet?.operation === 'extract' && responses.byDocument) {
      const targets = packet.targets ?? packet.documents.map((document) => document.id);
      const parts = targets.map(
        (id) => responses.byDocument[id] ?? { decisions: [], relationships: [] },
      );
      const relationships = parts
        .flatMap((part) => part.relationships)
        .filter(
          (relationship) =>
            !relationship.requiresEvidenceDocument ||
            packet.documents.some(
              (document) => document.id === relationship.requiresEvidenceDocument,
            ),
        )
        .map((relationship) => ({
          ...relationship,
          ...Object.fromEntries(
            ['from', 'to'].map((key) => {
              const endpoint = relationship[key];
              return [
                key,
                endpoint.startsWith('@existing:')
                  ? (packet.existing?.find((decision) => decision.document === endpoint.slice(10))
                      ?.id ?? endpoint)
                  : endpoint,
              ];
            }),
          ),
        }));
      return {
        decisions: parts.flatMap((part) => part.decisions),
        relationships,
        uncertainties: [],
      };
    }
    return responses[packet?.operation];
  }
  return null;
}
const emit = (frame) => process.stdout.write(JSON.stringify(frame) + '\n');
const handlers = {
  initialize: () => ({ userAgent: 'fixture' }),
  'account/read': () => ({ account: { type: 'chatgpt' } }),
  'model/list': () => ({
    data: [{ model: 'gpt-5.6-luna', supportedReasoningEfforts: [{ reasoningEffort: 'max' }] }],
    nextCursor: null,
  }),
  'config/read': () => ({
    origins: Object.fromEntries(
      ['model', 'model_provider', 'model_reasoning_effort', 'chatgpt_base_url'].map((key) => [
        key,
        { name: { type: 'sessionFlags' }, version: 'fixture-config-v1' },
      ]),
    ),
    config: {
      model: JSON.parse(options.model),
      model_reasoning_effort: JSON.parse(options.model_reasoning_effort),
      model_provider: 'openai',
      chatgpt_base_url:
        process.env.HIVEX_TEST_SCENARIO === 'redirected-provider'
          ? 'https://not-openai.invalid'
          : 'https://chatgpt.com',
      openai_base_url: null,
      model_providers: {},
      web_search: JSON.parse(options.web_search),
      project_doc_max_bytes: Number(options.project_doc_max_bytes),
      features: {
        ...Object.fromEntries(disabled.map((key) => [key, false])),
        skip_host_skill_discovery: process.argv.includes('skip_host_skill_discovery'),
      },
      memories: { use_memories: false, generate_memories: false },
      mcp_servers: ['configured-mcp', 'ignored-mcp', 'abort-isolation'].includes(
        process.env.HIVEX_TEST_SCENARIO,
      )
        ? {
            hivex_extra: {
              enabled:
                process.env.HIVEX_TEST_SCENARIO === 'ignored-mcp' ||
                options['mcp_servers.hivex_extra.enabled'] !== 'false',
              command: '/must-not-start',
            },
          }
        : {},
    },
  }),
  'thread/start': (params) => {
    if (
      process.env.HIVEX_TEST_SCENARIO === 'configured-mcp' &&
      options['mcp_servers.hivex_extra.enabled'] !== 'false'
    )
      throw new Error('configured MCP was not disabled before thread creation');
    if (params.model !== 'gpt-5.6-luna' || params.allowProviderModelFallback !== false)
      throw new Error('wrong model or fallback');
    const effort = process.env.HIVEX_TEST_SCENARIO === 'changed-effort' ? 'high' : 'max';
    return {
      thread: { id: 'thread1', ephemeral: true },
      model: params.model,
      modelProvider: 'openai',
      reasoningEffort: effort,
      cwd: process.cwd(),
      sandbox: { type: 'readOnly' },
      instructionSources:
        process.env.HIVEX_TEST_SCENARIO === 'instruction-source-metadata'
          ? ['/example/.codex/AGENTS.md']
          : [],
    };
  },
  'configRequirements/read': () => ({ requirements: null }),
  'mcpServerStatus/list': () => ({ data: [], nextCursor: null }),
  'turn/start': (params) => {
    if (process.env.HIVEX_TEST_CALLS_FILE)
      appendFileSync(process.env.HIVEX_TEST_CALLS_FILE, 'called\n');
    if (
      params.model !== 'gpt-5.6-luna' ||
      params.effort !== 'max' ||
      params.sandboxPolicy.networkAccess !== false
    )
      throw new Error('wrong turn controls');
    if (typeof params.input[0]?.text !== 'string' || !params.input[0].text.trim())
      throw new Error('prompt not supplied');
    const responseCandidate = responseForPrompt(params.input[0].text);
    if (process.env.HIVEX_TEST_SCENARIO === 'unconfirmed-interrupt') {
      setTimeout(() => process.exit(0), 20);
      return { turn: { id: 'turn1', status: 'inProgress' } };
    }
    if (['start-unconfirmed', 'update-uncertain'].includes(process.env.HIVEX_TEST_SCENARIO))
      return undefined;
    if (process.env.HIVEX_TEST_SCENARIO === 'oversized-frame')
      emit({ method: 'fixture/unknown', params: { text: 'x'.repeat(4_194_304) } });
    if (['timeout', 'timeout-unmeasured', 'cancel'].includes(process.env.HIVEX_TEST_SCENARIO)) {
      if (process.env.HIVEX_TEST_SCENARIO !== 'timeout-unmeasured')
        queueMicrotask(() =>
          emit({
            method: 'thread/tokenUsage/updated',
            params: {
              threadId: 'thread1',
              turnId: 'turn1',
              tokenUsage: {
                total: {
                  inputTokens: 100,
                  cachedInputTokens: 20,
                  outputTokens: 25,
                  reasoningOutputTokens: 25,
                  totalTokens: 125,
                },
              },
            },
          }),
        );
      if (process.env.HIVEX_TEST_SCENARIO === 'cancel')
        queueMicrotask(() => process.kill(process.ppid, 'SIGINT'));
      return { turn: { id: 'turn1', status: 'inProgress' } };
    }
    queueMicrotask(() => {
      emit({
        method: 'thread/tokenUsage/updated',
        params: {
          threadId: 'thread1',
          turnId: 'turn1',
          tokenUsage: {
            total: {
              inputTokens: 100,
              cachedInputTokens: 20,
              outputTokens: 50,
              reasoningOutputTokens: 30,
              totalTokens: 150,
            },
          },
        },
      });
      if (process.env.HIVEX_TEST_SCENARIO === 'usage-regression')
        emit({
          method: 'thread/tokenUsage/updated',
          params: {
            threadId: 'thread1',
            turnId: 'turn1',
            tokenUsage: {
              total: {
                inputTokens: 90,
                cachedInputTokens: 20,
                outputTokens: 50,
                reasoningOutputTokens: 30,
                totalTokens: 140,
              },
            },
          },
        });
      emit({
        method: 'item/completed',
        params: {
          threadId: 'thread1',
          turnId: 'turn1',
          item: {
            id: 'message1',
            type: 'agentMessage',
            text:
              process.env.HIVEX_TEST_SCENARIO === 'invalid-json'
                ? '{broken'
                : JSON.stringify(responseCandidate),
            phase: 'final_answer',
          },
        },
      });
      emit({
        method: 'turn/completed',
        params: { threadId: 'thread1', turn: { id: 'turn1', status: 'completed', error: null } },
      });
      if (process.env.HIVEX_TEST_SCENARIO === 'duplicate-terminal')
        emit({
          method: 'turn/completed',
          params: { threadId: 'thread1', turn: { id: 'turn1', status: 'completed' } },
        });
    });
    return { turn: { id: 'turn1', status: 'inProgress' } };
  },
  'turn/interrupt': () => {
    queueMicrotask(() =>
      emit({
        method: 'turn/completed',
        params: { threadId: 'thread1', turn: { id: 'turn1', status: 'interrupted', error: null } },
      }),
    );
    return {};
  },
};

async function observeModelRequest() {
  const calls = process.env.HIVEX_TEST_CALLS_PATH;
  if (!calls) return;
  appendFileSync(calls, 'called\n');
  const hold = process.env.HIVEX_TEST_HOLD_PATH;
  while (hold && existsSync(hold)) {
    if (readFileSync(hold, 'utf8') === 'after-first' && readFileSync(calls, 'utf8') === 'called\n')
      return;
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}

for await (const line of createInterface({ input: process.stdin })) {
  const frame = JSON.parse(line);
  if (frame.id === undefined) continue;
  try {
    const handler = handlers[frame.method];
    if (!handler) throw new Error('unsupported request');
    if (frame.method === 'turn/start') await observeModelRequest();
    const result = handler(frame.params);
    if (result !== undefined) emit({ id: frame.id, result });
  } catch {
    emit({ id: frame.id, error: { code: -32602, message: 'invalid fixture request' } });
  }
}
if (process.env.HIVEX_TEST_SCENARIO === 'abort-isolation') process.kill(process.ppid, 'SIGINT');
