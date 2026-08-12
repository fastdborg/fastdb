# FastDB Compatibility Matrix

FastDB implements a documented **subset** of SurrealQL. This matrix is
normative for language support. Each entry has exactly one status.

## Status legend

- **Supported** — implemented and covered by conformance tests.
- **Partial** — a documented subset is implemented; accepted and rejected
  forms are enumerated.
- **Planned** — intentionally absent from the current release but on the
  roadmap.
- **Unsupported** — not planned for the stated compatibility target.

## Reference

- Behavioral reference: **SurrealDB `v3.1.5`**.
- "SurrealQL-compatible subset" does not imply sponsorship, certification,
  or complete compatibility. See `CLEAN_ROOM.md`.

## Phase 0 status (disposable spike)

Phase 0 is a feasibility spike, **not** the public compatibility
contract. The four forms below are the entire Phase 0 user-facing slice.
All other grammar is `planned` or `unsupported`; **no** broad
statement-family row is fully supported.

| Feature ID | Statement family | Status | Phase 0 accepted form | Rejected / deferred |
| --- | --- | --- | --- | --- |
| `CREATE-001` | `CREATE` | Partial | `CREATE <table>:<id> SET <field> = '<string>';` (bare-string record id, one string-valued assignment) | Omitted id (generated ids), `CONTENT`, multiple `SET`, `RETURN`, `ONLY`, non-string values, nested paths, additional clauses → explicit error |
| `SELECT-001` | `SELECT` (record) | Partial | `SELECT * FROM <table>:<id>;` | `ONLY`, field projection, aliases, `WHERE` with record target, `ORDER BY`, `LIMIT`, `START`, `FETCH` → explicit error |
| `SELECT-002` | `SELECT` (equality filter) | Partial | `SELECT * FROM <table> WHERE <field> = '<string>';` | Non-equality operators, non-string RHS, multiple predicates, ordering, pagination → explicit error |
| `DELETE-001` | `DELETE` | Partial | `DELETE <table>:<id>;` | `WHERE` clauses, `RETURN BEFORE`, record ranges → explicit error |

## Values and expressions

| Feature ID | Feature | Status | Notes |
| --- | --- | --- | --- |
| `VAL-STR` | Single-quoted string | Partial | Phase 0 supports single-quoted strings with the documented escaping rule; other literals not accepted. |
| `VAL-NULL` | `null` | Unsupported (Phase 0) | Planned. |
| `VAL-NUM` | Numbers | Unsupported (Phase 0) | Planned. |
| `VAL-BOOL` | Booleans | Unsupported (Phase 0) | Planned. |
| `VAL-ARR` | Arrays | Unsupported (Phase 0) | Planned. |
| `VAL-OBJ` | Objects | Unsupported (Phase 0) | Planned. |
| `OP-CMP` | Comparison operators | Partial | Phase 0 supports only `=` (equality) on a single string field. |
| `OP-LOGIC` | `AND`/`OR`/`NOT` | Unsupported (Phase 0) | Planned. |

## Statements not in Phase 0

| Statement | Status |
| --- | --- |
| `UPDATE` | Planned |
| `DEFINE TABLE` / `DEFINE FIELD` / `DEFINE INDEX` | Planned (Phase 0 exposes a test-only index helper, not public `DEFINE INDEX`) |
| `BEGIN` / `COMMIT` / `CANCEL` (explicit transactions) | Planned |
| `RELATE` / graph | Unsupported |
| `INSERT` / `UPSERT` / `MERGE` / `PATCH` | Unsupported |
| `LET`, functions, subqueries | Unsupported |
| Permissions, users, namespaces, scopes, events, analyzers, views | Unsupported |
| Live queries | Unsupported |

## Record IDs

| Feature ID | Feature | Status | Notes |
| --- | --- | --- | --- |
| `RID-STR` | Bare string record id (`table:identifier`) | Partial | Phase 0 supports a single bare-string id component. |
| `RID-INT` | Integer record id | Unsupported (Phase 0) | Planned. |
| `RID-GEN` | Generated (UUIDv7) record id | Unsupported (Phase 0) | Planned. |
| `RID-ARR` / `RID-OBJ` | Array/object record ids | Unsupported | |

## Indexes

| Feature ID | Feature | Status | Notes |
| --- | --- | --- | --- |
| `IDX-FIELD` | Non-unique expression index on a field | Partial | Phase 0 proves one canonical expression index on `name` via a test-only helper; public `DEFINE INDEX` is Planned. |
| `IDX-UNIQUE` | Unique index | Unsupported (Phase 0) | Planned. |
| `IDX-COMPOSITE` | Composite index | Unsupported (Phase 0) | Planned. |
| `IDX-FTS` / `IDX-VECTOR` / `IDX-COUNT` | Full-text / vector / count | Unsupported |

## How this matrix is maintained

- Every `Supported`/`Partial` row must reference a provenance note under
  `docs/compat-research/` and a conformance test (see `CLEAN_ROOM.md`).
- Accepted forms must execute; rejected forms must return an explicit
  `UnsupportedSyntax` (or parse) error and never be silently ignored.
- This matrix expands one independently tested feature at a time in later
  phases. Phase 0 does not claim MVP completeness.

## Note on inherited upstream documentation

The repository root previously held Turso's *SQLite* compatibility
matrix (inherited from the upstream engine). That document was relocated
to `docs/upstream-turso-sqlite-compat.md` to avoid collision with this
FastDB matrix and to preserve its content and provenance. Turso's
*PostgreSQL* compatibility matrix remains at `postgres/COMPAT.md`.
