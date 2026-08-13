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
# Ok(())
# }
```

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

Always call consuming `close().await` when clean checkpoint/shutdown evidence
matters. The pinned engine can retain an empty `-wal` filesystem entry after
a clean checkpoint; it contains no durable frames. Nonempty WAL or other
Turso-owned sidecars may remain after abnormal exit and are recovered on open.
