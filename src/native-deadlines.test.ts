import { expect, test } from 'bun:test';
import type { AppServerConnection } from './model/connection.ts';
import { startKnowledgeThread } from './model/thread.ts';
import { assessmentArguments } from './graph/assessment-cohort.ts';
import { extractionArguments } from './ingestion/arguments.ts';

test('accepts the 30-minute native deadline and keeps the current default', () => {
  expect(extractionArguments(['source.md']).deadlineMilliseconds).toBe(600_000);
  expect(extractionArguments(['source.md', '--deadline-ms', '1800000']).deadlineMilliseconds).toBe(
    1_800_000,
  );
  expect(() => extractionArguments(['source.md', '--deadline-ms', '1800001'])).toThrow();

  expect(
    assessmentArguments(['--all', '--input', 'graph.json', '--deadline-ms', '1800000'], 'review')
      .deadlineMilliseconds,
  ).toBe(1_800_000);
});

test('extends only thread/start to a 90-second RPC timeout', async () => {
  const requests: { method: string; timeoutMilliseconds?: number }[] = [];
  const rpc = {
    request: async (
      method: string,
      _params: unknown,
      options?: { timeoutMilliseconds?: number },
    ) => {
      requests.push({ method, timeoutMilliseconds: options?.timeoutMilliseconds });
      if (method === 'thread/start')
        return {
          thread: { id: 'thread1', ephemeral: true },
          model: 'gpt-5.6-luna',
          modelProvider: 'openai',
          reasoningEffort: 'max',
          cwd: '/workspace',
          sandbox: { type: 'readOnly' },
          instructionSources: [],
        };
      return { data: [], nextCursor: null };
    },
  } as unknown as AppServerConnection;

  expect(
    await startKnowledgeThread({
      rpc,
      workspace: '/workspace',
      signal: new AbortController().signal,
    }),
  ).toBe('thread1');
  expect(requests).toEqual([
    { method: 'thread/start', timeoutMilliseconds: 90_000 },
    { method: 'mcpServerStatus/list', timeoutMilliseconds: undefined },
  ]);
});
