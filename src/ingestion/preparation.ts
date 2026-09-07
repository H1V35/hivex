import { hash, type Source } from '../sources/markdown.ts';
import { knowledgeModel, nativeVersion, requestedPolicyHash } from '../model/profile.ts';
import { extractionInstructions, extractionSchema } from './claims.ts';

export const maximumSourceBytes = 32_768;

export function processingContract() {
  return {
    model: knowledgeModel,
    nativeVersion,
    requestedPolicyHash: requestedPolicyHash(),
    schemaHash: hash(JSON.stringify(extractionSchema)),
    maximumSourceBytes,
  };
}

export function prepareExtraction(source: Source) {
  const prompt = `${extractionInstructions}\n\n${JSON.stringify({
    source: source.id,
    firstLine: source.section?.lineStart ?? 1,
    declaredAuthority: source.authority,
    markdown: source.content,
  })}`;
  return {
    prompt,
    sourceBytes: Buffer.byteLength(source.content),
    promptBytes: Buffer.byteLength(prompt),
    basePromptHash: hash(prompt),
  };
}
