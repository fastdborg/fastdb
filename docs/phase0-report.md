# FastDB Phase 0 Report

## Decision

**Proceed** to Phase 1.

Phase 0 proves the core architectural thesis: a clean-room FastDB frontend
parses a small SurrealQL subset, lowers it **directly into Turso AST**, and
executes through `Connection::prepare_translated_stmt_with_options`,
atomically persisting/querying/deleting a JSONB document with a usable
expression index — **with no Turso core change**. Every Section-11 stop
condition was checked; none occurred. Evidence is below.

## Pinned Inputs

- Turso SHA: `977383ff40edc44ef410af062ed0d2322252a869` (ancestor of `phase-0` HEAD; `upstream` → `tursodatabase/turso`).
- SurrealDB behavioral reference: `v3.1.5` (public docs + black-box only).
- Rust toolchain: `1.88` (`rust-toolchain.toml`), target `x86_64-unknown-linux-gnu`.
- Durability: stable WAL, full durability, single connection, single writer. MVCC, multiprocess WAL, experimental index methods, FTS, encryption, and sync disabled.

## Repository Changes

- New crates: `turso_fastdb_parser` (`fastdb-parser/`), `turso_fastdb` (`fastdb-frontend/`), `turso_fastdb_tests` (`fastdb-tests/`), `turso_fastdb_benchmarks` (`fastdb-benchmarks/`). All `publish = false`, no MIT license inheritance.
- Root `Cargo.toml`/`Cargo.lock`: registered the four crates and the `testing` feature; no other change.
- `rust-toolchain.toml` unchanged. Planning docs preserved.
- Turso's root `COMPAT.md` (SQLite matrix) relocated to `docs/upstream-turso-sqlite-compat.md` (history-preserving `git mv`) to free the root `COMPAT.md` for the FastDB matrix.
- **No Turso core change.** `git diff 977383ff HEAD -- core/ sqlite/ bindings/ postgres/ tests/ extensions/ cli/ sync/ serverless/` is empty.

## Vertical Slice Results (`turso_fastdb_tests::vertical_slice`)

`CREATE person:tobie SET name = 'Tobie';` returns the typed record
`{ id: person:tobie, name: 'Tobie' }`. After clean close + reopen,
`SELECT * FROM person:tobie;` returns the same record. Internal inspection
(via the test-only native connection) confirms: one **version-0** metadata
row, one `person` catalog row whose physical name is
`__fastdb_t_<32-hex>` (contains neither `person` nor `tobie`), and a
physical row storing canonical `rid` (`s:5:tobie`) and a `doc` with **no
`id` member**. `DELETE person:tobie;` returns the empty default; a second
reopen shows the record absent while catalog/table definitions remain.
`PRAGMA integrity_check` → `ok`.

## Atomicity Results (`turso_fastdb_tests::atomicity`)

All five injected failure points force the first `CREATE` to fail; on reopen
the schema is **empty** (no catalog tables, no physical table, no record),
`integrity_check == ok`, and a subsequent `CREATE` succeeds:

| Failpoint | After reopen |
| --- | --- |
| after bootstrap | empty schema, integrity ok, next CREATE ok |
| after catalog row | empty schema, integrity ok, next CREATE ok |
| after physical DDL | empty schema, integrity ok, next CREATE ok |
| after record prepare | empty schema, integrity ok, next CREATE ok |
| after record insert | empty schema, integrity ok, next CREATE ok |

This proves physical DDL and catalog/data DML participate in one
rollback-capable transaction. Duplicate explicit id → `Constraint` error;
reopen shows exactly one unchanged record.

## Index Evidence (`turso_fastdb_tests::index_plan`)

- Canonical expression (shared by index and filter): `json_extract(doc, '$.name')`.
- Physical index name: `__fastdb_i_<32-hex>`; physical table: `__fastdb_t_<32-hex>`.
- `EXPLAIN QUERY PLAN` detail (before and after reopen):

  ```
  SEARCH __fastdb_t_d659d404fd7890230040e5b03e8e3a65
    USING INDEX __fastdb_i_7ec6601309ea7260f5e4339928fd9c52
    (json_extract (doc, '$.name')=?)
  ```

  The plan names the opaque index and is a `SEARCH … USING INDEX`, not a
  full `SCAN`. Results are correct before and after reopen; deleting a record
  removes it from the index.

## Upstream Regression Results

| Command | Result |
| --- | --- |
| `cargo build -p turso_core -p turso_parser` | Finished; 2 pre-existing upstream warnings (`core/vdbe/mod.rs`, `core/thread.rs`), 0 errors. |
| `cargo test -p turso_core --lib` | `ok. 2286 passed; 0 failed; 17 ignored`. |

Pre-existing upstream warnings are documented in `docs/phase0-engine-audit.md`
and **not fixed** (per `plan-phase0.md`: "do not fix upstream failures"). Broader
upstream suites (`-p tests`, `-p turso_pg_tests`) are referenced for pin
updates; they are not gating for the Phase 0 vertical slice.

## Benchmark Results (`turso_fastdb_benchmarks`)

Release mode, same engine/schema/JSONB/durability; FastDB full path vs native
(statement prepared once). See `docs/benchmarks/phase0.md` for full numbers.

| Workload | FastDB | Native | Ratio |
| --- | ---: | ---: | ---: |
| cold_create | 1.57 ms | 305 µs | 5.1× |
| steady_create | 45.3 µs | 9.41 µs | 4.8× |
| point_read | 22.5 µs | 1.78 µs | 12.6× |
| indexed_filter | 23.3 µs | 1.81 µs | 12.9× |
| delete | 114.8 µs | 26.5 µs | 4.3× |

No pathological result. Overhead is concentrated in per-call parse, two
catalog round-trips, and re-prepare — explicit Phase 1–3 targets (catalog
cache, prepared/lowered-statement cache). Phase 0 has no performance pass
ratio; MVP gates in `revised_plan.md` §11 remain future.

## Compatibility and Clean-Room Review

- `COMPAT.md` Phase 0 rows implemented: `CREATE-001`, `SELECT-001`,
  `SELECT-002`, `DELETE-001` (each `Partial`, enumerated). All other grammar
  `planned`/`unsupported`; no statement family claims full support.
- Parser rejects every unsupported form (`RETURN`, `ONLY`, `LIMIT`, multiple
  `SET`, multiple statements, record-id+`WHERE`, `DELETE FROM/WHERE`, numeric
  ids, unterminated strings) with explicit errors — never silently accepts.
- Provenance: `docs/compat-research/phase0.md` cites public SurrealDB docs
  only; no SurrealDB source/tests/fixtures were read, copied, translated, or
  vendored.
- All user values are bound parameters; no user value or logical identifier
  is interpolated into SQL text; physical names are validated opaque; the
  canonical JSON path is built by one validated builder. Audit scan found no
  `unsafe` (forbid), no SQL string-building with user data, and no
  experimental feature enabling. Non-test `expect()`s are documented
  invariants (1-based param indices, parser-guaranteed ids).

## Cloud C0 Notes

`docs/cloud/phase0.md` records that `database_id` is persistent but not a
stable global id; lists the future log requirements (epoch/sequence/
idempotency/version/checksum); notes Phase 0 exposes no deterministic
logical mutation; flags `sync/engine`, `core/mvcc/persistent_storage`, and
`aristo` for later audit without endorsement; and lists C0 unknowns. Core is
not network-dependent.

## Deviation from the literal plan (documented)

1. **Crate directories** are `fastdb-parser/`, `fastdb-frontend/`,
   `fastdb-tests/`, `fastdb-benchmarks/` (the plan's `tests/` collided with
   Turso's `tests/`; a `fastdb-` prefix avoids all collisions and makes
   provenance auditable). The plan permits boundary adjustments.
2. **Physical `doc` column** is declared `BLOB` (not `JSONB`) under STRICT,
   because STRICT rejects the `JSONB` type name on the pinned engine. The
   logical JSONB invariant is preserved (content is `jsonb(...)`, read via
   `json`/`json_extract`). This is the plan-sanctioned "smallest valid
   physical declaration."
3. **Clippy verification** uses `#![deny(warnings)]` in-crate and
   `cargo clippy -p <fastdb> --all-targets` without a global `-D warnings`,
   which would otherwise fatalize two pre-existing upstream warnings. See
   `docs/phase0-engine-audit.md`.

## Risks and Follow-ups

- Performance: per-call catalog resolution (two round-trips) and statement
  re-prepare dominate read ratios → add a catalog cache and a
  prepared/lowered-statement cache (Phase 1–3).
- Phase 0 format is disposable (`version 0`); stable catalog/migration design
  is Phase 2.
- Public `DEFINE INDEX`, async API, CLI, transactions, richer values/types,
  and crash/recovery testing are out of Phase 0 scope (later phases).
- Re-audit upstream before any pin update; run `-p tests` and
  `-p turso_pg_tests` before rebasing.

## Definition of Done

Repository and policy, upstream feasibility, parser/frontend, atomic storage,
vertical slice/indexing, and performance/quality items are all satisfied —
see the checked items in `plan-phase0.md` §12 against the evidence above. The
only intentional, documented deviations are the three listed above. Phase 0 is
**not** claimed to be production-ready, fully SurrealQL-compatible,
cloud-ready, or ACID-certified.
