# Rust embedded client

The prototype Rust client is the `fastdb` crate under `fastdb/frontend`. It remains private and is not available as a published crates.io release. A local application can use a path dependency on an exact FastDB checkout:

```toml
[dependencies]
fastdb = { path = "../turso/fastdb/frontend" }
```

Adjust the path for the application. Keep that checkout at the intended commit, retain the application's Cargo.lock, and qualify dependency changes. Cargo does not automatically use a dependency repository's lockfile as the application's lockfile. The tested baseline uses Rust 1.88.0; the pinned engine and dependencies are documented in [UPSTREAM.md](../UPSTREAM.md).

```rust
use fastdb::{Database, Parameters, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let database = Database::open("application.db")?;
    let connection = database.connect()?;
    connection.execute("CREATE TABLE IF NOT EXISTS docs", &Parameters::new())?;
    connection.execute(
        "UPSERT docs:saved {value: $value}",
        &Parameters::from([("$value".into(), Value::Integer(7))]),
    )?;
    let row = connection.execute("SELECT docs:saved", &Parameters::new())?
        .exactly_one()?;
    println!("{row:?}");
    Ok(())
}
```

Keep the Database alive while using its connections and serialize operations on each connection. Public APIs do not expose the raw engine connection. Use execute_report and transaction_state when recovery needs the observed transaction state; an error does not universally imply either statement-only or full transaction rollback. See [contracts](contracts.md).

## Typed vector construction

`Value::vector32(&[f32])` and `Value::vector64(&[f64])` construct dense vectors. The Rust client also provides `Value::vector32_sparse(&[f32])`, `Value::vector8(&[f32])`, and `Value::vector1bit(&[f32])`. These accept dense float32 components and use the pinned engine's sparse, quantized and bit conversions:

```rust
let components = [1.0_f32, 0.0, -1.0];
let sparse = Value::vector32_sparse(&components)?;
let sparse_entries = Value::vector32_sparse_entries(3, &[(0, 1.0), (2, -1.0)])?;
let quantized = Value::vector8(&components)?;
let bits = Value::vector1bit(&components)?;
```

Pass these values through Parameters like any other typed value. Inputs require 1–65,536 finite components; conversion output is validated too. Quantized and bit conversions are lossy. For sparse input, `Value::vector32_sparse_entries(dimensions, &[(index, value)])` avoids allocating a dense array. Indices must be strictly increasing, unique and below the declared dimension, even for zero-valued entries. Values must be finite; positive and negative zeros are omitted. Empty entries represent an all-zero vector with the declared positive dimension. Dimension bounds are checked before constructor output allocation. Broader numerical and platform qualification remains open.

## Standalone consumer smoke

From the repository root, with the baseline toolchain and dependency cache available:

```sh
python3 fastdb/scripts/check-rust-client.py
```

The script requires Python 3.11+ and creates an application in a temporary directory outside the repository workspace. It uses only the FastDB path dependency, seeds the consumer lockfile from the pinned workspace lockfile, then lets Cargo add the consumer and prune unused packages. Registry/git package identities and checksums must remain a subset of the baseline before building. Host-filtered metadata resolution and the build run offline; uncached dependencies fail rather than being fetched implicitly.

The consumer runs outside the workspace, so it does not load this checkout's `.cargo/config.toml`. The script removes the general RUSTFLAGS environment overrides and uses a separate reusable build directory at `target/fastdb-rust-consumer`. It exercises typed record/int64 parameters, field CHECK validation, a unique index, transaction observations, rollback, bundled QuickJS, all five vector constructors and close/reopen persistence. It also imports the public profiling/audit result types, checks indexed profiling counters and repeat-call reset, rejects profiling writes, audits reopened data with exact byte limits, and verifies audit-limit failure preserves an active transaction. It also checks exact and rejected SELECT/FETCH result budgets, atomic write-result rejection with prior-work preservation, retry, and all three cancellable result-limit entry points. Temporary source, lockfile and database files are removed when it finishes. Build outputs remain cached.

This is a local path-consumer check, not cargo package/publish qualification, a guarantee for arbitrary dependency unification, or a cross-platform release claim. Registry distribution of FastDB and its engine/frontend dependency graph, platform/toolchain qualification, public API stabilization and complete distribution notices remain release work.

## Collection content audit

```rust
let audit = connection.check_collection_integrity(
    "posts",
    fastdb::IntegrityLimits::default(),
)?;
println!("{} documents, {} index entries", audit.documents, audit.index_entries);
```

This explicit snapshot audit checks typed IDs, field/CHECK validity and index entry consistency without repairing data. Defaults permit 100,000 documents and 64 MiB of processed encoded ID/document bytes; override the public limit fields for larger audits. FDB_LIMIT returns no partial report. These limits do not bound engine memory or elapsed time. See contracts.md for scope and error behavior; native page/B-tree checking remains separate.

## Cooperative execution cancellation

`CancellationToken::new()` creates a token that can be cloned and passed to another thread. `cancel()` is idempotent and sticky; `is_cancelled()` observes the request. Pass it to `Connection::execute_cancellable(sql, &parameters, &token)` or `execute_report_cancellable` to apply it to that execution. `profile_select_cancellable` uses the same token contract and returns the existing result/metrics shape. `check_collection_integrity_cancellable(table, limits, &token)` applies it to collection audits. The report includes transaction observations on success or failure. Use a fresh token for a retry. A token retains no database connection.

A pre-cancelled token rejects before parsing or writes with `FDB_CANCELLED`. During execution the frontend polls the token at engine progress boundaries and delivers one interruption, allowing statement/savepoint cleanup to proceed. Its handler is removed when execution returns or unwinds. Cancellation after an execution finishes cannot affect later executions that do not use that token. Calls on a connection must remain serialized.

Cancellation is cooperative and completion can win the race. Compilation, bundled function work and other non-engine work have no fixed cancellation latency; performance and platform qualification remain open. Inspect transaction state after an interrupted execution. The documented pinned trigger-interruption defect remains an unresolved release gate. Node AsyncDatabase query methods now have initial AbortSignal integration; other operation types remain open.


`execute_batch_cancellable(script, &token)` and `visit_batch_cancellable(script, &token, visitor)` apply cancellation to a whole script. Pre-cancellation returns FDB_CANCELLED before splitting. Once split, each statement gets a cancellable execution report; cancellation during or between statements becomes the final error entry, retaining completed reports and their effects. Visitor callbacks are not interrupted, and cancellation requested by a visitor stops the next statement. A visitor returning false still stops normally. No batch transaction is implied; inspect transaction observations and explicitly roll back if needed.


`import_documents_cancellable(table, input, format, &token)` and `export_documents_cancellable(table, format, &token)` use the same cooperative token contract. Pre-cancellation precedes parsing and catalog access. An interrupted import follows its existing atomic rollback path, preserving prior outer work; an export returns a complete payload or an error. Parsing, document conversion and serialization have no fixed cancellation latency. Completion can win a race, so use the returned outcome and transaction state rather than assuming an abort rolled back.


`migrate_cancellable(&plan, &token)` applies the token to the existing all-pending-migrations transaction. A pre-cancelled token rejects before validation; active cancellation follows migration rollback and retains prior applied history. A migration-wrapped interruption exposes FDB_CANCELLED while retaining version/offset/source details. Other migration execution errors retain FDB_MIGRATION. Parsing and compilation have no fixed cancellation latency; completion can win a race.

## Result limits and atomic write rejection

`select_with_limits(sql, &parameters, limits)` and
`profile_select_with_limits` accept `ResultLimits { max_rows,
max_payload_bytes }`. Both limits are explicit `usize` values; zero is valid.
Column names count even for empty results. Strings and object keys use UTF-8 byte
lengths, integers/numbers use eight bytes, null/booleans one byte, and binary/vector
values their byte length. Arrays, objects and records count their contained payload.
FETCH charges expanded output, including duplicate occurrences and missing nulls.

`write_with_result_limits` accepts one data write and rejects oversized RETURNING
output with `FDB_LIMIT`. Its operation savepoint restores rejected changes while
preserving prior pending work when recovery succeeds. It rejects transaction control,
DDL and reads. Writes without RETURNING can succeed with zero result budgets.
Inspect `transaction_state()` after an error; engine and rollback errors retain
their existing transaction semantics.

The matching `select_with_limits_cancellable`,
`profile_select_with_limits_cancellable` and
`write_with_result_limits_cancellable` methods take a final `&CancellationToken`.
Use a fresh token for retries. These APIs bound logical returned payload, **not**
engine buffers, candidate/snapshot memory, allocator overhead or execution time.
See [result-budgets.md](result-budgets.md) for the full policy.

Run the [result-limit example](../frontend/examples/result_limits.rs) from the
checkout root:

```sh
cargo run --locked -p fastdb --example result_limits
```

It verifies an exact read budget, rejected UPDATE output, preservation of a prior
pending insert, a successful retry, rollback and pre-cancelled write rejection.

`connection.with_write_buffer_limits(ResultLimits { max_rows, max_payload_bytes })`
opts into separate per-buffer limits for frontend collection-write candidates and
document snapshots. It consumes and returns the connection, preserving its
transaction. These limits are distinct from returned-result limits; ordinary reads,
native engine buffers and direct single-document Rust methods are outside this
setting. See [result-budgets.md](result-budgets.md) for exact buffer accounting and
initial coverage.

`Value::into_portable_value(self)` consumes a value to produce the same transfer-v1
JSON representation as `to_portable_value(&self)`, including decimal int64 and
binary64 bit strings. Use it when the value is no longer needed to avoid cloning
its tree for conversion. Both methods validate values before encoding.

`CancellationToken::with_deadline(std::time::Instant)` adds a monotonic deadline
to any existing cancellable Rust operation. Clones share manual cancellation and
the same deadline. An already-expired token rejects before parsing; expiry during
engine work uses the existing one-shot progress interruption and rollback path,
returning `FDB_CANCELLED`. No background timer is started. Parsing, non-engine
work, cleanup and completion races retain the existing cooperative limitations;
this is not a hard wall-clock execution bound. Use a fresh token for retry.
