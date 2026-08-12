# FastDB Phase 2 — Stable Storage, Catalog, Schema, and Indexes

Status: authoritative execution plan, 2026-08-12

## 1. Objective and boundary

Phase 2 replaces disposable format 0 with stable format and dialect version 1, catalog-backed logical schema, complete MVP value/RID codecs, schema enforcement, and expression indexes. It expands execution only far enough to prove those systems.

No file under Turso core, the SQLite parser, WAL, JSONB, optimizer, inherited tests, or `.github/workflows/` may change. GitHub Actions remain disabled. Full CRUD expressions, projections, pagination, return modes, parameters, scripts, updates, and explicit transactions remain Phase 3.

The executable source shapes are exactly:

```text
DEFINE TABLE name (SCHEMALESS | SCHEMAFULL)
DEFINE FIELD path ON [TABLE] name TYPE type
DEFINE INDEX name ON [TABLE] name FIELDS path [, ...] [UNIQUE]

CREATE table[:id] CONTENT <constant-object>
CREATE table[:id] SET path = <constant-value>

SELECT * FROM table[:id]
  [WHERE path = scalar [AND path = scalar ...]]

DELETE table:id
```

`CREATE ONLY`, every `RETURN`, multiple SET assignments, UPDATE, projections, ordering, pagination, parameters, scripts, and transaction statements remain spanned `UnsupportedSyntax` errors.

## 2. Language and value contract

- Add adjacent `u'…'` and `u"…"` UUID RID components. Accept only canonical lowercase, hyphenated RFC UUIDv4 and UUIDv7 values. Omitted CREATE IDs generate UUIDv7 values and render back to valid source.
- Explicit RIDs support bare/backtick string, signed `i64`, and typed UUID components.
- Constants support null, bool, finite integer/float, string, arrays, objects, record IDs, parentheses, and numeric unary signs. Duplicate object keys normalize last-value-wins into lexicographic key order.
- Parameters, field references, binary arithmetic, `NOT`, and predicates outside equality joined by `AND` fail at the smallest useful span.
- Top-level user `id` is reserved and rejected.

## 3. Stable format 1

Use strict internal tables with bound logical names and definitions:

- `__fastdb_meta`: singleton, format/dialect version, database ID, creation version, last migration.
- `__fastdb_tables`: immutable table ID, logical/physical name, mode, nullable definition (null means implicit schemaless registration).
- `__fastdb_fields`: table ID, canonical path key, canonical type AST, required flag, original definition.
- `__fastdb_indexes`: immutable index ID, table ID, logical/physical name, ordered canonical paths, uniqueness, expression version, original definition.

Physical names are validated opaque names derived from 128-bit IDs. Ownership, uniqueness, and persisted structure are validated transactionally.

Open behavior:

1. A genuinely empty database opens without mutation.
2. A nonempty database without metadata is refused.
3. Read `format_version` before interpreting other catalogs.
4. Refuse format 0, future format/dialect/migration values, malformed catalogs, missing/mismatched physical objects, and orphan reserved objects.
5. Migration 0 → 1 is a transactional no-op that changes only `last_migration`; reopen is idempotent.

A process-local coordinator shares a catalog snapshot and schema mutex for wrappers of the same database. Schema mutation holds the mutex and cache write lock through commit and publishes cache changes only after commit.

## 4. Codecs, paths, and schema

RID encoding is:

```text
v1:s:<UTF-8-byte-length>:<text>
v1:i:<canonical-i64>
v1:u:<lowercase-hyphenated-UUID>
```

Embedded record IDs use the reserved JSON envelope:

```json
{"$fastdb":{"v":1,"t":"rid","table":"person","id":"v1:s:5:tracy"}}
```

User objects colliding with the reserved key use the `t:"object"` envelope. Unknown/malformed stored tags and malformed JSONB are format corruption.

Every field path is encoded as JSON-escaped dot-quoted components, for example `$."profile"."age"`. The same builder owns reads, writes, filters, validation, and indexes.

Schema behavior:

- Declared fields are enforced in both modes; schemaless permits extras.
- Schemafull rejects undeclared paths. Declared descendants authorize ancestor object containers, not siblings.
- Plain types are required. `option<T>` permits absence but never null. Null is invalid for every declared type.
- `float` accepts integers and normalizes them to floats; `number` accepts integers/floats; no other coercion occurs.
- Object descendants must be declared in schemafull mode; arrays have no element subtype.
- New fields validate existing rows before commit. Duplicate definitions are constraint errors; FIELD/INDEX require an existing table.

## 5. Lowering, indexes, and atomicity

- Construct Turso AST directly and use `prepare_translated_stmt_with_options`. Never send FastDB input to Turso's parser or generate SQL from user data, logical identifiers, or AST `Debug`.
- Physical records use `(rid TEXT PRIMARY KEY, doc BLOB NOT NULL) STRICT`; `doc` is always JSONB produced from bound canonical JSON.
- CONTENT binds one encoded object. SET builds a nested document with `json_set` and JSON conversion.
- Indexes are ordered non-unique or unique expression indexes over scalar paths. Missing/null entries may repeat in unique indexes; equal fully non-null tuples may not.
- Existing rows are streamed and validated before index creation. Object, array, and record-valued indexed paths are rejected on definition and later writes.
- Filter and index expressions call the same AST builder. Every declared index needs a named-plan assertion before and after reopen.
- Bootstrap, migration, catalog writes, physical DDL, validation, insertion, commit, rollback, and real WAL-sync completion receive failure coverage. Persisted catalogs and shared cache remain unchanged on failure.
- Error categories include `Schema`; contextual constraint errors never expose physical names or internal SQL.

## 6. Tests and evidence

Required groups are `P2-UUID-*`, `P2-CAT-*`, `P2-CODEC-*`, `P2-PATH-*`, `P2-SCHEMA-*`, `P2-IDX-*`, `P2-ATOMIC-*`, `P2-CONC-*`, `P2-BRIDGE-*`, and `P2-COMPAT-*`. Preserve all Phase 0 Tracy behavior and every Phase 1 parser test.

Create `docs/compat-research/phase2.md`, `docs/format-v1.md`, and `docs/phase2-report.md`. Update `COMPAT.md`. The report leads with `Proceed to Phase 3` or `Stop for design review` and records exact commands/results, rollback/reopen evidence, index plans, changed-file provenance, and remaining risk.

Required local gates:

```sh
cargo metadata --no-deps --format-version 1
cargo fmt --all -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets
cargo test -p turso_fastdb_parser
cargo test -p turso_fastdb
cargo test -p turso_fastdb_tests
cargo bench -p turso_fastdb_benchmarks --bench phase0 --no-run
(cd fastdb-parser && cargo +nightly fuzz run parse -- -max_total_time=300)
cargo test -p turso_core --lib
cargo test -p core_tester --test integration_tests expression_index
cargo test -p core_tester --test integration_tests without_mvcc
cargo test -p core_tester --test integration_tests committed_wal_survives_power_loss
cargo test -p turso_pg_tests
git diff --check
```

## 7. Definition of Done

Proceed only when unknown formats are refused before mutation; schema/index/first-write boundaries roll back atomically; generated UUIDv7 IDs are source-addressable; matching queries select every cataloged index after reopen; all current Phase 1 tests remain green; Actions remain disabled; and no inherited implementation, test, or workflow file changed.
