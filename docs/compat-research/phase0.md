# Phase 0 Compatibility Research Notes

Provenance notes for the Phase 0 SurrealQL subset, per `CLEAN_ROOM.md`.
Behavioral reference: **SurrealDB `v3.1.5`**.

> **Provenance disclaimer:** the notes below are derived from public
> SurrealQL documentation (source kind `public-doc`). No SurrealDB source
> or test files were read, copied, translated, or vendored. Live
> `black-box` observations against an unmodified `v3.1.5` binary should be
> appended when available to strengthen these rows; their absence does not
> change the Phase 0 contract, which is the small slice enumerated below.

## Public documentation references

- CREATE: https://surrealdb.com/docs/reference/query-language/statements/create
- SELECT: https://surrealdb.com/docs/reference/query-language/statements/select
- DELETE: https://surrealdb.com/docs/reference/query-language/statements/delete
- Record IDs: https://surrealdb.com/docs/reference/data-types/recordid
- Reference version: SurrealDB `v3.1.5`
- Access date: 2026-08-12

## `CREATE-001`: create a record with an explicit id

- Source kind: public-doc
- Reference: CREATE docs (above); Record IDs docs (above).
- Input: `CREATE person:tracy SET name = 'Tracy';`
- Expected output: the created record, logically
  `{ id: person:tracy, name: 'Tracy' }`. The `id` is a typed record id,
  not the string `"person:tracy"`.
- Notes (Phase 0 narrowing): SurrealQL permits many `CREATE` forms
  (omitted id with generated values, `CONTENT`, `RETURN`, `ONLY`, multiple
  assignments, non-string values). Phase 0 implements only the single
  form above; every other form is rejected with an explicit error.
- FastDB behavior: implemented (Phase 0 slice).
- Date: 2026-08-12

## `SELECT-001`: select a record by id

- Source kind: public-doc
- Reference: SELECT docs (above).
- Input: `SELECT * FROM person:tracy;`
- Expected output: an array containing the record when present, otherwise
  an empty array. Record targets return arrays by default.
- FastDB behavior: implemented (Phase 0 slice).
- Date: 2026-08-12

## `SELECT-002`: select by equality filter

- Source kind: public-doc
- Reference: SELECT docs (above).
- Input: `SELECT * FROM person WHERE name = 'Tracy';`
- Expected output: an array of matching records.
- Notes (Phase 0 narrowing): SurrealQL permits arbitrary `WHERE`
  expressions, projections, ordering, and pagination. Phase 0 implements
  only a single equality predicate on one string field. Once the Phase 0
  expression index on `name` is installed, the predicate is expected to
  use that index (proven by an execution-plan test, not timing).
- FastDB behavior: implemented (Phase 0 slice).
- Date: 2026-08-12

## `DELETE-001`: delete a record by id

- Source kind: public-doc
- Reference: DELETE docs (above).
- Input: `DELETE person:tracy;`
- Expected output: the default empty result. A missing record target
  succeeds with zero deleted records.
- FastDB behavior: implemented (Phase 0 slice).
- Date: 2026-08-12

## String escaping rule (Phase 0 choice)

- Source kind: fastdb-choice (informed by SQL/SurrealQL single-quote
  convention).
- Rule: single-quoted strings; a single quote inside the string is
  encoded as two consecutive single quotes (`''`). Backslash is a literal
  backslash (no C-style escapes) for Phase 0. A backslash is **not** an
  escape character in Phase 0, matching the simplest SQL string-literal
  rule. This is documented as a Phase 0 choice; broader SurrealQL string
  semantics are deferred.
- Test implications: `'O''Brien'` decodes to `O'Brien`; `'; DROP--'` is
  data, not a statement boundary; an unterminated quote is a parse error.
- Date: 2026-08-12

## FastDB-specific choices (not derived from SurrealDB)

- Physical storage layout (`rid`, `doc JSONB`), opaque physical names,
  catalog shape, and the canonical JSON extraction expression are
  FastDB-internal storage decisions, not SurrealQL behavioral targets.
  They are documented in `plan-phase0.md` and the engine audit, not here.
