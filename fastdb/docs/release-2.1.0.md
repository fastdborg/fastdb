# FastDB / FastQL 2.1.0 candidate

Status: production qualification in progress; **not published**. Follow
[the finite checklist](v2.1-tasks.md). The public 2.0.0 release remains available
and its artifacts are unchanged.

## Changes

- [SQLite-file adoption](sqlite-adoption.md): check or import a consistent
  snapshot, including committed WAL data, without modifying the source. Supported
  tables remain relational. Unsupported schema features are explicitly rejected;
  existing destinations are never overwritten.
- [Nested projections](nested-projections.md): stored record IDs under plain
  selection, explicit wildcard fetching, first/last array indexes, dot indexes
  and negative-index extensions. Paths are finite and bounded. Compact SQLite
  bracketed identifiers retain their SQL meaning.
- [Native build policy](production-build.md): an optimized named Rust profile
  retaining assertions and overflow checks, with artifact provenance checks and
  a reproducible application workload. Measurements are a remaining gate.
- [Full-text construction](fulltext-build-evidence.md): bounded initial insert
  batches reduce repeated index commits while preserving validation, transaction
  rollback and persisted document mapping.
- [Deployment](deployment.md) and [operations](operations.md): one owning Linux
  process per file, multiple connections inside it, error/commit reconciliation,
  checkpoint-status handling, maintenance backups and restore drills.
- [Dependency updates](dependency-security.md): patched Rust dependencies,
  SQLite 3.53.2 for adoption and QuickJS-NG 0.16.2 for stored functions. Existing
  function definitions retain their metadata format; applications should verify
  deterministic outputs when adopting a new JavaScript runtime.
- [Native returned-I/O tests](native-io-errors-evidence.md): disk-full, I/O,
  partial-write and failed-sync cases preserve whole old/new indexed state and
  permit recovery/subsequent writes under the documented acknowledgement rules.

The user approved the exact six-file NORMAL-mode checkpoint correction, now
integrated into this candidate. It syncs WAL frames before database backfill and
preserves that barrier across failed asynchronous sync/retry, including automatic
checkpoint cleanup after a committed write. The seven permanent crash-model and
boundary regressions pass on the integrated source. Combined checks pass:
757 Rust, 121 Node/application and three C ABI tests, formatting/Clippy and
TypeScript, plus affected-core Clippy. Hosted scoped CI run 36150018518 passes
on integrated source `a4df9cc27`. Exact-artifact qualification remains pending.
See the [review and before/after evidence](proposals/checkpoint-wal-sync.md).

The first exact artifact matrix found an additional inherited cancellation
recovery defect: a canceled native write is undone, but a stale transaction
marker makes later COMMIT/root RELEASE reject earlier caller work. Python
3.10/3.12/3.14 and Node reproduce it. The candidate is **not release-qualified**;
the separately approved [correction](proposals/cancellation-savepoint-poison.md)
is now integrated as isolated core commit `3ae0065e5`. It restores the transaction
marker only after successful savepoint rollback and retains the abandoned-write
guard. Its isolated affected tests pass; combined checks and a fresh artifact
matrix remain required.
C ABI, CLI, SQLite adoption, Node installations, the exact-source Rust consumer,
PHP/Go/Swift/C#, ownership and V1/V2 upgrade/restore checks otherwise pass, but
these observations do not qualify a corrected artifact that has not been built.

## Intended distribution and compatibility

Linux x64 on the documented Ubuntu 24.04 baseline, with Rust, Node/TypeScript,
Python, PHP, Swift, C# and Go clients in the GitHub release bundle. No cloud,
browser/WASM, additional platform or graph-database product scope is added.
SQL joins, typed references, indexed inverse lookups and bounded document fetching
remain existing capabilities. Language-registry publication is not part of this
release gate.

The candidate preserves V2 catalog format 3 and reads older supported formats.
Preserve a pre-upgrade backup. Qualification must include actual released V1 and
2.0.0 artifacts, candidate writes/reopens and backup restores; a binary downgrade
is not a data rollback. SQLite adoption is separate from FastDB version upgrades.

## Remaining release evidence

- [ ] Clean source/lockfile identity and hosted scoped CI.
- [x] Integrated checkpoint correction's native regressions and combined checks.
- [ ] Exact seven-client artifacts, SQLite adoption and process ownership.
- [ ] Released-version upgrade and restore checks, including stored functions.
- [ ] Measured search/resource envelope and sustained readers/writer behavior.
- [ ] Versioned public download, checksum receipt and release URL.

Do not infer completed qualification from this candidate document. Final evidence
will identify the exact source, artifact hashes, runtime matrix and limitations.
