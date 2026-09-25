# Initial browser OPFS persistence

Development qualification, 2026-09-25. This extends the
[in-memory browser milestone](v2-browser-client.md); V2-C and V2-R remain open.
Cloud and upstream implementation sources are unchanged.

Subsequent fault qualification discovered the
[first-commit FULL-mode sync omission](proposals/wal-first-commit-sync.md).
The original artifact below predates its approved fix in `fe2ccd404`. The initial
checks remain valid page-close/reopen evidence; they do not establish power-loss
durability. The full fault matrix also passes on the integrated source; see
[integrated artifact qualification](v2-wal-durability-evidence.md) for the new
package hashes and results.

## Storage contract

`Database.open()` remains in-memory. `Database.open('my-app')` opens an origin-local
OPFS database. Names are 1..128 ASCII letters, digits, dots, underscores or hyphens,
starting with a letter or digit. Each name maps to its own directory under
`fastdb-v2`, containing `database` and `database-wal`. Names are identifiers, not
filesystem paths. Traversal and separators are rejected before starting workers.

A dedicated storage worker owns both synchronous access handles for the whole
session. The [File System Standard](https://fs.spec.whatwg.org/#api-filesystemfilehandle-createsyncaccesshandle)
specifies exclusive access for these handles; the adapter uses that ownership
as its database lock. A second owner, including another tab, fails with
`FDB_BUSY`. It does not enable unsafe shared access or concurrent reader sessions.
Different database names can remain open independently.

The Rust adapter calls `Database::open_with_io` with ordinary automatic WAL
management. Reads, writes, size, truncate and sync map to the host handles;
engine sync calls explicitly invoke
[flush](https://fs.spec.whatwg.org/#api-filesystemsyncaccesshandle-flush).
Rust threads send bounded copied buffers through the coordinator and block on
an atomic completion, while the coordinator remains responsive to cancellation.
I/O errors propagate to the engine; a short write is an error. No asynchronous
write is acknowledged merely because it was queued. A stalled host I/O call
terminates the session instead of returning a timeout and allowing later work
to race an unfinished write.

Close drains requests, drops the Rust session, waits for its worker cleanup,
then closes the OPFS handles before reporting success. Failed startup also
awaits handle cleanup before reporting the failure. An unresponsive storage
worker falls back to termination and an error, rather than successful close.

## Qualification

The actual installed package passes in Chromium 149.0.7827.55 and Firefox 151.0
on Linux with Playwright 1.61.0. These are real origin-private files, not a mocked
I/O backend. Tests include:

- Rows, unique constraints, catalog/index integrity, ANN and indexed spatial
  queries across close/reopen.
- Explicit rollback and rollback of uncommitted work on close.
- Independent named databases, competing owners in the same page and across tabs.
- Failed WASM startup followed by successful reopen of the same name.
- Interrupted native writes with zero partial rows and preserved caller work.
- Abrupt writer-page closure without database close/checkpoint: after a checkpoint,
  a new acknowledged WAL write survives and an uncommitted row is absent on
  reopen in another tab. The recovered collection and indexes pass integrity.
- The prior memory-client typed-value, cancellation, lifecycle and worker-error
  fixture, plus a strict TypeScript consumer through installed package exports.

The browser tarball is installed offline into a fresh consumer. Runtime checks
load assets from that installed package. The latest log is
`/tmp/fastdb-browser-opfs-installed-final.log`. Native protocol/browser Clippy,
threaded-WASI Clippy, formatting and `git diff --check` pass. Clippy logs:
`/tmp/fastdb-browser-opfs-native-clippy.log` and
`/tmp/fastdb-browser-opfs-wasm-clippy.log`. The unchanged upstream core emits the
same ten WASI warnings. This adapter changes only the browser package; the earlier
736-pass/two-known-failure Rust result and Python wheel in the prior milestone
remain their own evidence, not a new green combined run.

| Development artifact | Bytes | SHA-256 |
|---|---:|---|
| `fastdb.wasm` | 21,967,720 | `b1477b809356389451b3ff511250beb3805895be99bd7afd226732f68ad8c06a` |
| `fastdb-browser-2.0.0-dev.1.tgz` | 6,021,295 | `05b7c6060fd76fb1042c27074930a81f8f00c8f9b7527827ceef7a4ed230a762` |

The tarball is under `/tmp/fastdb-browser-opfs-final-packages/`. Toolchain,
development profile and dependency versions are unchanged from the in-memory
milestone; no new dependencies or core feature changes are introduced.

## Remaining qualification

This is initial persistence support, not complete storage/release qualification.
The integrated artifact now passes bounded quota, short-write, flush, checkpoint,
read, close and worker-failure checks. Process/OS crashes, previous-version
database import, prolonged workloads and the final supported browser/OS matrix
remain open. OPFS data is subject to browser quota and eviction policies; it is
not a backup. A database-deletion API and concurrent browser reader sessions are
not implemented. Browser FTS still requires the separately reviewed WASI core
proposal. User-function error semantics still require the scalar-error proposal.
Complete attribution and final distribution remain V2-R gates.
