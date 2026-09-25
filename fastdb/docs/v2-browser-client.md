# In-memory browser client qualification

Current follow-up: both core proposals are now approved and integrated. The
installed browser client passes FTS lifecycle, OPFS reopen/recovery, V1 upgrade
and the repeated storage-fault matrix in Chromium/Firefox. See
[combined integration evidence](v2-core-integration-evidence.md), which retains
an unexplained Firefox shutdown timeout from the first fault run. Remaining
release gates include final platform/distribution qualification and attribution.
The earlier milestone results and pending-state descriptions below are historical;
their artifact hashes do not identify the newly integrated build.


Development milestone, 2026-09-25. V2-C and V2-R remain open. Cloud is outside
this work. No V2 package has been published.
This records the initial in-memory artifact. The later
[OPFS milestone](v2-browser-opfs.md) adds persistent databases and supersedes its
browser artifact/capability limits; the Python and native regression evidence
below remains applicable.

## Implemented boundary

`@fastdb/browser` version `2.0.0-dev.1` provides an asynchronous FastDB frontend
client running in a browser worker. It supports in-memory databases only;
persistent paths return `FDB_UNSUPPORTED`. The public methods cover execution,
profiling, batches, migrations, document transfer, integrity inspection,
transaction state, vector construction and close. See the
[API overview](../bindings/browser/README.md).

The new `fastdb-protocol` Rust package shares the request operations and existing
version-1 portable typed-value encoding with Python. Both retain duplicate result
columns, signed 64-bit integers, records, binary bytes, nested values, all five
vector encodings and negative zero. Neither the stored document encoding nor the
catalog format changes for this client.

A WASI reactor runs one Rust session on its own thread. The JavaScript coordinator
keeps receiving cancellation messages while Rust executes. Each request owns its
token and identifier; a late cancellation cannot reach a later request. The input
buffer export validates ownership and length before consuming memory. Requests
are capped at 16 MiB, with 64 waiting operations. Deadlines include queue time.
Close drains accepted operations, drops the session, observes zero active Rust
workers and terminates the pool. Worker startup/runtime failures reject pending
work. These limits do not bound every engine allocation.

## Interrupted ordinary SQL writes

The real-browser fixture exposed a preexisting native engine behavior: cancelling
a plain `INSERT INTO sink SELECT ...` inside a caller transaction returned
`FDB_CANCELLED` but left 196 inserted rows in that transaction. A first native
control using a SQL callback missed the bug because emitting a function call
changes statement-journal eligibility in the pinned compiler.

The final native regression uses plain SQL without a callback or trigger. An
engine progress callback observes the atomic `last_insert_rowid()` value, then
interrupts after an actual write without re-entering SQL. The original frontend
fails this regression. It covers autocommit, BEGIN and caller SAVEPOINT with two
interruption paths. The fixed frontend leaves zero partial rows, preserves prior
caller work and allows retry/rollback.

The fix adds a private frontend savepoint around ordinary INSERT/UPDATE/DELETE
inside an existing caller transaction. Cancellation rolls that scope back.
Autocommit keeps its native transaction boundary; other native error dispositions
are preserved. The conflict oracle checks five policies, two transaction modes
and three source routes, including rows, transaction state, `changes()` and
`last_insert_rowid()`. Core sources are unchanged. This does not resolve the
separate readonly scalar-error transaction bug or promise rollback for every
ambiguous `FDB_ROLLBACK` cleanup outcome.

Control/fix logs:

- `/tmp/fastdb-browser-write-cancel-control.log`
- `/tmp/fastdb-native-write-cancel-plain-control.log`
- `/tmp/fastdb-native-write-cancel-fixed.log`
- `/tmp/fastdb-native-write-cancel-conflicts.log`

## Installed artifacts

Rust 1.88.0, WASI SDK 33.0 and libclang 18.1.1 build the threaded-WASI module.
The Rust development profile uses optimization level 1, assertions retained,
debug symbols disabled and no LTO. C/C++ uses optimization level 2. This is not a
release-profile build. Runtime browser checks use Node 24.19.0, pnpm 11.23.0,
Playwright 1.61.0, Chromium 149.0.7827.55 and Firefox 151.0 on Linux.

| Local artifact | Bytes | SHA-256 |
|---|---:|---|
| `fastdb.wasm` | 21,955,270 | `7469b8da40426b960979dd36339a7a8beb6826df19409a7906782d8e9c7a883d` |
| `fastdb-browser-2.0.0-dev.1.tgz` | 6,016,905 | `76956dc72c33f301406297d8dc33ca2bf5eb00e26f207d5a646ed1ac8a70f87e` |
| `fastdb_embedded-2.0.0.dev1-cp310-abi3-manylinux_2_35_x86_64.whl` | 81,028,766 | `bb220720d587ebc914a4613e3b67a44b84fcc4d8581b2c2e707bfc2e4dcc01c1` |

The browser tarball lives under `/tmp/fastdb-browser-fixed-packages/`; the wheel
under `/tmp/fastdb-python-write-fixed-wheels/`. They include the frontend write
cancellation fix and shared protocol. The earlier development packages remain
historical evidence; they do not include this fix.

The tarball installs offline into a fresh consumer. Its package exports pass a
strict TypeScript consumer check. Its installed WASM/JavaScript assets pass the
same fixture in Chromium and Firefox, including:

- Typed values, duplicate labels, migrations, schema errors and index integrity.
- Transactions, transfers, partial batch reports, profiling and write limits.
- Indexed ANN, H3 and a successful sandboxed JavaScript invocation.
- Active/queued cancellation and deadlines, no partial ordinary write rows,
  preserved caller work, retry and rollback.
- Close draining, zero active Rust threads, idempotent close, closed-client errors,
  missing WASM and deliberate worker failure with queued work.
- Explicit full-text unavailability while the WASI core proposal is pending.

The rebuilt Python ABI3 wheel passes all seven installed integration tests on
each of CPython 3.10.21, 3.12.3 and 3.14.7. Its cancellation fixture now also
interrupts an ordinary native write and verifies zero rows and retained prior work.
These are clean offline installs with isolated Python imports.

Logs: `/tmp/fastdb-browser-installed-fixed.log`,
`/tmp/fastdb-python-write-fixed-build.log`,
`/tmp/fastdb-python-write-fixed-py310.log`,
`/tmp/fastdb-python-write-fixed-py312.log`,
`/tmp/fastdb-python-write-fixed-py314.log`.

Native all-target Clippy passes for all eight FastDB packages. Threaded-WASI
Clippy passes for protocol/browser with FastDB warnings denied; the unchanged
upstream core emits ten target-specific warnings. Logs:
`/tmp/fastdb-browser-all-clippy-fixed.log` and
`/tmp/fastdb-browser-wasm-clippy-fixed.log`.

The full scoped command
`cargo test --locked -p fastql-parser -p fastdb -p fastdb-cli -p fastdb-tests --no-fail-fast`
finishes with **736 passed, 2 failed, 0 ignored**. Both failures are in
`user_functions`: `sandbox_limits_and_atomic_failure` and
`function_versions_follow_reader_snapshots_and_cancelled_writes_preserve_prior_work`.
They reproduce the documented scalar-error transaction bug on the unchanged
active core; the temporary proposed fix is not part of this run. Every other
target, including the new plain-write cancellation and enhanced conflict oracle,
passes. Log: `/tmp/fastdb-browser-frontend-scoped.log`. Scoped formatting and
`git diff --check` also pass. This is not a green combined acceptance result.

## Remaining gates

- OPFS persistence, concurrent-open/locking, reopen/crash recovery and catalog
  compatibility in real browsers. The native `open_with_io` API is available;
  it is not yet a browser adapter.
- Approved integration of [WASI full-text support](proposals/fts-wasi.md), followed
  by the actual client lifecycle/search tests. Temporary reactor evidence does
  not establish current client FTS support.
- The [scalar-error core proposal](proposals/udf-error-transaction.md), throwing
  function qualification and full combined acceptance.
- Supported browser/OS matrix, bundler integration, performance/resource limits,
  final artifacts and complete attribution. The current WASI source inventory
  has 194 declarations, 186 checked crate archives and 128 collected notice texts;
  eight workspace packages and 17 packages need further notice review. JS
  wasm-util's package/repository snapshot lacks a license text despite declaring
  MIT; SDK runtime notices also remain open. Inventories do not establish a
  complete license audit or prove which code is linked.

The Node addon was not rebuilt for this milestone. Its earlier ANN-era debug
artifact is not evidence for the current frontend or new user functions.

Later [notice packaging qualification](v2-notice-evidence.md) adds verified
repository-source supplements to the generated bundles and checks their packaged
copies. Its updated counts and artifact hashes supersede the notice-only counts
above; the earlier runtime evidence retains its original artifact scope.

[Later bundler qualification](v2-bundler-evidence.md) covers Vite 8.3.1 dev and
production using the new installed asset-copy command, absolute worker/WASM URLs
and a nested base path in Chromium/Firefox. Automatic worker-graph bundling,
other bundlers and SSR remain unqualified.
