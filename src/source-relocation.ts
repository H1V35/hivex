import { compareSerializedStrings } from './ordering.ts';
import { HivexError } from './errors.ts';
import { isMarkdownPath } from './markdown.ts';
import type { Project } from './documents.ts';
import type { Graph } from './knowledge-model.ts';

const protectedParts = new Set(['', '.', '..', '.git', '.hivex', 'node_modules']);

const isPortablePath = function isPortablePath(value: string) {
  if (!isMarkdownPath(value) || value.startsWith('/')) {
    return false;
  }
  if (value.includes('\\') || value.includes('\0')) {
    return false;
  }
  return value.split('/').every((part) => !protectedParts.has(part));
};

const fail = function fail(code: string, message: string): never {
  throw new HivexError({ code, message });
};

const relationshipVersions = function relationshipVersions(
  relationship: Graph['relationships'][number],
  document: string
) {
  return relationship.evidence
    .filter((evidence) => evidence.document === document)
    .flatMap((evidence) => (evidence.version === undefined ? [] : [evidence.version]));
};

const warningVersions = function warningVersions(
  warning: Graph['warnings'][number],
  document: string
) {
  if (typeof warning === 'string') {
    return [];
  }
  return warning.scope.filter((scope) => scope.document === document).map((scope) => scope.version);
};

const sourceVersions = function sourceVersions(graph: Graph, document: string) {
  const versions = [
    graph.documents[document],
    ...Object.values(graph.units)
      .filter((unit) => unit.document === document)
      .map((unit) => unit.version),
    ...graph.decisions
      .filter((decision) => decision.document === document)
      .map((decision) => decision.version),
    ...graph.relationships.flatMap((relationship) => relationshipVersions(relationship, document)),
    ...graph.warnings.flatMap((warning) => warningVersions(warning, document)),
  ].filter((version): version is string => version !== undefined);
  return [...new Set(versions)].toSorted(compareSerializedStrings);
};

const hasKnowledge = function hasKnowledge(graph: Graph, document: string) {
  if (graph.documents[document] !== undefined) {
    return true;
  }
  if (Object.values(graph.units).some((unit) => unit.document === document)) {
    return true;
  }
  if (graph.decisions.some((decision) => decision.document === document)) {
    return true;
  }
  if (
    graph.relationships.some((relationship) =>
      relationship.evidence.some((evidence) => evidence.document === document)
    )
  ) {
    return true;
  }
  return graph.warnings.some(
    (warning) =>
      typeof warning !== 'string' && warning.scope.some((scope) => scope.document === document)
  );
};

const selectedDocument = function selectedDocument(project: Project, id: string) {
  return project.documents.find((document) => document.id === id);
};

const mapCitation = function mapCitation<T extends { document: string }>(
  citation: T,
  from: string,
  to: string
) {
  return citation.document === from ? { ...citation, document: to } : citation;
};

const mapUnitId = function mapUnitId(id: string, from: string, to: string) {
  const prefix = `${from}:`;
  return id.startsWith(prefix) ? `${to}${id.slice(from.length)}` : id;
};

const mapWarning = function mapWarning(
  warning: Graph['warnings'][number],
  from: string,
  to: string
): Graph['warnings'][number] {
  return typeof warning === 'string'
    ? warning
    : { ...warning, scope: warning.scope.map((scope) => mapCitation(scope, from, to)) };
};

const mapDecision = function mapDecision(
  decision: Graph['decisions'][number],
  from: string,
  to: string
) {
  return { ...decision, document: decision.document === from ? to : decision.document };
};

const mapRelationship = function mapRelationship(
  relationship: Graph['relationships'][number],
  from: string,
  to: string
) {
  return {
    ...relationship,
    evidence: relationship.evidence.map((evidence) => mapCitation(evidence, from, to)),
  };
};

const mapUnits = function mapUnits(
  graph: Graph,
  from: string,
  to: string,
  isCoverageRelocated: boolean
) {
  const entries = Object.entries(graph.units).flatMap(([id, unit]) => {
    if (!isCoverageRelocated && (unit.document === from || unit.document === to)) {
      return [];
    }
    if (unit.document !== from) {
      return [[id, unit] as const];
    }
    const relocatedId = mapUnitId(id, from, to);
    return [[relocatedId, { ...unit, document: to }] as const];
  });
  return Object.fromEntries(entries);
};

const mapDocuments = function mapDocuments(
  graph: Graph,
  from: string,
  to: string,
  options: { isCoverageRelocated: boolean; sourceVersion: string | undefined }
) {
  const entries = Object.entries(graph.documents).filter(([id]) => id !== from && id !== to);
  if (options.isCoverageRelocated && options.sourceVersion !== undefined) {
    entries.push([to, options.sourceVersion]);
  }
  return Object.fromEntries(entries);
};

export interface SourceRelocation {
  destinationVersion: string;
  from: string;
  fromVersions: string[];
  graph: Graph;
  reused: boolean;
  to: string;
}

export const relocateSource = function relocateSource(
  graph: Graph,
  project: Project,
  from: string,
  to: string
): SourceRelocation {
  if (from === to || !isPortablePath(from) || !isPortablePath(to)) {
    fail(
      'INVALID_ARGUMENT',
      'Source relocation paths must be distinct project-local Markdown files'
    );
  }
  if (!hasKnowledge(graph, from)) {
    fail('SOURCE_NOT_FOUND', `Source is not present in knowledge: ${from}`);
  }
  if (selectedDocument(project, from) !== undefined) {
    fail('INVALID_ARGUMENT', `Source must no longer be selected: ${from}`);
  }
  const destination = project.currentDocuments.find((document) => document.id === to);
  if (destination === undefined) {
    return fail('SOURCE_NOT_FOUND', `Destination is not a selected current Markdown source: ${to}`);
  }
  const versions = sourceVersions(graph, from);
  const hasDestinationKnowledge = hasKnowledge(graph, to);
  const hasUnversionedEvidence = graph.relationships
    .flatMap((relationship) => relationship.evidence)
    .some((evidence) => evidence.document === from && evidence.version === undefined);
  const isReused =
    !hasDestinationKnowledge &&
    !hasUnversionedEvidence &&
    versions.length > 0 &&
    versions.every((version) => version === destination.hash);
  const documents = mapDocuments(graph, from, to, {
    isCoverageRelocated: isReused,
    sourceVersion: graph.documents[from],
  });
  const units = mapUnits(graph, from, to, isReused);
  const relocated: Graph = {
    ...graph,
    decisions: graph.decisions.map((decision) => mapDecision(decision, from, to)),
    documents,
    relationships: graph.relationships.map((relationship) =>
      mapRelationship(relationship, from, to)
    ),
    units,
    warnings: graph.warnings.map((warning) => mapWarning(warning, from, to)),
  };
  return {
    destinationVersion: destination.hash,
    from,
    fromVersions: versions,
    graph: relocated,
    reused: isReused,
    to,
  };
};
