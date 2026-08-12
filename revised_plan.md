# FastDB MVP Technical Plan and Roadmap

Status: proposed engineering baseline, 2026-08-12

## 1. Product Definition

FastDB will be a clean-room, SurrealQL-compatible document database frontend built on a pinned fork of Turso. The MVP will be an embedded Rust library and a command-line shell operating on a local Turso database. It will provide a deliberately small, documented subset of SurrealQL rather than claiming full SurrealDB compatibility.

The product goal is a low-latency database with SurrealDB-like document ergonomics and Turso's embedded storage model. Relational interoperability, graph traversal, direct key-value access, network serving, and edge synchronization are roadmap items; they are not part of the first release. The intended business is a publicly developed, source-available database that becomes open source after a defined delay and is funded by a managed service at `cloud.fastdb.org`.

For the MVP, "single file" means one durable `.fastdb` database artifact after a checkpoint and clean shutdown. WAL and shared-memory sidecars may exist while a database is open. A later synchronization mode may also create Turso-owned metadata sidecars. FastDB will not promise that a live database consists of exactly one filesystem entry.

### 1.1 Selected baselines

- Start engineering from Turso `main` commit [`977383ff40edc44ef410af062ed0d2322252a869`](https://github.com/tursodatabase/turso/commit/977383ff40edc44ef410af062ed0d2322252a869). Before implementation begins, audit the then-current `main`, run the relevant upstream tests, and either retain this commit or record a newer reviewed commit in the repository. Never build from a floating branch in CI or releases.
- Use [SurrealDB `v3.1.5`](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5) as the behavioral reference for the compatibility matrix. Later SurrealDB behavior does not silently change the MVP contract.
- Use stable Turso WAL with full durability as the default. Experimental MVCC and experimental multiprocess WAL are excluded until they are stable and pass FastDB's workload and recovery suites.
- Ship the embedded Rust API and CLI first. A server and non-Rust SDKs follow only after the local semantics and file format are stable.
- Use this workspace as the FastDB monorepo. Preserve the Turso repository history, configure Turso as `upstream`, and add FastDB crates and service components directly to the same workspace; do not use a nested repository or hide the engine behind an unpinned submodule.

### 1.2 Product surfaces and business model

FastDB has two product surfaces with a strict boundary:

- **FastDB Core:** the source-available parser, frontend, storage integration, embedded Rust API, CLI, conformance suite, and eventually the self-hostable query server and sync components. Local and self-hosted use within the Community License grant is the permanent free path. Each release becomes open source under its stated Change License on its Change Date.
- **FastDB Cloud:** a managed service that sells operation, durability, regional compute, authentication, metering, backups, restore, observability, support, and later edge synchronization. The hosted service must use the same public query semantics and file-format policy as Core.

FastDB-authored Core and Cloud source code will be public. Deployment credentials, customer data, signing material, incident data, and live infrastructure state are not source code and are never published. The commercial advantage is the operated service, brand, reliability, support, and accumulated operational experience rather than a hidden database implementation.

The project will not depend on a permanent free cloud tier. A time-limited, payment-card-backed trial or small one-time credit may be offered only with a hard spend/resource cap and abuse controls. The initial pricing hypothesis is a simple three-tier ladder:

| Tier | Target price | Intended user | Commercial shape |
| --- | ---: | --- | --- |
| Starter | $5/month | Individual developers, prototypes, and small agents | Small included storage and operation credits; community support |
| Builder | $20/month | Deployed applications and small teams | Larger included credits, longer restore retention, team access when available |
| Pro | $100/month | Production applications | Higher limits, audit/security features, longer retention, and prioritized support |

Names, prices, quotas, retention, and overage rates are hypotheses until load tests and a cost model validate them. No tier includes unlimited storage, writes, compute, egress, sync traffic, or support. Overage billing is opt-in, exposes budget alerts and hard caps, and must never surprise the customer. A formal uptime or durability SLA is offered only after the corresponding failure tests, operational history, staffing, and financial exposure have been reviewed.

### 1.3 Licensing direction

The preferred model, subject to qualified legal review, is dual licensing:

1. **Community License:** Business Source License 1.1 with a narrowly drafted Additional Use Grant. It should permit internal production use, self-hosting, modification, redistribution, and applications that use FastDB as an internal component. It should prohibit offering FastDB itself, or a substantial set of its database APIs and functionality, to third parties as a competing hosted or managed database service without a commercial agreement.
2. **Commercial License:** a paid alternative for cloud providers, managed service providers, OEM redistribution that falls outside the Additional Use Grant, or customers that require different terms.
3. **Change License:** Apache License 2.0, taking effect for each released version no later than the maximum period BSL 1.1 permits. Legal review should choose and publish a fixed, easy-to-calculate Change Date; the initial preference is four years after each version's first public release.

This model is source-available before the Change Date, not OSI open source. The Open Source Definition does not permit discrimination against a field of endeavor, so a current license cannot both qualify as open source and forbid competing cloud services. Project documentation must use these terms accurately and must not market pre-Change-Date releases as open source.

The Additional Use Grant must distinguish a prohibited database-as-a-service from an allowed application that merely stores its own data in FastDB. It must not prevent consultants from helping a customer self-host FastDB for that customer's internal use. Do not write custom legal text informally in the repository; have counsel adapt the standard BSL parameters and publish practical examples.

All inherited Turso files retain their MIT notices and rights. The FastDB license can govern FastDB-authored files and the combined FastDB distribution but cannot revoke permissions already granted for upstream Turso code. Keep file provenance mechanically auditable.

Dual licensing requires the project to retain sufficient relicensing rights. Adopt a contributor license agreement that grants the FastDB legal entity the necessary copyright license to distribute contributions under the Community, Commercial, and Change Licenses while contributors retain copyright. A DCO alone is not sufficient for this licensing goal. Establish the legal entity, license parameters, CLA, privacy terms, and trademark policy before accepting material third-party code.

### 1.4 MVP success criteria

The MVP is complete only when all of the following are true:

- The supported syntax in `COMPAT.md` has independently authored conformance tests against the pinned behavioral reference.
- Catalog, physical-schema, and record changes are atomic, including automatic table creation.
- A database survives clean reopen and crash/recovery tests without partial logical changes.
- Schemafull validation and every declared index type are covered by positive and negative tests.
- Execution-plan tests prove that supported indexed predicates avoid full scans.
- The embedded API and CLI expose the same statement ordering, values, errors, and transaction behavior.
- Published benchmarks meet the gates in section 11 or clearly block release; no unmeasured absolute latency or "production-ready ACID" claim is made.

## 2. Compatibility and Clean-Room Policy

FastDB will implement a documented compatibility subset from public SurrealQL specifications and observed public behavior. The team may run black-box queries against an unmodified SurrealDB `v3.1.5` binary and record inputs and outputs. FastDB's parser, implementation, fixtures, expected outputs, fuzz corpora, and conformance tests must be written independently.

Do not copy, translate, vendor, or adapt SurrealDB source code or test files. Keep behavioral research notes separate from implementation artifacts and record the public source or black-box experiment behind each compatibility decision. SurrealDB's current core is distributed under Business Source License 1.1, so legal review is required before public compatibility claims, naming, or trademark use. "SurrealQL-compatible subset" must never imply sponsorship, certification, or complete compatibility.

`COMPAT.md` will be normative for language support. Each grammar item is assigned exactly one status:

- **Supported:** implemented and covered by conformance tests.
- **Partial:** a documented subset is implemented; accepted and rejected forms are enumerated.
- **Planned:** intentionally absent from the current release but present on the roadmap.
- **Unsupported:** not planned for the stated compatibility target.

If the parser recognizes a clause that execution cannot honor, it must return a typed `UnsupportedSyntax` error with a source span. It must never ignore, approximate, or partially apply that clause.

## 3. Architecture

FastDB should mirror the separation used by Turso's PostgreSQL frontend: an independent language parser, a frontend/translator and executor, a CLI, integration tests, and an explicit compatibility document. The initial workspace layout should be conceptually similar to:

```text
fastdb/
  parser/       lexer, AST, parser, diagnostics
  frontend/     catalog, validation, lowering, execution, decoding
  bindings/rust public embedded API
  cli/          interactive shell and batch execution
  tests/        clean-room conformance, recovery, and integration tests
  fuzz/         parser and structured CRUD fuzz targets
  COMPAT.md     normative compatibility matrix
```

Exact crate names may follow the audited Turso workspace conventions, but parser and frontend boundaries must remain independently testable.

### 3.1 Request path

```text
SurrealQL text + named parameters
        |
        v
lexer -> FastDB AST -> validation/catalog resolution -> frontend execution plan
                                                        | prerequisites
                                                        | translated Turso AST
                                                        | bound values
                                                        | result decoder
                                                        v
                                             Turso prepare/execute
                                                        |
                                                        v
                                            typed FastDB results
```

The frontend lowers FastDB AST nodes directly into Turso's internal AST and prepares them using `prepare_translated_stmt_with_options`, following the integration pattern in Turso's PostgreSQL frontend. It must not generate SQLite text containing user data or user identifiers. Direct AST construction keeps parameter binding typed, avoids quoting as a security boundary, and permits exact expression reuse between queries and indexes.

The frontend execution plan contains:

1. Transaction and catalog prerequisites.
2. Zero or more internal schema/catalog statements.
3. The main translated statement or statements.
4. Typed parameter bindings.
5. A result decoder that restores FastDB values and record IDs.

Catalog creation, implicit logical-table registration, physical-table creation, index changes, and the associated record mutation execute in one transaction. The schema path is serialized by a database-level schema mutex while preserving engine transaction rollback. Avoid Turso core changes initially; any unavoidable change must be isolated, tested independently, documented, and shaped for an upstream contribution.

### 3.2 Parser and diagnostics

Build an independent hand-written lexer, a recursive-descent statement/value parser, and a Pratt expression parser. All tokens and AST nodes carry byte source spans. Diagnostics use `miette` and identify the unsupported token or invalid expression without exposing internal SQL.

Parser safeguards are part of the public contract:

- Configurable maximum input bytes, token count, nesting depth, object/array elements, statements per request, and identifier length.
- Linear scanning of strings and comments, including correct semicolon handling inside them.
- No recursive behavior without an enforced depth budget.
- No recovery rule that silently drops a recognized clause.
- Fuzz targets for the lexer, parser, formatter/debug display, and parse-to-plan boundary.

### 3.3 Storage model

Each logical table maps to one hidden physical table:

```sql
CREATE TABLE <opaque_physical_name> (
    rid TEXT PRIMARY KEY,
    doc JSONB NOT NULL
);
```

This SQL is illustrative internal DDL, never a user-facing translation. Keep Turso's `SqliteDialect` for persisted physical schema so definitions stored in `sqlite_schema` remain valid SQLite/Turso SQL. Store original FastDB definitions separately in the internal catalog.

Physical names are opaque and deterministic from immutable catalog IDs, for example a fixed prefix plus a lowercase encoding of a 128-bit table ID. User-provided identifiers are never interpolated into physical names. Catalog resolution is the only path from a logical name to a physical object.

`rid` stores an immutable, versioned, type-tagged canonical encoding of the ID component so a string, integer, and UUID cannot collide and can be decoded losslessly. `doc` stores user content only and never duplicates `id`. The result decoder synthesizes a typed `RecordId { table, id }`. Generated IDs use UUIDv7. MVP record ID components may be bare UTF-8 identifiers, backtick-quoted UTF-8 text, signed integers where accepted by the grammar, adjacent typed UUID literals (`u'…'` or `u"…"`) containing canonical UUIDv4/UUIDv7 values, or generated UUIDv7 values. Array/object IDs are deferred.

Native JSON values map directly to JSONB. Record IDs embedded in user documents require a versioned tagged representation because JSON has no record-ID type. The codec must escape any user object that would collide with its reserved tag shape, decode recursively, and have round-trip tests. Index support for tagged record-valued fields is deferred unless the canonical expression builder can prove correct ordering and equality semantics.

Nested reads lower to canonical `json_extract(doc, <bound-safe-path>)` expressions. Nested writes use `json_set`; removal uses `json_remove`. A single path encoder owns JSON-path escaping and is tested for quotes, dots, brackets, control characters, Unicode, and adversarial input. Filter lowering and index creation must call the same canonical expression builder; semantically equivalent but structurally different expressions may prevent optimizer index selection.

Direct external modification of hidden physical tables or internal catalogs is unsupported. The database remains SQLite-format compatible at the storage level, but arbitrary SQL writes can violate FastDB invariants.

### 3.4 Catalog and format

Bootstrap internal catalogs atomically on first open. At minimum, persist:

- Format metadata: format version, dialect version, creation version, and last migration.
- Logical tables: immutable table ID, logical name, mode (`SCHEMALESS` or `SCHEMAFULL`), and original definition.
- Fields: table ID, canonical path, type AST, required/optional state, and original definition.
- Indexes: immutable index ID, table ID, logical name, ordered field paths, uniqueness, physical name, expression format version, and original definition.

Catalog tables use a reserved prefix inaccessible through the FastDB grammar. Enforce uniqueness of logical names within their scope and use foreign keys or equivalent transactional checks for ownership.

The format version is monotonic. Open must reject an unknown future format before any mutation. Migrations are transactional, forward-only, idempotence-tested, and backed by reopen fixtures from every released format. Downgrade is not supported unless a future export/import tool explicitly provides it.

An undefined table referenced by a valid record mutation is atomically registered as `SCHEMALESS`. `SCHEMAFULL` tables reject unknown fields, missing required fields, and values that fail the declared type before storage changes are committed.

### 3.5 Indexes

Basic field and composite indexes lower to Turso expression indexes over the exact canonical JSON extraction expressions used by filters. `UNIQUE` creates a unique expression index. Index physical names derive from immutable catalog index IDs.

MVP index rules:

- One or more scalar field paths, preserving declared order.
- Equality and supported range predicates can use matching indexes.
- Unique constraints follow engine null behavior only after it is characterized and documented in `COMPAT.md`.
- Index creation over existing invalid or duplicate data fails atomically and leaves no catalog entry.
- Every index shape has an explain-plan assertion plus result-correctness tests before and after reopen.

Array-element, count, full-text, and vector indexes are outside the MVP.

### 3.6 Extension architecture

Do not create a stable third-party plugin ABI during the MVP, but keep extension points explicit inside the frontend. An internal extension registry should eventually compose:

- Value type parsing, validation, binary encoding, and result decoding.
- Scalar, aggregate, and table functions with typed signatures.
- Operators and FastDB-AST-to-Turso-AST lowering hooks.
- Index providers with catalog metadata, physical storage, maintenance, planner matching, and explain output.
- Capability and format versions so a database refuses to open when a required extension is absent or incompatible.

Fields may use one of three physical strategies without changing their logical document interface:

```text
JSONB-only field
  -> stored only in doc

Derived B-tree field
  -> canonical JSON expression index

Specialized typed/indexed field
  -> doc representation plus hidden typed column or auxiliary index table
```

Hidden typed columns and auxiliary tables use opaque catalog-derived names and are maintained in the same transaction as `doc`. The catalog records their provider, provider version, encoding version, options, and rebuild state. Reopen, rollback, migration, and corruption tests apply to them exactly as they do to ordinary indexes.

This is the expected path for vectors, full-text search, and geospatial data. Exact vector distance can initially lower to Turso's native vector functions, but efficient approximate-nearest-neighbor search should wait for a stable Turso vector index or a production-quality FastDB index provider. A JSON array is not an acceptable production vector-index representation; use a native vector BLOB in a hidden typed column.

A focused geospatial subset may later store canonical WKB/geometry values, expose selected predicates and distance functions, and add a spatial index. Full PostGIS compatibility is a separate major project: Turso's PostgreSQL syntax frontend does not provide PostgreSQL's extension ABI, geometric types, GiST/SP-GiST operator classes, planner hooks, or the PostGIS function surface. Do not place PostGIS compatibility on the normal extension roadmap unless Turso gains the required stable facilities and a separate specification is approved.

## 4. MVP Language Contract

### 4.1 Values and expressions

The initial AST and value model support:

- `null`, booleans, signed integers, floating-point numbers, strings, arrays, and objects.
- Field paths and named `$parameters` supplied by the Rust API.
- Typed record IDs.
- Comparisons, `AND`/`OR`/`NOT`, parentheses, and basic arithmetic.

Numeric coercion, overflow, division-by-zero, null/missing-field behavior, string ordering, and mixed-type comparisons must be characterized against SurrealDB `v3.1.5` and written into `COMPAT.md` before being labeled supported. Parameters are values, never identifiers or source fragments. `LET` is deferred.

### 4.2 Statements

Support multiple semicolon-separated statements with correct string and comment handling. Return one ordered `StatementResult` for each statement, including statements that return no rows.

#### `CREATE`

```sql
CREATE [ONLY] table[:id]
  (CONTENT object | SET path = expression [, ...])
  [RETURN AFTER | RETURN NONE | RETURN BEFORE];
```

- Omitted IDs generate UUIDv7 values.
- Duplicate explicit IDs are constraint errors; `CREATE` is not an upsert.
- Default and `RETURN AFTER` return the created record.
- `RETURN NONE` returns no records.
- For compatibility, `RETURN BEFORE` returns the empty pre-create result.
- `CONTENT` must evaluate to an object. `CONTENT` and `SET` are mutually exclusive.
- `ONLY` is accepted only where the target structurally guarantees at most one result.

#### `SELECT`

```sql
SELECT (* | field [AS alias] [, ...])
FROM [ONLY] (table | table:id)
[WHERE expression]
[ORDER BY field [ASC | DESC] [, ...]]
[LIMIT integer]
[START integer];
```

- Results are arrays by default, including record targets.
- `ONLY` is valid only for a single record target, where at most one result is structurally guaranteed.
- Selection supports `*` or named field paths with aliases.
- Ordering is deterministic only when the query supplies a complete ordering; tests must not rely on incidental engine row order.
- `LIMIT` and `START` reject negative values and values outside documented integer bounds.

#### `UPDATE`

```sql
UPDATE (table | table:id)
SET path = expression [, ...]
[WHERE expression]
[RETURN AFTER | RETURN NONE];
```

- A missing record target succeeds with zero changed records.
- The default is `RETURN AFTER`; `RETURN NONE` suppresses returned records.
- The `WHERE` expression sees the pre-update document. Multiple `SET` assignment evaluation order must be characterized before support is claimed.

#### `DELETE`

```sql
DELETE (table | table:id)
[WHERE expression]
[RETURN BEFORE];
```

- The default output is empty.
- `RETURN BEFORE` returns deleted records.
- A missing record target succeeds with zero deleted records.

#### Schema

```sql
DEFINE TABLE name (SCHEMALESS | SCHEMAFULL);
DEFINE FIELD path ON [TABLE] name TYPE type;
DEFINE INDEX name ON [TABLE] name FIELDS path [, ...] [UNIQUE];
```

Initial field types are `bool`, `int`, `float`, `number`, `string`, `object`, `array`, `record`, and `option<T>`. `option<T>` controls field presence as well as permitting the compatibility-defined null behavior; the exact distinction between missing and `null` must be recorded in `COMPAT.md`.

#### Transactions

Support `BEGIN`, `COMMIT`, and `CANCEL`. `CANCEL` maps to rollback semantics. Every standalone statement is atomic. A connection can have at most one explicit transaction. Any parse, validation, constraint, engine, or I/O error while executing a statement inside an explicit transaction poisons it and causes rollback of the entire transaction. Afterward, only the documented transaction cleanup behavior is allowed; no partial result may be committed.

For a multi-statement request outside an explicit transaction, each statement is independently atomic and earlier successful statements remain committed if a later one fails. For a multi-statement request inside an explicit transaction, any error rolls back all work since `BEGIN`. These semantics must be prominent in API and CLI documentation.

### 4.3 Explicit MVP exclusions

The following are parsed only if needed to produce a precise unsupported error; none may be silently accepted:

- Multiple targets, record ranges, `TIMEOUT`, `DIFF`, custom `RETURN` projections, and `RETURN VALUE`.
- `UPDATE CONTENT`, `MERGE`, `PATCH`, array mutation operators, `UPSERT`, and `INSERT`.
- Functions, `LET`, subqueries, grouping, graph traversal, and live queries.
- Permissions, users, authentication, namespaces, databases, events, analyzers, and views.
- Array-element, count, full-text, and vector indexes.
- Complex array/object record IDs.

## 5. Embedded Rust API

Expose an asynchronous interface patterned after Turso without leaking internal AST or storage types:

```rust
let db = fastdb::Builder::new_local("app.fastdb")
    .build()
    .await?;
let conn = db.connect()?;

let response = conn
    .query(
        "SELECT * FROM person WHERE profile.age >= $minimum;",
        fastdb::params! { "minimum" => 18 },
    )
    .await?;
```

The intended surface is:

- `Builder::new_local(path).build().await`
- `Database::connect()`
- `Connection::query(sql, params) -> QueryResponse`
- `Connection::execute(sql, params) -> ExecutionSummary`
- `Connection::transaction()` returning an explicit transaction guard
- Public `Value`, `Object`, `RecordId`, `Params`, `StatementResult`, `QueryResponse`, and `ExecutionSummary`

`QueryResponse` contains one ordered `StatementResult` per parsed input statement. Values preserve nested arrays/objects and typed record IDs rather than flattening everything to JSON strings. Objects require deterministic iteration/serialization behavior. The API must document integer and float bounds and reject non-finite values if they cannot round-trip through the selected JSONB representation.

Stable top-level error categories are:

- `Parse`
- `UnsupportedSyntax`
- `Schema`
- `Constraint`
- `Transaction`
- `Engine`
- `Io`

Errors include a stable category and human-readable detail; parse and unsupported errors include source spans. Engine internals may be attached as sources but are not part of the stable matching API.

Dropping an uncommitted transaction guard rolls it back. Commit consumes the guard. The API prevents queries from being concurrently interleaved on the same connection unless the underlying audited Turso API can guarantee correct serialization.

## 6. CLI

Ship a `fastdb` binary that opens a supplied `.fastdb` path or an in-memory database. It supports:

- Interactive and piped/batch input.
- Multiline statements based on lexer completeness, not merely a trailing semicolon heuristic.
- `BEGIN`, `COMMIT`, and `CANCEL` using the same frontend path as the library.
- Readable FastDB value output and a strict JSON output mode.
- Nonzero process status for batch parse or execution failures.
- Clear display of statement boundaries and error source spans.

Strict JSON output must define how typed record IDs are represented and must always emit valid JSON, not SurrealQL object syntax. Secrets and parameter values are not included in tracing by default.

## 7. Delivery Phases

Phases are ordered by risk. A later phase may be prototyped early, but it does not start formally until the preceding exit gate is met.

### Phase 0 — Fork, specification, and feasibility spike

Deliverables:

- Fork Turso, retain all MIT notices, configure an `upstream` remote, and document a regular upstream review/merge cadence.
- Record the audited Turso commit in a machine-readable dependency file and CI output.
- Add the clean-room policy, compatibility-research template, and initial `COMPAT.md` skeleton pinned to SurrealDB `v3.1.5`.
- Implement a disposable vertical slice for `CREATE person:tracy SET name = 'Tracy';`, reopen, select, delete, and reopen again.
- Add one JSON field index and prove with an explain-plan test that an equality predicate uses a B-tree access path.
- Test rollback of automatic catalog registration, physical table creation, and the first record as one unit.
- Benchmark the slice against equivalent native Turso JSONB operations to establish harness noise and frontend overhead.

Exit gate:

- The vertical slice survives reopen and rollback, uses an index, has reproducible benchmark instructions, and requires no Turso core modification. If a core modification appears necessary, stop and produce a design note with alternatives before Phase 1.

### Phase 1 — Parser, AST, and compatibility contract

Deliverables:

- Independent lexer, spanned AST, recursive-descent statement/value parser, and Pratt expression parser.
- Values, record IDs, expressions, all MVP statements, comments, and multi-statement input.
- Configured token, input, nesting, collection, and statement limits.
- `miette` diagnostics and explicit unsupported-clause errors.
- Table-driven parser tests, snapshot diagnostics, round-trip/debug tests where useful, and an initial fuzz target.
- A reviewed `COMPAT.md` whose status entries map to test IDs and reference observations.

Exit gate:

- The parser never panics on the fuzz corpus, all MVP grammar cases have positive and negative tests, and every accepted AST form is either executable in the next phase or explicitly rejected as unsupported at planning time.

### Phase 2 — Storage, catalog, schema, and translation

Deliverables:

- Atomic catalog bootstrap, format checking, and a first no-op migration fixture.
- Opaque logical-to-physical table and index mapping.
- Canonical JSON path encoder and expression builder.
- Direct FastDB-AST-to-Turso-AST lowering through translated statement preparation.
- Auto-registration of schemaless tables and schemafull field/type enforcement.
- Field, composite, and unique expression indexes.
- Database-level schema mutex with concurrent open/define tests.
- Result decoder that synthesizes typed IDs and nested FastDB values.

Exit gate:

- Catalog, schema, index, and first-write operations roll back atomically under injected failures; unknown future formats are refused; all declared indexes are selected by their matching query plans after reopen.

### Phase 3 — CRUD, parameters, and transactions

Deliverables:

- Complete MVP semantics for `CREATE`, `SELECT`, `UPDATE`, `DELETE`, and supported return modes.
- Named parameter binding at every supported expression position.
- Standalone statement atomicity and poisoned explicit transaction behavior.
- Multi-statement ordered results.
- Model-based tests comparing a simple in-memory document model with FastDB outcomes.
- Black-box conformance cases authored from public docs and observed SurrealDB `v3.1.5` behavior.

Exit gate:

- The entire supported matrix passes for in-memory and file-backed databases, including reopen, constraint, missing-record, and rollback cases. Parameter/identifier injection and JSON-path adversarial tests pass.

### Phase 4 — Embedded API and CLI

Deliverables:

- Public Rust types and async builder/database/connection/transaction APIs.
- Stable error categories and compile-tested examples.
- `fastdb` interactive and batch shell with readable and strict JSON output.
- API lifecycle, cancellation, drop/rollback, and connection serialization tests.
- CLI multiline, exit-status, transaction, and golden-output tests.

Exit gate:

- A consumer crate can create, reopen, query, mutate, transact, and inspect errors without importing internal crates. The CLI passes the same conformance fixtures through its public boundary.

### Phase 5 — Hardening and MVP release

Deliverables:

- Parser fuzzing and malformed-input resource-limit tests.
- Randomized CRUD model tests and parameter/identifier/path injection suites.
- Crash/recovery tests around bootstrap, implicit table creation, schema and index changes, writes, commits, and rollbacks.
- Relevant Turso core tests run unchanged, plus deterministic simulation where frontend operations cross transaction or I/O boundaries.
- Reopen fixtures, format migration tests, execution-plan assertions, and cross-platform filesystem tests.
- Published feature matrix, limitations, format policy, upgrade procedure, benchmark methodology/results, and clean-room/legal sign-off record.

Exit gate:

- All MVP acceptance and performance gates pass in release CI. Crash injection reveals no partial logical state, every index has a plan test, fuzzing has no known crash, and release documentation states measured limitations without broader durability or compatibility claims.

## 8. Verification Strategy

### 8.1 Test layers

- **Unit:** lexer/parser, spans, value conversion, type validation, path encoding, physical-name derivation, and result decoding.
- **Translation:** FastDB AST to expected Turso AST shape, with parameters kept separate from structure.
- **Integration:** every statement against in-memory and file-backed databases, before and after reopen.
- **Conformance:** independently authored input/output cases grouped by the `COMPAT.md` feature ID and pinned reference version.
- **Property/model:** generated valid operation sequences checked against a small reference document model.
- **Fuzz:** arbitrary bytes into parser and structured AST/value sequences into planning/execution.
- **Failure injection:** I/O and transaction failures at catalog/schema/data boundaries.
- **Crash/recovery:** kill/reopen around WAL writes, commits, checkpoints, and schema operations.
- **Upstream regression:** relevant unmodified Turso core, parser, JSONB, index, WAL, and simulator suites.
- **Performance:** criterion or equivalent microbenchmarks plus repeatable process-level workload benchmarks.

### 8.2 Required invariants

- No committed record points to a missing logical table or physical table.
- No committed catalog index points to a missing or structurally different physical index.
- `rid` never changes during update and never appears inside stored user `doc`.
- Result decoding always reconstructs the logical table name and typed ID from catalog plus `rid`.
- Schemafull validation happens before mutation and covers all affected nested paths.
- The expression used to create an index is structurally identical to the expression used by matching filters.
- An explicit transaction either commits all catalog/schema/data changes or none.
- Unknown database format versions cause a read-before-write open failure.

## 9. Durability, Concurrency, and File Semantics

MVP defaults favor known behavior over maximum write concurrency:

- Use stable WAL and the strongest supported synchronous/full-durability configuration established by the audited Turso baseline.
- Support the audited single-writer behavior; return a stable busy/transaction error rather than inventing retries that could duplicate statements.
- Do not enable experimental MVCC, `BEGIN CONCURRENT`, or experimental multiprocess WAL.
- Serialize catalog and physical schema mutation with a process-local database schema mutex. Document that cross-process concurrent access is not supported in the MVP.
- On clean close, perform or request a safe checkpoint according to the audited Turso API. Never claim sidecars cannot exist after abnormal termination.
- Recovery tests must include abandoned WAL files and reopening after process termination at each logical mutation boundary.

Turso facilities and defaults can change. Re-audit these choices whenever the pinned engine commit changes; do not infer safety from a feature name alone.

## 10. FastDB Cloud Architecture and Economics

Cloud research runs alongside Core because identifiers, logical mutation logging, CDC, sync metadata, and format choices can constrain a future service. Cloud implementation does not delay the embedded MVP and must not make local Core depend on a network service.

### 10.1 Cloud architecture principles

FastDB Cloud targets database-per-tenant and database-per-agent workloads: many strongly isolated databases, most of them idle or lightly active, with costs proportional to actual use. A single shared database with a user-supplied tenant predicate is not the default isolation boundary.

Object storage is not a mounted database filesystem. FastDB must not download and replace an entire `.fastdb` file for each request or acknowledge a commit merely because an asynchronous backup was scheduled. An object-native service requires an ordered durable commit path, immutable database generations, manifests, leases/fencing, checkpointing, cache invalidation, and tested recovery.

The initial deployment target for `cloud.fastdb.org` is Cloudflare. Keep the storage boundary behind a FastDB-owned object-store interface so the database format and recovery protocol can also work with AWS S3, MinIO, or another sufficiently compatible object store. Cloudflare product APIs are deployment adapters, not types exposed by FastDB Core.

The target Cloudflare architecture is:

```text
SDK / HTTP / WebSocket / sync clients
                  |
                  v
Cloudflare Worker API gateway
  TLS | authentication | quotas | metering | routing
                  |
                  v
named container-backed Durable Object per database
  writer coordination | epoch/lease | idempotency | lifecycle
                  |
                  v
Cloudflare Container running native FastDB/Turso
  query frontend | transaction owner | ephemeral local file/cache
                  |
                  v
ordered immutable WAL/log fragments in R2
  sequence | checksum | mutation ID | recovery metadata
                  |
                  v
checkpoint / compaction / generation maintenance
  coordinated by fencing tokens; Queues or Workflows may trigger work
                  |
                  v
immutable R2 database segments and conditional current manifest
  restore history | branching | backups | garbage-collection roots
```

Apply these Cloudflare-specific boundaries:

- Workers are the public edge and control-plane entry point, not the database process. Their request-local, memory-backed virtual filesystem is not persistent FastDB storage.
- A database ID deterministically routes to one container-backed Durable Object. The object coordinates the active writer, epoch/fencing token, request idempotency, and container lifecycle. Durable Object SQLite may store small coordination metadata, but it must not replace Turso as FastDB's query/storage engine or become an undocumented second record store.
- The native Rust FastDB/Turso engine runs in a Cloudflare Container with an ordinary local `.fastdb` file and WAL. Container disks are ephemeral; sleep, eviction, host replacement, and deployment must be treated as routine recovery events.
- The active local database is a disposable cache reconstructed from an R2 checkpoint generation plus ordered WAL/log fragments. It is never the sole durable copy of an acknowledged commit.
- Do not run the active mutable database directly on an R2 FUSE mount. FUSE may be useful for import, export, or diagnostics, but object-store filesystem semantics and latency are not a substitute for database pager/WAL semantics.
- Access R2 through Workers bindings/outbound handlers or its S3-compatible API with least-privilege, database-scoped prefixes. Immutable data objects use content-derived or generation/sequence keys; the small current manifest is published conditionally using its prior version/ETag.
- R2's consistency and conditional operations simplify publication but do not replace transaction ordering or fencing. The Durable Object remains the per-database coordinator, and every manifest/log transition must be independently recoverable.
- Queues and Workflows may schedule checkpointing, compaction, retention, verification, and garbage collection. Background workers may publish state only while holding a valid generation/epoch fence; delivery retries must be idempotent.
- Do not assume the Worker, Durable Object isolate, and Container are co-located. Measure each hop and use placement features only as optimizations, never correctness requirements.

The initial durable commit candidate is: execute and fsync locally, upload the transaction's immutable physical WAL fragment or independently specified logical mutation batch to R2, conditionally advance the database recovery manifest/high-water mark, and only then acknowledge success. C0 must determine the exact Turso artifact and prove replay, idempotency, crash behavior, and acceptable latency. If R2-per-commit latency or request cost is not viable, stop and redesign the ordered durable-log tier; do not acknowledge from ephemeral disk or silently weaken durability.

Turso Cloud demonstrates one implementation using local compute caches, S3 Express One Zone for recent durable commits, S3 for checkpointed generations, and cross-database/time batching to amortize object-request cost. FastDB may learn from that public architecture but does not inherit it from the Turso engine fork. Cloudflare R2 is not S3 Express and must be measured on the intended workload. Every required component must be located in audited upstream code, implemented independently, or purchased as an external service with its cost and guarantees recorded.

Cloud invariants include:

- A successful response never precedes the durability point promised by its plan.
- At most one unfenced writer owns a database/log epoch at a time.
- Retried requests and WAL uploads are idempotent and cannot duplicate logical mutations.
- A compute worker can disappear after acknowledgement without losing the commit.
- Manifests never expose a partial generation and can recover from an interrupted checkpoint.
- Local caches are disposable and never the sole durable copy of acknowledged data.
- Restore is continuously exercised, not inferred from object presence.
- Database deletion, retention, legal hold, and tenant erasure have explicit object-lifecycle semantics.

### 10.2 Staged cloud delivery

#### Cloud C0 — Architecture and cost feasibility

Run during Core Phases 0–2 without shipping a service:

- Specify global database IDs, log sequence/epoch rules, mutation IDs, and CDC/sync metadata.
- Determine which Turso sync/log facilities are stable and reusable at the pinned commit.
- Prototype replay from an immutable WAL/log into a clean `.fastdb` file.
- Prototype a Worker-to-container-backed-Durable-Object request path without making it a Core dependency.
- Prove that a native FastDB container can hydrate from an R2 generation, execute a transaction, publish its recovery artifact, lose all local disk, and recover the acknowledged result exactly once.
- Benchmark R2 PUT/GET/range latency, conditional-manifest publication, Worker-to-Durable-Object-to-Container hops, cold hydration, generation size, checkpoint frequency, cache hit rate, and restore time using realistic document and index workloads.
- Compare direct R2 commit publication with any audited Cloudflare durable-log alternative. Record limits, availability assumptions, request sizes, batching delay, and failure semantics; do not select by nominal storage price alone.
- Define an object-store portability contract and run the recovery prototype against R2 plus at least one S3-compatible local test service.
- Build a cost model for each proposed price tier and at low, expected, and adversarial utilization.
- Have counsel finalize the BSL 1.1 parameters, Additional Use Grant, commercial terms, Change License/Date, CLA, and trademark policy before accepting material external contributions.

Exit gate: a reviewed design demonstrates a recoverable log and database-generation model after forced Container loss, proves fencing and idempotent retry behavior, identifies all non-upstream and Cloudflare-specific components, and shows a credible path to positive unit economics. Failure does not block the local MVP; it blocks cloud implementation, pricing, and durability promises.

#### Cloud C1 — Paid Cloudflare native-container alpha

Start after the local MVP:

- Add a Cloudflare Worker HTTP/WebSocket gateway, authentication, organizations/projects, database provisioning, SDK credentials, quotas, and usage metering.
- Route each database to a named container-backed Durable Object that owns writer fencing, request idempotency, and the native FastDB Container lifecycle.
- Run authoritative query execution in the native Container. Hydrate the complete checkpoint eagerly on cold start, use ephemeral local disk while active, synchronously publish ordered WAL/log recovery artifacts to R2 before acknowledgement, and periodically publish versioned whole-file or coarse-generation checkpoints.
- Keep the C1 recovery format simple and auditable. Lazy page/segment fetch, shared cache infrastructure, branching, and aggressive multi-tenant packing belong to C2.
- Add encrypted transport, encryption/key policy at rest, tenant isolation tests, rate limits, audit events, observability, and automated restore drills.
- Offer a small invite-only paid alpha or card-backed capped trial; do not offer a permanent free cloud tier.

Exit gate: measured metering reconciles with engine activity; every acknowledged commit survives forced Container termination and reconstruction from R2; restore drills meet the documented objective; tenant isolation tests pass; and no workload can exceed its configured spend/resource cap.

#### Cloud C2 — Lazy-segment, multi-tenant Cloudflare beta

- Replace eager whole-database hydration with immutable chunked R2 generations, range/segment reads, bounded disposable local caches, background checkpointing, branching, and point-in-time restore.
- Keep recent ordered log data and historical generations in separately tunable R2 layouts, or introduce another durable log tier only if C0/C1 measurements and recovery tests justify it.
- Add safe container packing or sharding only after database-level isolation, resource accounting, and noisy-neighbor tests pass; one shared user-supplied tenant predicate is never the isolation boundary.
- Validate Worker, Durable Object, and Container eviction; fencing; replay; duplicate delivery; checkpoint interruption; stale cache; R2 throttling/unavailability; and documented regional failure scenarios.
- Add opt-in overages, budget alerts, hard limits, and published usage definitions.

Exit gate: acknowledged commits survive forced worker loss; randomized restore reaches the expected database hash; p95 cost and latency satisfy the approved tier model.

#### Cloud C3 — Object-native production service

- Batch commits across databases only with explicit maximum-delay and isolation guarantees.
- Automate Worker/Container placement, warm-cache policy, load shedding, noisy-neighbor protection, key rotation, restore verification, capacity planning, R2 lifecycle/garbage collection, and incident response.
- Add edge pull/push/checkpoint behavior only after the authoritative remote and conflict contract pass end-to-end recovery tests.
- Offer formal availability/durability objectives or SLAs only after operational evidence and legal/financial review.

Exit gate: multi-region or documented regional recovery, security review, billing reconciliation, disaster exercises, support readiness, and sustainable unit economics all pass. Object-store durability alone is not evidence for this gate.

### 10.3 Pricing and unit-economics gates

The initial `$5 / $20 / $100` monthly ladder optimizes for simple purchasing, not unlimited consumption. Each subscription buys a documented bundle of credits and service features. Exact quotas are set only after C0/C1 measurements.

Meter at least:

- Records scanned/read and records written, with definitions users can reproduce.
- Persistent logical and physical storage, including index amplification and retained generations.
- Sync ingress/egress and ordinary network egress.
- Compute-heavy operations such as full scans, vector search, full-text search, or large result decoding.
- Backup/restore retention beyond the included window.

Track the underlying cost of Worker and Durable Object requests/CPU/duration/storage, Container vCPU/memory/ephemeral disk and idle time, R2 bytes/operations, cache hydration, historical versions, egress, Queues/Workflows, observability, backups, payment processing, and support. Before public launch, the p95 customer using all included quota must have positive variable gross margin; target at least 70% blended gross margin at expected scale, and reduce quotas or raise prices rather than subsidizing structurally unprofitable usage.

Commercial rules:

- Local and self-hosted FastDB remain the free acquisition path.
- There is no permanent free hosted plan at launch.
- Trials require a hard expiration and resource/spend cap; requiring a payment card is an acceptable anti-abuse control.
- The `$5` tier receives community support and cannot include costly manual operations.
- Overage billing is disabled by default and can be capped by the customer.
- Annual discounts wait until retention, support load, and variable cost are understood.
- Do not offer lifetime plans or unbounded "unlimited" features.
- Pricing comparisons use equivalent durability, retention, region, egress, support, and workload assumptions.

FastDB should position the cloud as a lightweight, source-available and eventually open-source document database for embedded, edge, database-per-agent, and database-per-tenant applications. Price is a wedge, not proof of superiority. Claims against Turso or SurrealDB require reproducible feature-adjusted benchmarks and total-cost examples.

### 10.4 Minimum cloud security and operations

Before public availability, define and test:

- Tenant identity, authorization, API-key scope, revocation, rotation, and secret redaction.
- Per-tenant CPU, memory, query, connection, storage, log, and egress limits.
- Encryption in transit and at rest, key ownership, backup encryption, and deletion.
- Audit events for control-plane and database-administration actions.
- Backup retention, point-in-time recovery objectives, restore drills, and corruption response.
- Dependency and image patching, vulnerability disclosure, incident response, and status communication.
- Metering correctness, billing dispute evidence, refunds, data export, account closure, and object deletion.
- Data residency and subprocessor documentation before making regional or compliance claims.

## 11. Performance and Acceptance Gates

Compare FastDB with equivalent native operations on the exact same pinned Turso build, durability settings, filesystem, process model, data distribution, and warm/cold-cache policy. Publish the schema, generated data, commands, sample counts, confidence intervals or dispersion, and hardware/software environment.

Release gates:

- Point-record reads: p50 no worse than 1.5x and p99 no worse than 2x equivalent native Turso primary-key plus JSONB reads.
- Indexed field filters: p95 no worse than 2x the equivalent native Turso expression-index query.
- Document writes: p95 no worse than 2x the equivalent native Turso JSONB write.
- Persistent size after checkpoint and clean shutdown: no more than 1.5x the equivalent Turso JSONB dataset, excluding temporary WAL files.
- Every declared index is exercised by a result-correctness and execution-plan test.
- Explicit transactions are all-or-nothing across catalog, physical schema, and data changes.

Benchmarks are regression gates, not marketing claims. Do not publish absolute latency, scalability, edge, or "production-ready ACID" claims until the associated benchmark, concurrency, and crash-test reports are public.

Maintain a separate, non-gating competitive suite against the pinned SurrealDB behavioral release for embedded point reads, indexed document filters, document writes, startup, resident memory, storage amplification, and supported concurrent workloads. Publish complete configurations and distinguish embedded from networked deployments. These results inform positioning and pricing; they never replace Core's native-Turso regression gates or justify claims about unsupported SurrealDB features.

## 12. Risks and Decision Triggers

| Risk | Initial mitigation | Trigger for revisiting the design |
| --- | --- | --- |
| Turso internal AST APIs change | Pin a commit; isolate lowering behind one crate | Rebase breaks translation or requires broad core patches |
| Expression indexes do not cover required JSON predicates | One canonical expression builder; Phase 0 explain test | Supported filter cannot select its matching index |
| JSONB loses required value distinctions | Define round-trip tests and public value limits | Record IDs, integers, missing/null, or nested values cannot round-trip |
| DDL/catalog atomicity differs from assumptions | Failure injection in the vertical slice | Reopen exposes partial catalog or physical schema |
| Compatibility research crosses license boundaries | Clean-room policy, provenance notes, legal review | Any implementation fixture resembles upstream source/tests or public claims expand |
| Schema mutex is insufficient across handles/processes | Registry by canonical database identity; no multiprocess support | Concurrent opens bypass serialization or Turso adds stable native coordination |
| Stable WAL does not meet a durability assumption | Pin settings and run crash tests | Corruption, partial logical change, or unsupported checkpoint behavior appears |
| Performance target is missed | Profile parsing, planning, JSON encoding, and decoding separately | A release gate fails on the published benchmark harness |
| Specialized values cannot be indexed from JSONB efficiently | Use versioned hidden typed columns or auxiliary index tables | Vector, full-text, or geo prototype requires per-row conversion or a full scan |
| Experimental Turso index/extension APIs change | Keep providers internal and versioned; do not freeze a public ABI | Upstream API churn breaks reopen, maintenance, or planner matching |
| Object presence is mistaken for database durability | Specify ordered log, manifest, fencing, and acknowledgement invariants; inject failures | An acknowledged commit is absent or duplicated after worker loss/replay |
| Cloudflare Worker storage is treated as a persistent filesystem | Keep Workers at the edge; run native FastDB in a Container and persist recovery artifacts before acknowledgement | Any correctness path depends on Worker `/tmp`, isolate lifetime, or in-memory state |
| Ephemeral Container disk is mistaken for durable state | Reconstruct from R2 generations plus ordered logs; continuously kill Containers in recovery tests | Forced sleep/replacement loses an acknowledged commit or requires manual repair |
| R2 latency or request cost makes per-commit durability uneconomic | Measure realistic PUT, conditional-manifest, batching, and restore workloads in C0/C1 | Approved tier latency or p95 variable-margin gate fails |
| Cloudflare coupling prevents portability or recovery outside the service | FastDB-owned object-store/recovery interfaces; S3-compatible test backend; no platform types in Core | A database cannot be exported/recovered without proprietary live control-plane state |
| Multi-tenant isolation fails | Database-level tenancy, hard resource limits, adversarial tests, least-privilege credentials | Cross-tenant access, noisy-neighbor outage, or unbounded resource use |
| The `$5` tier is structurally unprofitable | Bounded credits, no included manual support, C0/C1 cost model | p95 included usage has non-positive variable gross margin |
| Licensing discourages adoption or weakens the hosted strategy | BSL Additional Use Grant, eventual Apache-2.0 conversion, practical examples, CLA, and legal review | Target users reject terms, cloud restriction is ambiguous, or copyright ownership prevents dual licensing |

Any failed Phase 0 feasibility assumption is a design decision point, not permission to add a hidden core fork or relax correctness criteria.

## 13. Post-MVP Roadmap

1. **Complete CRUD compatibility.** Add additional return modes, `TIMEOUT`, richer update operators, `UPSERT`, `INSERT`, functions, more value and record-ID forms, and broader `SELECT` clauses. Extend the matrix one independently tested feature at a time.
2. **Paid Cloudflare C1 alpha.** Add the Worker HTTP/WebSocket gateway, authentication, a JavaScript SDK, database provisioning, resource limits, and metering. Route each database through a container-backed Durable Object to a native FastDB Container, publish ordered recovery artifacts to R2 before acknowledgement, and continuously test reconstruction after ephemeral-disk loss. Preserve embedded semantics as the reference behavior.
3. **Experimental Turso Sync.** Reuse Turso's explicit `push`, `pull`, and `checkpoint` model where the audit shows it is suitable. Treat the remote database as the source of truth. Push local row-level logical mutations and pull physical updates. Follow Turso's Last-Push-Wins default, with an optional documented transform/conflict hook. Initially require schema and index definitions to execute against the authoritative remote; offline writes apply only to record data. Do not design independent peer-to-peer replication.
4. **Specialized extensions.** Stabilize the internal type/function/index-provider contract, then add exact vector operations, production vector indexing when available, full-text indexing, and a focused geospatial subset. Do not claim PostGIS compatibility from PostgreSQL syntax compatibility.
5. **Cloudflare object-backed C2/C3.** Add lazy R2 segments, bounded disposable Container caches, immutable generations, carefully bounded durable-log batching, branching, point-in-time restore, automated placement/maintenance, and production object-native durability after all section 10 gates pass.
6. **Graph records.** Introduce typed relation tables with `in` and `out` record IDs, `RELATE`, adjacency indexes, and traversal syntax. Define storage and query-plan gates before claiming graph support.
7. **Additional models and services.** Add direct key-value APIs, live queries/changefeeds, permissions, namespaces, WASM/mobile targets, partial sync, backups, and broader managed-cloud tooling.
8. **Concurrent writes.** Enable MVCC or multiprocess modes only after the corresponding Turso facilities are stable and pass FastDB's correctness, recovery, and performance workload suite.

Each roadmap item must update the format policy and `COMPAT.md`, add migration/reopen coverage where storage changes, and define an acceptance gate before implementation is considered complete.

## 14. Immediate Engineering Checklist

1. Make this workspace the FastDB monorepo based on the Turso fork history; record the pinned commit and configure the Turso `upstream` remote.
2. Have counsel finalize BSL 1.1 plus the Additional Use Grant, commercial license, Apache-2.0 Change License/Date, CLA, trademark policy, and inherited Turso notices.
3. Audit the pinned Rust binding, PostgreSQL frontend, JSONB/vector functions, expression and experimental index-method support, WAL/sync facilities, and failure-injection facilities.
4. Write the clean-room policy and initial `COMPAT.md` feature IDs.
5. Specify catalog DDL, format version 1, opaque-name encoding, canonical record-ID encoding, JSON-path encoding, and future hidden typed-column metadata.
6. Specify Cloud C0 database identity, log epoch/sequence, mutation identity, replay, generation, object-store portability, Durable Object fencing, ephemeral Container recovery, and conditional R2 manifest requirements without making Core network-dependent.
7. Implement the Phase 0 vertical slice without changing Turso core.
8. Run reopen, rollback, explain-plan, native-baseline, log-replay, and initial cloud-cost benchmarks.
9. Build the first `$5 / $20 / $100` cost model with bounded hypothetical quotas; do not publish quotas yet.
10. Record the Core and Cloud feasibility decisions and only then commit to the Phase 1 crate layout and public API details.

## 15. References

### Licensing

- [Open Source Definition](https://opensource.org/osd)
- [Business Source License 1.1](https://mariadb.com/bsl11/)
- [Adopting and developing BSL software](https://mariadb.com/bsl-faq-adopting/)

### Turso

- [Turso database manual and architecture](https://github.com/tursodatabase/turso/blob/main/docs/manual.md)
- [Turso PostgreSQL frontend compatibility design](https://github.com/tursodatabase/turso/blob/main/postgres/COMPAT.md)
- [Turso PostgreSQL frontend session implementation](https://github.com/tursodatabase/turso/blob/main/postgres/frontend/session.rs)
- [Initial pinned Turso commit](https://github.com/tursodatabase/turso/commit/977383ff40edc44ef410af062ed0d2322252a869)
- [Turso native extensions](https://docs.turso.tech/sql-reference/extensions)
- [Turso Cloud durability architecture](https://docs.turso.tech/cloud/durability)
- [Turso Cloud S3-native architecture](https://turso.tech/blog/turso-cloud-goes-diskless)
- [Turso Cloud pricing](https://turso.tech/pricing)

### Cloudflare deployment target

- [Cloudflare Workers limits](https://developers.cloudflare.com/workers/platform/limits/)
- [Workers virtual filesystem](https://developers.cloudflare.com/workers/runtime-apis/nodejs/fs/)
- [Cloudflare Durable Objects](https://developers.cloudflare.com/durable-objects/)
- [SQLite-backed Durable Object storage](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)
- [Cloudflare Containers architecture and ephemeral disk](https://developers.cloudflare.com/containers/platform-details/architecture/)
- [Cloudflare Container connections to Workers bindings](https://developers.cloudflare.com/containers/platform-details/workers-connections/)
- [Cloudflare R2 consistency](https://developers.cloudflare.com/r2/reference/consistency/)
- [Cloudflare R2 conditional Workers API](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/)

### SurrealDB behavioral reference

- [SurrealDB v3.1.5 release](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5)
- [`CREATE` statement](https://surrealdb.com/docs/reference/query-language/statements/create)
- [`SELECT` statement](https://surrealdb.com/docs/reference/query-language/statements/select)
- [`UPDATE` statement](https://surrealdb.com/docs/reference/query-language/statements/update)
- [`DELETE` statement](https://surrealdb.com/docs/reference/query-language/statements/delete)
- [`DEFINE TABLE` statement](https://surrealdb.com/docs/reference/query-language/statements/define/table)
- [`DEFINE INDEX` statement](https://surrealdb.com/docs/reference/query-language/statements/define/indexes)
- [SurrealDB repository and license notice](https://github.com/surrealdb/surrealdb)
- [SurrealDB architecture and storage backends](https://surrealdb.com/docs/architecture)
- [SurrealDB Cloud pricing](https://surrealdb.com/pricing)

These references inform design and behavioral research. They do not grant permission to copy SurrealDB implementation or tests, and links to moving `main` branches do not replace the recorded engine and behavior pins.
