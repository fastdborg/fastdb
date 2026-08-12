# FastDB Phase 0 Execution Plan

Audience: implementation agent taking over Core Phase 0

Status: ready for implementation

Baseline date: 2026-08-07

## 1. Mission

Phase 0 must answer one question: can a clean-room FastDB frontend parse a very small SurrealQL-compatible slice, lower it directly to Turso AST, and atomically persist/query/delete a JSONB document with a usable expression index, without changing Turso core?

This is a feasibility spike with production-quality evidence. It is not the MVP parser, public Rust API, CLI, cloud service, or stable database format. Prefer the smallest implementation that proves or disproves the architecture, but do not fake transactionality, durability, direct AST translation, parameter binding, reopen behavior, or index use.

The Phase 0 exit decision is binary:

- **Proceed:** every Definition of Done item in section 12 passes without Turso core changes.
- **Stop for design review:** any stop condition in section 11 occurs. Document evidence and alternatives instead of hiding the limitation or expanding scope.

Read [`revised_plan.md`](./revised_plan.md) completely before starting. It is normative when this execution plan is silent.

## 2. Fixed Decisions

Do not reopen these decisions during Phase 0 unless evidence triggers a stop condition:

- Use this workspace as one monorepo based on the Turso repository history. Do not create a nested Git repository or Turso submodule.
- Initial engine baseline: Turso commit `977383ff40edc44ef410af062ed0d2322252a869`.
- Configure `https://github.com/tursodatabase/turso.git` as the `upstream` remote. Add `origin` only when the FastDB repository URL is known.
- Retain the engine's `SqliteDialect`. Physical DDL in `sqlite_schema` must remain canonical SQLite/Turso SQL.
- FastDB statements lower directly to `turso_parser::ast::Stmt` and execute through `turso_core::Connection::prepare_translated_stmt_with_options`.
- Never generate SQLite text containing a user logical identifier, record ID, string value, or parameter value.
- Ordinary internal catalog DDL may be static SQLite text. Dynamically named physical objects must use opaque validated names derived from internal catalog IDs; direct AST construction is preferred.
- Use stable WAL. Do not enable MVCC, `BEGIN CONCURRENT`, experimental multiprocess WAL, experimental index methods, FTS, encryption, or sync.
- The vertical slice has one connection and one writer. Concurrency is not Phase 0 scope.
- The Phase 0 format version is `0` and explicitly disposable. Do not call it format version 1 or promise forward compatibility; Phase 2 owns the stable catalog and migration design.
- FastDB-authored code is intended for BSL 1.1 plus a commercial license and eventual Apache-2.0 conversion, but legal text requires counsel. Until final license files exist, set new crates to `publish = false`, do not inherit Turso's workspace MIT license for FastDB-authored crates, do not publish releases, and do not accept third-party contributions.
- SurrealDB `v3.1.5` is the behavioral reference. Use public documentation and independently authored black-box observations only. Do not read, copy, translate, or vendor SurrealDB source or tests.

## 3. Exact Scope

### 3.1 User-facing slice to prove

The end-to-end test runner must accept these forms as FastDB input:

```sql
CREATE person:tracy SET name = 'Tracy';
SELECT * FROM person:tracy;
SELECT * FROM person WHERE name = 'Tracy';
DELETE person:tracy;
```

Required behavior:

| Input | Required result |
| --- | --- |
| `CREATE person:tracy SET name = 'Tracy';` | Atomically auto-register `person` as schemaless, create its hidden physical table, store one JSONB document, and return the created record |
| `SELECT * FROM person:tracy;` | Return an array containing the record when present, otherwise an empty array |
| `SELECT * FROM person WHERE name = 'Tracy';` | Return the matching record and use the Phase 0 expression index once it has been installed by the test fixture |
| `DELETE person:tracy;` | Delete the record and return the default empty result |

The decoded created/selected record is logically:

```text
{
  id: person:tracy,
  name: 'Tracy'
}
```

The result type must keep `id` as a typed Phase 0 record ID, not the string `"person:tracy"`. It is acceptable for Phase 0 to support only a bare string record ID and a string-valued top-level `SET` assignment, provided unsupported forms produce explicit errors.

### 3.2 Parser slice

Implement only the tokens and AST necessary for the four forms:

- Keywords: `CREATE`, `SELECT`, `FROM`, `WHERE`, `SET`, `DELETE`.
- Punctuation/operators: `:`, `*`, `.`, `=`, `,`, `;`.
- Bare identifiers and single-quoted strings with a documented escaping rule.
- One optional trailing semicolon and surrounding whitespace/comments only if comments are deliberately implemented.
- Source spans on tokens and AST nodes.

Reject rather than ignore:

- Extra statements or trailing tokens.
- Additional clauses such as `RETURN`, `ONLY`, `LIMIT`, or `TIMEOUT`.
- Multiple assignments unless deliberately implemented and tested.
- Nested paths unless deliberately implemented and tested.
- Unsupported value types.
- Unterminated strings and malformed record IDs.

This parser should be a small foundation for Phase 1, not a regular-expression replacement. A minimal lexer plus recursive-descent parser is sufficient; the Pratt expression parser is Phase 1 work.

### 3.3 Internal index setup

`DEFINE INDEX` is not required in Phase 0. Provide a crate-private or test-only frontend helper that creates a non-unique expression index for logical field `name`. It must resolve the logical table through the catalog and use the same canonical JSON extraction expression builder as the `WHERE name = ...` lowering path.

Do not expose this helper as a stable public API. Phase 1/2 will implement and catalog `DEFINE INDEX` properly.

### 3.4 Explicit non-goals

Do not implement:

- The final async embedded API or CLI.
- Multiple semicolon-separated statements.
- Generated record IDs, arbitrary record-ID types, arrays, objects, nested updates, aliases, ordering, pagination, or schemafull validation.
- Public `DEFINE TABLE`, `DEFINE FIELD`, or `DEFINE INDEX`.
- Explicit user transactions or poisoned transaction behavior.
- Multiple connections, concurrent writers, MVCC, multiprocess access, replication, sync, HTTP, authentication, billing, or object storage.
- Stable catalog migrations or a release-compatible file format.
- Vector, full-text, graph, geospatial, or extension APIs.
- General performance optimization beyond avoiding obvious repeated parsing/catalog work in the timed steady-state benchmark.

## 4. Repository Bootstrap

### 4.1 Preserve the current workspace

The workspace initially contains planning documents and may not yet be a Git repository. Do not delete, overwrite, or temporarily move them without preserving their contents. Before importing Turso history:

1. Inspect `pwd`, `rg --files`, and repository status.
2. Preserve `plan.md`, `revised_plan.md`, and this file.
3. Initialize/fetch the Turso repository into this workspace so the pinned Turso commit is the Git ancestry of FastDB.
4. Restore the planning documents as FastDB additions.
5. Configure `upstream` to the Turso repository and verify the exact baseline commit exists locally.
6. Create the FastDB development branch according to the repository's naming convention.

Do not perform destructive resets against the workspace. If tracked upstream paths conflict with existing user files, stop and report the exact paths.

### 4.2 Monorepo layout

Add only the Phase 0 crates initially:

```text
fastdb/
  parser/
    Cargo.toml
    lib.rs
    ast.rs
    error.rs
    lexer.rs
    parser.rs
  frontend/
    Cargo.toml
    lib.rs
    catalog.rs
    connection.rs
    decode.rs
    error.rs
    execute.rs
    lower.rs
    names.rs
    test_failpoints.rs       # cfg(test) or test feature only
  tests/
    Cargo.toml
    integration/
      vertical_slice.rs
      atomicity.rs
      index_plan.rs
  benchmarks/
    Cargo.toml
    benches/
      phase0.rs
```

File boundaries may be adjusted if the code is clearer, but retain separate parser, frontend, integration-test, and benchmark crates. Suggested package names:

- `turso_fastdb_parser`
- `turso_fastdb`
- `turso_fastdb_tests`
- `turso_fastdb_benchmarks`

Add them to the root Cargo workspace. Do not add the future CLI, server, or public binding crates.

FastDB crates must use `publish = false`. Do not write `license.workspace = true`, because the Turso workspace license is MIT and that would accidentally state that new FastDB-authored files use MIT. Add the final license metadata only after counsel approves it.

### 4.3 Expected dependencies

Use workspace dependencies where available:

- `turso_core` with `default-features = true` and `features = ["conn_raw_api"]`.
- `turso_parser` for engine AST.
- `miette` and `thiserror` for spanned parse/frontend errors.
- `serde`/`serde_json` only for Phase 0 document result decoding and benchmark fixtures.
- `tempfile` for file-backed tests.
- `criterion` or the repository's established benchmark harness, plus a percentile-capable measurement tool if needed.

Do not add a parser generator or a large new runtime dependency for this slice. Record every new dependency and why the existing workspace did not already provide the capability.

## 5. Work Packages

Execute the work packages in order. Commit after each package passes its local checks so a later discovery can be bisected or reverted without disturbing upstream code.

### P0.1 — Pin, build, and audit upstream

Tasks:

1. Check out Turso commit `977383ff40edc44ef410af062ed0d2322252a869` as the base.
2. Record the Rust toolchain, OS, target, and relevant feature flags.
3. Run `cargo metadata` and identify exact package names before writing CI commands.
4. Build the unmodified workspace or the smallest documented default set.
5. Run the unmodified relevant suites:
   - Turso core unit tests.
   - Turso integration tests covering JSON/JSONB, expression indexes, transactions, WAL, and reopen.
   - PostgreSQL frontend tests, because FastDB follows the same translated-AST seam.
6. Record pre-existing failures separately; do not fix upstream failures as part of the spike.
7. Inspect and cite these pinned locations in `docs/phase0-engine-audit.md`:
   - `postgres/frontend/session.rs`
   - `postgres/frontend/catalog.rs`
   - `postgres/parser/`
   - `tests/integration/query_processing/test_expr_index.rs`
   - `tests/integration/query_processing/test_transactions.rs`
   - `tests/integration/wal/`
   - `core/dialect/mod.rs`
   - `core/connection.rs`
   - `core/statement.rs`
   - `core/json/`
8. Confirm the exact signatures and behavior of:
   - `Database::open` and `Database::connect`.
   - `OpenOptions::new(Arc::new(SqliteDialect))`.
   - `Connection::prepare_translated_stmt_with_options`.
   - `Statement::parameter_index`, `Statement::bind_at`, `run_ignore_rows`, and `run_with_row_callback`.
   - `EXPLAIN QUERY PLAN` detail output for expression indexes.

Deliverable: `docs/phase0-engine-audit.md` containing the pinned SHA, commands/results, API notes, known upstream limitations, and a statement that experimental facilities remain disabled.

Gate: do not start FastDB code if the clean pinned baseline cannot build or if relevant tests fail in a way that invalidates JSONB, expression-index, transaction, WAL, or reopen assumptions.

### P0.2 — Project policy and compatibility skeleton

Create:

- `UPSTREAM.md`: pinned SHA, remote setup, how to audit a new pin, merge/rebase policy, notice preservation, and rules for isolating upstreamable changes.
- `CLEAN_ROOM.md`: allowed public documentation/black-box research, forbidden SurrealDB source/test copying, provenance-note format, reviewer checklist, and trademark disclaimer.
- `COMPAT.md`: status legend and Phase 0 rows for the exact `CREATE`, record `SELECT`, equality-filter `SELECT`, and `DELETE` forms. Everything else is `planned` or `unsupported`; no broad statement-family row may say fully supported.
- `docs/compat-research/phase0.md`: independently written public-doc citations and any black-box observations against SurrealDB `v3.1.5`. Store inputs, observed outputs, version, and date; do not copy upstream fixtures.
- `docs/licensing.md`: the approved policy direction from `revised_plan.md`, inherited MIT provenance rule, public-release/contribution block pending counsel, and no informal license drafting.

Gate: a reviewer can identify which behavior came from public documentation, which came from a black-box observation, and which is a FastDB-specific choice.

### P0.3 — Minimal spanned parser

Implement the Phase 0 lexer, AST, and parser independently.

Suggested FastDB AST:

```rust
struct Spanned<T> {
    value: T,
    span: SourceSpan,
}

enum Statement {
    Create(CreateStatement),
    Select(SelectStatement),
    Delete(DeleteStatement),
}

struct RecordTarget {
    table: Identifier,
    id: Option<RecordIdPart>,
}

enum Predicate {
    StringEquals { field: Identifier, value: String },
}
```

The exact type design may change, but Turso AST types must not leak into the parser crate.

Required parser tests:

- The four exact supported forms.
- Case-insensitive keywords with identifiers retaining original Unicode text.
- Whitespace around punctuation.
- A string containing a quote using the chosen SurrealQL-compatible escaping rule.
- Semicolon inside a string is not a terminator.
- Unterminated string.
- Missing table, ID, field, value, or `FROM`.
- Unexpected/trailing clause produces `UnsupportedSyntax` or `Parse`, never success.
- Multiple statements are explicitly rejected in Phase 0.
- Invalid UTF-8 handling at the API boundary is documented if the parser accepts `&str` only.
- No panic on empty input and a small arbitrary-byte/string smoke corpus.

Gate: every accepted AST shape has an execution path in P0.6. No accepted field is ignored.

### P0.4 — Phase 0 physical names, IDs, and values

Implement internal codecs before catalog work:

- Generate an immutable random 128-bit catalog table ID.
- Physical table name: `__fastdb_t_` followed by exactly 32 lowercase hexadecimal characters.
- Physical index name: `__fastdb_i_` followed by exactly 32 lowercase hexadecimal characters.
- Validate generated physical names again at the AST/DDL boundary.
- Encode the supported bare string record ID in a versioned, type-tagged, length-delimited form such as `s:<utf8-byte-length>:<value>`. Decoding must reject malformed encodings.
- Represent returned `id` as a typed `RecordId { table, id }` in the frontend result model.
- Store user content only in `doc`; never duplicate `id` inside JSONB.

Required tests:

- Physical names contain no logical identifier text.
- Different catalog IDs produce different names.
- Record ID round trip for ASCII and UTF-8.
- Type tag and length prevent ambiguous decoding.
- Malformed ID encoding is a typed engine/format error, not a panic.

Do not implement generated UUIDv7 record IDs; the vertical slice always supplies `tracy`.

### P0.5 — Disposable catalog and database open path

Open the database using Turso core and `SqliteDialect`, following the pinned PostgreSQL frontend's open pattern but without a custom dialect. Do not route FastDB input through `Connection::prepare`, because that would invoke the SQLite parser.

Use this logical Phase 0 catalog shape or an equivalent reviewed shape:

```sql
CREATE TABLE __fastdb_meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    format_version INTEGER NOT NULL,
    dialect_version INTEGER NOT NULL,
    database_id TEXT NOT NULL
) STRICT;

CREATE TABLE __fastdb_tables (
    table_id TEXT PRIMARY KEY,
    logical_name TEXT NOT NULL UNIQUE,
    physical_name TEXT NOT NULL UNIQUE,
    mode TEXT NOT NULL CHECK (mode IN ('SCHEMALESS', 'SCHEMAFULL')),
    definition TEXT NOT NULL
) STRICT;
```

Requirements:

- Reserved catalog names are static and unavailable through the Phase 0 identifier grammar.
- `format_version` and `dialect_version` are `0`.
- `database_id` is stable across reopen.
- Missing catalogs on a read-only logical lookup mean an empty FastDB database, not permission to mutate.
- First mutation creates both catalog tables and their metadata inside the same transaction as implicit logical-table registration, physical-table creation, and record insertion.
- An existing metadata row with a version other than `0` is refused before mutation.
- Catalog queries use bound logical names. User identifiers are not interpolated into internal SQL.
- Static internal catalog DDL is isolated in one module and clearly marked as internal SQLite/Turso SQL.

The hidden physical table is:

```sql
CREATE TABLE <validated_opaque_name> (
    rid TEXT PRIMARY KEY,
    doc JSONB NOT NULL
) STRICT;
```

The exact `STRICT`/JSONB combination must be proven on the pinned engine. If unsupported, record the behavior and choose the smallest valid physical declaration without weakening the logical `doc` JSONB invariant.

Gate: bootstrap, reopen, future-version refusal, and catalog-name resolution tests pass before CRUD lowering starts.

### P0.6 — Direct AST lowering and execution

Create a frontend plan type even if it is small:

```text
FrontendPlan
  prerequisites: internal catalog/schema steps
  main: translated Turso statement
  bindings: ordered typed values
  decoder: no rows | record rows | created record
```

Requirements for all user-facing statements:

- Lower from FastDB AST directly to `turso_parser::ast::Stmt`.
- Prepare with `prepare_translated_stmt_with_options` and default `PrepareOptions` unless the audit proves another stable option is required.
- Convert source literal values to Turso parameters and bind with `Statement::bind_at`; do not embed them as generated SQL text.
- Resolve logical table names through `__fastdb_tables`, then use only the validated opaque physical name in the engine AST.
- Preserve the original FastDB source string only for diagnostics. It must not become persisted physical DDL.

`CREATE` transaction algorithm:

1. Start an internal `BEGIN IMMEDIATE` transaction.
2. Create Phase 0 catalog tables and metadata if missing.
3. Look up logical table `person` by a bound value.
4. If absent, generate its catalog ID/physical name, insert a `SCHEMALESS` catalog row, and create its hidden table.
5. Encode `tracy` as `rid`.
6. Build JSONB user content containing only `{ "name": "Tracy" }` using a bound value and an engine JSONB function or another audited typed path.
7. Insert the physical row; a duplicate explicit ID must fail.
8. Decode the created record.
9. Commit.
10. On any error after transaction start, attempt rollback, return the original typed error with rollback failure attached if necessary, and leave no partial logical change.

`SELECT` record algorithm:

1. Resolve `person` through the catalog.
2. If absent, return an empty array without creating anything.
3. Select by bound canonical `rid` from the opaque table.
4. Convert JSONB to an audited decodable representation, decode it, synthesize typed `id`, and return one record array.

`SELECT` filter algorithm:

1. Resolve `person` through the catalog.
2. Build the field expression only through `canonical_json_extract("name")`.
3. Compare it with a bound string value.
4. Decode all matching records as arrays.

`DELETE` algorithm:

1. Resolve `person`; if absent, return the default empty result.
2. Delete using a bound canonical `rid`.
3. Return the default empty result whether zero or one row was deleted.

Add a single transaction-owner helper so success commits and every failure path rolls back. Do not scatter manual transaction handling across statement implementations.

Gate: unit and in-memory integration tests pass, and a test containing a quote/semicolon in the name value proves it remains data rather than executable SQL.

### P0.7 — File-backed vertical slice and reopen

Write one end-to-end test that performs exactly this sequence:

1. Create a new temporary directory and choose `vertical.fastdb` inside it.
2. Open FastDB.
3. Execute `CREATE person:tracy SET name = 'Tracy';`.
4. Assert the returned typed record.
5. Drop all statements, connections, and database handles.
6. Reopen the same path.
7. Execute `SELECT * FROM person:tracy;` and assert exactly one typed record.
8. Inspect internal state through a test-only native connection:
   - One version-0 metadata row exists.
   - One `person` catalog row exists.
   - Its physical name matches the opaque-name format and does not contain `person` or `tracy`.
   - The physical row stores canonical `rid` and JSONB `doc` without an `id` member.
9. Execute `DELETE person:tracy;` and assert the empty default result.
10. Select again and assert an empty array.
11. Drop/reopen again and assert the record remains absent while catalog/table definitions remain.
12. Run `PRAGMA integrity_check` through a test-only native connection and require `ok`.

Do not delete WAL sidecars manually. Let handles close normally and retain the temporary directory until assertions finish.

### P0.8 — Atomicity and failure injection

Add deterministic test-only failure points at minimum after:

1. Metadata/catalog bootstrap.
2. Logical table catalog insertion.
3. Hidden physical table creation.
4. Record statement preparation but before insertion.
5. Record insertion but before commit.

For each point, begin from a new empty file, execute the `CREATE`, force the error, drop/reopen, and prove:

- No `person` catalog row exists.
- No hidden `person` physical table exists.
- No record exists.
- If bootstrap was part of the rolled-back transaction, no metadata/catalog tables exist; if the implementation deliberately pre-created them before this request, that violates the Phase 0 atomic-first-mutation requirement and must be treated as a failed test.
- `PRAGMA integrity_check` returns `ok`.
- A subsequent non-failing `CREATE` succeeds normally.

Also test duplicate explicit ID:

1. Create `person:tracy` successfully.
2. Attempt the same `CREATE` again.
3. Require a constraint-category error.
4. Reopen and prove exactly one unchanged record exists.

Do not simulate a failure by skipping work and returning success. Failure points must enter the real error/rollback path.

### P0.9 — Canonical JSON expression index

Implement one canonical builder for top-level field `name`. The same helper must build the expression used by:

- The expression index definition.
- `SELECT * FROM person WHERE name = ...`.
- Any test-only equivalent plan inspection.

Test procedure:

1. Create at least three `person` records through the frontend. The parser may remain single-ID/single-string; use distinct IDs and names.
2. Install the test-only non-unique `name` expression index with an opaque index name.
3. Execute the FastDB equality-filter query and assert the correct record.
4. Run `EXPLAIN QUERY PLAN` over the exact translated predicate shape.
5. Assert the plan detail mentions the opaque index name and does not describe a full scan of the hidden table.
6. Drop all handles, reopen, repeat result and plan assertions.
7. Update is out of scope, but delete one indexed record and prove the filter no longer returns it.

Follow the assertion style in pinned `tests/integration/query_processing/test_expr_index.rs`: inspect the plan detail column and match the actual index name. Do not accept timing alone as proof of index use.

If Turso cannot use an expression index over the chosen JSONB extraction shape, stop Phase 0 and document the exact DDL, translated AST, plan, and alternatives. Do not substitute an application-side index.

### P0.10 — Native-Turso comparison benchmark

Build a reproducible release-mode benchmark comparing the FastDB path with an equivalent native Turso path on the same pinned engine.

Workloads:

1. Cold first `CREATE`: includes catalog bootstrap, implicit table creation, JSONB write, and decoding.
2. Steady-state explicit-ID `CREATE` on an existing logical/physical table.
3. Point record read by `rid`.
4. Indexed equality filter on `name`.
5. Record delete.

For each steady-state workload:

- FastDB measures parse, catalog resolution, lowering, binding, execution, and result decoding.
- Native Turso uses the same physical schema, same JSONB functions, same bound values, same durability mode, and equivalent result materialization.
- Setup, data generation, connection opening, and verification are outside the timed region unless the workload explicitly measures them.
- Write iterations use unique IDs or reset state outside the timed region.
- Use identical data distributions and index state.
- Measure enough samples to report p50, p95, and p99 or clearly state when the chosen repository harness cannot produce a percentile.
- Record wall-clock distribution, throughput where meaningful, database size after checkpoint/clean close, CPU/OS/Rust versions, build flags, and cache policy.
- Run a correctness assertion before and after every benchmark group.

Deliverable: `docs/benchmarks/phase0.md` plus the executable benchmark. Include raw command lines and FastDB/native ratios. Phase 0 has no performance pass ratio beyond completing a fair measurement; the MVP targets in `revised_plan.md` remain future gates. A pathological result that makes the architecture unusable must trigger profiling and a design note, not benchmark manipulation.

### P0.11 — Minimal Cloud C0 decision note

Do not implement a cloud service. Create `docs/cloud/phase0.md` containing only decisions that affect the local format/frontend:

- A future cloud database needs a globally unique immutable database ID; the Phase 0 `database_id` demonstrates persistence but its encoding is not stable.
- A future durable log needs an epoch/fencing token, ordered sequence, idempotent mutation ID, database format/dialect version, and checksum.
- Identify pinned Turso logical-log/sync modules that deserve later audit; do not claim they satisfy FastDB Cloud requirements.
- State whether Phase 0 operations expose a deterministic logical mutation representation or require later CDC work.
- List what must remain undecided until Cloud C0 benchmarks: object provider, segment size, checkpoint interval, batching delay, retention, and pricing quotas.

This note is not on the Core feasibility critical path, but it prevents Phase 0 from accidentally choosing identifiers that make later logging impossible.

### P0.12 — Final audit and handoff

Before declaring completion:

1. Run formatting, linting, FastDB tests, relevant unchanged Turso suites, and release benchmarks from a clean build.
2. Review the complete diff from the pinned Turso commit.
3. Confirm no existing files under `core/`, `sqlite/parser/`, `bindings/rust/`, `postgres/`, or upstream test directories changed. Root workspace manifests and lockfile may change only to register FastDB crates/dependencies.
4. Search for generated SQL/string formatting that includes FastDB source values or logical identifiers.
5. Search for `unsafe`, experimental feature enabling, ignored errors, `unwrap`/`expect` in non-test FastDB paths, and TODOs that bypass correctness.
6. Verify all new files carry approved provenance/license headers once counsel provides them. Until then, keep crates unpublished and record the legal block.
7. Write `docs/phase0-report.md` using the template in section 13.

## 6. Implementation Constraints

### 6.1 No user-to-SQL string generation

The following are forbidden:

```rust
format!("INSERT INTO {table} ... '{user_value}'")
format!("SELECT ... FROM {logical_table}")
format!("... json_extract(doc, '$.{user_path}') ...")
```

Permitted inputs to internal SQL text are:

- Completely static catalog DDL.
- Validated opaque physical names generated exclusively from internal IDs, if direct AST construction is impractical and the exception is documented.
- Static canonical Phase 0 JSON path `$.name`, though direct AST construction remains preferred.

All user values are bound. All logical identifiers resolve through catalogs.

### 6.2 No silent compatibility

Every lexer token, AST property, and clause accepted by the Phase 0 parser must affect execution or produce an explicit unsupported error. Do not accept syntax merely because it resembles a future feature.

### 6.3 Error boundaries

Define at least these internal categories:

- `Parse`
- `UnsupportedSyntax`
- `Constraint`
- `Format`
- `Transaction`
- `Engine`
- `Io`

The full stable public error API is Phase 4, but Phase 0 tests must distinguish parse/unsupported, duplicate ID, unknown future format, injected rollback failure, and underlying engine failure.

### 6.4 No Turso core patch

Calling public/documented Turso core APIs from new FastDB crates is allowed. Editing Turso core, parser, WAL, JSON, optimizer, bindings, or existing frontends is not.

If a required public API is missing:

1. Prove the gap with the smallest test or compiler error.
2. Search for a supported alternative.
3. Write a design note describing the minimal potential upstream API.
4. Stop at the Phase 0 gate; do not patch around private internals or silently fork core behavior.

## 7. Test Matrix

Every row below must map to a stable test name in the Phase 0 report.

| ID | Area | Case | Required assertion |
| --- | --- | --- | --- |
| P0-PARSE-001 | Parser | Exact CREATE | Spanned Create AST |
| P0-PARSE-002 | Parser | Record SELECT | Spanned Select AST |
| P0-PARSE-003 | Parser | Equality filter SELECT | Spanned predicate AST |
| P0-PARSE-004 | Parser | DELETE | Spanned Delete AST |
| P0-PARSE-005 | Parser | Trailing unsupported clause | Explicit unsupported error at clause span |
| P0-PARSE-006 | Parser | Unterminated string | Parse error, no panic |
| P0-PARSE-007 | Parser | Semicolon/quote inside string | One value token, never second statement |
| P0-CODEC-001 | Codec | UTF-8 record ID round trip | Exact typed value recovered |
| P0-CAT-001 | Catalog | Empty read | No mutation and empty result |
| P0-CAT-002 | Catalog | First mutation | Metadata, table catalog, hidden table, and row commit together |
| P0-CAT-003 | Catalog | Unknown version | Refuse before mutation |
| P0-CRUD-001 | CRUD | Create and decode | Typed ID plus user content; no stored `id` |
| P0-CRUD-002 | CRUD | Duplicate create | Constraint error; original unchanged |
| P0-CRUD-003 | CRUD | Reopen/select/delete/reopen | Correct record then persistent absence |
| P0-INJECT-001 | Injection | Quoted/semicolon user value | Stored literally; schema unchanged |
| P0-ATOMIC-001 | Atomicity | Fail after bootstrap | No catalog/schema/data survives |
| P0-ATOMIC-002 | Atomicity | Fail after catalog row | No catalog/schema/data survives |
| P0-ATOMIC-003 | Atomicity | Fail after physical DDL | No catalog/schema/data survives |
| P0-ATOMIC-004 | Atomicity | Fail before record insert | No catalog/schema/data survives |
| P0-ATOMIC-005 | Atomicity | Fail before commit | No catalog/schema/data survives |
| P0-IDX-001 | Index | Equality result | Correct matching record |
| P0-IDX-002 | Index | Explain before reopen | Opaque expression index selected; no full table scan |
| P0-IDX-003 | Index | Explain after reopen | Same index selected and correct result |
| P0-WAL-001 | Durability | Clean close/reopen | Data/catalog/schema persist and integrity check is `ok` |
| P0-BENCH-001 | Benchmark | Native comparisons | Reproducible report with ratios and environment |

Add tests discovered during implementation; do not delete or weaken these rows without updating this plan and explaining why.

## 8. Required Commands

Discover exact package names with `cargo metadata` and record the final commands in the Phase 0 report. The expected command set is:

```sh
cargo fmt --all -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests --all-targets -- -D warnings
cargo test -p turso_fastdb_parser
cargo test -p turso_fastdb
cargo test -p turso_fastdb_tests
cargo test -p turso_core
cargo test -p turso_pg_tests
cargo bench -p turso_fastdb_benchmarks --bench phase0
```

If upstream package names or supported commands differ, use the verified equivalents and document them. Do not claim a suite passed if it was filtered, skipped, or run with experimental modes unless the report says so explicitly.

## 9. Evidence to Preserve

Commit these artifacts:

- Exact upstream SHA and `cargo metadata` package/toolchain summary.
- Upstream baseline test log summary.
- Parser and compatibility matrix mapping.
- Catalog/physical schema dump from the successful vertical test.
- Rollback failure-point results.
- Explain-plan detail before and after reopen.
- `PRAGMA integrity_check` result after success and each rollback family.
- Benchmark source, commands, environment, raw summary, percentile/throughput results, and ratios.
- Complete list of files changed relative to the pinned Turso commit.
- Any stop-condition design note.

Do not commit customer data, secrets, credentials, private legal advice, large build artifacts, or temporary database/WAL files.

## 10. Review Checkpoints

Pause for a focused self-review at these checkpoints:

1. **After P0.1:** is the upstream baseline actually healthy and are the APIs public?
2. **After P0.3:** does the parser reject everything it cannot execute?
3. **After P0.5:** is the empty file still unmodified by a read, and is the catalog explicitly disposable version 0?
4. **After P0.6:** is every user value bound and every logical name catalog-resolved?
5. **After P0.8:** does real DDL plus DML rollback after reopen at every failure point?
6. **After P0.9:** does the exact filter expression select the exact expression index after reopen?
7. **After P0.10:** is the native comparison fair and reproducible?
8. **Before completion:** is there any Turso core modification or legal/publication ambiguity?

## 11. Stop Conditions

Stop implementation and write `docs/phase0-blocker-<topic>.md` if any condition occurs:

- The pinned Turso baseline cannot pass relevant JSONB, index, transaction, WAL, or reopen tests.
- Physical DDL and catalog/data DML cannot participate in one rollback-capable transaction.
- Reopen exposes a catalog row without its table, a table without its catalog row, or a partial record.
- The canonical JSON extraction expression cannot be used by a stable ordinary Turso B-tree expression index.
- The translated-AST preparation API cannot execute the required statements or bind values without a core change.
- `SqliteDialect` cannot reopen the persisted physical schema.
- Correctness requires MVCC, multiprocess WAL, an experimental index method, or another excluded feature.
- A Turso core change is unavoidable.
- The implementation would require copying SurrealDB source/tests or relying on non-public behavior without recording a black-box observation.
- Licensing counsel determines the intended BSL/commercial model is incompatible with the proposed distribution or contribution model.

A stop-condition note must contain:

1. Minimal reproduction.
2. Expected and actual behavior.
3. Pinned SHA/toolchain.
4. Relevant logs, schema, AST, or explain plan.
5. At least two alternatives with trade-offs.
6. Recommended next decision.

Do not mark Phase 0 complete while a stop condition is unresolved.

## 12. Definition of Done

Phase 0 is done only when every checkbox is true.

### Repository and policy

- [ ] This workspace is a single monorepo whose Git history contains the pinned Turso commit.
- [ ] `upstream` points to the official Turso repository; the audited SHA is recorded.
- [ ] Existing planning documents are preserved.
- [ ] All Turso MIT notices are preserved and file provenance is auditable.
- [ ] New FastDB crates are `publish = false` and do not incorrectly inherit MIT licensing.
- [ ] `UPSTREAM.md`, `CLEAN_ROOM.md`, `COMPAT.md`, compatibility research notes, and the licensing decision record exist and agree.
- [ ] No release or third-party contribution is accepted before counsel finalizes license/CLA terms.

### Upstream feasibility

- [ ] The unmodified pinned baseline builds with the recorded toolchain.
- [ ] Relevant unmodified Turso JSONB, expression-index, transaction, WAL, reopen, and PostgreSQL frontend tests pass, or any unrelated pre-existing failure is reproducibly documented.
- [ ] Experimental MVCC, multiprocess WAL, index methods, FTS, encryption, and sync are disabled.
- [ ] `docs/phase0-engine-audit.md` records verified API signatures and constraints.

### Parser and frontend

- [ ] The independent spanned parser accepts all four Phase 0 statements.
- [ ] Unsupported clauses and malformed input return explicit errors and are never ignored.
- [ ] FastDB AST is independent of Turso AST.
- [ ] All user-facing statements lower directly into Turso AST.
- [ ] Main statements execute through `prepare_translated_stmt_with_options`.
- [ ] All user values are bound; no user value or logical identifier is interpolated into SQLite text.
- [ ] Logical names resolve only through the catalog; physical names are opaque and validated.
- [ ] Results synthesize a typed record ID and `doc` never stores `id`.

### Atomic storage behavior

- [ ] First `CREATE` atomically bootstraps metadata/catalog, registers the schemaless logical table, creates the hidden table, and inserts the record.
- [ ] Every required injected failure rolls back metadata, catalog, physical DDL, and data, proven after reopen.
- [ ] A duplicate explicit ID returns a constraint error and preserves the original record.
- [ ] Unknown nonzero/future Phase 0 format or dialect versions are refused before mutation.
- [ ] A read of a new empty database does not create catalog or user objects.
- [ ] `PRAGMA integrity_check` returns `ok` after success and rollback tests.

### Vertical slice and indexing

- [ ] `CREATE person:tracy SET name = 'Tracy';` returns the typed created record.
- [ ] Clean close/reopen followed by record `SELECT` returns the same record.
- [ ] `DELETE person:tracy;` returns the default empty result and persists across another reopen.
- [ ] The canonical `name` expression is shared by index creation and filter lowering.
- [ ] Equality-filter results are correct before and after reopen.
- [ ] `EXPLAIN QUERY PLAN` names the opaque B-tree expression index and does not show a full hidden-table scan before or after reopen.

### Performance and quality

- [ ] The native-Turso comparison benchmark covers cold create, steady create, point read, indexed filter, and delete.
- [ ] Benchmark setup is equivalent, values are bound, correctness is checked, and environment/commands/results/ratios are published.
- [ ] Formatting and linting pass with warnings denied for FastDB crates.
- [ ] All FastDB unit/integration tests and relevant unchanged Turso suites pass.
- [ ] No non-test FastDB path contains an unjustified panic, ignored error, or correctness TODO.
- [ ] No existing Turso core/parser/binding/frontend implementation file changed.
- [ ] `docs/cloud/phase0.md` records future identity/log requirements without claiming cloud readiness.
- [ ] `docs/phase0-report.md` is complete and recommends proceed or stop based on evidence.

## 13. Final Report Template

Create `docs/phase0-report.md` with these headings:

```markdown
# FastDB Phase 0 Report

## Decision
Proceed | Stop for design review

## Pinned Inputs
- Turso SHA
- SurrealDB behavioral version
- Rust toolchain and target

## Repository Changes
- New crates/files
- Root manifest/lock changes
- Confirmation of no Turso core changes

## Vertical Slice Results
- Create
- Reopen/select
- Delete/reopen
- Integrity check

## Atomicity Results
- Failure point table
- Reopen observations

## Index Evidence
- Canonical expression
- Physical index name
- Explain plan before reopen
- Explain plan after reopen

## Upstream Regression Results
- Commands
- Pass/fail/known baseline failures

## Benchmark Results
- Environment and methodology
- FastDB/native table
- Ratios and observations

## Compatibility and Clean-Room Review
- COMPAT rows implemented
- Research provenance
- Review result

## Cloud C0 Notes
- Identity/log implications only

## Risks and Follow-ups
- Items for Phase 1/2

## Definition of Done
- Completed checklist or explicit failures
```

The report must lead with the decision and evidence. Do not call the result production-ready, fully SurrealQL-compatible, cloud-ready, or ACID-certified.

## 14. Handoff Guidance for the Implementing Agent

- Re-read repository-local `AGENTS.md` files after importing Turso and obey the most specific applicable instructions.
- Inspect before editing; Turso changes rapidly and the pinned source is authoritative over remembered APIs.
- Use `rg`/`rg --files` for discovery and `apply_patch` for edits.
- Preserve unrelated user changes and never use destructive Git commands.
- Keep commits narrow: bootstrap/policy, parser, catalog, lowering, vertical test, rollback tests, index proof, benchmarks, report.
- Report a stop condition early. A precise failed feasibility result is a successful Phase 0 outcome; a hidden core patch is not.
- Do not begin Phase 1 features to make the spike look more complete.

## 15. Pinned Technical References

- [Turso pinned baseline](https://github.com/tursodatabase/turso/commit/977383ff40edc44ef410af062ed0d2322252a869)
- [Turso PostgreSQL frontend session](https://github.com/tursodatabase/turso/blob/977383ff40edc44ef410af062ed0d2322252a869/postgres/frontend/session.rs)
- [Turso PostgreSQL compatibility matrix](https://github.com/tursodatabase/turso/blob/977383ff40edc44ef410af062ed0d2322252a869/postgres/COMPAT.md)
- [Turso expression-index tests](https://github.com/tursodatabase/turso/blob/977383ff40edc44ef410af062ed0d2322252a869/tests/integration/query_processing/test_expr_index.rs)
- [Turso transaction tests](https://github.com/tursodatabase/turso/blob/977383ff40edc44ef410af062ed0d2322252a869/tests/integration/query_processing/test_transactions.rs)
- [Turso WAL tests](https://github.com/tursodatabase/turso/tree/977383ff40edc44ef410af062ed0d2322252a869/tests/integration/wal)
- [Turso dialect interface](https://github.com/tursodatabase/turso/blob/977383ff40edc44ef410af062ed0d2322252a869/core/dialect/mod.rs)
- [Turso connection translated-statement API](https://github.com/tursodatabase/turso/blob/977383ff40edc44ef410af062ed0d2322252a869/core/connection.rs)
- [Turso statement binding API](https://github.com/tursodatabase/turso/blob/977383ff40edc44ef410af062ed0d2322252a869/core/statement.rs)
- [SurrealDB v3.1.5 release](https://github.com/surrealdb/surrealdb/releases/tag/v3.1.5)
- [SurrealDB CREATE documentation](https://surrealdb.com/docs/reference/query-language/statements/create)
- [SurrealDB SELECT documentation](https://surrealdb.com/docs/reference/query-language/statements/select)
- [SurrealDB DELETE documentation](https://surrealdb.com/docs/reference/query-language/statements/delete)
