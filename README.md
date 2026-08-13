# FastDB

FastDB is a clean-room, SurrealQL-compatible document database frontend built
on a pinned Turso engine. The release-candidate MVP surface is a
runtime-neutral asynchronous Rust package and a local CLI. Compatibility is a documented
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

FastDB Core is open-source software under the [MIT License](LICENSE.md).
FastDB-authored crates remain version `0.0.0` and `publish = false` until the
first public alpha is deliberately packaged. Local hardening evidence is not
a production-readiness claim; see the [licensing decision](docs/licensing.md)
and [release-readiness checklist](docs/release-readiness.md).
