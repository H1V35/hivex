#!/usr/bin/env bun
import { createInterface } from 'node:readline';
import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';

if (process.argv.includes('--version')) {
  console.log('codex-cli 0.153.2');
  process.exit(0);
}
if (process.env.HIVEX_TEST_SCENARIO === 'descendant') {
  const descendant = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], {
    stdio: 'ignore',
  });
  descendant.unref();
  writeFileSync(process.env.HIVEX_TEST_PID_PATH, String(descendant.pid));
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
const candidate = {
  claims: [
    {
      id: 'c1',
      text: 'A cache must never be treated as authority.',
      kind: 'constraint',
      conditions: [],
      exceptions: [],
      evidence: [{ quote: 'Never treat a cache as authority.', lineStart: 3, lineEnd: 3 }],
    },
  ],
  relations: [],
};
if (process.env.HIVEX_TEST_SCENARIO === 'invented-evidence')
  candidate.claims[0].evidence[0].quote = 'An absent source statement.';
const emit = (frame) => process.stdout.write(JSON.stringify(frame) + '\n');
const handlers = {
  initialize: () => ({ userAgent: 'fixture' }),
  'account/read': () => ({ account: { type: 'chatgpt' } }),
  'model/list': () => ({
    data: [{ model: 'gpt-5.6-luna', supportedReasoningEfforts: [{ reasoningEffort: 'max' }] }],
    nextCursor: null,
  }),
  'config/read': () => ({
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
      mcp_servers:
        process.env.HIVEX_TEST_SCENARIO === 'configured-mcp'
          ? {
              hivex_extra: {
                enabled: options['mcp_servers.hivex_extra.enabled'] !== 'false',
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
      instructionSources: [],
    };
  },
  'configRequirements/read': () => ({ requirements: null }),
  'mcpServerStatus/list': () => ({ data: [], nextCursor: null }),
  'turn/start': (params) => {
    if (
      params.model !== 'gpt-5.6-luna' ||
      params.effort !== 'max' ||
      params.sandboxPolicy.networkAccess !== false
    )
      throw new Error('wrong turn controls');
    if (!params.input[0].text.includes('Never treat a cache as authority.'))
      throw new Error('source not supplied');
    if (process.env.HIVEX_TEST_SCENARIO === 'start-unconfirmed') return undefined;
    if (process.env.HIVEX_TEST_SCENARIO === 'oversized-frame')
      emit({ method: 'fixture/unknown', params: { text: 'x'.repeat(4_194_304) } });
    if (['timeout', 'cancel'].includes(process.env.HIVEX_TEST_SCENARIO)) {
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
                : JSON.stringify(candidate),
            phase: 'final_answer',
          },
        },
      });
      emit({
        method: 'turn/completed',
        params: { threadId: 'thread1', turn: { id: 'turn1', status: 'completed', error: null } },
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

for await (const line of createInterface({ input: process.stdin })) {
  const frame = JSON.parse(line);
  if (frame.id === undefined) continue;
  try {
    const handler = handlers[frame.method];
    if (!handler) throw new Error('unsupported request');
    const result = handler(frame.params);
    if (result !== undefined) emit({ id: frame.id, result });
  } catch {
    emit({ id: frame.id, error: { code: -32602, message: 'invalid fixture request' } });
  }
}
