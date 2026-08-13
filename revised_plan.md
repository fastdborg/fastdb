# FastDB Core Technical Plan and Roadmap

Status: Phase 11 stopped at the mandatory MVCC audit. Phase 12 begins the
approved broad-compatibility pre-1.0 track on serialized stable WAL. The Core
1.0 production-ready claim remains dormant behind a future exact-SHA
parallel-writer qualification gate.

## 1. Product Definition

FastDB is a clean-room, SurrealQL-compatible document database frontend built on a pinned fork of Turso. The completed MVP is an embedded Rust library and a command-line shell operating on a local Turso database. It provides a deliberately small, documented subset of SurrealQL rather than claiming full SurrealDB compatibility.

The active goal is a broad-compatibility pre-1.0 database pinned to SurrealDB
`v3.1.5`: richer values and queries, complete graph/search providers,
authentication and authorization, a self-hostable HTTP/WebSocket server,
multi-database routing, read-only attached databases, and local/remote Rust,
TypeScript, Go, and PHP SDKs. Geospatial behavior, versioned history,
changefeeds and realtime subscriptions, GraphQL/GQL, multiprocess access, and
parallel writers remain outside this track. FastDB Core is publicly developed
as MIT-licensed open-source software. A future proprietary managed service at
`cloud.fastdb.org` is a separate commercial product.

For the MVP, "single file" means one durable `.fastdb` database artifact after a checkpoint and clean shutdown. WAL and shared-memory sidecars may exist while a database is open. A later synchronization mode may also create Turso-owned metadata sidecars. FastDB will not promise that a live database consists of exactly one filesystem entry.

### 1.1 Selected baselines

- Start engineering from Turso `main` commit [`977383ff40edc44ef410af062ed0d2322252a869`](https://github.com/tursodatabase/turso/commit/977383ff40edc44ef410af062ed0d2322252a869). Before implementation begins, audit the then-current `main`, run the relevant upstream tests, and either retain this commit or record a newer reviewed commit in the repository. Never build from a floating branch in CI or releases.
- Use [SurrealDB `v3.1.5`](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5) as the behavioral reference for the compatibility matrix. Later SurrealDB behavior does not silently change the MVP contract.
- Use stable Turso WAL with full durability and serialized writers throughout
  Phases 12–22. Experimental multiprocess WAL remains excluded. A future Core
  1.0 may add opt-in parallel writers only after a new exact-SHA audit proves
  the complete Phase 23 gate without weakening the default.
- Preserve the embedded Rust API and CLI, then add a self-hostable server and
  local/remote Rust, TypeScript, Go, and PHP SDKs only through the reviewed
  FastDB parser/frontend boundary.
- Use this workspace as the FastDB monorepo. Preserve the Turso repository history, configure Turso as `upstream`, and add FastDB crates and service components directly to the same workspace; do not use a nested repository or hide the engine behind an unpinned submodule.

### 1.2 Product surfaces and business model

FastDB has two product surfaces with a strict boundary:

- **FastDB Core:** the MIT-licensed parser, frontend, storage integration, embedded Rust API, CLI, conformance suite, and eventually the self-hostable query server and sync components. The MIT grant makes local, self-hosted, redistribution, and commercial use open-source rights rather than a restricted free tier.
- **FastDB Cloud:** a future closed-source managed service that sells operation, durability, regional compute, authentication, metering, backups, restore, observability, support, and later edge synchronization. The hosted service may use Core under MIT and should preserve the same public query semantics and file-format policy.

FastDB Core source is public. FastDB Cloud code, deployment credentials, customer data, signing material, incident data, and live infrastructure state remain private. Cloud must stay outside the Core build and licensing boundary so using, building, testing, or self-hosting Core never depends on proprietary code.

The project will not depend on a permanent free cloud tier. A time-limited, payment-card-backed trial or small one-time credit may be offered only with a hard spend/resource cap and abuse controls. The initial pricing hypothesis is a simple three-tier ladder:

| Tier | Target price | Intended user | Commercial shape |
| --- | ---: | --- | --- |
| Starter | $5/month | Individual developers, prototypes, and small agents | Small included storage and operation credits; community support |
| Builder | $20/month | Deployed applications and small teams | Larger included credits, longer restore retention, team access when available |
| Pro | $100/month | Production applications | Higher limits, audit/security features, longer retention, and prioritized support |

Names, prices, quotas, retention, and overage rates are hypotheses until load tests and a cost model validate them. No tier includes unlimited storage, writes, compute, egress, sync traffic, or support. Overage billing is opt-in, exposes budget alerts and hard caps, and must never surprise the customer. A formal uptime or durability SLA is offered only after the corresponding failure tests, operational history, staffing, and financial exposure have been reviewed.

### 1.3 Licensing direction

FastDB-authored Core code is licensed under the repository's MIT License. The
grant permits use, modification, redistribution, sublicensing, sale, and
competing hosted services subject to the license's notice and disclaimer
conditions. The project must not describe Core as field-of-use restricted or
attempt to reserve managed-service rights in already MIT-licensed Core code.

All inherited Turso files retain their MIT notices and rights. Keep upstream
and FastDB-authored provenance mechanically auditable and include both Turso
and FastDB notices in distributions. Contributions accepted into Core must be
MIT-compatible; the former BSL/commercial/change-license model and its special
relicensing CLA requirement are retired.

FastDB Cloud is a separate proprietary product and may use Core under MIT.
Cloud-only orchestration, control-plane, billing, operational, and service
code may remain closed source, but proprietary components must not become a
build-time or run-time requirement for Core. Package versions remain `0.0.0`
and `publish = false` until the first alpha packaging decision; those controls
do not narrow the MIT license.

### 1.4 MVP success criteria

The MVP is complete only when all of the following are true:

- The supported syntax in `COMPAT.md` has independently authored conformance tests against the pinned behavioral reference.
- Catalog, physical-schema, and record changes are atomic, including automatic table creation.
- A database survives clean reopen and crash/recovery tests without partial logical changes.
- Schemafull validation and every declared index type are covered by positive and negative tests.
- Execution-plan tests prove that supported indexed predicates avoid full scans.
- The embedded API and CLI expose the same statement ordering, values, errors, and transaction behavior.
- Published benchmarks meet the gates in section 11 or clearly block release; no unmeasured absolute latency or "production-ready ACID" claim is made.

### 1.5 Pre-1.0 and dormant Core 1.0 success criteria

The broad-compatibility milestone is complete only after Phases 12–22 meet
their recorded gates:

- A locked machine-readable `v3.1.5` inventory assigns every atomic capability
  to a phase. At the Phase 22 gate every non-excluded entry is `Supported` or
  has an approved architecture stop report; no entry remains `Partial`.
- Format 1 and 2 databases migrate transactionally to format 3 and remain
  recoverable from committed fixtures, backups, and interrupted migrations.
- Rich values, expressions, CRUD, scripting, graph, FTS, exact vector, and
  FastDB-owned ANN behavior have clean-room conformance and physical-plan
  evidence through every public surface that claims support.
- Authentication and authorization execute inside the frontend; the server
  and SDKs never bypass logical catalogs, permissions, resource limits, or
  the direct translated-AST path.
- Cross-platform CI, sustained fuzzing, deterministic simulation, failure
  injection, crash recovery, migration, provider rebuild, protocol/security,
  SDK, provenance, and rollback gates pass on the exact pre-1.0 candidate.

This milestone does not authorize a production-ready or Core 1.0 claim. Phase
23 remains dormant until a new stable parallel-writer candidate qualifies.
Tagging, publishing, signing, and uploading always require separate approval.

## 2. Compatibility and Clean-Room Policy

FastDB will implement a documented compatibility subset from public SurrealQL specifications and observed public behavior. The team may run black-box queries against an unmodified SurrealDB `v3.1.5` binary and record inputs and outputs. FastDB's parser, implementation, fixtures, expected outputs, fuzz corpora, and conformance tests must be written independently.

Do not copy, translate, vendor, or adapt SurrealDB source code or test files. Keep behavioral research notes separate from implementation artifacts and record the public source or black-box experiment behind each compatibility decision. The project owner has approved the FastDB name and the precise phrase "SurrealQL-compatible subset"; it must never imply sponsorship, affiliation, certification, or complete compatibility, and it grants no rights in third-party marks.

`compat/surrealdb-v3.1.5.toml` is the locked machine-readable inventory and
`COMPAT.md` is its normative public view. Mechanical tests keep them in sync.
Each atomic capability is assigned exactly one status:

- **Supported:** implemented and covered by conformance tests.
- **Partial:** a documented subset is implemented; accepted and rejected forms are enumerated.
- **Planned:** assigned to a future active phase but not yet executable.
- **Unsupported:** not planned for the stated compatibility target.

`Partial` is a temporary phase-in-progress state. Phase 22 permits no Partial
rows: every capability must be Supported or carry a reviewed architecture stop
report. Splitting or merging rows after the inventory lock cannot be used to
improve the coverage result.

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

Format 2 may add opaque catalog-managed hidden typed columns for relation
endpoints, FTS text, and native vector encodings. These columns are derived
physical state, not user document fields. Their definitions and lifecycle are
owned by a closed internal provider and every change is atomic with `doc`.

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

Phase 6 migrates format 1 to format 2 and extends these catalogs with table
kind/relation metadata, analyzer definitions, index kind/provider/version/
options/state, capability requirements, and catalog-managed hidden typed
columns. Existing tables migrate as `NORMAL` and existing indexes as `BTREE`
without rewriting records or renaming physical objects. Format 2 remains the
completed Phase 6–11 format. Phase 12 migrates it transactionally to format 3;
format 3 remains pre-1.0 and is not frozen for a Core 1.0 claim.

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

This is the Phase 6–10 path for graph endpoints, full-text search, and vectors.
Phase 8 normalizes both the characterized SurrealQL FTS subset and the labeled
FastDB/Turso extension into one internal provider over hidden TEXT columns.
Phase 9 keeps vectors as public arrays while storing fixed-dimension finite
values in a native `vector64` BLOB hidden column for exact search. Phase 17 may
add FastDB-owned HNSW/MTREE providers over catalog-derived auxiliary state;
it must not expose the pinned toy sparse-IVF method, Turso's experimental
provider ABI, arbitrary loadable code, or an unqualified core change.

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

- All MVP acceptance and performance gates pass in the recorded local matrix. Crash injection reveals no partial logical state, every index has a plan test, fuzzing has no known crash, and release documentation states measured limitations without broader durability or compatibility claims. GitHub Actions is not required for the first public alpha; any later remote CI policy is a separate decision.

### Phase 6 — Format 2 and multimodel foundation

The authoritative implementation contract is
[`plan-phase6.md`](plan-phase6.md).

Deliverables:

- Retain the current Turso pin and record a read-only audit of its translated
  AST, FTS, vector, custom-index, explain, integrity, checkpoint, backup, and
  maintenance facilities. Perform the required read-only upstream comparison
  through the upstream-sync workflow, but do not merge or change the pin.
- Add independent AST/planner support for namespaced function calls,
  expression projections and aliases, structured provider options, `EXPLAIN`,
  `REMOVE INDEX`, and `REBUILD INDEX`. Unknown functions/providers/options fail
  before mutation.
- Transactionally migrate format 1 to format 2. Extend catalogs with table
  kind/relation metadata, analyzers, provider/version/options/state, capability
  requirements, and hidden typed columns. Migrate existing tables as `NORMAL`
  and indexes as built-in `BTREE` without rewriting documents.
- Introduce only a sealed internal provider contract. Do not expose dynamic
  native code, a public plugin ABI, generated user SQL, or logical-name
  interpolation.

Exit gate:

- Format 1 fixtures migrate, reopen, and remain usable; every injected
  migration failure rolls back; ordinary CRUD and index plans are unchanged;
  incompatible providers fail before mutation; executable Phase 6 additions
  have public-boundary evidence; and every Phase 5 technical gate still passes.

### Phase 7 — Graph records and bounded traversal

Deliverables:

- Implement the characterized SurrealDB `v3.1.5` subset of `DEFINE TABLE ...
  TYPE RELATION [IN|FROM table] [OUT|TO table] [ENFORCED]`.
- Add `RELATE [ONLY] record->relation->record [CONTENT|SET] [RETURN ...]` for
  record literals and bound record parameters, generated UUIDv7 edge IDs, and
  atomic registration of an absent schemaless relation table.
- Store edge user content in `doc`; store immutable endpoint table IDs and
  encoded RIDs in hidden typed columns; synthesize typed `id`, `in`, and `out`
  values while decoding.
- Maintain mandatory forward and reverse adjacency indexes and support chained
  fixed-depth `->`, `<-`, and `<->` traversal in `SELECT` projections, returning
  endpoint IDs or `.*` document materialization.
- Match the reference's dangling-edge default, validate endpoint existence for
  `ENFORCED`, and cascade-delete connected edges atomically when deleting a
  node.

Deferred: array/cartesian RELATE targets, explicit complex edge IDs, `OR
UPDATE`, edge-path filters, recursive paths, and standalone traversal
expressions.

Exit gate:

- Model, schemafull edge, reopen, abrupt-exit, cascade failure-injection, and
  conformance tests pass; execution plans prove both adjacency directions avoid
  scans; and cross-feature document behavior remains transactional.

### Phase 8 — Full-text search

Deliverables:

- Normalize two documented surfaces into one cataloged FTS provider: the
  characterized SurrealQL subset (`DEFINE ANALYZER`, single-field `FULLTEXT
  ANALYZER` indexes, `@@`/`@n@`, and selected `search::*` functions) and a
  labeled FastDB/Turso extension (`CREATE INDEX ... USING fts (...) WITH (...)`,
  `fts_match`, `fts_score`, and `fts_highlight`).
- Initially accept only Surreal analyzer configurations proven behaviorally
  equivalent, starting with the `blank` tokenizer and no function/filter
  pipeline. Expose Turso default/raw/simple/whitespace/ngram tokenizers and
  weights only through the extension surface.
- Maintain hidden TEXT and Turso FTS state atomically with documents. Keep the
  characterized Surreal scoring/highlighting contract distinct from Turso's
  native extension behavior.
- Map `REBUILD INDEX` to audited FTS optimization/segment maintenance. After an
  indexed write in an explicit transaction, reject a query touching that FTS
  index until commit instead of returning a stale pre-transaction view.

Exit gate:

- Actual FTS plan selection, churn, rollback, reopen, abrupt-exit, corruption,
  bounded-memory, ranking, highlighting, and rebuild tests pass. Unsupported
  WASM targets expose an explicit capability error rather than partial FTS.

### Phase 9 — Exact vector search and alpha-candidate surface

Deliverables:

- Add fixed-size `array<float, N>` fields. Preserve public arrays while
  maintaining a catalog-managed native `vector64` BLOB alongside JSON.
- Support exact `COSINE` and `EUCLIDEAN` KNN operator forms, corresponding
  `vector::` distance/similarity functions, bound vectors, ordinary predicate
  filtering before top-k selection, and distance projection.
- Require finite values and equal dimensions; enforce Turso's 65,536-dimension
  ceiling and `K <= 10,000`. Source literals keep the ordinary collection
  limit; larger embeddings must be bound parameters.
- Use bounded top-k memory, expose the exact scan through structured `EXPLAIN`,
  and reject HNSW, DiskANN, and the pinned `toy_vector_sparse_ivf` method.

Exit gate:

- Results match independent calculations, filtered top-k semantics are exact,
  document/BLOB state is atomic through rollback and reopen, and scan/storage
  benchmarks are within the native-equivalent gate. This surface is a `0.1`
  alpha candidate only; publishing still requires explicit authorization.

### Phase 10 — Operational readiness

Deliverables:

- Add bounded `QueryOptions` and `ResourceLimits`,
  `query_with_options`/`execute_with_options`, deterministic `Database::close`,
  consistent `Database::backup_to`, and CLI `check`, `backup`, `restore`, and
  index-rebuild commands.
- Bound time, output rows/bytes, graph hops, vector dimensions, and FTS query
  work without logging query source or parameter values.
- Provide one supported check path for catalogs, hidden columns, adjacency
  indexes, FTS state, vector encodings, and engine integrity.
- Document and test backup consistency, restore validation, interrupted
  backups, tracing/metrics hooks, graceful checkpointing, busy behavior, and
  upgrade/rollback procedures.

Exit gate:

- Randomized backup/restore hashes match, every provider index rebuilds from
  documents, limits fail without partial state, and clean/abrupt shutdown tests
  retain every acknowledged commit.

### Phase 11 — Parallel writers and snapshot isolation

Begin with a mandatory upstream audit. The pinned MVCC implementation is
experimental and is not a production candidate. No audited candidate proved
recovery, garbage collection, bounded memory, and acceptable checkpoint
behavior, so Phase 11 stopped the parallel-writer/Core 1.0 path. The separately
approved serialized-writer pre-1.0 compatibility track begins at Phase 12.

Deliverables, only after a candidate qualifies:

- Integrate one exact audited SHA through the repository's mandatory
  upstream-sync workflow, updating every pin and evidence record atomically.
- Add `ConcurrencyMode::{Serialized, ParallelWrites}` and
  `Builder::concurrency_mode`; retain stable WAL/serialized writers as the
  compatibility default and make parallel mode an opt-in supported capability.
- Guarantee snapshot isolation. Add `ErrorCategory::Conflict` and
  `Error::is_retryable`; return conflicts after rollback without implicit
  transaction replay.
- Keep schema/catalog work serialized while allowing non-conflicting data
  transactions to write concurrently. Cross-process access remains excluded.

Exit gate:

- Graph, B-tree, FTS, vector-derived state, backup, schema publication,
  conflicts, long readers, checkpoints, crash recovery, starvation, and memory
  growth pass under parallel writers. Public feature claims cannot waive the
  exact checked-out implementation audit. Passing this phase defines the `0.9`
  beta-candidate surface; publishing remains separately authorized.

### Phase 12 — Roadmap reset, inventory, and format 3

Lock the atomic SurrealDB `v3.1.5` inventory, transactionally migrate format 2
to format 3, and add collision-safe non-geospatial value encodings including
bytes, datetime, duration, decimal-compatible numbers, sets, ranges, and richer
typed collections. Preserve format 1/2 fixtures, migration rollback, reopen,
backup/restore, unknown-version refusal, and all Phase 6–10 behavior. The
authoritative contract is `plan-phase12.md`.

### Phase 13 — Expressions, operators, and built-in functions

Complete bounded collection/range access, indexing, slicing, casts, operators,
subexpressions, and the characterized pure function families. External-resource
functions are deny-by-default capabilities with SSRF, redirect, DNS, timeout,
size, and concurrency controls. Geo and history functions remain unsupported.

### Phase 14 — CRUD and query completeness

Add INSERT, UPSERT, richer mutation forms and return modes, subqueries,
aggregations, grouping, split/omit/fetch, multiple targets, and analyze forms.
Keep values bound, exact FastDB evaluation authoritative, predicate pushdown
proven safe, and every index claim backed by an execution plan.

### Phase 15 — Scripting, schema, views, and events

Add bounded LET/RETURN/control flow, custom functions and parameters, views,
events, defaults, assertions, computed/readonly fields, and matching
REMOVE/INFO operations. Events execute atomically inside the frontend with
recursion, statement, time, and output limits.

### Phase 16 — Graph compatibility completion

Add cartesian RELATE, complex edge IDs, OR UPDATE, relation endpoints, path
filters, standalone and recursive traversal. Preserve immutable endpoints,
two-way adjacency, cascade atomicity, deterministic cycle handling, resource
limits, and scan-free plans in both directions.

### Phase 17 — Search, analyzers, and specialized indexes

Complete behaviorally equivalent analyzers, multi-field/boolean FTS, remaining
exact vector forms, and non-geospatial specialized indexes. Add FastDB-owned
HNSW/MTREE providers with versioned derived state, exact fallback, rebuild,
failure/reopen/corruption tests, bounded memory, plan evidence, and recall
measurements. Do not change Turso core or expose its toy provider.

### Phase 18 — Authentication and authorization kernel

Add embedded session principals, database and record users, signup/signin,
JWT/session behavior, Owner/Editor/Viewer roles, reserved auth context, and
table/row/field/function permissions enforced before candidate materialization.
Use bounded Argon2id work, expiration/revocation, redaction, audit events, and
deny-by-default record-user permissions.

### Phase 19 — Secure single-database HTTP/WebSocket server

Add a `fastdb-server` crate and `fastdb serve` over the FastDB API. Clean-room
implement the applicable `v3.1.5` HTTP/WebSocket RPC methods and encodings,
excluding LIVE/KILL subscriptions and GraphQL/GQL. Require authenticated
administration, bounded workers/backpressure, loopback defaults, TLS for
non-loopback binds, resource/rate limits, and acknowledged-write recovery.

### Phase 20 — Multi-database control plane and read-only ATTACH

Add a durable namespace/database control catalog mapping opaque IDs to one
`.fastdb` file each. Implement USE and namespace/database lifecycle behavior.
Add connection-local, maximum-ten, read-only ATTACH/DETACH as a labeled FastDB
extension. Validate files and canonical paths; reject writes, cross-file
relations/transactions, and remote attachment without an administrator path
allowlist. Do not expose Turso's inherited experimental flag.

### Phase 21 — Rust, TypeScript, Go, and PHP SDKs

Add a versioned FastDB C ABI and local/remote clients. Raw paths and `file://`
open embedded files, `mem://` opens memory, and HTTP/WS URLs select remote
transport. Rust, Node/TypeScript, Go, and PHP support both modes where their
native runtimes permit; browsers remain remote-only. All local paths call the
FastDB API rather than inherited Turso bindings.

### Phase 22 — Broad-compatibility hardening gate

Resolve every locked non-excluded capability to Supported or an approved
architecture stop, with no Partial rows. Pass cross-platform migration,
fuzzing, simulation, failure/crash, provider rebuild, authorization, protocol,
SDK, backup/restore, security, dependency, provenance, and rollback gates.
Produce a pre-1.0 report only; publishing and release operations remain
separately authorized.

### Phase 23 — Dormant Core 1.0 gate

Start only when a new exact stable Turso parallel-writer candidate exists.
Repeat the upstream/MVCC audit from scratch. Core 1.0 remains blocked unless
snapshot isolation, conflicts, recovery, checkpointing, long-reader memory,
providers, security, server, and SDK suites all qualify.

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
- **Provider/model:** graph adjacency and cascade models, FTS ranking/highlight
  cases, independent vector-distance/top-k calculations, ANN recall/rebuild,
  and document/derived storage atomicity.
- **Security/protocol:** authentication, authorization non-disclosure,
  capability/SSRF boundaries, malformed HTTP/WebSocket frames, session
  isolation, backpressure, TLS, and opaque-client RPC differentials.
- **SDK:** one independently authored typed-value/query/authentication corpus
  across embedded and remote Rust, TypeScript, Go, and PHP clients.
- **Operations:** randomized backup/restore hashes, interrupted maintenance,
  resource ceilings, deterministic close, and upgrade/rollback drills.
- **Concurrency:** stable serialized writes, concurrent readers, busy/error
  behavior, long readers, checkpoint progress, crash recovery, and bounded
  server queues. Parallel-mode testing belongs only to dormant Phase 23.
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
- Unknown or incompatible providers, encodings, or catalog states cause a
  read-before-write failure and never silently skip derived data.
- Relation endpoints are immutable; both adjacency indexes and cascade cleanup
  commit or roll back with the edge/node mutation.
- FTS text and vector BLOBs remain derived from the same committed document;
  provider rebuild can recover them without changing the logical document.
- Authorization constrains graph, FTS, vector, aggregation, ordering, and event
  candidates before any hidden record can affect observable results.
- Server and SDK execution always enters through the independent FastDB parser
  and frontend; no remote or native binding exposes inherited Turso SQL.
- One logical server database maps to one opaque `.fastdb` file. Read-only
  attached files never participate in a write or cross-file transaction.

## 9. Durability, Concurrency, and File Semantics

The compatibility default favors known behavior over maximum write
concurrency:

- Use stable WAL and the strongest supported synchronous/full-durability configuration established by the audited Turso baseline.
- Support the audited single-writer behavior; return a stable busy/transaction error rather than inventing retries that could duplicate statements.
- Do not enable experimental MVCC, `BEGIN CONCURRENT`, or experimental multiprocess WAL.
- Serialize catalog and physical schema mutation with a process-local database schema mutex. Document that cross-process concurrent access is not supported in the MVP.
- On clean close, perform or request a safe checkpoint according to the audited Turso API. Never claim sidecars cannot exist after abnormal termination.
- Recovery tests must include abandoned WAL files and reopening after process termination at each logical mutation boundary.

Turso facilities and defaults can change. Re-audit these choices whenever the pinned engine commit changes; do not infer safety from a feature name alone.

Phase 23 may resume parallel-writer qualification only after a new exact stable
upstream candidate exists. Serialized writers remain the default. Busy or
future conflict errors are returned only after rollback and FastDB never
replays an application transaction implicitly. Catalog and schema operations
remain serialized, and the active pre-1.0 track does not support cross-process
concurrent access.

## 10. FastDB Cloud Architecture and Economics

Cloud research may inform durable Core boundaries because identifiers, logical
mutation logging, CDC, sync metadata, and format choices can constrain a future
service. Cloud implementation is not part of the active Phase 12–22 track, is
inactive without a separately approved cloud plan, and must never make local
Core depend on a network service.

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
  bounded authoritative recent recovery journal in DO SQLite
                  |
                  v
Cloudflare Container running native FastDB/Turso
  query frontend | transaction owner | ephemeral local file/cache
                  |
                  v
deterministic replay artifact returned to the Durable Object
  sequence | checksum | mutation ID | result digest
                  |
                  v
verified immutable WAL/log batches in R2
  contiguous sequence ranges | checksums | recovery metadata
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
- A database ID deterministically routes to one container-backed Durable Object. The object coordinates the active writer, epoch/fencing token, request idempotency, container lifecycle, and a bounded recent recovery journal. SQLite-backed Durable Object storage may be the synchronous durability point for that journal, but it must contain only protocol metadata, bound mutation intents, deterministic replay artifacts, and result/idempotency records. It must not store user documents as a queryable second database or replace Turso as FastDB's query/storage engine.
- The native Rust FastDB/Turso engine runs in a Cloudflare Container with an ordinary local `.fastdb` file and WAL. Container disks are ephemeral; sleep, eviction, host replacement, and deployment must be treated as routine recovery events.
- The active local database is a disposable cache reconstructed from an R2 checkpoint generation, verified R2 log batches, and any newer committed Durable Object journal tail. It is never the sole durable copy of an acknowledged commit.
- Do not run the active mutable database directly on an R2 FUSE mount. FUSE may be useful for import, export, or diagnostics, but object-store filesystem semantics and latency are not a substitute for database pager/WAL semantics.
- Access R2 through Workers bindings/outbound handlers or its S3-compatible API with least-privilege, database-scoped prefixes. Immutable data objects use content-derived or generation/sequence keys; the small current manifest is published conditionally using its prior version/ETag.
- R2's consistency and conditional operations simplify publication but do not replace transaction ordering or fencing. The Durable Object remains the per-database coordinator, and every manifest/log transition must be independently recoverable.
- Awaiting a Container request or R2 operation can permit other Durable Object events to interleave. The write path must use an explicit per-database queue or concurrency barrier around sequence allocation, Container execution, and journal finalization; correctness must not rely on automatic storage input/output gates alone.
- The journal is bounded, not an indefinite primary store. Alarms, Queues, or Workflows batch contiguous committed journal ranges into immutable R2 objects, verify publication, advance a fenced archive watermark, and only then delete the corresponding journal prefix. R2 delay or failure applies backpressure before the journal reaches its provider limit; it never causes journal overwrite or acknowledgement from ephemeral state.
- Queues and Workflows may also schedule checkpointing, compaction, retention, verification, and garbage collection. Background workers may publish state only while holding a valid generation/epoch fence; delivery retries must be idempotent.
- Do not assume the Worker, Durable Object isolate, and Container are co-located. Measure each hop and use placement features only as optimizations, never correctness requirements.

The primary C0 durable-commit candidate is a Durable Object journal with asynchronous R2 packing:

1. While holding the database write serialization boundary, durably allocate an epoch, monotonic sequence, and mutation/request ID in SQLite-backed Durable Object storage before asking the Container to mutate state. Bind all generated IDs and other nondeterministic inputs into the intent so retry and replay are deterministic.
2. Execute and fsync the local FastDB transaction. The Container returns the exact independently specified logical mutation batch or audited physical Turso WAL/sync artifact required to reconstruct the commit, plus its sequence, checksum, and result digest.
3. Atomically finalize the Durable Object journal entry and idempotency result. Do not acknowledge the client until this durable write is confirmed. If the Container may have advanced but finalization is absent or failed, fence and discard that local generation and reconstruct it before serving another mutation; an unacknowledged local "ghost" commit must never become visible as authoritative state.
4. Pack contiguous committed entries into substantially larger immutable R2 log objects on a size/time threshold, publish a fenced archive watermark or generation manifest, verify recovery, and delete only the safely archived journal prefix.
5. Recover by hydrating the latest verified R2 generation, replaying subsequent R2 batches, then replaying the committed Durable Object journal tail. A repeated request returns its stored outcome without applying the mutation twice.

C0 must still determine the exact replay artifact and protocol states from audited Turso facilities. Journal entries must be bounded and chunked within current provider limits, and point-in-time recovery of Durable Object storage is an operational aid rather than the database recovery protocol. Direct per-commit R2 publication remains a measured comparison, not the C1 default. If the Durable Object journal cannot meet latency, throughput, capacity, availability, or recovery gates, stop before paid service and move the authoritative log to the regional multi-tenant fallback below; never acknowledge from ephemeral disk or silently weaken durability.

Turso Cloud publicly describes one implementation using local compute caches, S3 Express One Zone for recent durable commits, S3 for checkpointed generations, and cross-database/time batching to amortize object-request cost. FastDB may learn from that architecture but does not inherit a complete Turso Cloud storage server from the public Turso engine fork. The current public repository exposes useful sync/WAL protocol concepts, not the production multi-tenant cloud data plane. Every required component must therefore be located in audited upstream code, implemented independently, or purchased as an external service with its cost and guarantees recorded.

The fallback is hybrid rather than an all-or-nothing Cloudflare exit: keep the Cloudflare Worker as the global API/auth/routing edge, but route each database to a regional FastDB storage shard that hosts many isolated database files, group-commits across active databases to a low-latency durable log such as S3 Express, caches locally, and checkpoints immutable generations to S3-compatible object storage. This is the closest FastDB analogue to Turso's public design. It is selected only if the Cloudflare-native journal fails its gates because it adds tenant placement, leases/fencing, cross-database group commit, noisy-neighbor control, cache eviction, hot-database movement, metering, and regional operations. Merely changing object-store vendors while retaining one object mutation per logical commit does not fix the economics.

Cloud invariants include:

- A successful response never precedes the durability point promised by its plan.
- At most one unfenced writer owns a database/log epoch at a time.
- Retried requests and WAL uploads are idempotent and cannot duplicate logical mutations.
- A compute worker can disappear after acknowledgement without losing the commit.
- Manifests never expose a partial generation and can recover from an interrupted checkpoint.
- Local caches are disposable and never the sole durable copy of acknowledged data.
- The Durable Object journal cannot be truncated until its archived R2 range and restore path are verified; backlog limits trigger admission control before data loss.
- Restore is continuously exercised, not inferred from object presence.
- Database deletion, retention, legal hold, and tenant erasure have explicit object-lifecycle semantics.

### 10.2 Staged cloud delivery

#### Activation rule

Cloud implementation does not begin merely because a Core phase completes. It
requires separate scope and authorization and may not broaden or modify the
active Core phase. An initial deliverable is C0 evidence plus an internal or
explicitly invited single-region technical preview, not a production launch,
multi-region service, durability SLA, or unlimited public signup.

Parallel work may build the separate Worker gateway, container image, database provisioning, credentials, metering in shadow mode, Durable Object journal, R2 archival path, recovery harness, and a provisional SDK. The cloud adapter consumes the same reviewed FastDB frontend boundary as local use and must not introduce network-only semantics into Core. External charging and durability claims remain blocked until the applicable C0/C1 exit gates pass.

#### Cloud C0 — Architecture and cost feasibility

Run only under a separately approved Cloud C0 plan and without shipping a
generally available service:

- Specify global database IDs, log sequence/epoch rules, mutation IDs, and CDC/sync metadata.
- Determine which Turso sync/log facilities are stable and reusable at the pinned commit.
- Prototype replay from an immutable WAL/log into a clean `.fastdb` file.
- Prototype a Worker-to-container-backed-Durable-Object request path and the bounded SQLite-backed Durable Object recovery journal without making either a Core dependency.
- Specify and failure-test every state in the intent, Container execution, journal finalization, R2 packing, archive-watermark, checkpoint, truncation, and reconstruction protocol. Inject loss after every transition, including Durable Object restart and Container commit before journal finalization.
- Prove that a native FastDB container can hydrate from an R2 generation plus R2 batches and a Durable Object journal tail, execute a transaction, lose all local disk, and recover every acknowledged result exactly once.
- Benchmark journal intent/finalization, Worker-to-Durable-Object-to-Container hops, R2 PUT/GET/range latency, conditional-manifest publication, batch size/delay, archive backlog, cold hydration, generation size, checkpoint frequency, cache hit rate, and restore time using realistic document and index workloads.
- Compare the Durable Object journal with direct per-commit R2 publication and a small regional multi-tenant group-commit prototype. Record limits, availability assumptions, request sizes, maximum batching delay, physical object operations per logical transaction, and failure semantics; do not select by nominal storage price alone.
- Define an object-store portability contract and run the recovery prototype against R2 plus at least one S3-compatible local test service.
- Build a cost model for each proposed price tier and at low, expected, and adversarial utilization.
- Keep proprietary Cloud code and operational data outside the MIT Core boundary, and define Cloud customer, privacy, security, ownership, and service terms before accepting external customers. These Cloud decisions do not block Core feature development.

Exit gate: a reviewed design demonstrates a recoverable journal/log and database-generation model after forced Durable Object and Container loss, proves fencing and idempotent retry behavior, bounds journal backlog under R2 outage, identifies all non-upstream and Cloudflare-specific components, and shows a credible path to positive unit economics. Failure does not block the local MVP; it blocks external charging, durability promises, and advancement beyond a disposable internal prototype.

#### Cloud C1 — Private Cloudflare native-container alpha

Start only after the C0 protocol gate and separate authorization, behind an
invite-only boundary:

- Add a Cloudflare Worker HTTP/WebSocket gateway, authentication, organizations/projects, database provisioning, SDK credentials, quotas, and usage metering.
- Route each database to a named container-backed Durable Object that owns writer fencing, request idempotency, the bounded authoritative recent recovery journal, archive watermark, and native FastDB Container lifecycle.
- Run authoritative query execution in the native Container. Hydrate the complete checkpoint and log tail eagerly on cold start, use ephemeral local disk while active, durably finalize the ordered recovery artifact in Durable Object SQLite before acknowledgement, asynchronously pack verified contiguous ranges into R2, and periodically publish versioned whole-file or coarse-generation checkpoints.
- Keep the C1 recovery format simple and auditable. Lazy page/segment fetch, shared cache infrastructure, branching, and aggressive multi-tenant packing belong to C2.
- Add encrypted transport, encryption/key policy at rest, tenant isolation tests, rate limits, audit events, observability, and automated restore drills.
- Begin with shadow billing and a hard account/resource allowlist. A small paid alpha or card-backed capped trial may start only after recovery, isolation, metering, spend-cap, legal, and support gates pass; do not offer a permanent free cloud tier.

Exit gate: measured metering reconciles with engine activity; every acknowledged commit survives forced Durable Object restart and Container termination and reconstruction from the verified R2 state plus journal tail; archive backlog remains bounded under fault injection; restore drills meet the documented objective; tenant isolation tests pass; and no workload can exceed its configured spend/resource cap.

#### Cloud C2 — Lazy-segment, multi-tenant Cloudflare beta

- Replace eager whole-database hydration with immutable chunked R2 generations, range/segment reads, bounded disposable local caches, background checkpointing, branching, and point-in-time restore.
- Keep recent ordered log data and historical generations in separately tunable R2 layouts. Retain the Durable Object journal as a bounded tail or replace it with the approved regional durable-log tier only if C0/C1 measurements and recovery tests justify the migration.
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

The cost worksheet must expose physical amplification rather than assuming one provider operation per customer request:

```text
DO requests / logical request
DO rows read and written / committed transaction
R2 Class A and Class B operations / committed transaction
durable and checkpoint bytes / logical written byte
Container active milliseconds and cold-start milliseconds / request
restore bytes and operations / database wake
```

The economic design target is `R2 Class A operations / committed transaction << 1` at sustained load, achieved by packing contiguous journal ranges and amortizing manifest/checkpoint publication. A low-traffic database may flush a small batch after a bounded delay, but the service-wide p95 mix must still pass the tier margin gate. Every cost report records the provider price sheet and date used because request prices, included allowances, rounding, and platform limits change. One R2 log object plus one manifest mutation per logical commit is a rejected steady-state design for the proposed low-price tiers unless new measurements prove otherwise.

Commercial rules:

- Local and self-hosted FastDB remain the free acquisition path.
- There is no permanent free hosted plan at launch.
- Trials require a hard expiration and resource/spend cap; requiring a payment card is an acceptable anti-abuse control.
- The `$5` tier receives community support and cannot include costly manual operations.
- Overage billing is disabled by default and can be capped by the customer.
- Annual discounts wait until retention, support load, and variable cost are understood.
- Do not offer lifetime plans or unbounded "unlimited" features.
- Pricing comparisons use equivalent durability, retention, region, egress, support, and workload assumptions.

FastDB should position Core as a lightweight MIT-licensed open-source document database for embedded, edge, database-per-agent, and database-per-tenant applications, with a proprietary managed service for users who want operations handled for them. Price is a wedge, not proof of superiority. Claims against Turso or SurrealDB require reproducible feature-adjusted benchmarks and total-cost examples.

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
- Graph traversal, FTS query, and exact vector search: p95 no worse than 2x
  equivalent native Turso workloads over the same physical endpoint columns,
  FTS index, or native vector representation and with equivalent result
  materialization.
- Every declared index is exercised by a result-correctness and execution-plan test.
- Explicit transactions are all-or-nothing across catalog, physical schema, and data changes.
- Provider-derived storage and provider rebuilds preserve logical document
  hashes across failure, reopen, backup, and restore.
- Parallel mode, if enabled, passes snapshot, conflict, recovery, checkpoint,
  starvation, and bounded-memory gates without regressing serialized mode.

Benchmarks are regression gates, not marketing claims. Do not publish absolute latency, scalability, edge, or "production-ready ACID" claims until the associated benchmark, concurrency, and crash-test reports are public.

Maintain a separate, non-gating competitive suite against the pinned SurrealDB behavioral release for embedded point reads, indexed document filters, document writes, startup, resident memory, storage amplification, and supported concurrent workloads. Publish complete configurations and distinguish embedded from networked deployments. These results inform positioning and pricing; they never replace Core's native-Turso regression gates or justify claims about unsupported SurrealDB features.

## 12. Risks and Decision Triggers

| Risk | Initial mitigation | Trigger for revisiting the design |
| --- | --- | --- |
| Turso internal AST APIs change | Pin a commit; isolate lowering behind one crate | Rebase breaks translation or requires broad core patches |
| Expression indexes do not cover required JSON predicates | One canonical expression builder; Phase 0 explain test | Supported filter cannot select its matching index |
| JSONB loses required value distinctions | Define round-trip tests and public value limits | Record IDs, integers, missing/null, or nested values cannot round-trip |
| DDL/catalog atomicity differs from assumptions | Failure injection in the vertical slice | Reopen exposes partial catalog or physical schema |
| Compatibility research crosses clean-room boundaries | Clean-room policy, provenance notes, independent review | Any implementation artifact derives from prohibited source/tests or public claims expand beyond recorded evidence |
| Schema mutex is insufficient across handles/processes | Registry by canonical database identity; no multiprocess support | Concurrent opens bypass serialization or Turso adds stable native coordination |
| Stable WAL does not meet a durability assumption | Pin settings and run crash tests | Corruption, partial logical change, or unsupported checkpoint behavior appears |
| Performance target is missed | Profile parsing, planning, JSON encoding, and decoding separately | A release gate fails on the published benchmark harness |
| Specialized values cannot be indexed from JSONB efficiently | Use versioned hidden typed columns or auxiliary index tables | Vector, full-text, or geo prototype requires per-row conversion or a full scan |
| Experimental Turso index/extension APIs change | Keep providers internal and versioned; do not freeze a public ABI | Upstream API churn breaks reopen, maintenance, or planner matching |
| FTS transactional visibility returns stale results | Track indexed writes and reject affected FTS reads until commit | Any explicit transaction can observe a stale or partially maintained FTS view |
| Exact vector search becomes an accidental ANN claim | Expose the bounded linear scan in `EXPLAIN`; reject toy/ANN methods | Documentation, planner output, or benchmarks imply an unavailable production ANN index |
| Relation cascade or adjacency state diverges | Hidden immutable endpoints, mandatory two-way indexes, atomic failure injection | A committed edge is missing from one direction or a deleted node leaves an unintended connected edge |
| Parallel writers depend on experimental MVCC | Mandatory exact-SHA audit and Phase 11 hard stop | Recovery, garbage collection, memory, checkpoint, or snapshot-isolation evidence is absent |
| Object presence is mistaken for database durability | Specify ordered log, manifest, fencing, and acknowledgement invariants; inject failures | An acknowledged commit is absent or duplicated after worker loss/replay |
| Cloudflare Worker storage is treated as a persistent filesystem | Keep Workers at the edge; run native FastDB in a Container and persist recovery artifacts before acknowledgement | Any correctness path depends on Worker `/tmp`, isolate lifetime, or in-memory state |
| Ephemeral Container disk is mistaken for durable state | Reconstruct from R2 generations, archived log batches, and the committed Durable Object journal tail; continuously kill Containers in recovery tests | Forced sleep/replacement loses an acknowledged commit or requires manual repair |
| Container state and the Durable Object journal diverge across their non-atomic boundary | Durable intent and sequence allocation, deterministic replay artifacts, idempotency records, explicit write serialization, and fencing/discard of any unjournaled local generation | An unacknowledged local commit becomes visible, an acknowledged result lacks replay data, or retry duplicates a mutation |
| Durable Object journal backlog exceeds capacity during R2 delay or outage | Size/time batching, archive watermark, alarms, backlog metrics, reserved headroom, and admission control before provider limits | The service must overwrite/truncate unarchived entries, cannot recover within the target, or availability assumptions make bounded backlog impractical |
| R2 latency or request cost makes the archive path uneconomic | Acknowledge against the bounded Durable Object journal, pack contiguous ranges, amortize manifests/checkpoints, and measure physical operations per logical commit | Approved tier latency or p95 variable-margin gate fails despite batching |
| Cloudflare coupling prevents portability or recovery outside the service | FastDB-owned object-store/recovery interfaces; S3-compatible test backend; no platform types in Core | A database cannot be exported/recovered without proprietary live control-plane state |
| Cloudflare-native durability cannot meet the service gates | Preserve the Worker edge and move the data plane to independently implemented regional multi-tenant FastDB shards with group commit and S3-compatible generations | Durable Object latency, throughput, capacity, regional behavior, or economics fail C0/C1 after the protocol is optimized |
| Multi-tenant isolation fails | Database-level tenancy, hard resource limits, adversarial tests, least-privilege credentials | Cross-tenant access, noisy-neighbor outage, or unbounded resource use |
| The `$5` tier is structurally unprofitable | Bounded credits, no included manual support, C0/C1 cost model | p95 included usage has non-positive variable gross margin |
| The Core/Cloud boundary becomes ambiguous | MIT metadata and notices in Core; proprietary service code kept separate | Core depends on private code, Cloud materials imply MIT rights were revoked, or distributions omit required notices |

Any failed Phase 0 feasibility assumption is a design decision point, not permission to add a hidden core fork or relax correctness criteria.

## 13. Pre-1.0 scope and later roadmap

Phases 12–22 are the ordered broad-compatibility track. They preserve the
SurrealDB `v3.1.5` behavior pin and stable serialized WAL while expanding Core
to local and remote delivery. Each phase updates the locked inventory and
`COMPAT.md` only for executable, evidenced behavior and updates format,
migration, reopen, failure, benchmark, protocol, security, and release records
when relevant.

This track includes general datetime/duration behavior but excludes versioned
history, changefeeds, time-series retention, and realtime/LIVE queries. It also
excludes geospatial/geometry, GraphQL/GQL, multiprocess access, and parallel
writers. Native FTS and ATTACH/DETACH remain labeled FastDB extensions and do
not count as SurrealQL compatibility.

After Phase 22, separately plan and gate:

1. Realtime subscriptions and changefeeds only after a logical, transaction-
   aware, resumable delivery design qualifies; Turso's early-preview physical
   CDC table is not sufficient by itself.
2. Geospatial/geometry and specialized time-series/history behavior.
3. Experimental Turso Sync using an authoritative remote and explicit
   push/pull/checkpoint semantics, never independently invented peer-to-peer
   replication.
4. Cloud C0–C3 and any regional-shard fallback under the independent recovery,
   isolation, security, and economics gates in section 10.
5. Phase 23 parallel writers and Core 1.0 only after a stable audited engine
   facility passes correctness, recovery, checkpoint, and bounded-memory gates.

## 14. Immediate Engineering Checklist

1. Preserve the completed Phase 10 baseline and the Phase 11 stopped audit in
   their existing plans/reports. Do not rewrite historical evidence.
2. Commit this roadmap reset as an isolated rollback point, then commit the
   authoritative `plan-phase12.md` before changing executable behavior.
3. Lock the machine-readable SurrealDB `v3.1.5` capability inventory using only
   public documentation and independently authored black-box observations.
4. Preserve format 2 fixtures, stable serialized WAL, direct translated AST,
   opaque physical names, sealed providers, and every earlier verification
   gate while implementing transactional format 3 migration.
5. Finish each phase with a report/checkpoint commit before starting the next.
   Do not tag, publish, upload, or claim production readiness without separate
   authorization.

## 15. References

### Licensing

- [Open Source Definition](https://opensource.org/osd)
- [FastDB and Turso MIT License](LICENSE.md)

### Turso

- [Turso database manual and architecture](https://github.com/tursodatabase/turso/blob/main/docs/manual.md)
- [Turso PostgreSQL frontend compatibility design](https://github.com/tursodatabase/turso/blob/main/postgres/COMPAT.md)
- [Turso PostgreSQL frontend session implementation](https://github.com/tursodatabase/turso/blob/main/postgres/frontend/session.rs)
- [Initial pinned Turso commit](https://github.com/tursodatabase/turso/commit/977383ff40edc44ef410af062ed0d2322252a869)
- [Turso native extensions](https://docs.turso.tech/sql-reference/extensions)
- [Turso full-text search functions and limitations](https://docs.turso.tech/sql-reference/functions/fts)
- [Turso vector search](https://docs.turso.tech/guides/vector-search)
- [Turso concurrent writes](https://docs.turso.tech/tursodb/concurrent-writes)
- [Turso Cloud durability architecture](https://docs.turso.tech/cloud/durability)
- [Turso Cloud S3-native architecture](https://turso.tech/blog/turso-cloud-goes-diskless)
- [Turso Cloud pricing](https://turso.tech/pricing)

### Cloudflare deployment target

- [Cloudflare Workers limits](https://developers.cloudflare.com/workers/platform/limits/)
- [Workers virtual filesystem](https://developers.cloudflare.com/workers/runtime-apis/nodejs/fs/)
- [Cloudflare Durable Objects](https://developers.cloudflare.com/durable-objects/)
- [Durable Object input/output gates and concurrency rules](https://developers.cloudflare.com/durable-objects/best-practices/rules-of-durable-objects/)
- [SQLite-backed Durable Object storage](https://developers.cloudflare.com/durable-objects/api/sqlite-storage-api/)
- [Durable Object limits](https://developers.cloudflare.com/durable-objects/platform/limits/)
- [Durable Object pricing](https://developers.cloudflare.com/durable-objects/platform/pricing/)
- [Cloudflare Containers architecture and ephemeral disk](https://developers.cloudflare.com/containers/platform-details/architecture/)
- [Cloudflare Container connections to Workers bindings](https://developers.cloudflare.com/containers/platform-details/workers-connections/)
- [Cloudflare Containers pricing](https://developers.cloudflare.com/containers/pricing/)
- [Cloudflare R2 consistency](https://developers.cloudflare.com/r2/reference/consistency/)
- [Cloudflare R2 conditional Workers API](https://developers.cloudflare.com/r2/api/workers/workers-api-reference/)
- [Cloudflare R2 pricing](https://developers.cloudflare.com/r2/pricing/)

### SurrealDB behavioral reference

- [SurrealDB v3.1.5 release](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5)
- [`CREATE` statement](https://surrealdb.com/docs/reference/query-language/statements/create)
- [`SELECT` statement](https://surrealdb.com/docs/reference/query-language/statements/select)
- [`UPDATE` statement](https://surrealdb.com/docs/reference/query-language/statements/update)
- [`DELETE` statement](https://surrealdb.com/docs/reference/query-language/statements/delete)
- [`DEFINE TABLE` statement](https://surrealdb.com/docs/reference/query-language/statements/define/table)
- [`DEFINE INDEX` statement](https://surrealdb.com/docs/reference/query-language/statements/define/indexes)
- [`RELATE` statement](https://surrealdb.com/docs/reference/query-language/statements/relate)
- [Graph model](https://surrealdb.com/docs/learn/data-models/graph/overview)
- [Graph traversal](https://surrealdb.com/docs/learn/data-models/graph/graph-traversal)
- [Full-text search overview](https://surrealdb.com/docs/learn/data-models/full-text-search/overview)
- [Vector search overview](https://surrealdb.com/docs/learn/data-models/vector-search/overview)
- [SurrealDB repository and license notice](https://github.com/surrealdb/surrealdb)
- [SurrealDB architecture and storage backends](https://surrealdb.com/docs/architecture)
- [SurrealDB Cloud pricing](https://surrealdb.com/pricing)

These references inform design and behavioral research. They do not grant permission to copy SurrealDB implementation or tests, and links to moving `main` branches do not replace the recorded engine and behavior pins.
