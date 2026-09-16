import { readFileSync, statSync } from 'node:fs';
import { parseArgs } from 'node:util';
import { z } from 'zod';
import { loadProject } from './documents.ts';
import { HivexError } from './errors.ts';
import {
  citationSchema,
  isWarningResolved,
  validCitation,
  warningId,
  warningSummary,
  withWarningResolution,
} from './knowledge-model.ts';
import { KnowledgeStore, storedGraph } from './knowledge-store.ts';
import type { Graph } from './knowledge-model.ts';
import type { Project } from './documents.ts';

const resolutionSchema = z
  .object({
    evidence: z
      .array(citationSchema.extend({ version: z.string().min(1) }))
      .min(1)
      .max(32),
    id: z.string().min(1),
    reason: z.string().trim().min(1).max(2048),
  })
  .strict();

const readResolutions = function readResolutions(file: string) {
  if (statSync(file).size > 2 * 1024 * 1024) {
    throw new HivexError({ code: 'INVALID_RESOLUTION', message: 'Resolution file exceeds 2 MiB.' });
  }
  return z
    .array(resolutionSchema)
    .min(1)
    .max(1024)
    .parse(JSON.parse(readFileSync(file, 'utf-8')));
};

const resolveWarnings = function resolveWarnings(
  graph: Graph,
  project: Project,
  resolutions: z.infer<typeof resolutionSchema>[]
) {
  const known = new Map(graph.warnings.map((warning) => [warningId(warning), warning]));
  const resolved = new Map<string, z.infer<typeof resolutionSchema>>();
  for (const resolution of resolutions) {
    const warning = known.get(resolution.id);
    const isCurrentEvidence = resolution.evidence.every((citation) => {
      const source = project.documents.find((document) => document.id === citation.document);
      return source?.hash === citation.version && validCitation(citation, project.documents);
    });
    if (warning === undefined || !isCurrentEvidence || resolved.has(resolution.id)) {
      throw new HivexError({
        code: 'INVALID_RESOLUTION',
        message: 'Each resolution needs a unique known warning ID and valid current evidence.',
      });
    }
    if (isWarningResolved(warning, project.documents)) {
      throw new HivexError({
        code: 'WARNING_ALREADY_RESOLVED',
        message: 'The warning already has a current resolution; its history is preserved.',
      });
    }
    resolved.set(resolution.id, resolution);
  }
  return {
    ...graph,
    warnings: graph.warnings.map((warning): Graph['warnings'][number] => {
      const resolution = resolved.get(warningId(warning));
      const applied =
        resolution === undefined
          ? null
          : {
              evidence: resolution.evidence,
              reason: resolution.reason,
            };
      return applied === null ? warning : withWarningResolution(warning, applied);
    }),
  };
};

export const warningCommand = function warningCommand(argumentsList: string[]) {
  const { positionals, values } = parseArgs({
    allowPositionals: true,
    args: argumentsList,
    options: { all: { type: 'boolean' }, resolve: { type: 'string' }, root: { type: 'string' } },
    strict: true,
  });
  if (positionals.length !== 1 || positionals[0] !== 'warnings') {
    throw new HivexError({
      code: 'INVALID_ARGUMENT',
      message: 'Use warnings [--all] [--resolve <resolutions.json>] [--root <project>].',
    });
  }
  const project = loadProject(values.root ?? process.cwd());
  const resolutions = values.resolve === undefined ? null : readResolutions(values.resolve);
  using store = resolutions === null ? null : new KnowledgeStore(project.root, { update: true });
  const original = store?.graph() ?? storedGraph(project.root);
  const graph = resolutions === null ? original : resolveWarnings(original, project, resolutions);
  if (resolutions !== null) {
    store?.saveGraph(graph);
  }
  const summary = warningSummary(graph.warnings, project.documents);
  const previous = warningSummary(original.warnings, project.documents);
  return {
    command: 'warnings',
    modelCalls: 0,
    resolved: summary.resolved - previous.resolved,
    warningSummary: summary,
    warnings: graph.warnings.flatMap((warning) => {
      const isResolved = isWarningResolved(warning, project.documents);
      if (isResolved && values.all !== true) {
        return [];
      }
      const entry = typeof warning === 'string' ? { message: warning, scope: [] } : warning;
      return [{ ...entry, id: warningId(warning), state: isResolved ? 'resolved' : 'active' }];
    }),
  };
};
