# FastDB Compatibility Matrix

FastDB implements a documented SurrealQL-compatible subset. This file is the normative feature matrix; parser acceptance alone does not mean a feature executes. The behavioral reference is SurrealDB `v3.1.5`. Compatibility does not imply sponsorship, certification, or complete compatibility; see `CLEAN_ROOM.md`.

## Status legend

- **Supported**: implemented and covered by conformance tests.
- **Partial**: the exact documented subset executes; broader forms are enumerated as deferred or rejected.
- **Planned**: parsed for the MVP contract but not yet executable.
- **Unsupported**: explicitly rejected and outside the MVP target.

Phase 2 executes the exact catalog, schema, index, constant-CREATE, constrained-SELECT, and legacy DELETE slice documented below. Parser-only Phase 3 forms remain `Planned` and are stopped by the frontend capability gate with a spanned `UnsupportedSyntax` error.

## Values and record IDs

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `VAL-NULL` | Partial | Execute `NULL` only as a constant CREATE value or scalar equality operand; no alternate spelling. Declared fields reject null. | `P1-EXPR-001` | [Null and schema probes](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-BRIDGE-002`, `P2-SCHEMA-003` |
| `VAL-BOOL` | Partial | Execute case-insensitive `TRUE` and `FALSE` as constant values and scalar equality operands. | `P1-LEX-003`, `P1-EXPR-001` | [Identifier and keyword case](docs/compat-research/phase1.md#identifiers-and-keyword-case) | `P2-BRIDGE-002` |
| `VAL-INT` | Partial | Execute decimal signed-`i64` constants, including both boundaries; reject overflow and incomplete forms. | `P1-EXPR-001`, `P1-EXPR-005` | [Numeric coercion](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-BRIDGE-002`, `P2-SCHEMA-003` |
| `VAL-FLOAT` | Partial | Execute finite `f64` constants with fraction and/or exponent; reject non-finite and incomplete forms. | `P1-EXPR-001`, `P1-EXPR-005` | [Numeric coercion](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-BRIDGE-002`, `P2-SCHEMA-003` |
| `VAL-STR` | Partial | Execute characterized single- and double-quoted UTF-8 strings as constants and scalar equality operands. Doubled single quotes remain a documented legacy extension. | `P1-LEX-002`, `P1-LEX-004` | [Strings and escapes](docs/compat-research/phase1.md#strings-and-escapes) | `P2-BRIDGE-001`, `P2-BRIDGE-002` |
| `VAL-ARRAY` | Partial | Execute constant arrays with one optional trailing comma; elements must themselves be constants. | `P1-EXPR-001`, `P1-STMT-004` | [Collections](docs/compat-research/phase1.md#collections) | `P2-BRIDGE-002` |
| `VAL-OBJECT` | Partial | Execute constant objects with quoted/bare keys and one optional trailing comma; duplicate keys normalize last-value-wins in lexicographic key order. | `P1-EXPR-001`, `P1-STMT-004` | [Duplicate object keys](docs/compat-research/phase2.md#uuid-record-ids-objects-and-reserved-id) | `P2-BRIDGE-002`, `P2-BRIDGE-004` |
| `VAL-PARAM` | Planned | Accept `$` followed by a Unicode identifier up to the configured UTF-8 byte limit; reject empty or oversized names. | `P1-EXPR-001`, `P1-LIMIT-006` | [Phase 1 interpretation](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-002` (gate) |
| `RID-BARE` | Partial | Execute `table:bare` with UTF-8 spelling preserved in the Phase 2 CREATE/SELECT/DELETE slice. | `P1-LEX-003`, `P1-EXPR-002` | [Record-ID components](docs/compat-research/phase1.md#record-id-components) | `P2-BRIDGE-001` |
| `RID-QUOTED` | Partial | Execute ``table:`complex UTF-8 text` ``; single/double-quoted value strings are not ID components. | `P1-EXPR-002` | [Record-ID components](docs/compat-research/phase1.md#record-id-components) | `P2-BRIDGE-001` |
| `RID-INT` | Partial | Execute `table:<signed-i64>` including both boundaries; reject overflow and noncanonical stored integers. | `P1-EXPR-002`, `P1-EXPR-005` | [Record-ID components](docs/compat-research/phase1.md#record-id-components) | `P2-BRIDGE-001`, `P2-CAT-006` |
| `RID-UUID` | Partial | Execute adjacent `u'…'` and `u"…"` canonical lowercase hyphenated UUIDv4/v7 components; reject other versions/forms. | `P2-UUID-001`, `P2-UUID-002` | [UUID record IDs](docs/compat-research/phase2.md#uuid-record-ids-objects-and-reserved-id) | `P2-UUID-001`, `P2-BRIDGE-001` |
| `RID-GENERATED` | Partial | An omitted CREATE ID generates a UUIDv7 component whose rendered form is directly source-addressable. | `P1-STMT-001` | [UUID record IDs](docs/compat-research/phase2.md#uuid-record-ids-objects-and-reserved-id) | `P2-BRIDGE-001` |
| `RID-COMPLEX` | Unsupported | Array/object IDs and record ranges are rejected. | `P1-EXPR-005` | [Phase 1 interpretation](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-003` |

## Expressions and paths

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `EXPR-PATH` | Partial | Execute dot-separated Unicode paths in one SET assignment, schema/index definitions, and equality filters; indexing and traversal remain excluded. | `P1-EXPR-001`, `P1-EXPR-005` | [Nested schema paths](docs/compat-research/phase2.md#nested-paths-and-duplicate-definitions) | `P2-BRIDGE-002`, `P2-SCHEMA-003`, `P2-IDX-001` |
| `EXPR-PAREN` | Partial | Parentheses execute only around constant CREATE values and preserve source spans. | `P1-EXPR-004`, `P1-LIMIT-003` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P2-BRIDGE-002`, `P2-BRIDGE-003` |
| `OP-UNARY` | Partial | Unary `+`/`-` execute only on numeric constants; keyword `NOT` remains capability-gated and symbolic `!` is rejected. | `P1-EXPR-004`, `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P2-BRIDGE-002`, `P2-BRIDGE-003` |
| `OP-ARITH` | Planned | Accept left-associative `*`, `/`, `+`, `-`; reject `%` and `**`. | `P1-EXPR-003`, `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-002` (gate) |
| `OP-REL` | Planned | Accept `<`, `<=`, `>`, `>=` below arithmetic precedence. | `P1-EXPR-003` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-002` (gate) |
| `OP-EQ` | Partial | Execute only `path = scalar` predicates. Parsed `!=` and broader predicate shapes remain gated; `==` and `IS` are rejected. | `P1-EXPR-003`, `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P2-BRIDGE-001`, `P2-BRIDGE-003` |
| `OP-BOOL` | Partial | Execute `AND` only as a join of path/scalar equalities. `OR`, `&&`, and `\|\|` do not execute. | `P1-EXPR-003`, `P1-EXPR-005` | [Composite indexes](docs/compat-research/phase2.md#unique-null-missing-and-composite-indexes) | `P2-IDX-001`, `P2-BRIDGE-003` |
| `EXPR-EXCLUDED` | Unsupported | Functions, casts, subqueries, traversal, ranges, indexing, modulo, power, and broader comparison operators fail explicitly. | `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-003` |

## Statements and clauses

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `STMT-CREATE` | Partial | Execute `CREATE table[:id] CONTENT <constant-object>` or exactly one `SET path = <constant>` without `ONLY`/`RETURN`; all broader parsed forms are gated. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P2-BRIDGE-001`, `P2-BRIDGE-003` |
| `STMT-SELECT` | Partial | Execute `SELECT * FROM table[:id]` with optional path/scalar equalities joined by `AND`; projection, ONLY, ordering, and pagination remain gated. | `P1-STMT-001`, `P1-STMT-002`, `P1-STMT-004`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P2-BRIDGE-001`, `P2-IDX-001`, `P2-BRIDGE-003` |
| `STMT-UPDATE` | Planned | Parse `UPDATE target SET assignments [WHERE expr] [RETURN AFTER \| NONE]`; reject other return modes and mutation operators. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `STMT-DELETE` | Partial | Execute only the legacy bare-record `DELETE table:id` form; table targets, WHERE, and RETURN remain gated. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P2-BRIDGE-001`, `P2-BRIDGE-003` |
| `CLAUSE-ONLY` | Planned | Accept only after `CREATE` or `FROM`; retain it structurally; reject misplaced/duplicate use. | `P1-STMT-001`, `P1-STMT-004` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `CLAUSE-CONTENT-SET` | Partial | CREATE executes a constant object CONTENT or exactly one nested constant SET assignment. Multiple assignments and every UPDATE SET remain gated. | `P1-STMT-001`, `P1-STMT-004`, `P1-LIMIT-004` | [Duplicate object keys](docs/compat-research/phase2.md#uuid-record-ids-objects-and-reserved-id) | `P2-BRIDGE-001`, `P2-BRIDGE-002`, `P2-BRIDGE-003` |
| `CLAUSE-PROJECTION` | Planned | Accept `*` alone or `path [AS alias] [, ...]`; never mix `*` with named fields. | `P1-STMT-001`, `P1-STMT-002`, `P1-STMT-004` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `CLAUSE-WHERE` | Partial | Execute SELECT path/scalar equality predicates joined only by `AND`; UPDATE/DELETE WHERE and broader expressions remain gated. | `P1-STMT-001`, `P1-STMT-004` | [Composite indexes](docs/compat-research/phase2.md#unique-null-missing-and-composite-indexes) | `P2-BRIDGE-001`, `P2-IDX-001`, `P2-BRIDGE-003` |
| `CLAUSE-ORDER` | Planned | Accept `ORDER BY path [ASC \| DESC] [, ...]`; default ascending is structural. | `P1-STMT-001`, `P1-STMT-002` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `CLAUSE-PAGE` | Planned | Accept `LIMIT` then `START` with decimal integers in `0..=i64::MAX`; reject negatives, fractions, overflow, duplicates, and reordering. | `P1-STMT-004`, `P1-STMT-006` | [Numbers](docs/compat-research/phase1.md#numbers) | `P1-BRIDGE-002` (gate) |
| `CLAUSE-RETURN` | Planned | CREATE: `AFTER`, `NONE`, `BEFORE`; UPDATE: `AFTER`, `NONE`; DELETE: `BEFORE`; reject all other combinations. | `P1-STMT-001`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `SCRIPT-MULTI` | Planned | Accept ordered nonempty statements separated by one semicolon and one optional trailing semicolon; reject empty statements and missing separators. `parse_one` requires exactly one. | `P1-STMT-003`, `P1-LIMIT-005` | [Phase 1 interpretation](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-003` |

## Schema and transactions

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `SCHEMA-TABLE` | Supported | Execute the exact `DEFINE TABLE name SCHEMALESS` or `SCHEMAFULL` grammar; duplicates are logical constraints. | `P1-STMT-001` | [Duplicate definitions](docs/compat-research/phase2.md#nested-paths-and-duplicate-definitions) | `P2-BRIDGE-001`, `P2-SCHEMA-006` |
| `SCHEMA-FIELD` | Supported | Execute `DEFINE FIELD path ON [TABLE] name TYPE type`, requiring an existing table and validating existing rows atomically. | `P1-STMT-001` | [Fields and coercion](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-SCHEMA-003`, `P2-SCHEMA-005`, `P2-SCHEMA-006` |
| `SCHEMA-INDEX` | Supported | Execute ordered scalar `DEFINE INDEX name ON [TABLE] name FIELDS path [, ...] [UNIQUE]`; excluded index kinds remain rejected. | `P1-STMT-001`, `P1-STMT-005` | [Unique and composite indexes](docs/compat-research/phase2.md#unique-null-missing-and-composite-indexes) | `P2-IDX-001`, `P2-IDX-002`, `P2-IDX-003`, `P2-IDX-004` |
| `TYPE-BASE` | Supported | Enforce case-insensitive `bool`, `int`, `float`, `number`, `string`, `object`, `array`, and `record`; null is not a member of any base type. | `P1-STMT-001` | [Fields and coercion](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-SCHEMA-003`, `P2-SCHEMA-004` |
| `TYPE-OPTION` | Supported | Enforce recursive `option<T>` within the nesting budget: absence is permitted but null is not. | `P1-STMT-001`, `P1-LIMIT-003` | [Fields and coercion](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-SCHEMA-003` |
| `TXN-BEGIN` | Planned | Accept bare `BEGIN`; reject `BEGIN TRANSACTION` in the fixed MVP grammar. | `P1-STMT-001`, `P1-STMT-005` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `TXN-COMMIT` | Planned | Accept bare `COMMIT`; reject the optional reference suffix. | `P1-STMT-001`, `P1-STMT-005` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `TXN-CANCEL` | Planned | Accept bare `CANCEL`; reject the optional reference suffix. | `P1-STMT-001`, `P1-STMT-005` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |

## Explicit exclusions

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `EXCL-STMT` | Unsupported | `INSERT`, `UPSERT`, `RELATE`, `LET`, `REMOVE`, `INFO`, `USE`, `LIVE`, `SHOW`, `SLEEP`, `THROW`, `FOR`, and `IF` statement families fail at their introducer. | `P1-STMT-005` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-003` |
| `EXCL-CLAUSE` | Unsupported | `TIMEOUT`, `FETCH`, `GROUP`, `SPLIT`, `OMIT`, `EXPLAIN`, `WITH`, `VALUE`, `MERGE`, `PATCH`, `PARALLEL`, and excluded index kinds fail at their introducer. | `P1-STMT-005` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-003` |
| `EXCL-GRAPH` | Unsupported | Graph traversal, relation statements, complex record IDs, and ranges are not accepted. | `P1-EXPR-005`, `P1-STMT-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-003` |
| `EXCL-ADVANCED` | Unsupported | Functions, casts, subqueries, permissions, users, namespaces, scopes, events, analyzers, views, live queries, full-text/vector/count indexes, and arbitrary native extensions are outside the MVP. | `P1-EXPR-005`, `P1-STMT-005` | [Phase 1 interpretation](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-003` |

## Maintenance rule

Every row retains a stable feature ID, exact syntax boundary, parser test ID, allowed-source provenance, and execution/conformance evidence when any exists. A row can move from `Planned` only when its accepted form reaches the frontend and its rejected boundary is tested. Phase 2 evidence is checked mechanically by `P2-COMPAT-001`. Inherited Turso SQLite and PostgreSQL compatibility documents remain separate under `docs/upstream-turso-sqlite-compat.md` and `postgres/COMPAT.md`.
