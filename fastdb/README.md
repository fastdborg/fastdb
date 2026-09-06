# FastDB embedded prototype

An initial FastQL frontend over pinned Rust Turso. **V1 is still in development.** See [status](docs/status.md), [contracts](docs/contracts.md), and [engine provenance](UPSTREAM.md).

From the repository root:

```sh
fastdb/scripts/check.sh
cargo run --locked -p fastdb-cli -- /tmp/example-fastdb.db < fastdb/examples/persistent.fastql
```

Use a fresh database for the example. `fastdb/examples/sql-writes.fastql` additionally exercises column-list inserts, nested SET, and transaction rollback. The prototype CLI opens an interactive prompt for terminal input and reads semicolon-delimited scripts from piped stdin. It supports multiline statements and emits tagged JSON results with statement byte offsets. Use `--interactive` to force prompts or `--script` to force EOF-delimited script input. SQL input buffers default to 16 MiB; `--max-input-bytes N` changes the byte limit. Each report is written and flushed before the next statement executes. It stops at the first execution or output error and exits nonzero; an output failure does not undo prior committed work. Use `--line` for legacy one-statement-per-line processing that continues after errors; this mode also exits nonzero if any statement fails. It is not the final import/export or client wire format.

On Unix terminals, the CLI supports cursor editing and up/down history recall, including multiline statements. Ctrl-C at the prompt clears pending input and leaves any transaction active; `.quit` exits and connection close rolls back uncommitted work. History stays in memory by default. Use `--history /path/to/fastdb-history` to load and save it across sessions; start a statement with a space to omit it. History retains up to 100 entries and skips new entries over 64 KiB. The submission byte limit is checked after terminal line editing and does not bound the editor's live buffer. Running-query interruption and broader platform qualification remain unfinished.

The Rust client entry point is `fastdb::Database::open(path)?.connect()?`. `execute(sql, &Parameters)` handles the implemented FastQL subset and ordinary SQL. Typed `insert`, `get`, `patch`, `delete`, `define_field`, `create_index`, and `lookup_index` APIs exercise document storage without exposing raw engine access. Collection SELECT supports the initial AST-lowered subset in status.md. SQL-shaped VALUES inserts, SET/UNSET/DELETE, and ID-based UPSERT use the document validation path. Collection/index drops and field removal are transactional; INFO reports logical schema metadata. Field CHECK expressions validate final candidates and existing data when definitions change. Catalog writes use version 2; see contracts.md for prototype compatibility. SQL SELECT/SET/VALUES now support typed document/array/record helpers and lazy null helpers; see status.md for the supported subset. Remaining write/read forms, FastQL features, Node bindings, tools, and release validation are still pending.

Use `connection.execute_report(sql, &params)` when error handling needs transaction observations. It returns the original result/error and before/after `TransactionState` values. `transaction_state()` also reads the current state directly. CLI JSON lines include `transaction.before` and `transaction.after`; runtime engine errors may end an outer transaction. See contracts.md for observation limits.

`connection.execute_batch(script)` returns per-statement execution reports with UTF-8 byte offsets and stops after the first failure. It does not wrap the script in a transaction: include BEGIN/COMMIT when needed and inspect transaction state after errors. Lexical splitting errors are detected before any statement runs. The final statement may omit its semicolon. Batch scripts currently use literal values; use `execute`/`execute_report` for bound parameters.

A tested [offline backup and restore procedure](docs/backup-restore.md) preserves relational data, collection schema/indexes and migration history using an exclusive maintenance window and successful checkpoint. Online backup and interrupted-copy/recovery qualification remain pending.

`INFO FOR DB` lists tables and views. `INFO FOR TABLE name` reports logical collection validation/indexes or native relational column and index metadata. `INFO FOR INDEX name` includes explicit relational index SQL and key details such as expressions, direction and collation. See [inspection contracts](docs/contracts.md#relational-inspection-details) for the prototype result shape.

The [native Node client](bindings/node/README.md#local-package-smoke) includes an offline tarball-install smoke for local packaging qualification. Platform-specific release binaries remain pending.

The [Rust client guide](docs/rust-client.md) describes local path dependencies and the standalone offline consumer smoke. Registry distribution remains pending.
