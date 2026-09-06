# FastDB embedded prototype

An initial FastQL frontend over pinned Rust Turso. **V1 is still in development.** See [status](docs/status.md), [contracts](docs/contracts.md), and [engine provenance](UPSTREAM.md).

From the repository root:

```sh
fastdb/scripts/check.sh
cargo run --locked -p fastdb-cli -- /tmp/example-fastdb.db < fastdb/examples/persistent.fastql
```

Use a fresh database for the example. `fastdb/examples/sql-writes.fastql` additionally exercises column-list inserts, nested SET, and transaction rollback. The prototype CLI accepts one statement per input line and emits tagged JSON results. It is not the final import/export or client wire format.

The Rust client entry point is `fastdb::Database::open(path)?.connect()?`. `execute(sql, &Parameters)` handles the implemented FastQL subset and ordinary SQL. Typed `insert`, `get`, `patch`, `delete`, `define_field`, `create_index`, and `lookup_index` APIs exercise document storage without exposing raw engine access. Collection SELECT supports the initial AST-lowered subset in status.md. SQL-shaped VALUES inserts, SET/UNSET/DELETE, and ID-based UPSERT use the document validation path. Collection/index drops and field removal are transactional; INFO reports logical schema metadata. Field CHECK expressions validate final candidates and existing data when definitions change. Catalog writes use version 2; see contracts.md for prototype compatibility. SQL SELECT/SET/VALUES now support typed document/array/record helpers and lazy null helpers; see status.md for the supported subset. Remaining write/read forms, FastQL features, Node bindings, tools, and release validation are still pending.

Use `connection.execute_report(sql, &params)` when error handling needs transaction observations. It returns the original result/error and before/after `TransactionState` values. `transaction_state()` also reads the current state directly. CLI JSON lines include `transaction.before` and `transaction.after`; runtime engine errors may end an outer transaction. See contracts.md for observation limits.
