import { parseArgs } from 'node:util';
import { resolve } from 'node:path';
import { HivexError } from '../errors.ts';
import { parseLimit } from '../cli/arguments.ts';
import { buildGraph } from './build.ts';
import { readGraph, checkGraph } from './verify.ts';
import { budgeted, readNode, searchGraph, neighbors } from './query.ts';

function input(args: string[]) {
  try {
    return parseArgs({
      args,
      allowPositionals: true,
      strict: true,
      options: {
        root: { type: 'string' },
        store: { type: 'string' },
        export: { type: 'boolean' },
        input: { type: 'string' },
        against: { type: 'string' },
        limit: { type: 'string' },
        'max-bytes': { type: 'string' },
        cursor: { type: 'string' },
      },
    });
  } catch (error) {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: error instanceof Error ? error.message : 'Invalid graph arguments',
    });
  }
}

function argumentsFor(args: string[]) {
  const parsed = input(args);
  const [operation, value] = parsed.positionals;
  const expectsValue = ['read', 'search', 'neighbors'].includes(operation ?? '');
  if (
    !['build', 'check', 'read', 'search', 'neighbors'].includes(operation ?? '') ||
    parsed.positionals.length !== (expectsValue ? 2 : 1) ||
    Object.values(parsed.values).some((item) => item === '') ||
    (expectsValue && !value?.trim())
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Choose a supported graph operation and its required input',
    });
  if (
    operation === 'build' &&
    [
      parsed.values.input,
      parsed.values.against,
      parsed.values.limit,
      parsed.values['max-bytes'],
      parsed.values.cursor,
    ].some((item) => item !== undefined)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Build options cannot be mixed with graph inspection',
    });
  if (
    operation !== 'build' &&
    (parsed.values.export !== undefined ||
      parsed.values.store !== undefined ||
      !parsed.values.input)
  )
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Inspection requires --input and cannot use build options',
    });
  if (parsed.values.cursor !== undefined && operation !== 'neighbors')
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'A graph cursor belongs to neighbor expansion',
    });
  return {
    ...parsed.values,
    operation,
    value: value ?? '',
    root: parsed.values.root ?? process.cwd(),
    limit: parseLimit(parsed.values.limit, { fallback: 8, minimum: 1, maximum: 20 }),
    maxBytes: parseLimit(parsed.values['max-bytes'], {
      fallback: 16_384,
      minimum: 1024,
      maximum: 524_288,
    }),
  };
}

export function graphCommand(args: string[]) {
  const options = argumentsFor(args);
  if (options.operation === 'build') {
    const graph = buildGraph(
      options.root,
      options.store ?? resolve(options.root, '.hivex/ingestion.sqlite'),
    );
    const bytes = Buffer.byteLength(JSON.stringify(graph)) + 1;
    if (bytes > 64 * 1024 * 1024)
      throw new HivexError({
        code: 'GRAPH_TOO_LARGE',
        message: 'The graph snapshot exceeds 64 MiB',
      });
    if (options.export) return graph;
    return {
      command: 'graph',
      operation: 'build',
      accepted: false,
      hash: graph.hash,
      sourceSnapshot: graph.sourceSnapshot,
      inputHash: graph.inputHash,
      sources: graph.sources.length,
      nodes: graph.nodes.length,
      edges: graph.edges.length,
      bytes,
    };
  }
  if (!options.input)
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Graph inspection requires --input',
    });
  const input = readGraph(options.input);
  const check = checkGraph(input, options.root, options.against);
  if (options.operation === 'read') return readNode(input, options.value, check, options.maxBytes);
  if (options.operation === 'search')
    return searchGraph({ ...options, input, check, query: options.value });
  if (options.operation === 'neighbors')
    return neighbors({ ...options, input, check, id: options.value });
  return budgeted(check, options.maxBytes);
}
