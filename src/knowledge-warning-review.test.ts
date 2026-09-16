import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import nodePath from 'node:path';
import { expect, test } from 'bun:test';
import { loadProject } from './documents.ts';
import {
  applyWarningReview,
  warningBaseline,
  warningChanges,
  warningReviewCandidates,
} from './knowledge-warning-review.ts';
import { emptyGraph, warningId } from './knowledge-model.ts';
import { hash, rawMarkdownLines } from './markdown.ts';
import type { Document } from './documents.ts';
import type { Graph, SuppliedDocument, WarningScope } from './knowledge-model.ts';

const writeFiles = function writeFiles(root: string, files: Record<string, string>) {
  for (const [path, text] of Object.entries(files)) {
    writeFileSync(nodePath.join(root, path), text, 'utf-8');
  }
};

const withProject = function withProject(
  files: Record<string, string>,
  run: (documents: Document[]) => void
) {
  const root = mkdtempSync(nodePath.join(tmpdir(), 'hivex-warning-review-'));
  try {
    writeFiles(root, files);
    run(loadProject(root).documents);
  } finally {
    rmSync(root, { force: true, recursive: true });
  }
};

const documentFor = function documentFor(documents: Document[], id: string) {
  const document = documents.find((entry) => entry.id === id);
  if (document === undefined) {
    throw new Error(`Expected document ${id}`);
  }
  return document;
};

const warningByMessage = function warningByMessage(graph: Graph, message: string) {
  const warning = graph.warnings.find(
    (entry) => typeof entry !== 'string' && entry.message === message
  );
  if (warning === undefined || typeof warning === 'string') {
    throw new Error(`Expected warning ${message}`);
  }
  return warning;
};

const scopeFor = function scopeFor(
  document: Document,
  lineStart = 1,
  lineEnd = lineStart,
  version = document.hash
): WarningScope {
  return { document: document.id, lineEnd, lineStart, version };
};

const citationFor = function citationFor(document: Document, lineStart = 1, lineEnd = lineStart) {
  return { document: document.id, lineEnd, lineStart };
};

const suppliedFor = function suppliedFor(
  document: Document,
  lineEnd = rawMarkdownLines(document.text).length
): SuppliedDocument {
  return {
    id: document.id,
    lines: rawMarkdownLines(document.text)
      .slice(0, lineEnd)
      .map((line, index) => [index + 1, line]),
  };
};

const warningById = function warningById(graph: Graph, id: string) {
  const warning = graph.warnings.find((entry) => warningId(entry) === id);
  if (warning === undefined || typeof warning === 'string') {
    throw new Error(`Expected warning ${id}`);
  }
  return warning;
};

const sourceFiles = {
  'amendment.md': '# Amendment\n\nThe final amendment records the review condition.\n',
  'contract.md':
    '# Future contract\n\nThe provider contract remains open.\n\nThe renewal owner is not selected.\n\nAmendment: review the contract after the live deployment.\n',
  'deploy.md':
    '# Deploy procedure\n\nRun the release command.\n\nVerify the live URL and record the result.\n',
  'unrelated.md': '# Unrelated notes\n\nA separate design question remains.\n',
};

test('offers new limitations and expired closures only with complete implicated sources', () => {
  withProject(sourceFiles, (documents) => {
    const deploy = documentFor(documents, 'deploy.md');
    const contract = documentFor(documents, 'contract.md');
    const unrelated = documentFor(documents, 'unrelated.md');
    const newMessage = 'The deploy procedure lacks live proof.';
    const partialMessage = 'The future contract ownership remains open.';
    const expiredMessage = 'The earlier contract closure needs review.';
    const currentMessage = 'The current closure remains supported.';
    const unrelatedMessage = 'An unrelated question is still active.';
    const findingMessage = 'The implementation finding remains open.';
    const validationMessage = 'The citation validation failed.';
    const oldClosure = {
      evidence: [scopeFor(contract, 3, 3, 'old-contract-version')],
      reason: 'The earlier source was used before the amendment.',
    };
    const graph: Graph = {
      ...emptyGraph(),
      documents: Object.fromEntries(documents.map((document) => [document.id, document.hash])),
      warnings: [
        {
          kind: 'limitation',
          message: newMessage,
          scope: [scopeFor(deploy, 3, 3)],
        },
        {
          kind: 'limitation',
          message: partialMessage,
          scope: [scopeFor(contract, 3, 3)],
        },
        {
          kind: 'limitation',
          message: expiredMessage,
          resolution: oldClosure,
          scope: [scopeFor(deploy, 3, 3)],
        },
        {
          kind: 'limitation',
          message: currentMessage,
          resolution: { evidence: [scopeFor(deploy, 3, 3)], reason: 'Current evidence is enough.' },
          scope: [scopeFor(deploy, 3, 3)],
        },
        {
          kind: 'limitation',
          message: unrelatedMessage,
          scope: [scopeFor(unrelated, 3, 3)],
        },
        {
          kind: 'finding',
          message: findingMessage,
          resolution: oldClosure,
          scope: [scopeFor(deploy, 3, 3)],
          target: 'deployment-decision',
        },
        {
          kind: 'validation',
          message: validationMessage,
          resolution: oldClosure,
          scope: [scopeFor(deploy, 3, 3)],
        },
      ],
    };
    const incomplete = warningReviewCandidates(graph, {
      documents,
      supplied: [suppliedFor(deploy), suppliedFor(contract, 3)],
      uncertainties: [newMessage, partialMessage],
    });

    expect(incomplete.map((entry) => entry.message)).toEqual([newMessage]);

    const complete = warningReviewCandidates(graph, {
      documents,
      supplied: documents.map((document) => suppliedFor(document)),
      uncertainties: [newMessage, partialMessage],
    });

    expect(complete.map((entry) => entry.message)).toEqual([
      newMessage,
      partialMessage,
      expiredMessage,
    ]);
  });
});

test('applies only contextual current evidence while preserving warning identity and omitted questions', () => {
  withProject(sourceFiles, (documents) => {
    const deploy = documentFor(documents, 'deploy.md');
    const contract = documentFor(documents, 'contract.md');
    const unrelated = documentFor(documents, 'unrelated.md');
    const closureMessage = 'The earlier contract closure needs review.';
    const questionMessage = 'The future contract ownership remains open.';
    const previous = {
      evidence: [scopeFor(deploy, 3, 3, 'older-deploy-version')],
      reason: 'An earlier review considered the procedure descriptive.',
    };
    const originalResolution = {
      evidence: [scopeFor(contract, 3, 3, 'old-contract-version')],
      reason: 'The earlier source was used before the amendment.',
    };
    const graph: Graph = {
      ...emptyGraph(),
      documents: Object.fromEntries(documents.map((document) => [document.id, document.hash])),
      warnings: [
        {
          kind: 'limitation',
          message: closureMessage,
          previousResolutions: [previous],
          resolution: originalResolution,
          scope: [scopeFor(deploy, 3, 3)],
        },
        {
          kind: 'limitation',
          message: questionMessage,
          scope: [scopeFor(contract, 3, 3)],
        },
      ],
    };
    const closure = warningByMessage(graph, closureMessage);
    const question = warningByMessage(graph, questionMessage);
    const supplied = [suppliedFor(deploy), suppliedFor(contract)];
    const candidates = warningReviewCandidates(graph, {
      documents,
      supplied,
      uncertainties: [questionMessage],
    });
    const closureId = warningId(closure);
    const questionId = warningId(question);

    expect(candidates.map((entry) => entry.id)).toEqual([questionId, closureId]);

    const rejected = applyWarningReview(graph, {
      candidates,
      documents,
      resolutions: [
        {
          evidence: [citationFor(deploy, 3, 3)],
          id: 'unknown-warning-id',
          reason: 'This ID is not offered.',
        },
        {
          evidence: [citationFor(unrelated, 3, 3)],
          id: closureId,
          reason: 'This source was outside the supplied context.',
        },
        {
          evidence: [citationFor(deploy, 999, 999)],
          id: questionId,
          reason: 'This range is outside the document.',
        },
      ],
      supplied,
    });

    expect(rejected).toEqual(graph);

    const reason = 'The current documents support the closure after reviewing the amendment.';
    const applied = applyWarningReview(graph, {
      candidates,
      documents,
      resolutions: [{ evidence: [citationFor(deploy, 3, 3)], id: closureId, reason }],
      supplied,
    });
    const updated = warningById(applied, closureId);
    if (updated.resolution === undefined) {
      throw new Error('Expected the closure to have a current resolution');
    }

    expect(warningId(updated)).toBe(closureId);
    expect(updated.previousResolutions).toEqual([previous, originalResolution]);
    expect(updated.resolution.reason).toBe(reason);
    expect(new Set(updated.resolution.evidence.map((entry) => entry.document))).toEqual(
      new Set([deploy.id, contract.id])
    );
    for (const evidence of updated.resolution.evidence) {
      const document = documentFor(documents, evidence.document);
      expect(evidence).toEqual({
        document: document.id,
        lineEnd: rawMarkdownLines(document.text).length,
        lineStart: 1,
        version: document.hash,
      });
    }
    expect(warningById(applied, questionId)).toEqual(question);
  });
});

test('reports only new, reopened, and resolved warnings against the baseline', () => {
  withProject({ 'notes.md': '# Notes\n\nThe current warning evidence is here.\n' }, (documents) => {
    const document = documentFor(documents, 'notes.md');
    const newMessage = 'A new warning appeared.';
    const reopenedMessage = 'A prior closure became stale.';
    const resolvedMessage = 'An active warning is now resolved.';
    const untouchedActiveMessage = 'An active backlog warning remains.';
    const untouchedResolvedMessage = 'A resolved backlog warning remains resolved.';
    const currentEvidence = [scopeFor(document, 3, 3)];
    const staleEvidence = [scopeFor(document, 3, 3, 'stale-version')];
    const graph: Graph = {
      ...emptyGraph(),
      warnings: [
        {
          kind: 'limitation',
          message: newMessage,
          scope: currentEvidence,
        },
        {
          kind: 'limitation',
          message: reopenedMessage,
          resolution: {
            evidence: staleEvidence,
            reason: 'The old source supported this closure.',
          },
          scope: currentEvidence,
        },
        {
          kind: 'limitation',
          message: resolvedMessage,
          resolution: {
            evidence: currentEvidence,
            reason: 'Current source evidence supports closure.',
          },
          scope: currentEvidence,
        },
        {
          kind: 'limitation',
          message: untouchedActiveMessage,
          scope: currentEvidence,
        },
        {
          kind: 'limitation',
          message: untouchedResolvedMessage,
          resolution: {
            evidence: currentEvidence,
            reason: 'The current source remains unchanged.',
          },
          scope: currentEvidence,
        },
      ],
    };
    const newWarning = warningByMessage(graph, newMessage);
    const reopenedWarning = warningByMessage(graph, reopenedMessage);
    const resolvedWarning = warningByMessage(graph, resolvedMessage);
    const untouchedActive = warningByMessage(graph, untouchedActiveMessage);
    const untouchedResolved = warningByMessage(graph, untouchedResolvedMessage);
    const changes = warningChanges(graph, documents, {
      [warningId(reopenedWarning)]: 'resolved',
      [warningId(resolvedWarning)]: 'active',
      [warningId(untouchedActive)]: 'active',
      [warningId(untouchedResolved)]: 'resolved',
    });

    expect(changes.new).toEqual([
      { id: warningId(newWarning), message: newMessage, state: 'active' },
    ]);
    expect(changes.reopened).toEqual([
      { id: warningId(reopenedWarning), message: reopenedMessage, state: 'active' },
    ]);
    expect(changes.resolved).toEqual([
      { id: warningId(resolvedWarning), message: resolvedMessage, state: 'resolved' },
    ]);
    expect(Object.values(changes).flat()).not.toContainEqual(
      expect.objectContaining({ message: untouchedActiveMessage })
    );
    expect(Object.values(changes).flat()).not.toContainEqual(
      expect.objectContaining({ message: untouchedResolvedMessage })
    );
  });
});

test('bases warning changes on resolution freshness rather than graph coverage', () => {
  withProject({ 'notes.md': '# Notes\n\nThe current warning evidence is here.\n' }, (documents) => {
    const document = documentFor(documents, 'notes.md');
    const message = 'A prior closure must follow the current source.';
    const graph: Graph = {
      ...emptyGraph(),
      documents: { [document.id]: 'v1' },
      warnings: [
        {
          kind: 'limitation',
          message,
          resolution: {
            evidence: [scopeFor(document, 3, 3)],
            reason: 'The current source supports the closure.',
          },
          scope: [scopeFor(document, 3, 3)],
        },
      ],
    };
    const warning = warningByMessage(graph, message);
    const baseline = warningBaseline(graph);
    const current = warningChanges(graph, documents, baseline);

    expect(baseline).toEqual({ [warningId(warning)]: 'resolved' });
    expect(current).toEqual({ new: [], reopened: [], resolved: [] });

    const changedText = `${document.text}Amendment changes the source.\n`;
    const changedDocument = { ...document, hash: hash(changedText), text: changedText };
    expect(changedDocument.hash).not.toBe(document.hash);

    expect(warningChanges(graph, [changedDocument], baseline)).toEqual({
      new: [],
      reopened: [{ id: warningId(warning), message, state: 'active' }],
      resolved: [],
    });
  });
});
