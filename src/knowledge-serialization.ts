// Field order is part of the persisted v1 cache format. Keep model inputs stable
// when source formatting changes; unknown future fields remain part of the input.
const fieldOrder = {
  citation: [
    "document",
    "lineStart",
    "lineEnd",
    "version",
    "historical",
    "text",
  ],
  code: ["path", "side", "lineStart", "lineEnd", "version", "text"],
  context: [
    "command",
    "snapshot",
    "documents",
    "unavailableDocuments",
    "unexpandedDecisions",
    "decisions",
    "relationships",
    "pendingDocuments",
    "warnings",
  ],
  decision: [
    "id",
    "document",
    "text",
    "kind",
    "status",
    "conditions",
    "exceptions",
    "reason",
    "lineStart",
    "lineEnd",
    "version",
    "batch",
    "localId",
    "quality",
  ],
  document: [
    "id",
    "title",
    "status",
    "historical",
    "version",
    "lineCount",
    "lines",
  ],
  extraction: [
    "operation",
    "targets",
    "repairReason",
    "units",
    "documents",
    "existing",
    "previousRelationships",
    "scope",
    "extraction",
  ],
  file: ["path", "before", "after"],
  finding: ["target", "reason"],
  implementation: ["baseCommit", "diff", "files", "warnings", "fingerprint"],
  packet: [
    "operation",
    "implementation",
    "task",
    "context",
    "documents",
    "omittedUnits",
    "warnings",
  ],
  queriedDecision: [
    "historical",
    "id",
    "document",
    "version",
    "text",
    "kind",
    "status",
    "quality",
    "conditions",
    "exceptions",
    "reason",
    "evidence",
  ],
  relationship: [
    "id",
    "from",
    "to",
    "type",
    "reason",
    "evidence",
    "batch",
    "localId",
    "quality",
  ],
  reviewFinding: ["assessment", "explanation", "documents", "code"],
  unit: ["id", "document", "hash", "lineStart", "lineEnd", "text"],
  version: ["version", "lines"],
  warning: ["path", "message"],
};

const isRecord = function isRecord(
  value: unknown
): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
};
const hasFields = function hasFields(
  value: Record<string, unknown>,
  fields: string[]
) {
  return fields.every((field) => Object.hasOwn(value, field));
};
const orderRecord = function orderRecord(
  value: Record<string, unknown>,
  order: string[]
) {
  const existing = Object.keys(value);
  const names = new Set(Iterator.concat(order, existing));
  return Object.fromEntries(
    names
      .values()
      .filter((name) => existing.includes(name))
      .map<[string, unknown]>((name) => [name, value[name]])
  );
};

const hasOrderPrefix = function hasOrderPrefix(
  value: Record<string, unknown>,
  fields: string[]
) {
  const keys = Object.keys(value);
  return fields.every((field, index) => keys[index] === field);
};

const sourceOrder = function sourceOrder(value: Record<string, unknown>) {
  if (hasFields(value, ["kind", "id"])) {
    if (Object.hasOwn(value, "historical")) {
      return fieldOrder.queriedDecision;
    }
    // Retained packets may contain live graph records, whose provenance fields
    // were appended in a different order from records decoded by the store.
    return hasOrderPrefix(value, fieldOrder.decision.slice(0, 10))
      ? null
      : fieldOrder.decision;
  }
  if (hasFields(value, ["from", "to", "type"])) {
    return hasOrderPrefix(value, fieldOrder.relationship.slice(0, 6))
      ? null
      : fieldOrder.relationship;
  }
  if (hasFields(value, ["id", "title"])) {
    return fieldOrder.document;
  }
  if (hasFields(value, ["document", "lineStart", "lineEnd"])) {
    return Object.hasOwn(value, "id") ? fieldOrder.unit : fieldOrder.citation;
  }
  if (hasFields(value, ["path", "side"])) {
    return fieldOrder.code;
  }
  if (hasFields(value, ["version", "lines"])) {
    return fieldOrder.version;
  }
  if (hasFields(value, ["path", "message"])) {
    return fieldOrder.warning;
  }
  if (hasFields(value, ["target", "reason"])) {
    return fieldOrder.finding;
  }
  return null;
};

const packetOrder = function packetOrder(
  key: string,
  value: Record<string, unknown>
) {
  if (key === "" && typeof value.operation === "string") {
    const isExtraction =
      value.operation === "extract" || value.operation === "check";
    return isExtraction ? fieldOrder.extraction : fieldOrder.packet;
  }
  if (key === "context" && hasFields(value, ["command", "snapshot"])) {
    return fieldOrder.context;
  }
  if (hasFields(value, ["baseCommit", "files"])) {
    return fieldOrder.implementation;
  }
  if (hasFields(value, ["path", "before", "after"])) {
    return fieldOrder.file;
  }
  return sourceOrder(value);
};

export const stringifyKnowledge = function stringifyKnowledge(packet: unknown) {
  return JSON.stringify(packet, (key: string, value: unknown) => {
    if (!isRecord(value)) {
      return value;
    }
    const order = packetOrder(key, value);
    return order === null ? value : orderRecord(value, order);
  });
};

const schemaOrder = function schemaOrder(properties: Record<string, unknown>) {
  if (hasFields(properties, ["kind", "id"])) {
    return fieldOrder.decision;
  }
  if (hasFields(properties, ["from", "to", "type"])) {
    return fieldOrder.relationship;
  }
  if (hasFields(properties, ["document", "lineStart", "lineEnd"])) {
    return fieldOrder.citation;
  }
  if (hasFields(properties, ["path", "side"])) {
    return fieldOrder.code;
  }
  if (hasFields(properties, ["assessment", "explanation"])) {
    return fieldOrder.reviewFinding;
  }
  if (hasFields(properties, ["target", "reason"])) {
    return fieldOrder.finding;
  }
  return null;
};

const schemaNode = function schemaNode(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map(schemaNode);
  }
  if (!isRecord(value)) {
    return value;
  }
  const result = Object.fromEntries(
    Object.entries(value).map(([key, node]) => [key, schemaNode(node)])
  );
  if (!isRecord(result.properties)) {
    return result;
  }
  const order = schemaOrder(result.properties);
  if (order === null) {
    return result;
  }
  result.properties = orderRecord(result.properties, order);
  if (Array.isArray(result.required)) {
    const { required } = result;
    const names = new Set(Iterator.concat(order, required));
    result.required = [...names].filter((name) => required.includes(name));
  }
  return result;
};

export const schemaForKnowledge = function schemaForKnowledge(
  schema: Record<string, unknown>
) {
  const result = schemaNode(schema);
  if (!isRecord(result)) {
    throw new Error("Expected a JSON schema object");
  }
  return result;
};
