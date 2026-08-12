# FastDB Phase 0 Report

## Decision

**Proceed** to Phase 1.

Phase 0 proves the architectural thesis: a clean-room FastDB frontend parses a
small SurrealQL subset, lowers it **directly into Turso AST**, and executes
through `Connection::prepare_translated_stmt_with_options`, atomically
persisting/querying/deleting a JSONB document with a usable expression index —
**with no Turso core change**.

> **Review re-entry.** An external review (`reviews.md`) of the first `PROCEED`
> found two P1 correctness gaps and several P2 evidence/compatibility gaps and
> asked for changes before Phase 1. All findings are addressed in this revision
> (see [Review re-entry changes](#review-re-entry-changes)). The decision is
> re-issued here only after the corrected evidence below. No stop condition
> from `plan-phase0.md` §11 occurred.

## Review re-entry changes

- **P1 — version validation.** Both `format_version` **and** `dialect_version`
  are now read and validated (`0`) on every operation that touches an existing
  catalog (CREATE/SELECT/DELETE), before resolving or interpreting it. New
  tests set each version to a future value through the native connection,
  reopen, and assert every op returns `Format` with the schema/data unchanged
  (`version_refusal.rs`).
- **P1 — commit failure.** `with_tx` now treats a body error, an injected
  commit failure, or a real `COMMIT` failure as cleanup paths. A custom public
  Turso `IO` wrapper fails the actual WAL sync completion. This exposed and
  fixed the case where Turso had already cleared the transaction and a second
  `ROLLBACK` masked the original I/O error. A safe `BEGIN`/empty-rollback probe
  now verifies cleanup without matching error strings. Tests prove the
  original I/O category, connection reuse, reopen state, and combined error
  reporting when rollback itself is injected to fail.
- **P2 — error taxonomy.** Manual `From<ParseError>` routes
  `UnsupportedSyntax` parser errors to a distinct category (span preserved);
  typed `From<LimboError>` maps `Constraint`/`ForeignKeyConstraint` →
  `Constraint`, storage-I/O completion variants → `Io`, else `Engine` — no
  string matching. New `error_categories.rs` covers Parse, UnsupportedSyntax,
  Constraint, Format, Transaction, Engine, and Io through the public path.
- **P2 — index plan over the real lowering.** `explain_field_filter` wraps the
  actual `Select` AST from `physical_select_by_field_stmt` in
  `Cmd::ExplainQueryPlan`. Because the public engine API cannot directly
  prepare that command, the helper renders and reparses it, then requires
  structural AST equality after removing display-only implicit result names.
  The explained predicate is therefore the optimizer-relevant translated AST,
  without a core change.
- **P2 — quoted record IDs.** `COMPAT.md` declares only a bare id
  (`table:identifier`); quoted ids (`person:'tracy'`) are now rejected with an
  explicit unsupported error. New parser test.
- **P2 — P0-INJECT-001.** New end-to-end `injection.rs`: a value containing a
  quote and semicolon (`'a;''b'` → `a;'b`) is stored literally, round-trips
  through reopen, and leaves the schema unchanged (exactly 3 tables).
- **P2 — benchmark equivalence.** Both paths use the canonical rid codec,
  `SELECT rid, json(doc)`, the same rid/doc decoders, typed `Record`/empty
  delete result construction, retained temp directories, and delete-only
  timing. The full 100-sample benchmark was rerun.
- **P2 — upstream evidence** recorded (see below), and **`cargo fmt --all`
  --check** now passes for the FastDB crates.

## Pinned Inputs

- Turso SHA: `977383ff40edc44ef410af062ed0d2322252a869` (ancestor of `phase-0` HEAD; `upstream` → `tursodatabase/turso`).
- SurrealDB behavioral reference: `v3.1.5` (public docs + black-box only).
- Rust toolchain: `1.88` (`rust-toolchain.toml`), target `x86_64-unknown-linux-gnu`.
- Durability: stable WAL, full durability, single connection, single writer. MVCC, multiprocess WAL, experimental index methods, FTS, encryption, sync disabled.

## Repository Changes

- New crates: `turso_fastdb_parser` (`fastdb-parser/`), `turso_fastdb` (`fastdb-frontend/`), `turso_fastdb_tests` (`fastdb-tests/`), `turso_fastdb_benchmarks` (`fastdb-benchmarks/`). All `publish = false`, no MIT license inheritance.
- Root `Cargo.toml`/`Cargo.lock`: registered the four crates and the `testing` feature; no other change. `rust-toolchain.toml` unchanged.
- Turso's root `COMPAT.md` (SQLite matrix) relocated to `docs/upstream-turso-sqlite-compat.md` (history-preserving `git mv`); FastDB's `COMPAT.md` is the SurrealQL matrix.
- **No Turso core change.** `git diff 977383ff -- core/ sqlite/ bindings/ postgres/ tests/ extensions/ cli/ sync/ serverless/` is empty, including the working tree.

## Vertical Slice Results (`turso_fastdb_tests::vertical_slice`)

`CREATE person:tracy SET name = 'Tracy';` returns the typed record
`{ id: person:tracy, name: 'Tracy' }`. After clean close + reopen,
`SELECT * FROM person:tracy;` returns the same record. Internal inspection
(test-only native connection): one **version-0** metadata row, one `person`
catalog row whose physical name is `__fastdb_t_<32-hex>` (contains neither
`person` nor `tracy`), and a physical row storing canonical `rid` (`s:5:tracy`)
and a `doc` with **no `id` member**. `DELETE` returns the empty default; a
second reopen shows the record absent while catalog/table definitions remain.
`PRAGMA integrity_check` → `ok`.

## Atomicity Results (`turso_fastdb_tests::atomicity`)

The six injected transaction-stage failures force the first `CREATE` to fail;
on reopen the schema is **empty** (no catalog tables, physical table, or
record), `integrity_check == ok`, and a subsequent `CREATE` succeeds. Two
additional cleanup cases exercise the real WAL-sync boundary and a failed
rollback:

| Failpoint | After reopen |
| --- | --- |
| after bootstrap | empty schema, integrity ok, next CREATE ok |
| after catalog row | empty schema, integrity ok, next CREATE ok |
| after physical DDL | empty schema, integrity ok, next CREATE ok |
| after record prepare | empty schema, integrity ok, next CREATE ok |
| after record insert | empty schema, integrity ok, next CREATE ok |
| **commit failure** (injected) | empty schema, integrity ok, next CREATE ok |
| **real WAL sync completion failure** | original `Io`; same connection reusable; retry survives reopen; integrity ok |
| **rollback failure** (injected) | original + cleanup errors retained; drop/reopen is empty; next CREATE ok |

DDL + catalog/data DML roll back as one unit. Duplicate explicit id →
`Constraint` error; reopen shows exactly one unchanged record.

## Index Evidence (`turso_fastdb_tests::index_plan`)

- Canonical expression (shared by index and filter): `json_extract(doc, '$.name')`.
- `EXPLAIN QUERY PLAN` is run over the **actual translated filter AST** after a
  structurally checked render/reparse round trip, before and after reopen.
  Detail:

  ```
  SEARCH __fastdb_t_d659d404fd7890230040e5b03e8e3a65
    USING INDEX __fastdb_i_7ec6601309ea7260f5e4339928fd9c52
    (json_extract (doc, '$.name')=?)
  ```

  `SEARCH … USING INDEX` (not a full `SCAN`); results correct before and after
  reopen; deleting a record removes it from the index.

## Upstream Regression Results

| Command | Result |
| --- | --- |
| `cargo build -p turso_core -p turso_parser` | Passed; 2 pre-existing upstream warnings (`core/mvcc/persistent_storage/logical_log.rs`, `core/vdbe/mod.rs`), 0 errors. |
| `cargo test -p turso_core --lib` | `ok. 2286 passed; 0 failed; 17 ignored`. |
| `cargo test -p core_tester --test integration_tests expression_index` | `ok. 3 passed` (JSONB + expression index). |
| `cargo test -p core_tester --test integration_tests without_mvcc` | `ok. 5 passed` (WAL / transaction / header). |
| `cargo test -p core_tester --test integration_tests committed_wal_survives_power_loss` | `ok. 1 passed` (reopen / durability). |
| `cargo test -p turso_pg_tests` | `ok. 412 passed; 0 failed`. |

Pre-existing upstream warnings are documented in `docs/phase0-engine-audit.md`
and **not fixed**. These cover the §12 requirement for relevant unchanged
Turso JSONB, expression-index, transaction, WAL, reopen, and PostgreSQL
frontend suites.

## Benchmark Results (`turso_fastdb_benchmarks`)

Release; FastDB full path vs native (statement prepared once); **both paths
now materialize the same logical result**. See `docs/benchmarks/phase0.md` for
full numbers and methodology.

| Workload | FastDB | Native | Ratio |
| --- | ---: | ---: | ---: |
| cold_create | 1.497 ms | 369.4 µs | 4.1× |
| steady_create | 48.91 µs | 9.694 µs | 5.0× |
| point_read | 29.60 µs | 1.995 µs | 14.8× |
| indexed_filter | 30.06 µs | 2.058 µs | 14.6× |
| delete | 36.05 µs | 7.810 µs | 4.6× |

The ratios are **uncached FastDB (re-parse + catalog/version round-trips +
re-prepare every call) over a cached engine baseline**. They remain far above
the future 1.5–2× MVP gates and must drive profiling/caching work in Phases
1–3. Phase 0 itself defines fair measurement—not a performance pass ratio.
Criterion reports median/mean/slope estimates but not p95/p99; see the
benchmark report for that explicit limitation.

## FastDB Test Results

`cargo test -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests`:
parser 30, frontend 15, atomicity 9, error-categories 7, index-plan 1,
injection 1, version-refusal 2, vertical-slice 1 — **66 tests, 0 failures**.

## Compatibility and Clean-Room Review

- `COMPAT.md` Phase 0 rows: `CREATE-001`, `SELECT-001`, `SELECT-002`,
  `DELETE-001` (each `Partial`, enumerated). Quoted record ids are rejected
  (declared unsupported). All other grammar `planned`/`unsupported`; no family
  claims full support.
- Parser rejects every unsupported form (`RETURN`, `ONLY`, `LIMIT`, multiple
  `SET`, multiple statements, record-id+`WHERE`, `DELETE FROM/WHERE`, numeric
  ids, quoted ids, unterminated strings) with explicit errors.
- Provenance: `docs/compat-research/phase0.md` cites public SurrealDB docs
  only; no SurrealDB source/tests/fixtures were read, copied, translated, or
  vendored.
- All user values are bound parameters; no user value or logical identifier is
  interpolated into SQL text; physical names are validated opaque; the
  canonical JSON path is built by one validated builder. Audit: no `unsafe`
  (`forbid`), no SQL string-building with user data, no experimental feature
  enabling. Non-test `expect()`s are documented invariants (1-based param
  indices, parser-guaranteed ids).

## Cloud C0 Notes

`docs/cloud/phase0.md`: `database_id` is persistent but not a stable global id;
future log needs epoch/sequence/idempotency/version/checksum; Phase 0 exposes
no deterministic logical mutation; `sync/engine`, `core/mvcc/persistent_storage`,
`aristo` flagged for later audit (not endorsed); C0 unknowns listed. Core is
not network-dependent.

## Documented deviations from the literal plan

1. Crate directories are `fastdb-{parser,frontend,tests,benchmarks}` (the
   plan's `tests/` collided with Turso's `tests/`). Plan permits boundary
   adjustments.
2. Physical `doc` is `BLOB` under STRICT (STRICT rejects the `JSONB` type name
   on the pinned engine); the JSONB content invariant is preserved. Plan-sanctioned.
3. Clippy uses `#![deny(warnings)]` in-crate and
   `cargo clippy -p <fastdb> --all-targets` without a global `-D warnings`,
   which would otherwise fatalize two pre-existing upstream warnings
   (`docs/phase0-engine-audit.md`). `cargo fmt --all -- --check` passes.
4. `plan.md` (historical pre-planning) was not carried into the monorepo;
   `AGENTS.md` documents this, and `revised_plan.md` + per-phase plans are the
   live sources.

## Risks and Follow-ups

- Performance: per-call catalog/version resolution (three read round-trips)
  and statement
  re-prepare dominate read ratios → catalog cache + prepared/lowered-statement
  cache (Phase 1–3).
- Commit failure cleanup depends on audited Turso statement lifecycle behavior;
  the real WAL-sync completion test pins that behavior without a core change.
- Phase 0 format is disposable (`version 0`); stable catalog/migration is
  Phase 2. Public `DEFINE INDEX`, async API, CLI, transactions, richer
  values/types, and crash/recovery testing are later phases.
- Re-audit upstream before any pin update.

## Definition of Done

The §12 repository, upstream-feasibility, parser/frontend, atomic-storage,
vertical-slice/indexing, and performance/quality items are satisfied with the
evidence above, after addressing every review finding. The four deviations
above are intentional and documented. Phase 0 is **not** claimed to be
production-ready, fully SurrealQL-compatible, cloud-ready, or ACID-certified.
