# FastDB

FastDB is a clean-room, SurrealQL-compatible document database frontend built
on a pinned Turso engine. The current MVP surface is a runtime-neutral
asynchronous Rust package and a local CLI. Compatibility is a documented
subset, not sponsorship, certification, or complete SurrealDB compatibility.

```rust
let database = fastdb::Builder::new_local("app.fastdb").build().await?;
let connection = database.connect()?;
let response = connection
    .query(
        "SELECT * FROM person WHERE age >= $minimum",
        fastdb::params! { "minimum" => 18 },
    )
    .await?;
connection.close().await?;
```

```sh
fastdb app.fastdb -c "CREATE person:one SET name='One'"
fastdb --memory --output json -c "SELECT * FROM person"
```

See [the embedded API](docs/api.md), [CLI contract](docs/cli.md),
[compatibility matrix](COMPAT.md), [format policy](docs/format-v1.md), and
[clean-room policy](CLEAN_ROOM.md).

FastDB-authored crates remain version `0.0.0`, `publish = false`, and
source-available pending counsel-approved license, CLA, entity, and trademark
materials. No release or production-readiness claim is made.
