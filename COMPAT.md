# FastDB Compatibility Matrix

FastDB implements a documented SurrealQL-compatible subset. This file is the normative feature matrix; parser acceptance alone does not mean a feature executes. The behavioral reference is SurrealDB `v3.1.5`. Compatibility does not imply sponsorship, certification, or complete compatibility; see `CLEAN_ROOM.md`.

## Status legend

- **Supported**: implemented and covered by conformance tests.
- **Partial**: the exact documented subset executes; broader forms are enumerated as deferred or rejected.
- **Planned**: parsed for the MVP contract but not yet executable.
- **Unsupported**: explicitly rejected and outside the MVP target.

Phase 1 keeps the four executable Phase 0 shapes `Partial`. Every broader accepted AST is stopped by the frontend capability gate with a spanned `UnsupportedSyntax` error.

## Values and record IDs

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `VAL-NULL` | Planned | Accept `NULL` case-insensitively as an expression; no alternate null spelling. | `P1-EXPR-001` | [Values](docs/compat-research/phase1.md#values-record-ids-and-expressions) | `P1-BRIDGE-002` (gate) |
| `VAL-BOOL` | Planned | Accept `TRUE` and `FALSE` case-insensitively. | `P1-LEX-003`, `P1-EXPR-001` | [Identifier and keyword case](docs/compat-research/phase1.md#identifiers-and-keyword-case) | `P1-BRIDGE-002` (gate) |
| `VAL-INT` | Planned | Accept decimal signed-`i64` literals; reject overflow and incomplete forms. | `P1-EXPR-001`, `P1-EXPR-005` | [Numbers](docs/compat-research/phase1.md#numbers) | `P1-BRIDGE-002` (gate) |
| `VAL-FLOAT` | Planned | Accept decimal finite `f64` literals with fraction and/or exponent; reject `.5`, `1.`, incomplete exponents, overflow, and non-finite results. | `P1-EXPR-001`, `P1-EXPR-005` | [Numbers](docs/compat-research/phase1.md#numbers) | `P1-BRIDGE-002` (gate) |
| `VAL-STR` | Partial | Parse single- and double-quoted UTF-8 strings with characterized escapes and multiline content. Execution is limited to one single-quoted string in a Phase 0 `CREATE SET` or equality predicate. Doubled single quotes remain accepted only for that legacy seam; invalid escapes fail. | `P1-LEX-002`, `P1-LEX-004` | [Strings and escapes](docs/compat-research/phase1.md#strings-and-escapes) | `P1-BRIDGE-001`, `quoted_semicolon_value_stored_literally_and_schema_unchanged` |
| `VAL-ARRAY` | Planned | Accept ordered expression arrays with one optional trailing comma; reject missing elements. | `P1-EXPR-001`, `P1-STMT-004` | [Collections](docs/compat-research/phase1.md#collections) | `P1-BRIDGE-002` (gate) |
| `VAL-OBJECT` | Planned | Accept ordered key/expression fields, quoted or bare keys, duplicate keys, and one optional trailing comma; reject missing separators. | `P1-EXPR-001`, `P1-STMT-004` | [Collections](docs/compat-research/phase1.md#collections) | `P1-BRIDGE-002` (gate) |
| `VAL-PARAM` | Planned | Accept `$` followed by a Unicode identifier up to the configured UTF-8 byte limit; reject empty or oversized names. | `P1-EXPR-001`, `P1-LIMIT-006` | [Phase 1 interpretation](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-002` (gate) |
| `RID-BARE` | Partial | Parse `table:bare` preserving UTF-8 spelling. Execution supports one bare component in the four Phase 0 shapes. | `P1-LEX-003`, `P1-EXPR-002` | [Record-ID components](docs/compat-research/phase1.md#record-id-components) | `P1-BRIDGE-001` |
| `RID-QUOTED` | Planned | Accept ``table:`complex UTF-8 text` ``; reject single- or double-quoted value strings as ID components. | `P1-EXPR-002` | [Record-ID components](docs/compat-research/phase1.md#record-id-components) | `P1-BRIDGE-002` (gate) |
| `RID-INT` | Planned | Accept `table:<signed-i64>` including both boundaries; reject overflow. | `P1-EXPR-002`, `P1-EXPR-005` | [Record-ID components](docs/compat-research/phase1.md#record-id-components) | `P1-BRIDGE-002` (gate) |
| `RID-GENERATED` | Planned | `CREATE table ...` represents an omitted ID explicitly for later UUIDv7 generation; Phase 1 never generates it. | `P1-STMT-001` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `RID-COMPLEX` | Unsupported | Array/object IDs and record ranges are rejected. | `P1-EXPR-005` | [Phase 1 interpretation](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-003` |

## Expressions and paths

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `EXPR-PATH` | Planned | Accept dot-separated Unicode field segments with per-segment spans; no indexing or graph traversal. A single field executes only in the Phase 0 equality shape. | `P1-EXPR-001`, `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-001`, `P1-BRIDGE-002` |
| `EXPR-PAREN` | Planned | Accept parenthesized expressions and retain the parentheses node/span. | `P1-EXPR-004`, `P1-LIMIT-003` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-002` (gate) |
| `OP-UNARY` | Planned | Accept keyword `NOT` and unary `+`/`-`; reject symbolic `!`. | `P1-EXPR-004`, `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-002` (gate) |
| `OP-ARITH` | Planned | Accept left-associative `*`, `/`, `+`, `-`; reject `%` and `**`. | `P1-EXPR-003`, `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-002` (gate) |
| `OP-REL` | Planned | Accept `<`, `<=`, `>`, `>=` below arithmetic precedence. | `P1-EXPR-003` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-002` (gate) |
| `OP-EQ` | Partial | Parse `=` and `!=`; reject `==`, `IS`, and broader comparisons. Execution supports only one `path = 'string'` table predicate. | `P1-EXPR-003`, `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-001`, `P1-BRIDGE-002` |
| `OP-BOOL` | Planned | Accept left-associative keyword `AND` then `OR`; reject `&&` and `\|\|`. | `P1-EXPR-003`, `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-002` (gate) |
| `EXPR-EXCLUDED` | Unsupported | Functions, casts, subqueries, traversal, ranges, indexing, modulo, power, and broader comparison operators fail explicitly. | `P1-EXPR-005` | [Binding power and exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-003` |

## Statements and clauses

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `STMT-CREATE` | Partial | Parse `CREATE [ONLY] table[:id] (CONTENT expr \| SET path = expr [, ...]) [RETURN AFTER \| NONE \| BEFORE]`. Execute only `CREATE table:bare SET field = 'string'`. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-001`, `P1-BRIDGE-002` |
| `STMT-SELECT` | Partial | Parse exact wildcard or named projection form with `FROM [ONLY] target`, optional `WHERE`, `ORDER BY`, `LIMIT`, then `START`. Execute only record wildcard and table wildcard plus one string equality predicate. | `P1-STMT-001`, `P1-STMT-002`, `P1-STMT-004`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-001`, `P1-BRIDGE-002` |
| `STMT-UPDATE` | Planned | Parse `UPDATE target SET assignments [WHERE expr] [RETURN AFTER \| NONE]`; reject other return modes and mutation operators. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `STMT-DELETE` | Partial | Parse `DELETE target [WHERE expr] [RETURN BEFORE]`; execute only `DELETE table:bare`. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-001`, `P1-BRIDGE-002` |
| `CLAUSE-ONLY` | Planned | Accept only after `CREATE` or `FROM`; retain it structurally; reject misplaced/duplicate use. | `P1-STMT-001`, `P1-STMT-004` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `CLAUSE-CONTENT-SET` | Partial | `CREATE` requires exactly one of `CONTENT` or `SET`; `UPDATE` requires `SET`; comma lists are bounded. Only one flat string `CREATE SET` assignment executes. | `P1-STMT-001`, `P1-STMT-004`, `P1-LIMIT-004` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-001`, `P1-BRIDGE-002` |
| `CLAUSE-PROJECTION` | Planned | Accept `*` alone or `path [AS alias] [, ...]`; never mix `*` with named fields. | `P1-STMT-001`, `P1-STMT-002`, `P1-STMT-004` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `CLAUSE-WHERE` | Partial | Parse one expression in `SELECT`, `UPDATE`, and `DELETE` at the fixed position. Only Phase 0 string equality on a table target executes. | `P1-STMT-001`, `P1-STMT-004` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-001`, `P1-BRIDGE-002` |
| `CLAUSE-ORDER` | Planned | Accept `ORDER BY path [ASC \| DESC] [, ...]`; default ascending is structural. | `P1-STMT-001`, `P1-STMT-002` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `CLAUSE-PAGE` | Planned | Accept `LIMIT` then `START` with decimal integers in `0..=i64::MAX`; reject negatives, fractions, overflow, duplicates, and reordering. | `P1-STMT-004`, `P1-STMT-006` | [Numbers](docs/compat-research/phase1.md#numbers) | `P1-BRIDGE-002` (gate) |
| `CLAUSE-RETURN` | Planned | CREATE: `AFTER`, `NONE`, `BEFORE`; UPDATE: `AFTER`, `NONE`; DELETE: `BEFORE`; reject all other combinations. | `P1-STMT-001`, `P1-STMT-006` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `SCRIPT-MULTI` | Planned | Accept ordered nonempty statements separated by one semicolon and one optional trailing semicolon; reject empty statements and missing separators. `parse_one` requires exactly one. | `P1-STMT-003`, `P1-LIMIT-005` | [Phase 1 interpretation](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-003` |

## Schema and transactions

| Feature ID | Status | Exact accepted / rejected syntax | Phase 1 parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `SCHEMA-TABLE` | Planned | Accept `DEFINE TABLE name SCHEMALESS` or `SCHEMAFULL` only. | `P1-STMT-001` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `SCHEMA-FIELD` | Planned | Accept `DEFINE FIELD path ON [TABLE] name TYPE type`. | `P1-STMT-001` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `SCHEMA-INDEX` | Planned | Accept `DEFINE INDEX name ON [TABLE] name FIELDS path [, ...] [UNIQUE]`; reject full-text and other index kinds. The Phase 0 test helper is not this syntax. | `P1-STMT-001`, `P1-STMT-005` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002`; `expression_index_selected_before_and_after_reopen` (test helper only) |
| `TYPE-BASE` | Planned | Accept case-insensitive `bool`, `int`, `float`, `number`, `string`, `object`, `array`, and `record`. | `P1-STMT-001` | [Statements and clauses](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-002` (gate) |
| `TYPE-OPTION` | Planned | Accept recursive `option<T>` within the nesting budget; reject missing delimiters/arguments. | `P1-STMT-001`, `P1-LIMIT-003` | [Phase 1 interpretation](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-002` (gate) |
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

Every row retains a stable feature ID, exact syntax boundary, parser test ID, allowed-source provenance, and execution/conformance evidence when any exists. A row can move from `Planned` only when its accepted form reaches the frontend and its rejected boundary is tested. Inherited Turso SQLite and PostgreSQL compatibility documents remain separate under `docs/upstream-turso-sqlite-compat.md` and `postgres/COMPAT.md`.
