# FastDB Core operations

Phase 10 supports one embedded Rust process and the local CLI. Multiprocess
access and online in-place restore remain unsupported.

## Consistent backup and validated restore

`Database::backup_to` takes the database maintenance write lease and refuses
to run while an explicit transaction is active. It checkpoints the stable WAL
in truncate mode, copies the main file to a uniquely owned temporary file in
the destination directory, fsyncs it, opens it through FastDB's supported
check path, atomically renames it, and fsyncs the parent directory where the
platform supports that operation.

The destination must not exist and must differ from the source. Memory
databases cannot be backed up. A failure before rename removes only FastDB's
unique temporary file. The source and any pre-existing destination are never
overwritten.

CLI restore follows the same publication rule: validate the backup, copy and
fsync a unique temporary artifact, validate it again, then rename. Restore into
a fresh path and switch applications only after the command succeeds. Keep the
pre-upgrade file as the rollback artifact; format migrations are forward-only
and there is no in-place downgrade.

## Integrity checks and provider rebuilds

The supported check path validates format and dialect versions, catalog rows,
opaque physical objects, graph adjacency indexes, hidden FTS/vector columns,
document-to-vector agreement, and `PRAGMA integrity_check`. The pinned Turso
FTS directory index can emit one characterized diagnostic; a clean report
exposes `pinned_fts_exception: true` when that exact catalog-derived diagnostic
is accepted. Every other diagnostic fails the check.

Rebuild indexes from cataloged documents with `Database::rebuild_index` or:

```text
fastdb rebuild-index app.fastdb article body_idx
```

Run `check` after restore, an interrupted host shutdown, or provider rebuild.

## Busy, shutdown, and recovery behavior

Ordinary requests share the maintenance read lease. Backup, check, and rebuild
take its write lease; they therefore wait for ordinary standalone work and
reject an already-active explicit transaction instead of deadlocking. Schema
work remains serialized by the schema mutex.

For deterministic shutdown, commit or roll back every transaction, close every
connection, then close the database. Abrupt process loss may leave Turso-owned
WAL/sidecar files; the next open performs normal pinned-engine recovery. Never
copy a live main file directly or delete its sidecars by hand.

## Resource limits and telemetry

Use `QueryOptions`/`ResourceLimits` for request-local timeout, returned rows and
bytes, graph hops, vector dimensions, and FTS query bytes. Invalid values and
syntax-derived breaches fail in preflight. A limit error inside an explicit
transaction rolls back that entire transaction. Standalone multi-statement
requests retain their documented commit boundary, so use `execute` with
`RETURN NONE` for mutations that do not need returned rows.

Event hooks receive metadata only. Hook panics are contained and do not stop
the connection worker. Export these events to the application's tracing or
metrics system; FastDB does not choose a telemetry backend or log query text.

## Upgrade and rollback drill

1. Stop writers and close FastDB cleanly.
2. Run `fastdb check` on the current file.
3. Create and retain `fastdb backup` output.
4. Open the working copy with the new FastDB build, then run `check` again.
5. Exercise representative indexed graph, FTS, and vector queries.
6. If validation fails, stop the new build and restore the retained pre-upgrade
   backup to a fresh path with the prior compatible build.

Do not replace an upgraded file with an older binary in place. Unknown future
formats are refused before mutation.
