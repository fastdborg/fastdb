# FastDB Phase 10 — Operational Readiness

Status: technically complete locally on 2026-08-13

## 1. Purpose and baselines

Phase 10 turns the committed Phase 9 alpha-candidate surface into an
operable embedded database. It adds bounded request controls, deterministic
lifecycle operations, consistent backup/validated restore, a provider-aware
integrity path, maintenance commands, and observability hooks without changing
format 2, the Turso pin, or the SurrealDB `v3.1.5` behavior reference.

The engine remains pinned to
`977383ff40edc44ef410af062ed0d2322252a869`. No inherited Turso source change
or upstream merge is planned. All operational SQL is static, reviewed internal
SQL; FastDB source and logical identifiers never become generated SQLite text.

## 2. Public Rust contract

Add public `QueryOptions` and `ResourceLimits`. Defaults remain bounded and
preserve existing successful workloads. Per-request overrides may only tighten
documented hard ceilings. Invalid zero or over-ceiling limits fail before the
request reaches Turso.

Support:

- `Connection::query_with_options` and `execute_with_options`;
- the existing methods as default-option wrappers;
- timeout, returned row, returned byte, graph hop, vector dimension, and FTS
  query-byte limits;
- `Database::close` which refuses new connections and succeeds only after all
  connection workers have closed;
- `Database::backup_to`, producing one checkpointed destination artifact;
- `Database::check`, returning a structured provider-aware report;
- a minimal event hook receiving operation name, duration, mutation count,
  output rows/bytes, and success category, never source or parameter values.

Timeout uses the pinned engine query timeout on the connection worker and is
reset after every request. Limit failures in explicit transactions follow the
existing poison-and-rollback contract. Result accounting uses the public value
tree without serializing or logging user values.

## 3. Maintenance serialization and backup

Add a database-scoped maintenance read/write latch. Ordinary requests hold a
shared lease. Backup, restore validation, and whole-database checks take the
exclusive lease so the main file cannot change while it is copied or checked.
This latch is not Phase 11 parallel-writer MVCC and does not change the stable
serialized-writer default.

`backup_to` rejects memory databases, the source path, existing destinations,
and non-UTF-8 paths. It checkpoints the stable WAL with FULL/TRUNCATE semantics,
copies to a same-directory temporary file, fsyncs the file, validates the copy
through the supported check path, atomically renames it, and fsyncs the parent
where supported. Any failure removes only the uniquely owned temporary file;
the source and a pre-existing destination remain untouched.

Restore is a CLI workflow: validate the backup before mutation, reject an
existing destination, copy through a unique temporary artifact, fsync, rename,
reopen, and validate the destination. In-place overwrite and live-database
restore remain unsupported.

## 4. Supported check and rebuild path

`Database::check` validates format/catalog versions, physical tables and
columns, graph adjacency indexes, FTS provider metadata/state, vector encoding
and document agreement, and engine integrity. It recognizes only the pinned
Turso FTS directory-index diagnostic already characterized in Phase 8; any
other integrity row is corruption. A clean report states whether the pinned FTS
exception was observed rather than presenting it as generic engine success.

The CLI exposes `check`, `backup`, `restore`, and `rebuild-index`. Rebuild uses
the existing parser/AST maintenance statement and therefore resolves logical
names through catalogs. A whole-provider rebuild mode enumerates cataloged
indexes internally and never interpolates logical names into SQL.

## 5. CLI contract

Move the shell behind a `shell` subcommand while retaining the current
`fastdb PATH`, `--memory`, `-c`, `--param`, and `--output` invocation as a
compatibility alias. Operational commands are:

```text
fastdb check PATH [--output human|json]
fastdb backup PATH DESTINATION [--output human|json]
fastdb restore BACKUP DESTINATION [--output human|json]
fastdb rebuild-index PATH TABLE INDEX [--output human|json]
```

Every JSON success or failure is one deterministic envelope. Error output and
hooks never include source text or parameter values.

## 6. Verification

Independently authored API, CLI, integration, failure-injection, crash, and
randomized tests must prove:

- each limit rejects below/at/above boundaries safely and a connection remains
  reusable;
- timeout is request-local and does not leak to the next request;
- explicit-transaction limit failures poison and roll back;
- close rejects live workers, prevents later connects, and is idempotent at the
  underlying connection boundary;
- randomized graph/FTS/vector databases have matching logical hashes after
  backup and restore;
- interrupted backup/restore leaves no published partial destination;
- provider indexes rebuild from documents and plans select them afterward;
- check detects catalog, adjacency, hidden-column, FTS-state, vector-BLOB, and
  ordinary engine corruption while accepting only the exact pinned FTS
  diagnostic;
- clean and abrupt shutdown preserve every acknowledged mutation;
- hooks report metadata only and remain bounded if the consumer panics.

Run the complete Phase 9 matrix, unchanged Turso core/PostgreSQL/Whopper
regressions, release builds, fixture hashes, structured fuzz targets, and the
unchanged Phase 5 performance gate. Preserve commands and results in
`docs/phase10-report.md`.

## 7. Stop conditions

Stop rather than weaken the contract if a consistent single-file backup
requires an inherited Turso edit, if the maintenance latch cannot exclude main
file changes during copy, if check must silently accept an unknown integrity
failure, if limits require logging or reconstructing source values, or if any
Phase 5–9 compatibility, durability, provider, or performance gate regresses.

## 8. Definition of done

Phase 10 completes only when the bounded Rust API, deterministic lifecycle,
consistent backup, validated restore, provider-aware check, CLI maintenance
commands, observability hooks, operational documentation, and all verification
gates pass. The complete diff is then committed as the isolated Phase 10
rollback point. Phase 11 remains separately gated by the mandatory MVCC audit;
no production-ready or 1.0 claim follows from Phase 10 alone.
