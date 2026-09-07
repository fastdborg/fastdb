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

`execute` returns columns, positional rows, bigint affected count, and transaction before/after observations. `all`, `first` and `exactlyOne` provide row-cardinality helpers; first returns undefined for no rows and exactlyOne throws unless there is one row. A cardinality mismatch is a `RangeError` with code `FDB_CARDINALITY` and the completed statement's `transaction` observations. The statement has already executed: the helper does not undo writes or roll back an outer transaction. Engine/frontend execution failures throw Error with `code` and `transaction` fields. Public Database/AsyncDatabase operations rejected after close (or while the worker is closing) use `FDB_CLOSED` without a transaction field because no statement was submitted. Constructor and JavaScript argument errors currently use ordinary errors without that envelope; the final cross-client error contract is unfinished.

`isFastDBError(error)` narrows a caught `unknown` value to the exported `FastDBError` interface. It recognizes Error instances with an `FDB_*` code and validates transaction observations when present. `transaction` is optional: closed-handle, worker and queue errors can occur before execution. Constructor and argument errors can fail this guard.


`Database` methods are synchronous and block the calling JavaScript thread. Each instance owns one frontend connection and its database lifetime. Close is idempotent; calls after close fail. Closing an active transaction uses the engine's normal connection-drop behavior. Do not share native handles across workers; use AsyncDatabase to own a separate native connection on a dedicated worker. AsyncDatabase exposes cooperative connection interruption and AbortSignal cancellation for queries, profiling, integrity audits, batches, document transfers and migrations. See [operation cancellation](#operation-abortsignal-cancellation) for signatures and outcome handling.

Values use the transfer-v1 tagged encoding internally. Integer results are always bigint. Safe integer Number inputs become int64; unsafe integer Numbers are rejected, requiring bigint. Fractional Numbers and negative zero retain their binary64 bits. Booleans/nulls/strings, arrays and plain objects map explicitly. Buffer/Uint8Array map to binary; Record takes a table plus string/bigint key, and Vector takes validated native-format bytes. References are never inferred from strings. Unsupported JavaScript types, non-finite numbers, out-of-range int64 values and excessive nesting fail. The bridge uses own object entries and safe object construction, preserving prototype-looking string fields as data.

The current frontend query subset remains in force. Standalone SELECT projections such as `SELECT $object AS value` now preserve composite/record/vector/boolean parameters, as do typed helper expressions and DOCUMENT writes. Broader source-query type propagation remains unfinished. Full alias/type propagation, prepared statements, streaming, async cancellation/lifecycle qualification, packaging and release qualification remain work for V1. This package is private and has not been published.


`migrate(migrations)` accepts the complete ordered history with `{ version: bigint, name: string, sql: string }` entries. It returns `{ alreadyApplied: number, applied: bigint[], transaction }`. The Rust runner's autocommit requirement, exact-source checks, limits and atomic pending-run behavior apply unchanged. The Node client does not read migration directories; application code supplies the scripts.

Migration errors retain `code`, `message` and the runner's transaction observations in both clients. Version and UTF-8 byte-offset context appears in the message; there are currently no separate JavaScript `version` or `offset` error properties.

| Migration failure | Error and interpretation |
|---|---|
| Script splitting/tokenization | `FDB_SYNTAX`, with migration version and byte offset within that script; the plan has not begun executing. |
| Edited or incomplete applied history | `FDB_VALIDATION`; a mismatch message distinguishes the version sequence, name or exact SQL source. Restore the original applied prefix before retrying. |
| Input or stored-history size limit | `FDB_LIMIT`; inspect the message to identify the rejected limit. |
| Incompatible ledger schema or invalid stored types | `FDB_STORAGE`; the runner does not repair the ledger. |
| Pending statement failure | Usually `FDB_MIGRATION`, with version, statement byte offset and underlying error text. The runner attempts to roll back all pending scripts and history together. |
| Cooperative cancellation | `FDB_CANCELLED`; a pending statement's version and offset remain in its message. Completion can win the abort race. |
| Rollback failure | `FDB_ROLLBACK`; cleanup did not establish the normal rollback outcome. |

Inspect `error.transaction` and the actual result before retrying. A worker transport failure (`FDB_WORKER`) has no transaction observation and does not establish whether accepted migration work committed. JavaScript argument validation may fail before reaching the runner. Keep applied scripts unchanged, including comments and whitespace; correcting an unrecorded pending script is supported after a confirmed failed run.

`exportDocuments(table, format = 'json')` returns the versioned typed transfer string. `importDocuments(table, input, format = 'json')` returns `{ imported: number, transaction }`. Format is explicitly `json` or `ndjson`; imports target an existing collection and preserve validation, indexes and rollback behavior. These methods materialize strings and use the frontend transfer limits. They do not provide schema backups, streaming or implicit file I/O. Migration/import/export failures use the same code and transaction envelope as execute; parameter validation retains the limitations above; closed-handle operations use `FDB_CLOSED` without transaction observations.


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

The initial queue permits 256 outstanding operations and 128 MiB of encoded argument strings, including the active operation. Excess requests reject with FDB_LIMIT before submission. These are queue limits, not total-memory or query-time guarantees. Close stops new submissions, drains previously accepted operations, closes the native connection and waits for worker exit. Repeated close calls share the same Promise. Closing an active transaction uses native rollback-on-drop behavior, covered by reopen tests. Applications must await close to release the worker; no automatic idle shutdown or force-termination API is provided.

Query errors reject their own promises without stopping later queued requests. An explicit transaction belongs to the whole connection, not to an individual caller or Promise chain. Applications must coordinate transaction ownership and await/check steps before submitting dependent work. The current API does not provide transaction callback isolation. Unexpected worker errors/exits reject outstanding requests; deterministic worker-crash recovery, native-memory accounting, broader cancellation qualification and long-running lifecycle stress remain release gates.


`asyncDb.interrupt()` synchronously requests interruption of the active engine statement without entering the blocked worker queue. It returns false after native close, and otherwise true to indicate a live connection; true does not confirm interruption. The addon uses a process-local registry of weak interrupt handles and removes entries on native close/drop. No raw engine pointer crosses the JavaScript boundary. A cancelled execute rejects with FDB_CANCELLED and observed transaction state, and the worker can run subsequent operations. This is connection-wide, best-effort interruption: it neither targets a particular Promise nor removes queued requests. Coordinate submissions and stop repeated interrupt requests before reusing the connection. Parsing, frontend-only work and a running bundled function are outside direct engine interruption; no hard deadline is promised.


Fatal worker transport errors use FDB_WORKER, retain the first cause, and reject every outstanding request. They do not carry a transaction observation: a missing response does not establish whether an accepted write committed. Later requests reject with the same failure. After such a failure, close waits for worker exit; if a failure occurs during an already-running close, that close rejects only after cleanup finishes. A message-deserialization failure requests graceful native close through the remaining send channel. If that send itself fails, cleanup falls back to worker termination. This fatal-channel fallback is separate from cooperative query interruption and does not prove database recovery or write disposition. Do not automatically retry unknown-outcome writes.

Worker-transport state transitions have deterministic fault-injection coverage in an isolated test process. Native crashes, interrupted commit/recovery and forced-termination resource behavior still require separate release qualification.


`executeBatch(script)` returns an array of `{ offset, transaction, result }` or `{ offset, transaction, error: { code, message } }` entries. AsyncDatabase returns a Promise for that array and submits the entire batch as one worker operation. Offsets are UTF-8 byte positions. Successful result objects contain columns, typed positional rows and a bigint affected count. Statement errors are returned as entries and stop the batch; callers must inspect them. Lexical/pre-execution errors throw or reject through the normal error envelope and execute no statements.

Batches use the same semicolon-aware parser as Rust and the CLI, including trigger bodies and multiline documents. They have no implicit transaction and accept no parameter map. Explicit transaction statements belong in the script; inspect the last entry's transaction state on failure. Result encoding is performed before the next statement, so an unrepresentable result also stops the batch. Neither stopping nor an encoding error undoes previously executed work. Results remain materialized, and scripts/result-memory limits still need broader qualification.


Native lock contention uses FDB_BUSY; a stale read transaction that cannot become a writer uses FDB_BUSY_SNAPSHOT. These replace the former FDB_ENGINE classification for those native variants. Inspect the error's transaction observation before recovery; stale-snapshot work needs a fresh transaction. The client performs no automatic retries. A real shared-file test covers both codes and committed-value preservation through two Database instances. Broader cross-process contention/recovery qualification remains pending.


Native constraint, foreign-key and trigger-raise errors use FDB_CONSTRAINT and retain their engine message. Frontend candidate validation remains FDB_VALIDATION. The native category includes some runtime validation failures and does not specify a constraint subtype or rollback scope: ordinary SQL OR FAIL can retain earlier rows. Inspect transaction observations and apply the statement's conflict policy when recovering.


## Local package smoke

Normal npm packing first checks that the native addon loads; a missing or incompatible build fails before creating the archive. The npm file allowlist includes the native addon, synchronous/worker JavaScript entry points, declarations, README, MIT license and third-party notices. Build the current platform's addon, then verify the actual tarball from the repository root:

```sh
fastdb/scripts/check-node.sh
node fastdb/scripts/check-node-package.cjs
```

The smoke packs into a temporary directory, verifies the exact file inventory, installs the tarball into a separate consumer with npm offline and lifecycle scripts disabled, and tests public synchronous/worker queries, typed int64/record values, rollback and reopen. Profiling and audit calls cover both clients, lossless bigint counters, exact audit limits, retained active work on limit failure, repeat-call profiling counters and reopened index counts. It also compiles a consumer TypeScript import against the installed declarations, including public profiling/audit types, async results and rejection of numeric audit limits. Temporary artifacts are removed afterward. The TypeScript compiler is a checkout development tool; the installed package has no runtime registry dependencies.

This is a maintainer packaging check, separate from routine CI because the current debug addon is large. The verified local artifact is Linux x64 with Node 24.19.0, not a universal binary. Keep private=true: platform-specific prebuild selection, Node-version/platform coverage, optimized artifact sizing and complete distribution notices remain release work. No package has been published.

## SELECT profiling

Both clients provide `profileSelect(sql, parameters?)`; the async version returns a Promise and uses the existing worker queue. It returns `{ result, metrics }`, where `result` has the same typed rows, bigint affected count and transaction observations as `execute`. Every metric is a bigint: `rowsRead`, `rowsWritten`, `fullscanSteps`, `indexSteps`, `vmSteps`, `sortOperations`, `btreeSeeks`, `fetchBatches`, `fetchRowsRead` and `fetchVmSteps`. Native transport encodes counters as decimal strings before converting them to bigint, without a JavaScript Number conversion.

```js
const profile = await asyncDb.profileSelect(
  'SELECT id FROM posts WHERE author=$author',
  { $author: new Record('users', 'alice') },
);
console.log(profile.result.rows, profile.metrics.rowsRead);
```

The fetch-prefixed counters separately report target SELECT batches, physical row reads and VM instructions (zero without target reads); the other counters cover the primary statement. Deduplicated references share target reads. Fetch profiling uses the ordinary fetch budgets and one atomic snapshot scope; metadata/lowering queries, Rust/JavaScript decoding and transport are excluded. Physical row reads are not logical document counts. Only one SQL SELECT is accepted; writes, multiple statements, EXPLAIN and direct-record shorthand are rejected. Failures throw/reject through the usual error/transaction envelope without partial counters. Connection-wide async interruption and close behavior follow `execute`. Async profiling also accepts an optional third argument `{ signal }` for cooperative request-scoped cancellation; this is not a deadline guarantee.

## Collection integrity audit

`checkCollectionIntegrity(table, limits?)` is available on both clients; AsyncDatabase returns a Promise and runs the audit on its worker. Its optional third argument `{ signal }` requests operation-scoped cancellation using the same token and cleanup contract as query execution. It returns bigint `documents`, `indexes`, `indexEntries`, `encodedBytes`, plus transaction observations. Optional `maxDocuments` and `maxEncodedBytes` limits require nonnegative bigint values fitting uint64. Omitted limits use Rust defaults: 100,000 documents and 64 MiB of encoded ID/document bytes. Zero is valid for checking an empty collection.

```js
const audit = await asyncDb.checkCollectionIntegrity('posts', {
  maxDocuments: 100_000n,
  maxEncodedBytes: 64n * 1024n * 1024n,
});
console.log(audit.documents, audit.indexEntries);
```

The audit checks typed IDs, validation and index entry consistency in one snapshot, without repairing data. FDB_LIMIT returns no partial report; native errors include transaction observations. Limits do not bound engine allocations or time. It is not physical page/B-tree verification or a complete corruption-recovery tool. Async queue, close and connection-wide interruption behavior are unchanged.

## Constructing typed vectors

Vector factories work without opening a database and produce values accepted by either client:

```js
const { Vector } = require('@fastdb/node');
const dense = Vector.float32([1, 0, -1]);
const precise = Vector.float64(new Float64Array([0.1, 0.2, 0.3]));
const sparse = Vector.sparse32(new Float32Array([1, 0, -1]));
const quantized = Vector.quantized8([1, 0, -1]);
const bits = Vector.bit1([1, 0, -1]);
const sparseEntries = Vector.sparse32Entries(65536, [[0, 1], [65535, -1]]);
```

The five dense-input factories accept number arrays (including readonly arrays in TypeScript), Float32Array or Float64Array. Sparse, quantized and bit factories also take dense components. Inputs must contain 1–65,536 finite numbers. float64 retains binary64 inputs; the other factories first convert to float32 and reject overflow to infinity. Quantized and bit conversion are lossy. Inputs are copied, and factories run synchronously even when their results will be used with AsyncDatabase.

`Vector.sparse32Entries(dimensions, entries)` accepts an array of `[index, value]` pairs (readonly tuples are supported by the exported TypeScript `SparseVectorEntry` type). Dimensions must be 1–65,536. Indices must be unique, strictly increasing and in range, including zero-valued entries. Values must be finite and fit float32. Zero entries are omitted after float32 conversion; empty entries create an all-zero vector with the declared dimensions. Input and output storage scale with entry count, with no dense intermediate. Invalid indices throw RangeError; malformed pairs throw TypeError.

The native adapter uses the Rust Value constructors and validates conversion output. JavaScript passes a bounded binary64 buffer, preserving floating-point inputs without JSON number conversion. Invalid container/component types throw TypeError; invalid dimensions and float32 overflow throw RangeError. Native conversion/validation failures throw ordinary native errors without transaction observations because construction opens no connection. `new Vector(encodedBytes)` retains its existing encoded-byte path; binding that value still validates the encoding.

The installed-package smoke exercises all five dense-input factories and the sparse-entry factory through both clients and type-checks VectorComponents, readonly numeric arrays and rejection of bigint components.

Factory precision tests cover float32 halfway rounding, subnormal underflow and signed zero with known IEEE-754 bits. float64 preserves the smallest positive/negative subnormal, an adjacent-to-one value and the largest finite binary64 value. Sparse/quantized/bit factories operate on the narrowed float32 values. Quantized conversion can fail even for finite inputs: the pinned scale calculation overflows for a range spanning negative to positive float32 maximum. Such constructor errors occur locally and do not change an existing database transaction.

## Operation AbortSignal cancellation

Pass `{ signal: controller.signal }` in the options position below. Supply `undefined` for an earlier optional argument when using its default.

| Async method | Call with cancellation options |
|---|---|
| Query and row helpers | `execute(sql, parameters, options)`, `all(sql, parameters, options)`, `first(sql, parameters, options)`, `exactlyOne(sql, parameters, options)` |
| Query profiling | `profileSelect(sql, parameters, options)` |
| Collection integrity | `checkCollectionIntegrity(table, limits, options)` |
| Script batch | `executeBatch(script, options)` |
| Document export | `exportDocuments(table, format, options)` |
| Document import | `importDocuments(table, input, format, options)` |
| Migrations | `migrate(plan, options)` |

For example, `await db.exportDocuments('posts', undefined, { signal: controller.signal })` uses the default JSON format. Opening and closing do not accept a signal; close drains accepted work. Batch cancellation can be returned as an error entry in a resolved array, as described below.

`execute(sql, parameters?, { signal }?)`, `all`, `first`, `exactlyOne` and `profileSelect` on AsyncDatabase accept an AbortSignal. Each signalled query receives a separate native cancellation token. An aborted queued request executes no SQL when its turn arrives; accepted requests keep their original queue order and their queue slot until the worker responds. Cancellation does not close the database or cancel neighboring requests.

Active cancellation is cooperative at engine progress boundaries. Completion can win a race with abort, so use the returned result/error; abort alone does not prove rollback. A cancelled query rejects with `code: 'FDB_CANCELLED'` and the usual transaction observations. `signal.reason` is not substituted for the database report. No fixed cancellation latency is promised for compilation or non-engine work. The pinned trigger-interruption defect remains a release gate.

Listeners and token registry entries are released on response, send failure or worker failure. Tokens do not retain connections; a late abort cannot target another request. The process-local registry permits up to 16,384 outstanding signalled operations across clients, in addition to each client's existing queue bounds. Unsignalled queries use their existing path. Queries, batches, profiles, audits, transfers and migrations now accept signals; broader cancellation qualification remains open. Sync Database methods are unchanged.


Async `executeBatch(script, { signal }?)` supports operation-scoped cancellation. A pre-aborted request rejects before script splitting. Once execution starts, cancellation during or between statements appears as the final `FDB_CANCELLED` batch entry with that statement's byte offset and transaction observations; earlier entries are retained and later statements are skipped. Inspect every entry: a resolved batch promise does not imply that every statement succeeded. Earlier successful writes keep their effects, and an explicit transaction may remain active. No implicit rollback or commit is added. Rust visitor work and other non-engine work have no cancellation latency bound. Use a fresh signal for a retry and coordinate any rollback explicitly.


Async `importDocuments(table, input, format?, {signal}?)` and `exportDocuments(table, format?, {signal}?)` support operation-scoped cancellation. Aborted requests reject with FDB_CANCELLED and transaction observations. Import uses its existing atomic rollback scope, preserving prior outer work; export returns a complete string or an error. Accepted queued requests retain their place until the worker responds. Parsing, conversion and serialization have no fixed cancellation latency, and completion can win the race. Sync signatures and transfer encodings are unchanged.


Async `migrate(plan, {signal}?)` supports cooperative cancellation. Interrupted migration statements report FDB_CANCELLED while retaining migration version/byte-offset context in the message. All pending scripts and history rows share one atomic scope and roll back together on failure; previously applied migrations remain intact. A pre-aborted request does no database work, though JavaScript argument validation still runs. Compilation and plan parsing have no fixed cancellation latency, and a successful commit can win the race. Inspect the outcome before retrying.


`close()` drains accepted requests before dropping the worker connection; it does not implicitly abort them. Signals can still cancel accepted operations while close is pending. Later submissions reject with `FDB_CLOSED` and no transaction observations. An existing worker failure retains `FDB_WORKER` precedence. Aborted queued requests retain their queue slots until a response or worker failure. The real-worker close regression verifies that dropping an active outer transaction leaves only committed data after reopening; callers should still explicitly commit or roll back during normal operation.


If the native addon is absent or cannot be loaded, importing the package throws `FDB_NATIVE_LOAD`. The message identifies the current platform, architecture and Node version and points to the source build instructions. The original loader error remains in `error.cause` for diagnosing missing libraries, invalid binaries or other loader failures. This diagnostic does not select, download or rebuild an addon automatically.


The local package smoke also verifies pre-aborted calls for every signalled API, preserved transaction observations, listener disposal and fresh/late-token behavior using the installed package. Its consumer TypeScript check covers all cancellation options. These supplement the real-worker active-cancellation tests in the checkout.

## Dependency declaration inventory

From the checkout root, with the pinned Rust toolchain available:

```sh
node fastdb/scripts/inventory-node-dependencies.cjs x86_64-unknown-linux-gnu fastdb/docs/node-dependencies-linux-x64.json
node fastdb/scripts/inventory-node-dependencies.cjs x86_64-unknown-linux-gnu fastdb/docs/node-dependencies-linux-x64.json --check
```

This offline, locked, package-scoped Cargo query records normal and build dependencies, declared license expressions, the target and the lockfile hash. The checked-in Linux inventory contains 192 package/version entries. `cfg_block` 0.1.1 has no license expression in the query output: its pinned manifest instead declares `license-file = "LICENSE"`. That source file contains an Apache 2.0 notice; the package notice file includes it and the Apache 2.0 text. The inventory retains the absent expression rather than replacing Cargo metadata with an inferred value. Build tools are included; development dependencies are excluded. The inventory is an audit input, not evidence that every listed crate is linked into the addon, a complete component inventory, or a replacement for license texts. Bundled C/C++ sources and other vendored components require separate inspection. Regenerate for dependency changes and qualify other advertised targets separately.

With Python 3.11+ and the same Cargo cache, inspect cached source archives:

```sh
python3 fastdb/scripts/audit-crate-notices.py fastdb/docs/node-dependencies-linux-x64.json fastdb/docs/node-crate-notices-linux-x64.json
python3 fastdb/scripts/audit-crate-notices.py fastdb/docs/node-dependencies-linux-x64.json fastdb/docs/node-crate-notices-linux-x64.json --check
```

The audit checks archive checksums against Cargo.lock and records declared license-file paths plus hashes of license/notice filename candidates, including nested bundled sources. It reads archives without extracting them. Missing/ambiguous archives, checksum mismatches and stale reports fail explicitly. The current report verifies 185 archives with 316 candidate files; 13 archives have no matching candidates and seven workspace packages require separate inspection. Filename discovery can miss inline notices or unconventional names, and identifying a file does not establish that all required notices are included in the package. The report is kept outside the npm runtime package.
