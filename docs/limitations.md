# FastDB MVP limitations

Status: Phase 5 release-candidate contract

FastDB implements only the Supported and Partial rows in `COMPAT.md`.
Functions, casts, joins/subqueries, graph traversal, relations, ranges, live
queries, permissions/users/namespaces, network service, sync, and specialized
indexes are not part of this MVP. Unsupported syntax fails explicitly; parser
acceptance alone never expands the compatibility claim.

The embedded API and CLI are local-only. Each API connection owns one worker
thread and serializes complete requests. Stable WAL supports FastDB's tested
process-local connections; experimental MVCC and experimental multiprocess
WAL are disabled. Multiple independent processes must not concurrently mutate
one database.

Interruption is cooperative and targets only the active engine statement.
Earlier standalone statements in the same request may already be committed.
Dropping an in-flight future does not cancel it. A transaction-guard error
rolls back the complete guard, while standalone scripts retain successful
earlier statements.

Database paths must be UTF-8; non-UTF-8 paths return `Io`. A clean close
checkpoints durable frames, although the pinned engine may retain an empty
`-wal` filesystem entry. Nonempty WAL files after abrupt process exit are
recovery state and must not be manually removed. Direct modification of
FastDB catalogs, opaque tables/indexes, or their sidecars is unsupported.

Format/dialect version 1 is stable, but only the explicit migration level
0-to-1 path exists. Format 0 is disposable. There is no downgrade support,
online backup API, replication, encryption contract, or cross-version release
promise beyond `docs/format-v1.md`.

FastDB Core is MIT licensed. The crates remain version `0.0.0` and
`publish = false` while compatibility work continues toward the first public
alpha. Local tests and benchmarks are not a production-readiness or ACID
certification claim.
