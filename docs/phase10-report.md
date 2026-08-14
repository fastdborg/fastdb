# FastDB Phase 10 report

Status: technically complete locally on 2026-08-13
Engine pin: `977383ff40edc44ef410af062ed0d2322252a869`
Compatibility reference: SurrealDB `v3.1.5`
Release authority: none; no tag, package publication, upload, beta, or production-ready claim

## Outcome

Phase 10 adds bounded `QueryOptions`/`ResourceLimits`, request-local engine
timeouts, metadata-only event hooks, deterministic database lifecycle,
provider-aware check/rebuild, consistent validated backup, and CLI
check/backup/restore/rebuild commands. The format remains version 2, the engine
pin did not change, and no inherited Turso source was modified.

Maintenance uses a database-scoped read/write lease. Ordinary requests share
the read side. Check, backup, and rebuild take the write side and reject an
already-active explicit transaction. Backup checkpoints the stable WAL, copies
through a unique same-directory temporary file, fsyncs, validates, atomically
renames, and fsyncs the parent. Restore validates before and after its copy and
uses the same no-overwrite publication rule.

The supported check path reloads and validates format/catalog ownership,
physical objects, hidden graph/FTS/vector columns, adjacency indexes, vector
document/BLOB agreement, and engine integrity. It accepts only the exact
catalog-derived pinned FTS directory-index diagnostic characterized in Phase 8
and reports that exception explicitly.

## Resource and lifecycle contract

Defaults are bounded at 30 seconds, 10,000 returned rows, 16 MiB returned
values, 8 graph hops, 65,536 vector dimensions, and 4 KiB FTS query text. Hard
ceilings are 300 seconds, 100,000 rows, 64 MiB, 16 hops, 65,536 dimensions, and
64 KiB. Invalid limits and syntax-derived breaches fail before engine work.
Limit failures in a transaction guard roll back the entire transaction.

Returned-size limits are checked after successful materialization. Therefore a
standalone multi-statement request retains the established Phase 3 boundary:
earlier mutations can already be committed. The API and operations guide direct
mutation callers that do not need rows to `execute` with `RETURN NONE`; this is
documented rather than presented as implicit replay or rollback.

`Database::close` rejects live connections, prevents new connections after a
successful close, and is idempotent. Hook callbacks receive operation,
duration, mutation count, output rows/bytes, and error category only. Consumer
panics are contained and do not stop the worker.

## Verification evidence

Independent Phase 10 API, CLI, and frontend integration tests cover all limit
families, request-local reset, guarded rollback, lifecycle refusal/idempotence,
active-transaction maintenance refusal, mixed graph/FTS/vector check and
backup, deterministic pseudo-random document hashes across backup, provider
rebuild and plan selection, injected interruption after durable temporary copy,
invalid restore cleanup, catalog and adjacency corruption, strict operational
JSON envelopes, and panic-contained metadata events. The unchanged Phase 5,
Phase 7 graph, Phase 8 FTS, and Phase 9 vector abrupt-exit tests provide the
acknowledged-data recovery evidence.

The committed unchanged Phase 5 workload result is
`docs/benchmarks/phase10-phase5-regression.json`: 5,000 seed records, 200
warmups, and 200 samples. It passed with point-read p50 1.32996x, point-read
p99 1.10733x, indexed-filter p95 1.70717x, write p95 1.01616x, and checkpointed
storage 1.01131x. Two preceding runs were discarded as timing-noisy after
failing different latency gates; no threshold or workload was weakened.

The local matrix passed:

```text
cargo metadata --locked --no-deps --format-version 1
cargo fmt --all -- --check
cargo fmt --manifest-path fastdb-parser/fuzz/Cargo.toml -- --check
cargo fmt --manifest-path fastdb-tests/fuzz/Cargo.toml -- --check
cargo clippy -p turso_fastdb_parser -p turso_fastdb -p fastdb -p fastdb-cli \
  -p turso_fastdb_tests -p turso_fastdb_benchmarks --all-targets --no-deps -- -D warnings
cargo test -p turso_fastdb_parser
cargo test -p turso_fastdb
cargo test -p fastdb --all-targets
cargo test -p fastdb --doc
cargo test -p fastdb-cli --all-targets
cargo test -p turso_fastdb_tests --all-targets
cargo test -p turso_core --lib
cargo test -p turso_pg_tests
cargo test -p turso_whopper
cargo build --release -p fastdb-cli -p turso_fastdb_benchmarks
target/release/phase5-release-bench --records 5000 --samples 200 \
  --output docs/benchmarks/phase10-phase5-regression.json
sha256sum -c SHA256SUMS  # from fastdb-tests/fixtures
git diff --check
```

The inherited Turso build retains its pre-existing unused `CollationSeq`
warning. FastDB crates deny warnings and pass scoped Clippy with `-D warnings`
and `--no-deps`.

## Remaining gate

Phase 10 does not authorize a release or a production-ready claim. Parallel
writers remain prohibited. Phase 11 must first audit an exact Turso
implementation for snapshot isolation, recovery, garbage collection, bounded
memory, and checkpoint behavior; the roadmap stops there if no stable candidate
qualifies.
