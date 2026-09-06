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

## Standalone consumer smoke

From the repository root, with the baseline toolchain and dependency cache available:

```sh
python3 fastdb/scripts/check-rust-client.py
```

The script requires Python 3.11+ and creates an application in a temporary directory outside the repository workspace. It uses only the FastDB path dependency, seeds the consumer lockfile from the pinned workspace lockfile, then lets Cargo add the consumer and prune unused packages. Registry/git package identities and checksums must remain a subset of the baseline before building. Host-filtered metadata resolution and the build run offline; uncached dependencies fail rather than being fetched implicitly.

The consumer runs outside the workspace, so it does not load this checkout's `.cargo/config.toml`. The script removes the general RUSTFLAGS environment overrides and uses a separate reusable build directory at `target/fastdb-rust-consumer`. It exercises typed record/int64 parameters, field CHECK validation, a unique index, transaction observations, rollback, bundled QuickJS, vector values and close/reopen persistence. Temporary source, lockfile and database files are removed when it finishes. Build outputs remain cached.

This is a local path-consumer check, not cargo package/publish qualification, a guarantee for arbitrary dependency unification, or a cross-platform release claim. Registry distribution of FastDB and its engine/frontend dependency graph, platform/toolchain qualification, public API stabilization and complete distribution notices remain release work.
