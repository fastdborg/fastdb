# FastDB Phase 2 Report

## Proceed to Phase 3

Phase 2 satisfies the stable format-1, catalog, value/RID codec, canonical path, schema, public DEFINE, expression-index, concurrency, atomicity, compatibility, and local verification gates. Empty open remains read-only; malformed/unknown formats fail closed; failed schema/index/first-write operations publish neither persistent ownership nor cache state; generated UUIDv7 IDs are source-addressable; and every tested cataloged index is selected after reopen.

The executable surface remains exactly the Phase 2 slice. UPDATE, broader expressions/projections, pagination, return modes, parameters, scripts, ONLY, multiple SET assignments, and explicit transactions remain spanned `UnsupportedSyntax` boundaries for Phase 3. No Turso core change or public API/release-readiness claim is part of this decision.

## Inputs and upstream decision

- Active contract: `revised_plan.md` and authoritative `plan-phase2.md`, dated 2026-08-12.
- Retained Turso pin: `977383ff40edc44ef410af062ed0d2322252a869`.
- Audited fetched `upstream/main`: `5b109c79461bd31ae05c8ce0f7b0075f7fb4def7`.
- Upstream decision: retain the pin. Current main contains substantial schema, optimizer, transaction, and API changes; advancing requires a dedicated sync branch and comparative audit.
- SurrealDB reference: official unmodified `v3.1.5` Linux x86-64 binary plus public documentation. Archive SHA-256: `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`.
- Rust: stable `rustc 1.88.0`; fuzz toolchain `rustc 1.99.0-nightly`; `cargo-fuzz 0.13.2`.

The upstream audit followed `.claude/skills/upstream-sync/SKILL.md`: the official remote was fetched, both exact SHAs were recorded, and no merge, cherry-pick, or pin change occurred.

## Stable format and open behavior

Format/dialect 1 use four strict catalogs: singleton metadata; immutable table ownership; canonical field path/type definitions; and ordered, versioned index definitions. Logical names and original definitions are bound values. Physical table/index names derive only from validated 128-bit catalog IDs.

Physical records use exact strict `(rid TEXT PRIMARY KEY, doc BLOB NOT NULL)` tables. `doc` is Turso JSONB produced from bound canonical JSON. Catalog and physical schemas are compared with the reviewed direct-AST DDL representation on every nonempty open.

`P2-CAT-001` through `P2-CAT-006` prove:

- repeated empty opens create no schema object;
- first mutation creates four catalogs plus one opaque physical table;
- format 0, future format/dialect/migration, and foreign nonempty databases are refused;
- malformed IDs, missing/mismatched physical tables, and orphan reserved objects are refused;
- migration level 0 to 1 is atomic and idempotent, with cache publication only after commit;
- database clones share the committed catalog snapshot;
- malformed stored RIDs and unknown value tags are `Format` corruption.

The shared coordinator holds its schema mutex and cache write lock through each schema transaction. Mutations operate on a cloned candidate snapshot and replace the shared snapshot only after engine commit.

## Values, IDs, paths, and schema

The public value model covers null, bool, signed `i64`, finite `f64`, UTF-8 string, recursive arrays, deterministic objects, and typed record IDs. Duplicate object keys normalize last-value-wins. Top-level `id` is rejected and synthesized from the immutable RID when decoding.

Stable RID encoding is:

```text
v1:s:<UTF-8-byte-length>:<text>
v1:i:<canonical-i64>
v1:u:<lowercase-hyphenated-UUID>
```

Embedded record IDs use the version-1 `$fastdb` envelope; colliding user objects use its object escape. Recursive decode rejects malformed/unknown tags, invalid numbers/RIDs, invalid document roots, and stored top-level IDs as format corruption.

The parser accepts adjacent `u'…'` and `u"…"` canonical lowercase UUIDv4/v7 RID components. Omitted CREATE IDs use `Uuid::now_v7`; `P2-BRIDGE-001` verifies version/variant properties and re-addresses the generated record using its source rendering. String and UUID RIDs do not collide.

Paths use JSON-escaped dot-quoted form such as `$."profile"."age"`. `P2-PATH-001` covers dots, quotes, brackets, backslashes, controls, and Unicode; `P2-PATH-002` covers nested traversal; `P2-PATH-003` structurally compares the filter and index expression ASTs.

`P2-SCHEMA-003` through `P2-SCHEMA-006` cover both table modes; all base/option types; absence/null behavior; numeric normalization; undeclared/nested paths; schemaless extras; existing-row validation and normalization; duplicate/missing owners; reserved `id`; and contradictory parent/descendant types. `Schema` is a distinct error category. Constraint messages expose only logical context.

## Index evidence

Phase 2 creates ordered non-unique/unique expression indexes over missing, null, bool, integer, finite float, and string paths. Existing rows are decoded and validated before DDL; future writes validate every indexed path. Missing/null components may repeat, while equal complete non-null tuples may not.

`P2-IDX-001` defines one field and one ordered two-field index, resolves their opaque catalog names, and asserts actual `EXPLAIN QUERY PLAN` rows contain `USING INDEX <cataloged-name>` without an unrelated full scan. Both pass before close and after reopen, and the composite-AND query returns the correct record.

`P2-IDX-002` through `P2-IDX-004` prove unique null/missing repetition, existing/future duplicate refusal, write maintenance, non-scalar rejection, cleanup after failed definition, and logical duplicate-index errors. `P2-PATH-003` separately fixes structural identity of the optimizer-relevant expression.

## Atomicity and concurrency

The Phase 0 atomicity suite now runs over format 1 and covers bootstrap, table catalog insertion, physical DDL, record prepare/insert, injected commit/rollback failures, and a real WAL-sync completion error. The WAL test proves original `Io` classification, no visible failed record, same-connection reuse, retry across reopen, and `integrity_check = ok`.

Additional Phase 2 boundaries:

| Boundary | Evidence |
| --- | --- |
| migration update/commit | `P2-CAT-003` |
| existing-row field validation and field catalog write | `P2-ATOMIC-009` |
| index validation, physical DDL, and catalog write | `P2-ATOMIC-010` |

Each injected failure compares the shared snapshot to its pre-operation value and inspects persisted ownership/objects. `P2-CONC-001` through `P2-CONC-003` use synchronized threads to prove one duplicate-DEFINE winner, one implicit table with two records, no orphan objects, and read-only concurrent empty opens.

## Compatibility and clean-room evidence

`docs/compat-research/phase2.md` records independent v3.1.5 probes for UUID RIDs, text/UUID non-collision, duplicate keys, reserved `id`, required/optional/null behavior, numeric coercion, nested schemas, duplicate definitions, unique missing/null behavior, existing duplicates, and composite unique indexes. Observed differences are explicit where FastDB follows its narrower contract.

`COMPAT.md` retains stable feature rows and Phase 1 parser provenance. Phase 2 executable rows are `Partial` or `Supported`; Phase 3 forms remain `Planned`; exclusions remain `Unsupported`. `P2-COMPAT-001` verifies matrix shape, statuses, provenance links, and real Phase 2 evidence.

No SurrealDB source, tests, fixtures, expected-output files, or fuzz corpus was read, copied, translated, or committed.

## Test and fuzz results

| Package | Result |
| --- | --- |
| `turso_fastdb_parser` | 30 passed: 29 parser/UUID tests plus contract test |
| `turso_fastdb` | 14 passed, including codec/path/schema/AST tests |
| `turso_fastdb_tests` | 48 passed across Phase 0/1 regressions and Phase 2 groups |

The five-minute parser fuzz gate ran from its detached package:

```text
(cd fastdb-parser && cargo +nightly fuzz run parse -- -max_total_time=300)
Done 8737100 runs in 301 second(s)
```

No crash, assertion, sanitizer finding, artifact, or hang occurred. The root has an unrelated inherited `fuzz/` package without `parse`; the active plan therefore records the intended working directory. That inherited package and lockfile remain unchanged.

Unchanged upstream suites:

| Command | Result |
| --- | --- |
| `cargo test -p turso_core --lib` | 2,286 passed; 17 ignored |
| `cargo test -p core_tester --test integration_tests expression_index` | 3 passed |
| `cargo test -p core_tester --test integration_tests without_mvcc` | 5 passed |
| `cargo test -p core_tester --test integration_tests committed_wal_survives_power_loss` | 1 passed |
| `cargo test -p turso_pg_tests` | 412 passed |

## Local gates

All required gates passed locally on 2026-08-12:

```text
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

The benchmark gate compiles the Phase 0 harness in release mode; Phase 2 makes no new performance claim. Lints/tests report only inherited Turso warnings in `core/vdbe/mod.rs`, `core/json/cache.rs`, and core-test-only logical-log imports.

## Actions and changed-file audit

Final GitHub API state is `{"enabled":false,"sha_pinning_required":false}`. All verification stayed local. The 36 inherited workflow files are present and unchanged.

Changed implementation is confined to FastDB parser/frontend/test/benchmark files, dependency lock entries, compatibility/format/research/report documents, and active plans. No file under `core/`, `sqlite/parser/`, `postgres/`, inherited `tests/`, WAL, JSONB, optimizer, or `.github/workflows/` changed. The inherited root `fuzz/Cargo.lock` is unchanged.

The lowering audit found no FastDB input passed to Turso's SQLite parser, no user-input SQL generation, no AST `Debug` lowering, and no logical name used as a physical identifier. Production execution remains direct Turso AST through `prepare_translated_stmt_with_options`. The only SQLite-parser use is the existing test-only EXPLAIN structural round trip.

## Remaining Phase 3 boundary and risks

- Format 1 is stable for these catalogs/codecs, but the product is not release-ready.
- The coordinator is process-local; cross-process catalog mutation remains unsupported under the MVP's stable single-process WAL choice.
- Existing-row schema/index validation currently collects a streamed engine result into memory before validating; correct but not optimized for very large tables.
- Index selection is proven for the equality/composite-AND slice, not deferred predicates/order/projection/page forms.
- Explicit transaction poisoning, general CRUD/update, parameters, return modes, scripts, async API, and CLI remain later-phase work.
- Actions remain disabled, so later phases require local evidence unless explicitly re-enabled.

No Phase 2 stop condition remains. Proceed to Phase 3 without broadening this into complete SurrealQL compatibility, production readiness, ACID certification, cloud readiness, or release-quality performance claims.
