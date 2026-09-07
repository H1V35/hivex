import { hash } from '../sources/markdown.ts';
import { loadSnapshot, type Snapshot } from '../workspace/snapshot.ts';
import { maximumSourceBytes, prepareExtraction, processingContract } from './preparation.ts';
import { planArguments } from './plan-arguments.ts';
import { pagePlan } from './plan-page.ts';

export function planCommand(args: string[]) {
  const options = planArguments(args);
  const snapshot = loadSnapshot({ ...options, selection: { collection: options.collection } });
  return pagePlan(createPlan(snapshot, options.collection ?? null), options);
}

export function createPlan(snapshot: Snapshot, collection: string | null) {
  const units = snapshot.sources
    .map((source) => {
      const prepared = prepareExtraction(source);
      return {
        id: source.id,
        path: source.path,
        collection: source.collection,
        contentHash: source.contentHash,
        section: source.section,
        authority: source.authority,
        sourceBytes: prepared.sourceBytes,
        promptBytes: prepared.promptBytes,
        basePromptHash: prepared.basePromptHash,
        readiness: prepared.sourceBytes > maximumSourceBytes ? 'requires-section' : 'extractable',
      };
    })
    .sort((a, b) => {
      if (a.id === b.id) return 0;
      return a.id < b.id ? -1 : 1;
    });
  const cohort = {
    version: 1,
    selection: { collection },
    snapshot: { commit: snapshot.commit, configHash: snapshot.configHash },
    processing: processingContract(),
    units,
  };
  return {
    command: 'plan',
    accepted: false,
    planHash: hash(JSON.stringify(cohort)),
    ...cohort,
    summary: {
      sourceCount: units.length,
      sourceBytes: units.reduce((total, unit) => total + unit.sourceBytes, 0),
      promptBytes: units.reduce((total, unit) => total + unit.promptBytes, 0),
      oversizedSources: units.filter((unit) => unit.readiness === 'requires-section').length,
      modelCalls: 0,
    },
  };
}
