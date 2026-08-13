# Proceed to Phase 5

## FastDB Phase 4 report

Phase 4 satisfies the asynchronous embedded Rust API, dedicated connection
worker, transaction guard, cooperative interruption, exact execution summary,
stable public error, interactive-completeness, CLI, strict JSON, compile-only
consumer, and Phase 3 CLI bridge gates. The implementation retains Turso pin
`977383ff40edc44ef410af062ed0d2322252a869`, format version 1, dialect
version 1, stable WAL/full durability, and direct translated-AST architecture.

This decision authorizes Phase 5 hardening only. It is not a release,
production-readiness, complete compatibility, cloud-readiness, ACID
certification, or performance claim.

## Baseline and upstream decision

- Phase 3 baseline: clean merge `bc607c045` on branch `phase-next`.
- Official remote: `https://github.com/tursodatabase/turso.git`.
- Retained engine pin: `977383ff40edc44ef410af062ed0d2322252a869`.
- Fetched and audited `upstream/main`:
  `a94102c20b4c1c554f7c246606c2ed74db47199c`, dated 2026-08-12.
- Candidate range: 327 files, 20,036 insertions, 4,734 deletions, including
  core VDBE, optimizer, transaction, parser, CLI, and binding changes.
- Decision: retain the pin. Phase 4 needs no engine change; no merge,
  cherry-pick, pin update, or push occurred.

## Public API and execution contract

FastDB-owned package `fastdb` exposes `Builder`, `Database`, `Connection`,
`Transaction`, `InterruptHandle`, deterministic public values/objects/record
IDs/parameters/results, seven error categories, and byte `SourceSpan`. It does
not expose Turso AST, parser, connection, statement, storage, or catalog types.
The compile-only lifecycle example imports only `fastdb`.

Every public connection owns one dedicated worker thread. Requests are FIFO
serialized as complete units. `P4-API-006` proves concurrent caller
serialization; `P4-API-008` proves a canceled queued receiver skips execution;
`P4-API-009` proves canceling an in-flight receiver does not cancel its
mutation. A cloneable interrupt handle targets only the active engine
statement. `P4-API-007` interrupts a 5,760-record ordered scan, receives
`Engine`, then successfully reuses the same connection.

The synchronous frontend now records exact mutation counts at the operation
boundary, independent of returned rows. `P4-API-001` proves CREATE, UPDATE,
DELETE, SELECT, and `RETURN NONE` accounting plus file reopen. `query` returns
ordered `StatementResult`; `execute` reports total statement count and exact
record mutation count.

Public errors contain exactly `Parse`, `UnsupportedSyntax`, `Schema`,
`Constraint`, `Transaction`, `Engine`, and `Io`. Parse/unsupported diagnostics
retain UTF-8 byte spans. Synchronous internal format/corruption is deliberately
collapsed into public `Engine`. Error detail remains human-facing rather than
a stable matching surface.

## Transaction guard and cleanup

The guard mutably borrows its connection. Source transaction control is
rejected even when it precedes a later malformed statement. Commit and
rollback consume the guard. Dropping an active guard queues `CANCEL` in worker
order. Any guarded operation error first triggers the Phase 3 complete
rollback, then `CANCEL` clears the poisoned state; the original error is
returned unless cleanup also fails, in which case a combined `Transaction`
error is produced. `P4-API-002/003` prove commit, drop rollback, guard-source
rejection, original parse span preservation, and post-cleanup reuse.

## CLI and JSON boundary

FastDB-owned package `fastdb-cli` provides binary `fastdb` with `--memory` or
a path, `-c`, human/JSON output, and repeatable `--param NAME=JSON`. Command,
piped batch, and interactive modes use the public API. The interactive shell
keeps no persistent history. Parser completeness uses lexical/parser state,
so semicolons inside strings/comments and delimiter nesting are handled
without a trailing-semicolon rule.

Human output has deterministic `-- statement N --` boundaries. JSON mode
emits one version-1 `$fastdb` envelope per request. Typed record IDs carry
table plus explicit string/integer/UUID component type; reserved-key user
objects use a recursive escaped-object envelope. Batch errors are JSON on
stderr with exit code 1. Duplicate parameter names fail without echoing the
values. `P4-CLI-001..005` cover golden output, multiline input, JSON success
and error shapes, redaction, memory/disk Phase 3 fixture execution, Unicode and
spaced paths, and reopen.

## Required gate results

| Command | Result |
| --- | --- |
| `cargo metadata --no-deps --format-version 1` | Passed; six FastDB packages resolve at `0.0.0`, all `publish = false`. |
| `cargo fmt --all -- --check` | Passed. |
| `cargo clippy -p turso_fastdb_parser -p turso_fastdb -p fastdb -p fastdb-cli -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets` | Passed; only the two documented inherited Turso warnings appeared. |
| `cargo test -p turso_fastdb_parser` | Passed: 34 tests plus doc tests. |
| `cargo test -p turso_fastdb` | Passed: 14 tests plus doc tests. |
| `cargo test -p fastdb --all-targets` | Passed: 9 API/worker tests and compile-only example. |
| `cargo test -p fastdb-cli --all-targets` | Passed: 5 CLI tests. |
| `cargo test -p turso_fastdb_tests` | Passed: 69 Phase 0-3 integration tests. |
| `cargo test --doc -p fastdb` | Passed. |
| `cargo build --release -p fastdb-cli` | Passed. |
| `cargo bench -p turso_fastdb_benchmarks --bench phase0 --no-run` | Passed; release benchmark executable built. |
| `cargo test -p turso_core --lib` | Passed: 2,286; 17 ignored. |
| `cargo test -p core_tester --test integration_tests expression_index` | Passed: 3. |
| `cargo test -p core_tester --test integration_tests without_mvcc` | Passed: 5. |
| `cargo test -p core_tester --test integration_tests test_transaction_visibility` | Passed: 1. |
| `cargo test -p turso_pg_tests` | Passed: 412. |
| `git diff --check` | Passed. |

## Provenance and risk review

Implementation changes are confined to FastDB-authored crates, tests,
workspace registration, plans, and documentation. No file under Turso core,
SQLite parser, bindings, PostgreSQL frontend, inherited test directories, WAL,
JSONB, optimizer, or workflows changed. FastDB user input still reaches only
the independent parser, frontend evaluation/planning, directly constructed
Turso AST, typed bindings, and `prepare_translated_stmt_with_options`. No user
value or logical identifier was added to generated SQL.

No unsafe code, external message, release, publishing action, workflow
publishing step, format change, or cloud surface was introduced. The remaining
technical risks are Phase 5 work: bounded caches, expanded fuzz/model/injection
suites, stable fixtures, process-crash and public-I/O failure coverage,
cross-platform CI, and measured release benchmarks. Legal approval and remote
CI remain separate external release gates.
