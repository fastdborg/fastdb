# FastDB embedded prototype

An initial FastQL frontend over pinned Rust Turso. **V1 is still in development.** See [status](docs/status.md), [contracts](docs/contracts.md), and [engine provenance](UPSTREAM.md).

From the repository root:

```sh
fastdb/scripts/check.sh
cargo run --locked -p fastdb-cli -- /tmp/example-fastdb.db < fastdb/examples/persistent.fastql
```

Use a fresh database for the example. The prototype CLI accepts one statement per input line and emits tagged JSON results. It is not the final import/export or client wire format.

The Rust client entry point is `fastdb::Database::open(path)?.connect()?`. `execute(sql, &Parameters)` handles the implemented FastQL subset and ordinary SQL. Typed `insert`, `get`, `patch`, `delete`, `define_field`, `create_index`, and `lookup_index` APIs exercise document storage without exposing raw engine access. Collection SELECT supports the initial AST-lowered subset in status.md. SQL-shaped collection writes, remaining FastQL features, Node bindings, tools, and release validation are still pending.
