# FastDB Phase 3 — CRUD, Parameters, Results, and Transactions

Status: authoritative execution plan, 2026-08-12

## 1. Objective and boundary

Phase 3 completes the synchronous MVP frontend semantics over Phase 2 commit
`c364bbbe6`. It retains Turso pin
`977383ff40edc44ef410af062ed0d2322252a869` and format/dialect version 1.

The phase implements CRUD expressions, named value parameters, projections,
ordering, pagination, exact result shapes, lazy ordered scripts, and explicit
transactions. The asynchronous public API, transaction guard, CLI,
cancellation, release hardening, functions, richer update operators, and
broader SurrealQL remain Phase 4 or later.

No file under Turso core, the SQLite parser, WAL, JSONB, optimizer, inherited
tests, or `.github/workflows/` may change. FastDB input continues through the
independent parser and directly constructed Turso AST with bound values via
`prepare_translated_stmt_with_options`. Format 1 catalogs and physical storage
do not change.

## 2. Request and result contract

The provisional synchronous interface is:

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

`Connection::execute(source)` uses empty parameters and
`Connection::execute_with_params(source, &params)` executes a script. A
response is returned only if the complete request succeeds. Outside an
explicit transaction, earlier statements remain committed after a later
failure even though that call returns only the error.

Schema and transaction statements return `None`. CRUD returns `Rows`, except
`CREATE ONLY` and `SELECT ... FROM ONLY record`, which return `Value`; their
no-match shape is `Value::Null`. `RETURN NONE` and default DELETE return empty
rows. `CREATE RETURN BEFORE` returns one null row. Full records are objects
with a synthesized typed `id`; projections may omit or overwrite it.

Parameter names exclude `$`, are case-sensitive, and use the parser's
identifier contract. Validate names, finite numbers, recursive limits, and
record IDs before execution. Extra parameters are ignored. Every parameter
referenced by a statement must exist before that statement runs. Parameters
are values only and cannot alter identifiers, paths, clauses, or source.

## 3. Expression and document semantics

Evaluation uses an internal `Missing` value. Missing, null, false, numeric
zero, and empty strings/arrays/objects are falsey. `NOT` returns bool;
`AND`/`OR` short-circuit and return operands. Integer arithmetic is checked;
mixed numeric arithmetic promotes to finite floats; integer division
truncates; division by zero returns null. Type errors, overflow, and
non-finite results are `Schema` errors.

Equality is recursive, including exact integer/float equality across all
`i64`. Ordering is
`Missing < Null < Bool < Number < String < Array < Object < RecordId`, with
lexicographic arrays/objects and record IDs ordered by table and component
rank (integer, string, UUID).

Top-level `id` is a virtual read-only field available to expressions,
filters, projections, and ordering. It remains invalid in stored documents,
assignments, declarations, and index paths.

CREATE SET expressions see an empty pre-create document. UPDATE WHERE and all
RHS expressions see the pre-update document. Evaluate every RHS before
applying assignments in source order; later conflicts win. Assigning Missing
removes the path without pruning empty ancestors.

SELECT applies WHERE, ORDER BY, START, LIMIT, then projection. Unaliased
nested projections preserve nesting; aliases write one top-level key;
missing projections become null; collisions are last-write-wins. `SELECT
ONLY` is valid only for a record target. Missing UPDATE/DELETE record targets
succeed with zero rows.

## 4. Parsing, planning, and lowering

Add a lazy `StatementCursor` which tokenizes and parses one statement at a
time while retaining source-global spans and cumulative input, token, and
statement limits. `parse` and `parse_one` consume this cursor. This preserves
commits from earlier standalone statements when a later lexical, parse, or
unsupported-syntax error occurs.

FastDB's evaluator is authoritative. Candidate rows are decoded and filtered
in Rust. Push down only predicates that cannot exclude a true FastDB match:
record RID constraints, typed scalar equality, safe required-field ranges,
and safe conjuncts under `AND`. Mixed types, missing/null, OR, NOT,
field-to-field comparisons, and complex values remain exact post-filters.
Pushed equality/range expressions use the canonical JSON extraction builder
shared with index DDL.

UPDATE and DELETE select candidates within the statement transaction,
evaluate and validate all resulting documents, then bind whole-document
updates or RID deletes. Turso maintains physical indexes. Any intermediate
unique conflict aborts the statement.

## 5. Transactions and concurrency

Each connection serializes execution and has states `Idle`, `Active`,
`Poisoned`, and unrecoverable `Broken`. `BEGIN` uses direct-AST `BEGIN
IMMEDIATE`. An active transaction owns a private catalog candidate. Catalog
changes become globally visible only after engine COMMIT succeeds.

A coordinator-owned schema lease serializes schema-capable writers. Other
catalog consumers wait only after an explicit transaction has made an
uncommitted schema change. DEFINE and implicit table creation work inside
explicit transactions.

Any parse, unsupported, parameter, schema, constraint, engine, or I/O error
during an active transaction attempts a full rollback, discards its catalog
candidate, releases its schema lease, and enters `Poisoned`. Only `CANCEL`
clears `Poisoned`; COMMIT and every other statement return `Transaction`.
Rollback cleanup failure enters `Broken`; the connection must be reopened.
Drop performs best-effort rollback and always releases the lease.

Standalone writes remain individually atomic. Busy/locked engine variants
map to `Transaction` without retry. Constraint messages preserve logical
operation context and never expose generated SQL or opaque physical names.

## 6. Evidence and gates

Independently authored groups are `P3-PARSE-*`, `P3-EXPR-*`, `P3-PARAM-*`,
`P3-CRUD-*`, `P3-RESULT-*`, `P3-SCRIPT-*`, `P3-TXN-*`, `P3-ATOMIC-*`,
`P3-IDX-*`, `P3-MODEL-*`, `P3-COMPAT-*`, and `P3-BRIDGE-*`. Cover in-memory
and file-backed execution, reopen, malformed/injected input, missing records,
schema/index maintenance, rollback, and exact shapes. Every promoted matrix
row needs parser, provenance, execution, file-backed, and in-memory evidence.

Record independent SurrealDB v3.1.5 probes in
`docs/compat-research/phase3.md`. Finish with `docs/phase3-report.md`, leading
with `Proceed to Phase 4` or `Stop for design review`, and record exact
commands, results, durability evidence, index plans, changed-file provenance,
and remaining risk.

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
cargo test -p core_tester --test integration_tests test_transaction_visibility
cargo test -p core_tester --test integration_tests test_bind_parameters_update_query
cargo test -p core_tester --test integration_tests test_rollback_on_unique_constraint_violation
cargo test -p core_tester --test integration_tests committed_wal_survives_power_loss
cargo test -p turso_pg_tests
git diff --check
```

## 7. Definition of Done

Proceed only when the promoted matrix passes in memory and on disk; explicit
transaction errors leave no partial catalog, schema, or data state;
parameter/identifier/path injection cannot change structure; safe indexed
predicates select cataloged indexes after reopen; format 1 opens without
migration; GitHub Actions remain disabled; and no inherited implementation,
test, or workflow file changed.

The audited current `upstream/main` for this phase is
`a94102c20b4c1c554f7c246606c2ed74db47199c`; no upstream integration or pin
change is part of Phase 3.
