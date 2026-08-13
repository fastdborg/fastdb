# Embedded Rust API

FastDB's public package is `fastdb` at version `0.0.0` with
`publish = false`. Applications do not import the parser, synchronous
frontend, Turso bindings, or Turso core.

```rust
use fastdb::{params, Builder};

# async fn example() -> Result<(), fastdb::Error> {
let database = Builder::new_local("app.fastdb").build().await?;
let mut connection = database.connect()?;

let created = connection
    .execute(
        "CREATE person:one SET age=$age RETURN NONE",
        params! { "age" => 42 },
    )
    .await?;
assert_eq!(created.statement_count, 1);
assert_eq!(created.mutation_count, 1);

let mut transaction = connection.transaction().await?;
transaction
    .execute("UPDATE person:one SET age=43", params! {})
    .await?;
transaction.commit().await?;

let result = connection
    .query("SELECT * FROM person:one", params! {})
    .await?;
assert_eq!(result.statements.len(), 1);
connection.close().await?;
database.close().await?;
# Ok(())
# }
```

## Request limits and observability

Every request has bounded defaults. `QueryOptions` can select limits only
within the hard ceilings: 300 seconds, 100,000 returned rows, 64 MiB of
returned values, 16 graph hops, 65,536 vector dimensions, and 64 KiB of FTS
query text.

```rust
# use fastdb::{params, Connection, QueryOptions, ResourceLimits};
# use std::time::Duration;
# async fn bounded(connection: &Connection) -> Result<(), fastdb::Error> {
let limits = ResourceLimits::default()
    .with_timeout(Duration::from_secs(2))
    .with_output_rows(100)
    .with_output_bytes(256 * 1024)
    .with_graph_hops(4)
    .with_vector_dimensions(1_536)
    .with_fts_query_bytes(1_024);
let options = QueryOptions::default().with_resource_limits(limits);
let result = connection
    .query_with_options("SELECT * FROM article", params! {}, options)
    .await?;
# let _ = result;
# Ok(())
# }
```

Preflight limits fail before execution. Returned row/byte limits are checked
after the engine materializes a successful response. A guarded transaction is
rolled back if that check fails. Outside a transaction, the normal
multi-statement rule still applies: an earlier successful mutation can already
be committed. Use `execute` plus `RETURN NONE` for mutation requests that do
not need returned values.

`Builder::event_hook` accepts a panic-contained callback. Events contain only
operation name, elapsed time, mutation count, returned row/byte counts, and an
optional stable error category. FastDB never puts source or parameter values
in the event.

## Check, backup, rebuild, and close

`Database::check`, `backup_to`, and `rebuild_index` run on a dedicated
maintenance worker. Check validates format/catalog ownership, hidden columns,
graph adjacency, FTS state, vector encodings, and engine integrity. Backup
checkpoints, copies, fsyncs, validates, and atomically publishes a new file; it
never overwrites an existing destination. Maintenance returns `Transaction`
while an explicit transaction is active.

`Database::close` refuses to close while connections are alive, prevents later
connections after success, and is idempotent. Close every connection first,
then close the database handle when deterministic shutdown matters.

Phase 8 full-text search is available through the same query surface:

```rust
# use fastdb::{params, Connection};
# async fn search(connection: &Connection) -> Result<(), fastdb::Error> {
connection.execute(
    "DEFINE ANALYZER blankish TOKENIZERS blank; \
     DEFINE INDEX body_idx ON article FIELDS body \
       FULLTEXT ANALYZER blankish HIGHLIGHTS",
    params! {},
).await?;
let rows = connection.query(
    "SELECT id, search::score(1) AS score FROM article \
     WHERE body @1@ $query ORDER BY score DESC",
    params! { "query" => "Rust database" },
).await?;
# let _ = rows;
# Ok(())
# }
```

Only the documented case-sensitive `blank` analyzer subset counts as
SurrealQL compatibility. `CREATE INDEX ... USING fts` and the `fts_*`
functions are labeled FastDB/Turso extensions. A query touching an FTS index
after an indexed write in the same explicit transaction returns a Transaction
error and rolls back that transaction, preventing the pinned provider's stale
pre-commit view from escaping.

Each connection owns a dedicated worker thread. Requests from concurrent
callers are serialized as complete units. If a queued request future is
dropped, that request is skipped. Dropping a future after its request starts
does not cancel or roll back it. Use the cloneable interrupt handle for
cooperative cancellation of the currently active engine statement:

```rust
# fn example(connection: &fastdb::Connection) {
let interrupt = connection.interrupt_handle();
interrupt.interrupt();
# }
```

An interrupted operation returns category `Engine`; preceding standalone
statements in the same request may already be committed. The connection can be
reused when cleanup succeeds.

## Values, parameters, and results

`Value` preserves null, bool, `i64`, finite `f64`, UTF-8 string, array,
deterministically ordered object, and typed record ID values. `RecordIdValue`
distinguishes UTF-8 string, `i64`, and UUIDv4/v7 components. Safe `From`
conversions exist for bool, signed integers through `i64`, unsigned integers
through `u32`, floats, strings, arrays, objects, record IDs, and options. The
fallible `TryFrom<u64>` rejects values above `i64::MAX`. Request validation
rejects non-finite floats and recursive values above public limits.

`query` returns one ordered `StatementResult` per source statement. `execute`
intentionally discards rows and returns an `ExecutionSummary`. Its mutation
count includes CREATE, UPDATE, and DELETE records even for `RETURN NONE`;
SELECT and schema/transaction statements contribute zero.

## Transactions and errors

The transaction guard mutably borrows its connection. Commit and rollback
consume it. Dropping it queues rollback. Transaction-control source inside the
guard is rejected. Any guarded operation error rolls back the entire guard and
clears the synchronous frontend's poisoned state; a cleanup failure produces a
combined `Transaction` error.

The stable categories are `Parse`, `UnsupportedSyntax`, `Schema`,
`Constraint`, `Transaction`, `Engine`, and `Io`. Parse and unsupported errors
include a half-open UTF-8 byte span. Internal format/corruption diagnostics map
to `Engine`; rendered detail strings are not stable matching APIs.

Always call consuming `Connection::close().await`, followed by
`Database::close().await`, when clean checkpoint/shutdown evidence matters.
The pinned engine can retain an empty `-wal` filesystem entry after a clean
checkpoint; it contains no durable frames. Nonempty WAL or other Turso-owned
sidecars may remain after abnormal exit and are recovered on open.
