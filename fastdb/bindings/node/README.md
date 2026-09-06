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

All methods are synchronous and block the calling JavaScript thread. Each instance owns one frontend connection and its database lifetime. Close is idempotent; calls after close fail. Closing an active transaction uses the engine's normal connection-drop behavior. Do not share native handles across workers; asynchronous execution, cancellation and worker lifecycle APIs have not been implemented.

Values use the transfer-v1 tagged encoding internally. Integer results are always bigint. Safe integer Number inputs become int64; unsafe integer Numbers are rejected, requiring bigint. Fractional Numbers and negative zero retain their binary64 bits. Booleans/nulls/strings, arrays and plain objects map explicitly. Buffer/Uint8Array map to binary; Record takes a table plus string/bigint key, and Vector takes validated native-format bytes. References are never inferred from strings. Unsupported JavaScript types, non-finite numbers, out-of-range int64 values and excessive nesting fail. The bridge uses own object entries and safe object construction, preserving prototype-looking string fields as data.

The current frontend query subset remains in force. Bare standalone composite parameters such as `SELECT $object` still follow the ordinary SQL path and can fail; typed helper expressions and DOCUMENT writes preserve those values. Full alias/type propagation, prepared statements, streaming, batch/migration/transfer client APIs, async execution, packaging and release qualification remain work for V1. This package is private and has not been published.
