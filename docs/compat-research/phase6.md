# Phase 6 clean-room language and index-maintenance research

Status: public-document syntax review and independently authored FastDB
conformance tests, 2026-08-13. SurrealDB `v3.1.5` remains the immutable
behavioral reference.

## Method and provenance

No SurrealDB source, tests, fixtures, expected-output files, or fuzz corpus was
read, copied, translated, or committed. The Phase 3 reference binary was not
retained in this workspace, so this Phase 6 slice makes no new black-box output
claim. It uses public syntax documentation plus independently authored FastDB
tests and records FastDB-specific output differences explicitly.

Public sources accessed 2026-08-13:

- [SELECT](https://surrealdb.com/docs/reference/query-language/statements/select)
- [EXPLAIN](https://surrealdb.com/docs/reference/query-language/statements/explain)
- [REMOVE](https://surrealdb.com/docs/reference/query-language/statements/remove)
- [REBUILD](https://surrealdb.com/docs/reference/query-language/statements/rebuild)
- [DEFINE INDEX](https://surrealdb.com/docs/reference/query-language/statements/define/indexes)
- [SurrealDB v3.1.5 release](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5)

## Expression projections

The public SELECT reference documents arithmetic and boolean expressions in a
projection with `AS` aliases. FastDB now executes its existing scalar
expression subset in the same projection position. Non-field expressions must
have an explicit alias; unaliased dot paths retain the existing nested-object
shape. `P6-LANG-001` proves evaluation and that aliases do not modify the
stored document. Broader expressions, including function execution, remain
unsupported.

## Explain

The public reference documents `EXPLAIN` as a statement prefix and warns that
its output is informational and subject to change. FastDB accepts only
`EXPLAIN SELECT ...`. It returns ordinary result rows shaped as
`{ ordinal: int, detail: string }` from the pinned Turso query-plan adapter.
This is a deliberate FastDB output contract, not a claim that Turso plan text
matches SurrealDB's operator tree. `ANALYZE`, format selection, arbitrary value
statements, and the trailing `SELECT ... EXPLAIN` clause remain unsupported.

## Index removal and rebuild

The public references document `REMOVE INDEX name ON [TABLE] table` and
`REBUILD INDEX name ON [TABLE] table`. FastDB implements exactly those
blocking forms for its built-in B-tree provider. `IF EXISTS`, `CONCURRENTLY`,
and other resource families remain unsupported. `P6-INDEX-001` proves planner
selection before maintenance, transactional rollback after physical and
catalog boundaries, successful removal, reopen, and integrity.

## Provider syntax boundary

The public DEFINE INDEX reference documents `FULLTEXT ANALYZER` and later
specialized vector providers. FastDB Phase 6 represents namespaced function
calls and provider option values structurally in its independent AST, but does
not execute any specialized provider. `P6-AST-004`, `P6-LANG-002`, and
`P6-CLI-002` prove that these forms fail explicitly before catalog bootstrap.
They remain Unsupported in `COMPAT.md` until their implementation phases have
behavioral conformance evidence.
