# FastDB Phase 4 — Asynchronous Embedded API and CLI

Status: authoritative execution plan, 2026-08-13

## 1. Objective and boundary

Phase 4 starts from clean Phase 3 merge `bc607c045` and delivers the first
public embedded Rust API and `fastdb` CLI. It retains Turso pin
`977383ff40edc44ef410af062ed0d2322252a869`, format version 1, dialect
version 1, stable WAL/full durability, and the direct FastDB-AST-to-Turso-AST
request path.

The current `upstream/main` audit is read-only. An engine upgrade, Turso core
change, Cloud C0/C1 implementation, broader SurrealQL, non-Rust SDK, network
server, publishing, and release claims are outside this phase.

## 2. Public embedded API

Add FastDB-owned package `fastdb` without modifying inherited Turso bindings.
Its runtime-neutral asynchronous surface is:

```rust
Builder::new_local(path).build().await
Database::connect()
Connection::query(source, Params).await
Connection::execute(source, Params).await
Connection::transaction().await
Connection::interrupt_handle()
Connection::close().await
Transaction::{query, execute, commit, rollback}
```

Every connection owns one dedicated worker thread. Complete requests are FIFO
serialized. A queued request whose result receiver was dropped is skipped. An
already-started request completes despite receiver drop. Interrupt handles are
cloneable, target only the currently active engine statement, and map a
cooperative interruption to public `Engine`.

Public values are `Value`, deterministic `Object`, `RecordId`,
`RecordIdValue`, `Params`, `StatementResult`, `QueryResponse`,
`ExecutionSummary`, `Error`, `ErrorCategory`, and byte `SourceSpan`.
`ExecutionSummary` reports exact statement and record-mutation counts,
including mutations hidden by `RETURN NONE`; SELECT reports zero mutations.

The stable public error categories are exactly `Parse`, `UnsupportedSyntax`,
`Schema`, `Constraint`, `Transaction`, `Engine`, and `Io`. Internal format and
corruption failures map to `Engine`. Parse and unsupported errors retain their
byte spans.

## 3. Transaction guard

The guard borrows the connection mutably. Transaction-control source inside
the guard is rejected. Any guarded parse, validation, execution, engine, or
I/O error rolls the complete transaction back, queues `CANCEL` to clear the
frontend poison state, closes the guard, and returns the original error unless
cleanup also fails. A combined cleanup error is `Transaction`. Commit and
rollback consume the guard. Dropping an active guard queues rollback before a
later connection request can execute.

## 4. CLI and JSON contract

Add FastDB-owned package `fastdb-cli` with binary `fastdb`:

```text
fastdb [--memory | PATH] [-c SOURCE]
       [--output human|json] [--param NAME=JSON]
```

Support command, piped batch, and interactive input. Interactive multiline
classification comes from lexer/parser state, not the last byte or a required
semicolon. The shell deliberately keeps no persistent history, so parameter
values are not retained. Human output prints deterministic statement
boundaries. Strict mode prints exactly one JSON object per request; batch
errors are one JSON object on stderr with exit code 1. Duplicate parameter
names fail before database execution.

Strict JSON reserves a versioned `$fastdb` envelope. Typed record IDs carry
table and explicit string, integer, or UUID component type. User objects that
contain `$fastdb` are wrapped in an escaped-object envelope, recursively.

## 5. Evidence and gates

Required independently authored groups are `P4-PARSE-*`, `P4-API-*`, and
`P4-CLI-*`. Cover lifecycle, reopen, safe conversions, exact counts, error
spans/categories, request serialization, queued/in-flight future drop,
interrupt/reuse, guard commit/rollback/error/drop, JSON collisions, multiline
input, batch exits, parameter redaction, golden output, Unicode/spaced paths,
and the Phase 3 conformance bridge in memory and on disk.

Required local gates:

```sh
cargo metadata --no-deps --format-version 1
cargo fmt --all -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb -p fastdb \
  -p fastdb-cli -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets
cargo test -p turso_fastdb_parser
cargo test -p turso_fastdb
cargo test -p fastdb --all-targets
cargo test -p fastdb-cli --all-targets
cargo test -p turso_fastdb_tests
cargo test --doc -p fastdb
cargo build --release -p fastdb-cli
cargo bench -p turso_fastdb_benchmarks --bench phase0 --no-run
cargo test -p turso_core --lib
cargo test -p core_tester --test integration_tests expression_index
cargo test -p core_tester --test integration_tests without_mvcc
cargo test -p core_tester --test integration_tests test_transaction_visibility
cargo test -p turso_pg_tests
git diff --check
```

## 6. Definition of Done

Proceed only when a consumer importing solely `fastdb` can create, reopen,
query, mutate, transact, interrupt, close, and inspect stable errors; the CLI
passes the same Phase 3 fixture through memory and disk; counts and JSON shapes
are exact; transaction cleanup and dropped-future semantics are tested; all
local gates pass; no inherited Turso implementation/test/workflow changes; and
`docs/phase4-report.md` leads with `Proceed to Phase 5`.

The Phase 4 upstream audit retained the pin. Fetched `upstream/main` was
`a94102c20b4c1c554f7c246606c2ed74db47199c`; no merge or pin change is part
of Phase 4.
