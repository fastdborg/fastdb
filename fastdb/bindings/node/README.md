# FastDB Node client prototype

This private package builds a native N-API addon backed by the checked FastDB Rust frontend. It does not expose the raw Turso connection. Local evidence covers Linux with Rust 1.88 and Node 24.19.0; release prebuilds, Windows/macOS qualification and additional Node versions remain pending.

From the repository root, install the declaration checker and run the scoped checks:

```sh
npm ci --prefix fastdb/bindings/node --ignore-scripts
fastdb/scripts/check.sh
```

Use the repository's documented Rust environment when its temporary toolchain is needed. `check-node.sh` builds the addon, copies it to the ignored `fastdb.node` path, runs Node tests, and type-checks declarations. There are no npm runtime dependencies. TypeScript 5.8.3 is pinned only for declaration checks.

```js
const { Database, Record } = require('./fastdb/bindings/node/index.cjs');
const db = new Database('application.db');
try {
  db.execute('CREATE TABLE IF NOT EXISTS posts');
  db.execute('INSERT INTO posts DOCUMENT $doc', {
    $doc: { id: new Record('posts', 'p1'), title: 'Hello', visits: 1n }
  });
  const row = db.exactlyOne('SELECT * FROM posts WHERE id = $id', {
    $id: new Record('posts', 'p1')
  });
  console.log(row[0]);
} finally {
  db.close();
}
```

`execute` returns columns, positional rows, bigint affected count, and transaction before/after observations. `all`, `first` and `exactlyOne` provide row-cardinality helpers; first returns undefined for no rows and exactlyOne throws unless there is one row. Engine/frontend execution failures throw Error with `code` and `transaction` fields. Constructor, closed-handle and JavaScript argument errors currently use ordinary errors without that envelope; the final cross-client error contract is unfinished.

`Database` methods are synchronous and block the calling JavaScript thread. Each instance owns one frontend connection and its database lifetime. Close is idempotent; calls after close fail. Closing an active transaction uses the engine's normal connection-drop behavior. Do not share native handles across workers; use AsyncDatabase to own a separate native connection on a dedicated worker. Cancellation is not yet implemented.

Values use the transfer-v1 tagged encoding internally. Integer results are always bigint. Safe integer Number inputs become int64; unsafe integer Numbers are rejected, requiring bigint. Fractional Numbers and negative zero retain their binary64 bits. Booleans/nulls/strings, arrays and plain objects map explicitly. Buffer/Uint8Array map to binary; Record takes a table plus string/bigint key, and Vector takes validated native-format bytes. References are never inferred from strings. Unsupported JavaScript types, non-finite numbers, out-of-range int64 values and excessive nesting fail. The bridge uses own object entries and safe object construction, preserving prototype-looking string fields as data.

The current frontend query subset remains in force. Bare standalone composite parameters such as `SELECT $object` still follow the ordinary SQL path and can fail; typed helper expressions and DOCUMENT writes preserve those values. Full alias/type propagation, prepared statements, streaming, batch client APIs, async cancellation/lifecycle qualification, packaging and release qualification remain work for V1. This package is private and has not been published.


`migrate(migrations)` accepts the complete ordered history with `{ version: bigint, name: string, sql: string }` entries. It returns `{ alreadyApplied: number, applied: bigint[], transaction }`. The Rust runner's autocommit requirement, exact-source checks, limits and atomic pending-run behavior apply unchanged. The Node client does not read migration directories; application code supplies the scripts.

`exportDocuments(table, format = 'json')` returns the versioned typed transfer string. `importDocuments(table, input, format = 'json')` returns `{ imported: number, transaction }`. Format is explicitly `json` or `ndjson`; imports target an existing collection and preserve validation, indexes and rollback behavior. These methods materialize strings and use the frontend transfer limits. They do not provide schema backups, streaming or implicit file I/O. Migration/import/export failures use the same code and transaction envelope as execute; parameter validation and closed-handle errors retain the limitations above.


## Asynchronous database

```js
const { AsyncDatabase } = require('./fastdb/bindings/node/index.cjs');
const db = await AsyncDatabase.open('application.db');
try {
  const rows = await db.all('SELECT name FROM accounts');
  console.log(rows);
} finally {
  await db.close();
}
```

AsyncDatabase provides Promise-returning execute/all/first/exactlyOne, migrate, importDocuments and exportDocuments methods with the same value and result shapes. Opening waits for native initialization and rejects on failure. One dedicated [Node worker](https://nodejs.org/api/worker_threads.html) owns the native connection; submissions execute sequentially in send order. Separate AsyncDatabase instances have separate workers and connections. Native database execution occurs off the calling event loop; argument encoding, message copying and result decoding still consume time and memory on the caller.

The initial queue permits 256 outstanding operations and 128 MiB of encoded argument strings, including the active operation. Excess requests reject with FDB_LIMIT before submission. These are queue limits, not total-memory or query-time guarantees. Close stops new submissions, drains previously accepted operations, closes the native connection and waits for worker exit. Repeated close calls share the same Promise. Closing an active transaction uses native rollback-on-drop behavior, covered by reopen tests. Applications must await close to release the worker; no automatic idle shutdown or force-termination/cancellation API is provided.

Query errors reject their own promises without stopping later queued requests. An explicit transaction belongs to the whole connection, not to an individual caller or Promise chain. Applications must coordinate transaction ownership and await/check steps before submitting dependent work. The current API does not provide transaction callback isolation. Unexpected worker errors/exits reject outstanding requests; deterministic worker-crash recovery, native-memory accounting, cancellation and long-running lifecycle stress remain release gates.
