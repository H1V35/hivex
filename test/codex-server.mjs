#!/usr/bin/env bun
import { createInterface } from 'node:readline';
import { spawn } from 'node:child_process';
import { appendFileSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { once } from 'node:events';

if (
  process.env.HIVEX_TEST_SCENARIO === 'secret-environment' &&
  ['GH_TOKEN', 'DATABASE_URL', 'HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY'].some(
    (name) => process.env[name]
  )
) {
  process.exit(Number('19'));
}

if (process.argv.includes('--version')) {
  console.log(
    process.env.HIVEX_TEST_SCENARIO === 'future-version' ? 'codex-cli 9.99.0' : 'codex-cli 0.153.2'
  );
  process.exit(Number('0'));
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
    })
  );
  writeFileSync(process.env.HIVEX_TEST_PID_PATH, JSON.stringify(descendants));
}

const endpointPrefixLength = 10;
const firstIndex = 0;
const firstResultLimit = 1;
const frameLimitBytes = 4_194_304;
const interruptExitDelayMilliseconds = 20;
const invalidRequestCode = -32_602;
const nextArgumentOffset = 1;
const noIndex = -1;
const noResponse = Symbol('no response');
const payloadStartOffset = 2;
const pollIntervalMilliseconds = 10;
const turnCompletedMethod = 'turn/completed';
const tokenUsageUpdatedMethod = 'thread/tokenUsage/updated';
const modelName = 'gpt-5.6-luna';

const options = Object.fromEntries(
  process.argv.flatMap((argument, index, argumentList) => {
    if (argument !== '-c') {
      return [];
    }
    const setting = argumentList[index + nextArgumentOffset];
    const split = setting.indexOf('=');
    return [[setting.slice(firstIndex, split), setting.slice(split + nextArgumentOffset)]];
  })
);
const disabledArgument = function disabledArgument(argument, index, argumentList) {
  if (argument !== '--disable') {
    return [];
  }
  return [argumentList[index + nextArgumentOffset]];
};
const disabled = process.argv.flatMap(disabledArgument);
const payloadFromPrompt = function payloadFromPrompt(prompt) {
  const marker = prompt.lastIndexOf('\n\n');
  if (marker === noIndex) {
    return null;
  }
  try {
    return JSON.parse(prompt.slice(marker + payloadStartOffset));
  } catch {
    return null;
  }
};

const responseForPrompt = function responseForPrompt(prompt) {
  if (process.env.HIVEX_TEST_RESPONSES) {
    const responses = JSON.parse(readFileSync(process.env.HIVEX_TEST_RESPONSES, 'utf-8'));
    const packet = payloadFromPrompt(prompt);
    if (responses.capturePackets) {
      appendFileSync(`${process.env.HIVEX_TEST_RESPONSES}.packets`, `${JSON.stringify(packet)}\n`);
    }
    if (packet?.operation === 'extract' && responses.fromVisibleRules) {
      const visibleDecisions = function visibleDecisions(documentEntry) {
        const isVisibleRule = function isVisibleRule([number, line]) {
          if (!/^Rule \d+ requires/u.test(line)) {
            return false;
          }
          if (!packet.units) {
            return true;
          }
          const isUnitInRange = function isUnitInRange(unit) {
            return (
              unit.document === documentEntry.id &&
              unit.lineStart <= number &&
              unit.lineEnd >= number
            );
          };
          return packet.units.some(isUnitInRange);
        };
        const createVisibleDecision = function createVisibleDecision([number, line]) {
          return {
            ...responses.extract.decisions[firstIndex],
            document: documentEntry.id,
            id: `c${number}`,
            lineEnd: number,
            lineStart: number,
            text: `${line.split('. ', firstResultLimit)[firstIndex]}.`,
          };
        };
        return documentEntry.lines.filter(isVisibleRule).map(createVisibleDecision);
      };
      const isTargetDocument = function isTargetDocument(documentEntry) {
        return packet.targets.includes(documentEntry.id);
      };
      const decisions = packet.documents.filter(isTargetDocument).flatMap(visibleDecisions);
      return { decisions, relationships: [], uncertainties: [] };
    }
    if (packet?.operation === 'extract' && responses.byDocument) {
      const targets = packet.targets ?? packet.documents.map((document) => document.id);
      const parts = targets.map(
        (id) => responses.byDocument[id] ?? { decisions: [], relationships: [] }
      );
      const hasEvidenceDocument = function hasEvidenceDocument(relationship) {
        if (!relationship.requiresEvidenceDocument) {
          return true;
        }
        const isEvidenceDocument = function isEvidenceDocument(documentEntry) {
          return documentEntry.id === relationship.requiresEvidenceDocument;
        };
        return packet.documents.some(isEvidenceDocument);
      };
      const rewriteRelationship = function rewriteRelationship(relationship) {
        return {
          ...relationship,
          ...Object.fromEntries(
            ['from', 'to'].map((key) => {
              const endpoint = relationship[key];
              const isExistingDecision = function isExistingDecision(decision) {
                return decision.document === endpoint.slice(endpointPrefixLength);
              };
              return [
                key,
                endpoint.startsWith('@existing:')
                  ? (packet.existing?.find(isExistingDecision)?.id ?? endpoint)
                  : endpoint,
              ];
            })
          ),
        };
      };
      const relationships = parts
        .flatMap((part) => part.relationships)
        .filter(hasEvidenceDocument)
        .map(rewriteRelationship);
      return {
        decisions: parts.flatMap((part) => part.decisions),
        relationships,
        uncertainties: [],
      };
    }
    return responses[packet?.operation];
  }
  return null;
};
const emit = (frame) => process.stdout.write(`${JSON.stringify(frame)}\n`);
const createConfigOrigin = function createConfigOrigin(key) {
  return [key, { name: { type: 'sessionFlags' }, version: 'fixture-config-v1' }];
};
const handlers = {
  'account/read': () => ({ account: { type: 'chatgpt' } }),
  'config/read': function configRead() {
    const features = Object.fromEntries([
      ...disabled.map((key) => [key, false]),
      ['skip_host_skill_discovery', process.argv.includes('skip_host_skill_discovery')],
    ]);
    const mcpServers = ['configured-mcp', 'ignored-mcp', 'abort-isolation'].includes(
      process.env.HIVEX_TEST_SCENARIO
    )
      ? Object.fromEntries([
          [
            'hivex_extra',
            Object.fromEntries([
              ['command', '/must-not-start'],
              [
                'enabled',
                process.env.HIVEX_TEST_SCENARIO === 'ignored-mcp' ||
                  options['mcp_servers.hivex_extra.enabled'] !== 'false',
              ],
            ]),
          ],
        ])
      : {};
    const memories = Object.fromEntries([
      ['generate_memories', false],
      ['use_memories', false],
    ]);
    const config = Object.fromEntries([
      [
        'chatgpt_base_url',
        process.env.HIVEX_TEST_SCENARIO === 'redirected-provider'
          ? 'https://not-openai.invalid'
          : 'https://chatgpt.com',
      ],
      ['features', features],
      ['mcp_servers', mcpServers],
      ['memories', memories],
      ['model', JSON.parse(options.model)],
      ['model_provider', 'openai'],
      ['model_providers', {}],
      ['model_reasoning_effort', JSON.parse(options.model_reasoning_effort)],
      ['openai_base_url', null],
      ['project_doc_max_bytes', Number(options.project_doc_max_bytes)],
      ['web_search', JSON.parse(options.web_search)],
    ]);
    return {
      config,
      origins: Object.fromEntries(
        ['model', 'model_provider', 'model_reasoning_effort', 'chatgpt_base_url'].map(
          createConfigOrigin
        )
      ),
    };
  },
  'configRequirements/read': () => ({ requirements: null }),
  initialize: () => ({ userAgent: 'fixture' }),
  'mcpServerStatus/list': () => ({ data: [], nextCursor: null }),
  'model/list': function modelList() {
    return {
      data: [
        {
          model: modelName,
          supportedReasoningEfforts: [{ reasoningEffort: 'max' }],
        },
      ],
      nextCursor: null,
    };
  },
  'thread/start': (parameters) => {
    if (
      process.env.HIVEX_TEST_SCENARIO === 'configured-mcp' &&
      options['mcp_servers.hivex_extra.enabled'] !== 'false'
    ) {
      throw new Error('configured MCP was not disabled before thread creation');
    }
    if (parameters.model !== modelName || parameters.allowProviderModelFallback !== false) {
      throw new Error('wrong model or fallback');
    }
    const effort = process.env.HIVEX_TEST_SCENARIO === 'changed-effort' ? 'high' : 'max';
    return {
      cwd: process.cwd(),
      instructionSources:
        process.env.HIVEX_TEST_SCENARIO === 'instruction-source-metadata'
          ? ['/example/.codex/AGENTS.md']
          : [],
      model: parameters.model,
      modelProvider: 'openai',
      reasoningEffort: effort,
      sandbox: { type: 'readOnly' },
      thread: { ephemeral: true, id: 'thread1' },
    };
  },
  'turn/interrupt': () => {
    queueMicrotask(() => {
      emit({
        method: turnCompletedMethod,
        params: {
          threadId: 'thread1',
          turn: { error: null, id: 'turn1', status: 'interrupted' },
        },
      });
    });
    return {};
  },
  'turn/start': (parameters) => {
    if (process.env.HIVEX_TEST_CALLS_FILE) {
      appendFileSync(process.env.HIVEX_TEST_CALLS_FILE, 'called\n');
    }
    if (
      parameters.model !== modelName ||
      parameters.effort !== 'max' ||
      parameters.sandboxPolicy.networkAccess !== false
    ) {
      throw new Error('wrong turn controls');
    }
    if (
      typeof parameters.input[firstIndex]?.text !== 'string' ||
      !parameters.input[firstIndex].text.trim()
    ) {
      throw new Error('prompt not supplied');
    }
    if (process.env.HIVEX_TEST_SCENARIO === 'unconfirmed-interrupt') {
      setTimeout(() => {
        process.exit(Number('0'));
      }, interruptExitDelayMilliseconds);
      return { turn: { id: 'turn1', status: 'inProgress' } };
    }
    if (['start-unconfirmed', 'update-uncertain'].includes(process.env.HIVEX_TEST_SCENARIO)) {
      return noResponse;
    }
    if (process.env.HIVEX_TEST_SCENARIO === 'oversized-frame') {
      emit({
        method: 'fixture/unknown',
        params: { text: 'x'.repeat(frameLimitBytes) },
      });
    }
    if (['timeout', 'timeout-unmeasured', 'cancel'].includes(process.env.HIVEX_TEST_SCENARIO)) {
      if (process.env.HIVEX_TEST_SCENARIO !== 'timeout-unmeasured') {
        queueMicrotask(() => {
          emit({
            method: tokenUsageUpdatedMethod,
            params: {
              threadId: 'thread1',
              tokenUsage: {
                total: {
                  cachedInputTokens: 20,
                  inputTokens: 100,
                  outputTokens: 25,
                  reasoningOutputTokens: 25,
                  totalTokens: 125,
                },
              },
              turnId: 'turn1',
            },
          });
        });
      }
      if (process.env.HIVEX_TEST_SCENARIO === 'cancel') {
        queueMicrotask(() => {
          process.kill(process.ppid, 'SIGINT');
        });
      }
      return { turn: { id: 'turn1', status: 'inProgress' } };
    }
    const responseCandidate = responseForPrompt(parameters.input[firstIndex].text);
    queueMicrotask(() => {
      emit({
        method: tokenUsageUpdatedMethod,
        params: {
          threadId: 'thread1',
          tokenUsage: {
            total: {
              cachedInputTokens: 20,
              inputTokens: 100,
              outputTokens: 50,
              reasoningOutputTokens: 30,
              totalTokens: 150,
            },
          },
          turnId: 'turn1',
        },
      });
      if (process.env.HIVEX_TEST_SCENARIO === 'usage-regression') {
        emit({
          method: tokenUsageUpdatedMethod,
          params: {
            threadId: 'thread1',
            tokenUsage: {
              total: {
                cachedInputTokens: 20,
                inputTokens: 90,
                outputTokens: 50,
                reasoningOutputTokens: 30,
                totalTokens: 140,
              },
            },
            turnId: 'turn1',
          },
        });
      }
      emit({
        method: 'item/completed',
        params: {
          item: {
            id: 'message1',
            phase: 'final_answer',
            text:
              process.env.HIVEX_TEST_SCENARIO === 'invalid-json'
                ? '{broken'
                : JSON.stringify(responseCandidate),
            type: 'agentMessage',
          },
          threadId: 'thread1',
          turnId: 'turn1',
        },
      });
      emit({
        method: turnCompletedMethod,
        params: {
          threadId: 'thread1',
          turn: { error: null, id: 'turn1', status: 'completed' },
        },
      });
      if (process.env.HIVEX_TEST_SCENARIO === 'duplicate-terminal') {
        emit({
          method: turnCompletedMethod,
          params: {
            threadId: 'thread1',
            turn: { id: 'turn1', status: 'completed' },
          },
        });
      }
    });
    return { turn: { id: 'turn1', status: 'inProgress' } };
  },
};

const captureHandlerFailure = function captureHandlerFailure(frame) {
  const handler = handlers[frame.method];
  if (handler === undefined) {
    return { error: new Error('unsupported request') };
  }
  try {
    return { value: handler(frame.params) };
  } catch (error) {
    return { error };
  }
};

const readHoldFile = function readHoldFile(filePath) {
  try {
    return readFileSync(filePath, 'utf-8');
  } catch (error) {
    if (Error.isError(error) && 'code' in error && error.code === 'ENOENT') {
      return null;
    }
    throw error;
  }
};

const waitForHoldRelease = (holdPath, callsPath) => {
  const released = Promise.withResolvers();
  const check = () => {
    if (!existsSync(holdPath)) {
      released.resolve();
      return;
    }
    const holdContents = readHoldFile(holdPath);
    if (
      holdContents === null ||
      (holdContents === 'after-first' && readFileSync(callsPath, 'utf-8') === 'called\n')
    ) {
      released.resolve();
      return;
    }
    setTimeout(check, pollIntervalMilliseconds);
  };
  check();
  return released.promise;
};

const observeModelRequest = async function observeModelRequest() {
  const calls = process.env.HIVEX_TEST_CALLS_PATH;
  if (calls === undefined || calls === '') {
    return;
  }
  appendFileSync(calls, 'called\n');
  const hold = process.env.HIVEX_TEST_HOLD_PATH;
  if (hold === undefined || hold === '') {
    return;
  }
  await waitForHoldRelease(hold, calls);
};

const handleFrame = async function handleFrame(frame) {
  if (frame.method === 'turn/start') {
    await observeModelRequest();
  }
  const result = captureHandlerFailure(frame);
  if ('error' in result) {
    emit({
      error: { code: invalidRequestCode, message: 'invalid fixture request' },
      id: frame.id,
    });
  } else if (result.value !== noResponse) {
    emit({ id: frame.id, result: result.value });
  }
};

const inputLines = createInterface({ input: process.stdin });
for await (const line of inputLines) {
  const frame = JSON.parse(line);
  if (frame.id === undefined) {
    continue;
  }
  await handleFrame(frame);
}

if (process.env.HIVEX_TEST_SCENARIO === 'abort-isolation') {
  process.kill(process.ppid, 'SIGINT');
}
