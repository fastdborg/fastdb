# Proceed to Phase 4

## FastDB Phase 3 report

Phase 3 satisfies the synchronous MVP CRUD, expression, named-parameter, result-shape, lazy-script, safe-index-lowering, explicit-transaction, rollback, and compatibility gates. The implementation retains Turso pin `977383ff40edc44ef410af062ed0d2322252a869`, format version 1, and dialect version 1. No Turso core, inherited test, Turso parser, WAL, JSONB, optimizer, or workflow file changed.

The decision is limited to proceeding with the Phase 4 embedded asynchronous API, transaction guard, and CLI. It is not a production-readiness, complete SurrealQL compatibility, cloud-readiness, ACID-certification, or release-performance claim.

## Baseline and scope

- Phase 2 baseline: `c364bbbe6`.
- Retained Turso pin: `977383ff40edc44ef410af062ed0d2322252a869`.
- Previously audited `upstream/main`: `a94102c20b4c1c554f7c246606c2ed74db47199c`; no merge or pin change was performed.
- Active contract: `revised_plan.md` and authoritative `plan-phase3.md`.
- Behavioral reference: unmodified official SurrealDB `v3.1.5` binary.
- Local verification completed 2026-08-13 in Asia/Ho_Chi_Minh. GitHub Actions remained disabled.

Async API design, a public transaction guard, CLI behavior, cancellation tokens, functions, richer update operators, broader SurrealQL, cross-process access, and release hardening remain outside Phase 3.

## Implemented execution contract

The old single-statement response seam is replaced by:

```rust
type Params = BTreeMap<String, Value>;

struct QueryResponse {
    statements: Vec<StatementResult>,
}

enum StatementResult {
    None,
    Rows(Vec<Value>),
    Value(Value),
}
```

`Connection::execute` uses an empty map; `execute_with_params` executes an ordered script with validated named values. `QueryResponse` is produced only when the complete call succeeds. Results preserve source order and distinguish schema/transaction `None`, ordinary CRUD `Rows`, and ONLY `Value` exactly. Full records synthesize typed `id` values; projections may omit or overwrite `id` without storing it.

Parameter maps are validated before execution for case-sensitive Unicode identifier names, recursive collection/depth limits, finite floats, and valid typed record IDs, including UUIDv4/v7 restrictions. Every referenced parameter must exist before its statement runs. Extra valid entries are ignored. Bindings never enter identifier, path, target, clause, or source positions. `P3-PARAM-001` covers repetition, case, Unicode, UPDATE/DELETE/SELECT/CREATE positions, invalid names, non-finite values, excessive recursion, invalid UUID record IDs, and quote/semicolon injection payloads.

## Parser and evaluator

`StatementCursor` lexes and parses one statement per request from the caller while retaining global byte spans and cumulative input, token, and statement ceilings. `parse` and `parse_one` consume the cursor. Later lexical, parse, and unsupported-syntax errors are therefore discovered only after prior standalone statements have committed. Empty statements and malformed separators remain errors.

The FastDB evaluator is authoritative over decoded values and an internal `Missing` state. Evidence covers:

- falsey missing, null, false, numeric zero, and empty string/array/object;
- boolean-returning `NOT` and short-circuit operand-returning `AND`/`OR`;
- checked integer arithmetic, finite float promotion, truncating integer division, null division by zero, overflow/type errors, and exact integer/float comparison across `i64`;
- recursive equality and total ordering across missing, null, bool, number, string, array, object, and typed record ID;
- virtual read-only top-level `id` in expressions, filters, projections, and ordering;
- pre-document RHS snapshots, source-order assignment application, last-write-wins conflicts, and missing-path removal without ancestor pruning.

SELECT performs WHERE, ORDER BY, START, LIMIT, then projection. Unaliased nested projections retain their shape, aliases write top-level keys, missing fields become null, and projection collisions are last-write-wins.

## CRUD, schema, and index evidence

CREATE, SELECT, UPDATE, and DELETE execute every parsed MVP target and return form. Missing UPDATE/DELETE record targets succeed with zero rows. CREATE expressions observe an empty pre-create document; UPDATE filters and RHS expressions observe each pre-update document. UPDATE and DELETE select, decode, evaluate, validate, and mutate candidates inside the statement transaction. Bound whole-document updates and RID deletes leave physical index maintenance to Turso; an intermediate unique conflict aborts the entire statement.

The candidate planner pushes only safe scalar equality conjuncts and required-field typed ranges. Exact Rust post-filtering remains mandatory. Every pushed predicate reuses the canonical JSON extraction AST used by index DDL. `P3-IDX-001` proves:

- parameterized `name = $name AND age = $age` selects the cataloged composite index;
- parameterized required-field `score >= $minimum` selects the range index;
- both plans remain indexed after file reopen;
- an unsafe OR predicate selects neither index yet returns the exact expected record.

Format-1 schema and index definitions remain readable without migration. File-backed CRUD, uniqueness maintenance, the Phase 0 vertical slice, version refusal, and index plan tests all pass after reopen.

## Explicit transactions and atomicity

Each connection serializes execution and tracks `Idle`, `Active`, `Poisoned`, or unrecoverable `Broken`. BEGIN uses the audited direct-AST `BEGIN IMMEDIATE` path. Active transactions hold private catalog candidates; schema/catalog changes publish only after engine COMMIT. A coordinator schema lease blocks other catalog consumers only after uncommitted schema change.

Any active parse, unsupported-syntax, parameter, schema, constraint, engine, or I/O failure attempts full rollback, discards private catalog state, releases the lease, and enters `Poisoned`. Only CANCEL clears poison. Rollback cleanup failure enters `Broken`; drop retries rollback best-effort and always releases the lease. Busy/locked engine variants map without retry to the stable message `database is busy or locked by another transaction`.

Atomicity evidence includes failures before and after UPDATE/DELETE mutation, catalog/schema/data rollback, commit and rollback failpoints, drop rollback, visibility across connections, schema publication, unique-index conflicts, and connection reuse. The real WAL-sync fault at explicit COMMIT produced the expected cleanup-failure `Broken` state because Turso reported no active transaction to roll back; after dropping the connection, reopen showed no failed record, `integrity_check` returned `ok`, a new mutation committed, and a second reopen preserved it.

Standalone scripts commit each successful statement independently. Tests prove later lexical, parse, unsupported-syntax, and runtime failures preserve earlier commits. Inside BEGIN, the same failure classes roll back all transaction data and catalog/schema changes and require CANCEL.

## Compatibility and clean-room provenance

`docs/compat-research/phase3.md` records independently authored public-documentation references and strict-JSON black-box probes against the unmodified v3.1.5 Linux binary. The archive SHA-256 was `f7d515203ba0010bde3fc6a5706ce7327d356aca293fbba8424d442f5dcb5002`. Probes cover truthiness and operand returns, integer division and numeric equality, ONLY and projection shapes, virtual-id overwrite, pre-create/pre-update return values, snapshot SET evaluation, conflicting assignments, missing targets, ordering, and one-script BEGIN/CANCEL/COMMIT visibility. Parameter-map validation is explicitly identified as a FastDB embedded-API contract because the CLI cannot reproduce that boundary.

`COMPAT.md` now promotes the exact executable MVP rows while retaining explicit syntax exclusions and stable feature IDs. `P3-COMPAT-001` mechanically validates matrix shape, statuses, parser provenance, research links, and real execution-test IDs. `P3-BRIDGE-001/002` execute the promoted surface in memory and on disk with reopen; `P3-BRIDGE-003` keeps excluded syntax spanned and `UnsupportedSyntax`.

## Required gate results

| Command | Result |
| --- | --- |
| `cargo metadata --no-deps --format-version 1` | Passed; workspace and four FastDB packages resolved. |
| `cargo fmt --all -- --check` | Passed after rustfmt corrected one newly added test wrap. |
| `cargo clippy -p turso_fastdb_parser -p turso_fastdb -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets` | Passed; only inherited Turso warnings were emitted. |
| `cargo test -p turso_fastdb_parser` | Passed: 33 tests, including `P3-PARSE` and `P3-COMPAT`. |
| `cargo test -p turso_fastdb` | Passed: 14 tests. |
| `cargo test -p turso_fastdb_tests` | Passed: 69 tests across Phase 0-3 groups. |
| `cargo bench -p turso_fastdb_benchmarks --bench phase0 --no-run` | Passed; release benchmark executable built. |
| `(cd fastdb-parser && cargo +nightly fuzz run parse -- -max_total_time=300)` | Passed: 5,799,935 runs in 301 seconds; no crash; final coverage 3,788 and feature count 17,398. |
| `cargo test -p turso_core --lib` | Passed: 2,286 tests; 17 ignored. |
| `cargo test -p core_tester --test integration_tests expression_index` | Passed: 3 tests. |
| `cargo test -p core_tester --test integration_tests without_mvcc` | Passed: 5 tests. |
| `cargo test -p core_tester --test integration_tests test_transaction_visibility` | Passed: 1 test. |
| `cargo test -p core_tester --test integration_tests test_bind_parameters_update_query` | Passed: 4 tests. |
| `cargo test -p core_tester --test integration_tests test_rollback_on_unique_constraint_violation` | Passed: 1 test. |
| `cargo test -p core_tester --test integration_tests committed_wal_survives_power_loss` | Passed: 1 test. |
| `cargo test -p turso_pg_tests` | Passed: 412 tests. |
| `git diff --check` | Passed. |

The inherited warnings were the existing unused `CollationSeq` re-export, test-only logical-log imports, and an unfulfilled JSON-cache lint expectation. They were not changed because Phase 3 prohibits unrelated Turso core edits.

## Provenance and final risk review

All implementation changes are confined to FastDB-authored parser, frontend, tests, benchmark adaptation, plans, and documentation. The response-adapter used by old Phase 0-2 tests exists only under test/testing configuration; the normal public `QueryResponse` has exactly the Phase 3 `statements` field. User values remain bound parameters. Production execution still constructs Turso AST directly and calls `prepare_translated_stmt_with_options`; no generated SQLite text or Turso SQLite parsing was introduced. The existing test-only EXPLAIN diagnostic continues to round-trip rendered AST solely to inspect plans and never executes FastDB user input.

The principal remaining risks belong to the planned later phases: the current API is synchronous and provisional, there is no public transaction guard or cancellation surface, crash/fuzz hardening is not exhaustive, and the CLI does not yet expose these semantics. The explicit real-sync failure demonstrates the conservative Broken path and clean reopen rather than same-connection recovery. No Phase 3 stop condition remains.
