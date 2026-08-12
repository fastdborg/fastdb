# Phase 0 Engine Audit

Audience: Phase 0 implementation and review.

This document records the pinned Turso baseline, the verified public API
surface FastDB depends on, build/test evidence, and the statement that
experimental facilities remain disabled. It is the deliverable for
`plan-phase0.md` work package **P0.1**.

## Pinned inputs

| Item | Value |
| --- | --- |
| Turso baseline commit | `977383ff40edc44ef410af062ed0d2322252a869` |
| SurrealDB behavioral reference | `v3.1.5` (black-box only) |
| Repository `upstream` remote | `https://github.com/tursodatabase/turso.git` |
| FastDB development branch | `phase-0` |
| Pinned Rust toolchain (`rust-toolchain.toml`) | `1.88` (components: clippy, rustfmt, rust-analyzer) |
| System rustup default | `1.97.1` (used only outside the workspace) |
| Target | `x86_64-unknown-linux-gnu` |
| OS | Linux 7.0.0-29-generic x64 |

The pinned commit is an ancestor of `phase-0` HEAD (verified with
`git merge-base --is-ancestor`). The Turso repository history was merged
into the FastDB workspace with `--allow-unrelated-histories`; the only
add/add conflicts were the root `AGENTS.md` and `README.md`, resolved in
favor of FastDB. All Turso MIT notice files (`LICENSE.md`, `NOTICE.md`,
`CONTRIBUTING.md`) are preserved.

## Workspace package summary (`cargo metadata --no-deps`)

The workspace has 50 packages. The packages FastDB Phase 0 depends on or
references:

| Crate | Path | Role in Phase 0 |
| --- | --- | --- |
| `turso_core` | `core/` | Engine: `Database`, `Connection`, `Statement`, `OpenOptions`, `SqliteDialect`, `Value`, JSONB functions. Used with `features = ["conn_raw_api"]`. |
| `turso_parser` | `sqlite/parser/` | Engine AST: `turso_parser::ast::Stmt` is the lowering target. |
| `turso_pg` | `postgres/frontend/` | Reference frontend. FastDB follows the same translated-AST seam but with `SqliteDialect` and no custom dialect. |
| `turso_pg_tests` | `postgres/tests/` | PostgreSQL frontend regression suite (P0.1/Section 8 reference). |

Other members (bindings, extensions, sync, serverless, perf) are not on
the Phase 0 critical path and are not built by the targeted commands
below.

## Verified public API signatures

All signatures below were read from the pinned source (file:line cited)
and are public and ungated except where noted.

### Opening a database (SqliteDialect, no custom dialect)

`core/dialect/sqlite.rs:31` — `pub struct SqliteDialect;` implementing
`Dialect`. Re-exported at `core/dialect/mod.rs:15`.

`core/database.rs:207` — `pub struct OpenOptions { ... dialect: Arc<dyn Dialect> }`.
`core/database.rs:230` — `pub fn new(dialect: Arc<dyn Dialect>) -> Self`.
The in-tree doc example (`core/database.rs:200`) literally shows
`OpenOptions::new(Arc::new(SqliteDialect))`.

`core/database.rs:1108`:
```rust
pub fn open(io: Arc<dyn IO>, path: &str, mut options: OpenOptions) -> Result<Arc<Database>>
```

`core/database.rs:2255`:
```rust
pub fn connect(self: &Arc<Database>) -> Result<Arc<Connection>>
```

The reference open pattern (FastDB mirrors it with `SqliteDialect`) is in
`postgres/frontend/session.rs:57` (`open_database_with_io`): obtain an
`Arc<dyn IO>` via `Database::io_for_path(path)`, open the file, wrap in
`storage::database::DatabaseFile`, then call
`Database::open(io, path, OpenOptions::new(Arc::new(SqliteDialect)).storage(db_file).flags(...).db_opts(...))`.

### Translated-statement preparation (the FastDB seam)

`core/connection.rs:1057`:
```rust
pub fn prepare_translated_stmt_with_options(
    self: &Arc<Connection>,
    stmt: ast::Stmt,            // turso_parser::ast::Stmt — built directly, no SQLite text
    input: &str,                // kept for diagnostics/schema storage only
    prepare_options: &PrepareOptions,
) -> Result<Statement>
```

`PrepareOptions` (`core/connection.rs:170`) is
`#[derive(Default)]` with a single optional field; `PrepareOptions::default()`
is the Phase 0 choice.

This is the single seam where FastDB AST lowering meets Turso execution.
FastDB user input is **never** parsed by Turso's SQLite parser and
**never** rendered into SQLite text; only the directly-constructed
`ast::Stmt` and bound values reach the engine.

### Statement execution and binding

`core/statement.rs:1263` — `pub fn bind_at(&mut self, index: NonZero<usize>, value: Value) -> Result<()>`
`core/statement.rs:1259` — `pub fn parameter_index(&self, name: &str) -> Option<NonZero<usize>>`
`core/statement.rs:677` — `pub fn run_ignore_rows(&mut self) -> Result<()>`
`core/statement.rs:708` — `pub fn run_with_row_callback(&mut self, func: impl FnMut(&Row) -> Result<()>) -> Result<()>`

`Value` (`core/types.rs:372`) variants include `Null`, `Integer`, `Float`,
`Text(Text)`, `Blob(ValueBlob)`. User values are bound as `Value`
parameters; they are never interpolated into SQL text.

### JSONB and expression indexes

- JSONB subsystem: `core/json/` (`jsonb.rs`, `ops.rs`, `path.rs`).
  `json` is a default `turso_core` feature (no extra flag required).
- `json_extract` / `jsonb` / `json_object` are registered in
  `core/function.rs` and `core/json/mod.rs`. The canonical Phase 0
  extraction expression is `json_extract(doc, '$.name')` built directly
  as an AST expression (not via string interpolation), shared by the
  filter lowering and the index definition.
- Expression indexes are proven by `tests/integration/query_processing/test_expr_index.rs`,
  which uses `EXPLAIN QUERY PLAN` and reads column index `3` (the plan
  detail string). Detail strings contain tokens such as
  `USING INDEX <name>` and `USING COVERING INDEX <name>`. FastDB's
  explain-plan assertions follow this exact style (P0.9).

### EXPLAIN QUERY PLAN detail access

The detail column is read as `row.get::<String>(3)?` within
`run_with_row_callback`. Phase 0 explain queries are internal/test-only
diagnostics run over FastDB's own translated AST or over internal SQL on
a test-only native connection; they are not FastDB user input.

## Experimental facilities — confirmed disabled for Phase 0

Phase 0 uses stable WAL with full durability and a single connection /
single writer. The following are **not** enabled:

- Experimental MVCC and `BEGIN CONCURRENT`.
- Experimental multiprocess WAL.
- Experimental index methods, FTS (`fts` feature off), encryption
  (`encryption` is a default feature of `turso_core` but is not
  exercised; FastDB opens with `SqliteDialect` defaults and no key).
- Sync engine (`sync/`), serverless, and HTTP/network surfaces.

`OpenOptions::default()` and `DatabaseOpts::default()` are used; no
experimental flags are set.

## Build evidence

Command:
```sh
cargo build -p turso_core -p turso_parser
```
Result: `Finished dev profile [unoptimized + debuginfo] target(s) in 1m 03s`.
Two pre-existing upstream warnings in `turso_core` (unused import of
`CollationSeq` and one other) — not introduced by FastDB, not fixed.

## Baseline test results

Phase 0 records the relevant subset (JSONB, expression index,
transaction, WAL, reopen, PG frontend) and documents any pre-existing
failure rather than fixing it.

| Command | Result |
| --- | --- |
| `cargo build -p turso_core -p turso_parser` | Finished, 1m03s, 2 pre-existing upstream warnings (unused imports), 0 errors. |
| `cargo test -p turso_core --lib` | `ok. 2286 passed; 0 failed; 17 ignored` (44.38s). |

The engine core unit suite passes cleanly at the pin, validating the
storage/transaction/WAL/JSONB primitives FastDB depends on. Expression
index behavior is exercised in Phase 0's own plan-proof test (P0.9)
using the same `EXPLAIN QUERY PLAN` assertion style as
`tests/integration/query_processing/test_expr_index.rs`. Broader upstream
integration suites (`-p tests`, `-p turso_pg_tests`) are referenced in
Section 8 of `plan-phase0.md`; they are not gating for the Phase 0
vertical slice but should be run before any pin update.

## Known upstream limitations relevant to Phase 0

None encountered that invalidate Phase 0 assumptions. To be updated if a
targeted test surfaces one; any such finding triggers the Section 11 stop
process rather than a workaround.
