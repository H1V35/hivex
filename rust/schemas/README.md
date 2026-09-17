# Knowledge output schemas

These protocol assets preserve the JSON Schema property and required-field order used by the TypeScript runtime in v0.3.11. They were exported with `schemaForKnowledge(z.toJSONSchema(schema))` from the extraction/check schemas in `src/knowledge-model.ts`, the guarded and warning-aware checks and answer schema in `src/knowledge.ts`, and the review schema in `src/review.ts`.

The native output schema is part of each persisted model-input fingerprint. Keep its ordering and constraints stable when changing runtime implementation. Output validation also strips unknown fields and applies the original defaults before retaining a result. The runtime fingerprint and SQLite v1 cache fixtures verify compatibility without model calls.
