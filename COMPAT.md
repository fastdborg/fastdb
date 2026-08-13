# FastDB Compatibility Matrix

FastDB implements a documented SurrealQL-compatible subset. This file is the normative feature matrix; parser acceptance alone does not mean a feature executes. The behavioral reference is SurrealDB `v3.1.5`. Compatibility does not imply sponsorship, certification, or complete compatibility; see `CLEAN_ROOM.md`.

## Status legend

- **Supported**: the exact row executes and is covered by conformance tests.
- **Partial**: the documented subset executes, while broader SurrealQL forms remain explicitly rejected.
- **Planned**: reserved for parsed behavior that has not reached execution.
- **Unsupported**: explicitly rejected and outside the MVP target.

Phase 5 hardens the complete MVP expression, parameter, CRUD, result, ordered-script, schema, index, transaction, asynchronous Rust API, and CLI surface. Anything not listed as Supported or Partial remains explicitly outside this release candidate.

## Values and record IDs

| Feature ID | Status | Exact accepted / rejected syntax | Parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `VAL-NULL` | Partial | Execute `NULL` in every value expression; missing is internal and distinct. Declared fields reject null. | `P1-EXPR-001` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001`, `P2-SCHEMA-003` |
| `VAL-BOOL` | Partial | Execute case-insensitive `TRUE` and `FALSE` in every value expression. | `P1-LEX-003`, `P1-EXPR-001` | [Phase 1 keyword case](docs/compat-research/phase1.md#identifiers-and-keyword-case) | `P3-EXPR-001`, `P2-BRIDGE-002` |
| `VAL-INT` | Partial | Execute decimal signed-`i64` values with checked arithmetic; reject literal or result overflow. | `P1-EXPR-001`, `P1-EXPR-005` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001`, `P2-SCHEMA-003` |
| `VAL-FLOAT` | Partial | Execute finite `f64` values and numeric promotion; reject non-finite input or results. | `P1-EXPR-001`, `P1-EXPR-005` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001`, `P2-SCHEMA-003` |
| `VAL-STR` | Partial | Execute characterized single- and double-quoted UTF-8 strings; doubled single quotes remain a documented extension. | `P1-LEX-002`, `P1-LEX-004` | [Phase 1 strings](docs/compat-research/phase1.md#strings-and-escapes) | `P3-EXPR-001`, `P2-BRIDGE-002` |
| `VAL-ARRAY` | Partial | Execute recursively bounded arrays with one optional trailing comma. | `P1-EXPR-001`, `P1-STMT-004` | [Phase 3 parameter contract](docs/compat-research/phase3.md#parameter-map-contract) | `P3-EXPR-001`, `P3-PARAM-001` |
| `VAL-OBJECT` | Partial | Execute recursively bounded objects; duplicate keys and projection collisions resolve last-write-wins. | `P1-EXPR-001`, `P1-STMT-004` | [Phase 3 projections](docs/compat-research/phase3.md#result-and-projection-shapes) | `P3-EXPR-001`, `P2-BRIDGE-004` |
| `VAL-PARAM` | Supported | Accept `$` plus a bounded Unicode identifier in value positions only; API map keys omit `$` and are case-sensitive. | `P1-EXPR-001`, `P1-LIMIT-006` | [Parameter-map contract](docs/compat-research/phase3.md#parameter-map-contract) | `P3-PARAM-001` |
| `RID-BARE` | Partial | Execute `table:bare` with UTF-8 spelling preserved across every CRUD target and value position. | `P1-LEX-003`, `P1-EXPR-002` | [Phase 1 record IDs](docs/compat-research/phase1.md#record-id-components) | `P3-CRUD-001`, `P2-BRIDGE-001` |
| `RID-QUOTED` | Partial | Execute backtick-quoted record components; ordinary value strings are not ID components. | `P1-EXPR-002` | [Phase 1 record IDs](docs/compat-research/phase1.md#record-id-components) | `P3-PARAM-001`, `P2-BRIDGE-001` |
| `RID-INT` | Partial | Execute signed-`i64` components including both boundaries; reject overflow and noncanonical stored integers. | `P1-EXPR-002`, `P1-EXPR-005` | [Phase 1 record IDs](docs/compat-research/phase1.md#record-id-components) | `P3-CRUD-002`, `P2-CAT-006` |
| `RID-UUID` | Partial | Execute adjacent canonical lowercase UUIDv4/v7 components; reject other versions/forms. | `P2-UUID-001`, `P2-UUID-002` | [Phase 2 UUID probes](docs/compat-research/phase2.md#uuid-record-ids-objects-and-reserved-id) | `P3-PARAM-001`, `P2-UUID-001` |
| `RID-GENERATED` | Partial | An omitted CREATE ID generates a source-addressable UUIDv7 component. | `P1-STMT-001` | [Phase 2 UUID probes](docs/compat-research/phase2.md#uuid-record-ids-objects-and-reserved-id) | `P3-RESULT-001`, `P2-BRIDGE-001` |
| `RID-COMPLEX` | Unsupported | Array/object IDs and record ranges are rejected. | `P1-EXPR-005` | [Phase 1 exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-003` |

## Expressions and paths

| Feature ID | Status | Exact accepted / rejected syntax | Parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `EXPR-PATH` | Partial | Execute dot-separated Unicode paths plus virtual top-level `id`; indexing and traversal are excluded. | `P1-EXPR-001`, `P1-EXPR-005` | [Virtual id and SET](docs/compat-research/phase3.md#set-evaluation-and-conflicting-assignments) | `P3-CRUD-001`, `P3-CRUD-002` |
| `EXPR-PAREN` | Partial | Parentheses execute recursively within the configured nesting limit. | `P1-EXPR-004`, `P1-LIMIT-003` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001`, `P2-BRIDGE-002` |
| `OP-UNARY` | Partial | Unary `+`, `-`, and keyword `NOT` execute; symbolic `!` remains rejected by the independent grammar. | `P1-EXPR-004`, `P1-EXPR-005` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001`, `P2-BRIDGE-003` |
| `OP-ARITH` | Partial | Execute checked left-associative `*`, `/`, `+`, `-`; reject `%` and `**`. | `P1-EXPR-003`, `P1-EXPR-005` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001` |
| `OP-REL` | Partial | Execute `<`, `<=`, `>`, `>=` using the documented total type order. | `P1-EXPR-003` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001`, `P3-CRUD-002` |
| `OP-EQ` | Partial | Execute recursive `=` and `!=`; `==` and `IS` are rejected. | `P1-EXPR-003`, `P1-EXPR-005` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001`, `P2-IDX-001` |
| `OP-BOOL` | Partial | Execute short-circuit operand-returning `AND` and `OR`; symbolic boolean operators are rejected. | `P1-EXPR-003`, `P1-EXPR-005` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-EXPR-001`, `P3-IDX-001` |
| `EXPR-EXCLUDED` | Unsupported | Functions, casts, subqueries, traversal, ranges, indexing, modulo, power, and broader operators fail explicitly. | `P1-EXPR-005` | [Phase 1 exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-003`, `P3-PARAM-001` |

## Statements and clauses

| Feature ID | Status | Exact accepted / rejected syntax | Parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `STMT-CREATE` | Partial | Execute `CREATE [ONLY] table[:id]` with CONTENT or one or more SET assignments and supported RETURN modes. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [Phase 3 results](docs/compat-research/phase3.md#result-and-projection-shapes) | `P3-RESULT-001`, `P2-BRIDGE-001` |
| `STMT-SELECT` | Partial | Execute table or record targets, record-only ONLY, MVP expressions, projection, ordering, and pagination. | `P1-STMT-001`, `P1-STMT-002`, `P1-STMT-004`, `P1-STMT-006` | [Phase 3 results](docs/compat-research/phase3.md#result-and-projection-shapes) | `P3-CRUD-002`, `P3-IDX-001` |
| `STMT-UPDATE` | Partial | Execute record or table UPDATE SET with optional WHERE and `RETURN AFTER` or `NONE`; richer operators are rejected. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [SET evaluation](docs/compat-research/phase3.md#set-evaluation-and-conflicting-assignments) | `P3-CRUD-001`, `P3-ATOMIC-001` |
| `STMT-DELETE` | Partial | Execute record or table DELETE with optional WHERE and `RETURN BEFORE`; other returns are rejected. | `P1-STMT-001`, `P1-STMT-004`, `P1-STMT-006` | [Return modes](docs/compat-research/phase3.md#return-modes-ordering-and-missing-targets) | `P3-CRUD-001`, `P3-ATOMIC-002` |
| `CLAUSE-ONLY` | Supported | Execute CREATE ONLY and SELECT FROM ONLY record; SELECT ONLY table is rejected. | `P1-STMT-001`, `P1-STMT-004` | [Phase 3 results](docs/compat-research/phase3.md#result-and-projection-shapes) | `P3-RESULT-001` |
| `CLAUSE-CONTENT-SET` | Partial | CREATE accepts CONTENT or SET; UPDATE accepts SET. RHS values use a document snapshot and later writes win. | `P1-STMT-001`, `P1-STMT-004`, `P1-LIMIT-004` | [SET evaluation](docs/compat-research/phase3.md#set-evaluation-and-conflicting-assignments) | `P3-CRUD-001`, `P2-BRIDGE-002` |
| `CLAUSE-PROJECTION` | Partial | Execute `*` alone or comma-separated paths with optional aliases; aliases write top-level keys. | `P1-STMT-001`, `P1-STMT-002`, `P1-STMT-004` | [Phase 3 results](docs/compat-research/phase3.md#result-and-projection-shapes) | `P3-RESULT-001` |
| `CLAUSE-WHERE` | Partial | Execute all MVP expressions on SELECT, UPDATE, and DELETE; unsupported expressions fail explicitly. | `P1-STMT-001`, `P1-STMT-004` | [Phase 3 expressions](docs/compat-research/phase3.md#expressions-and-truthiness) | `P3-CRUD-001`, `P3-IDX-001` |
| `CLAUSE-ORDER` | Partial | Execute comma-separated paths with optional ASC or DESC before pagination and projection. | `P1-STMT-001`, `P1-STMT-002` | [Ordering](docs/compat-research/phase3.md#return-modes-ordering-and-missing-targets) | `P3-CRUD-002` |
| `CLAUSE-PAGE` | Partial | Execute LIMIT then START in `0..=i64::MAX`; reject negatives, fractions, overflow, duplicates, and reordering. | `P1-STMT-004`, `P1-STMT-006` | [Ordering](docs/compat-research/phase3.md#return-modes-ordering-and-missing-targets) | `P3-CRUD-002` |
| `CLAUSE-RETURN` | Supported | Execute CREATE AFTER/NONE/BEFORE, UPDATE AFTER/NONE, DELETE BEFORE, plus statement defaults. | `P1-STMT-001`, `P1-STMT-006` | [Phase 3 results](docs/compat-research/phase3.md#result-and-projection-shapes) | `P3-RESULT-001`, `P3-CRUD-001` |
| `SCRIPT-MULTI` | Supported | Lazily execute ordered nonempty statements with semicolon separators; stop at the first parse or runtime error. | `P1-STMT-003`, `P1-LIMIT-005` | [Explicit transactions](docs/compat-research/phase3.md#explicit-transactions) | `P3-PARSE-001`, `P3-PARSE-002`, `P3-PARSE-003`, `P3-SCRIPT-001` |

## Schema and transactions

| Feature ID | Status | Exact accepted / rejected syntax | Parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `SCHEMA-TABLE` | Supported | Execute exact DEFINE TABLE SCHEMALESS or SCHEMAFULL grammar, including explicit transactions. | `P1-STMT-001` | [Phase 2 definitions](docs/compat-research/phase2.md#nested-paths-and-duplicate-definitions) | `P2-SCHEMA-006`, `P3-TXN-001` |
| `SCHEMA-FIELD` | Supported | Execute DEFINE FIELD on an existing table and validate all existing rows atomically. | `P1-STMT-001` | [Phase 2 fields](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-SCHEMA-005`, `P3-ATOMIC-003` |
| `SCHEMA-INDEX` | Supported | Execute ordered scalar DEFINE INDEX fields with optional UNIQUE; excluded index kinds remain rejected. | `P1-STMT-001`, `P1-STMT-005` | [Phase 2 indexes](docs/compat-research/phase2.md#unique-null-missing-and-composite-indexes) | `P2-IDX-001`, `P3-IDX-001` |
| `TYPE-BASE` | Supported | Enforce bool, int, float, number, string, object, array, and record; null belongs to no base type. | `P1-STMT-001` | [Phase 2 fields](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-SCHEMA-003`, `P3-CRUD-003` |
| `TYPE-OPTION` | Supported | Enforce recursive option types: absence is permitted but null is not. | `P1-STMT-001`, `P1-LIMIT-003` | [Phase 2 fields](docs/compat-research/phase2.md#required-optional-null-and-numeric-coercion) | `P2-SCHEMA-003`, `P3-CRUD-003` |
| `TXN-BEGIN` | Supported | Execute bare BEGIN through BEGIN IMMEDIATE; nested BEGIN is a transaction error. | `P1-STMT-001`, `P1-STMT-005` | [Explicit transactions](docs/compat-research/phase3.md#explicit-transactions) | `P3-TXN-001`, `P3-TXN-003` |
| `TXN-COMMIT` | Supported | Execute bare COMMIT only while active; engine success precedes catalog publication. | `P1-STMT-001`, `P1-STMT-005` | [Explicit transactions](docs/compat-research/phase3.md#explicit-transactions) | `P3-TXN-001`, `P3-TXN-004` |
| `TXN-CANCEL` | Supported | Execute bare CANCEL while active or poisoned; rollback cleanup failure makes the connection broken. | `P1-STMT-001`, `P1-STMT-005` | [Explicit transactions](docs/compat-research/phase3.md#explicit-transactions) | `P3-TXN-002`, `P3-TXN-004` |

## Explicit exclusions

| Feature ID | Status | Exact accepted / rejected syntax | Parser tests | Provenance | Execution / conformance |
| --- | --- | --- | --- | --- | --- |
| `EXCL-STMT` | Unsupported | INSERT, UPSERT, RELATE, LET, REMOVE, INFO, USE, LIVE, SHOW, SLEEP, THROW, FOR, and IF fail at their introducer. | `P1-STMT-005` | [Phase 1 statements](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-003`, `P3-SCRIPT-001` |
| `EXCL-CLAUSE` | Unsupported | TIMEOUT, FETCH, GROUP, SPLIT, OMIT, EXPLAIN, WITH, VALUE, MERGE, PATCH, PARALLEL, and excluded index kinds fail explicitly. | `P1-STMT-005` | [Phase 1 statements](docs/compat-research/phase1.md#statements-and-clauses) | `P1-BRIDGE-003`, `P3-PARAM-001` |
| `EXCL-GRAPH` | Unsupported | Graph traversal, relation statements, complex record IDs, and ranges are not accepted. | `P1-EXPR-005`, `P1-STMT-005` | [Phase 1 exclusions](docs/compat-research/phase1.md#binding-power-and-exclusions) | `P1-BRIDGE-003` |
| `EXCL-ADVANCED` | Unsupported | Functions, casts, subqueries, permissions, users, namespaces, scopes, events, analyzers, views, live queries, and specialized indexes are outside the MVP. | `P1-EXPR-005`, `P1-STMT-005` | [Phase 1 exclusions](docs/compat-research/phase1.md#phase-1-interpretation) | `P1-BRIDGE-003` |

## Phase 3 execution contracts

- `Params` is `BTreeMap<String, Value>`; names omit `$`. The complete map is validated before execution, extra values are ignored, and every referenced value must exist before its statement runs.
- Successful requests return `QueryResponse { statements }` in source order. Schema/transaction statements return `None`; ordinary CRUD returns `Rows`; CREATE ONLY and SELECT ONLY return `Value`.
- Standalone statements commit individually. A later script error does not undo earlier standalone mutations, although the call returns only the error.
- Any error in an active explicit transaction rolls back data and private catalog state and enters `Poisoned`; only CANCEL clears it. Cleanup failure enters unrecoverable `Broken`.
- Predicate lowering is only a candidate-selection optimization. Exact Rust evaluation remains authoritative, and only proven-safe equality/range conjuncts are pushed through the canonical indexed JSON expression.

## Phase 4 delivery contracts

- Each public connection serializes complete asynchronous requests through one dedicated worker. Dropped queued requests are skipped; dropped in-flight requests complete.
- `ExecutionSummary` reports exact statement and mutation counts, including UPDATE/DELETE/CREATE under `RETURN NONE`. SELECT contributes zero mutations.
- A transaction guard rejects source transaction control, consumes on commit/rollback, queues rollback on drop, and fully cleans up after any guarded operation error.
- Public errors expose exactly seven stable categories. Parse and unsupported errors preserve half-open UTF-8 byte spans; internal format failures are `Engine`.
- The CLI exposes command, piped batch, and interactive parser-completeness modes. Its strict JSON is one collision-safe version-1 `$fastdb` envelope per request.

## Phase 5 release-candidate contracts

- Successful parse caching is bounded to 128 entries/4 MiB and excludes sources over 64 KiB. Prepared SELECT caching is bounded to 64 value-free candidate keys and is disabled across explicit transaction execution.
- Committed format-1 and migration-level-0 fixtures prove reopen, transactional migration, further mutation, integrity, and actual expression-index selection.
- The release benchmark compares the public async API with an equivalent native Turso worker under identical values, durability, indexes, and result materialization. Raw samples and percentile ratios are committed.
- FastDB Core is MIT licensed. Crates remain version `0.0.0` and `publish = false` while compatibility work continues toward the first public alpha; the alpha gate is the recorded local verification matrix, not remote CI.

## Maintenance rule

Every row retains a stable feature ID, exact syntax boundary, parser test ID, allowed-source provenance, and execution/conformance evidence. A row can move from Planned only when its accepted form reaches the frontend and its rejected boundary is tested. Phase 3 evidence is checked mechanically by `P3-COMPAT-001`; inherited Turso compatibility documents remain separate.
