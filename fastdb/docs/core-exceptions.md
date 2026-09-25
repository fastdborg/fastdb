# Maintained engine exceptions

Required review checklist for every upstream sync. Baseline and full provenance
are in [UPSTREAM.md](../UPSTREAM.md). The five active exceptions below are approved and
integrated. The retired WASI exception is recorded separately. No equivalent upstream fix or upstream issue/PR has been verified for
this register yet; do not infer that upstream still lacks a fix from that status.
No automatic removal or periodic monitoring is configured.

| Exception / local commit | Required behavior and regression | When to update or remove |
|---|---|---|
| Trigger interruption `ded389aea` | `OpProgram` preserves interruption versus contention; keep the FastDB after-write trigger cancellation regression and affected native trigger suite. [Review](trigger-interrupt-review.md). | Upstream propagates Interrupt correctly through subprograms, with saved state and unchanged Busy handling. |
| FTS cache isolation `fb246a8e4` | `test_fts_cache_preserves_connection_snapshots`: no uncommitted membership/score leaks; existing reader snapshot survives writer commit. [Review](proposals/fts-cache-snapshot.md). | Upstream directory caching is scoped to the owning pager or otherwise proves equivalent isolation and rollback behavior. |
| First FULL commit WAL sync `fe2ccd404` | Native first-commit sync probe and FULL/NORMAL/OFF controls; browser flush-fault matrix. [Review](proposals/wal-first-commit-sync.md). | Upstream flushes prepared frames before publishing/acknowledging FULL commits, including the first commit. |
| FTS backing storage `12109384a` | `test_fts_backing_storage_integrity`, `test_drop_table_frees_backing_and_ordinary_indexes`, `test_fts_backing_storage_physical_corruption`; upgrade/restore rehearsal. [Review](proposals/fts-integrity.md). | Upstream excludes backing trees only from inappropriate logical counts, retains physical checks, and reclaims all backing roots on teardown/rollback. Existing orphan pages still need separate handling. |
| Scalar read errors `f26014f04` | `scalar_read_error_preserves_transactions_and_named_savepoints`, FastDB user-function tests and Node cancellation/transaction tests. [Review](proposals/udf-error-transaction.md). | Upstream read-only extension failures preserve caller transactions, savepoints and changes(), while autocommit cleanup and writer rollback controls pass. |

## Retired exception: WASI FTS

The user removed browser support from active V2 development. Commit `ae6777a17`
reverts `2ef619c07` in isolation, restoring the ten core files to their prior state.
Frontend WASI feature wiring, the browser package and its build/probe code are
also removed. This exception is **not active** and must not be reapplied on an
upstream sync. The reviewed patches remain historical evidence for any separately
requested future browser work. See [the removal record](browser-removal.md).

## Upstream sync checklist

- [ ] Record the exact proposed upstream SHA and inspect its source, changes and
  issues/PRs for each exception. Add verified issue/PR URLs here when available.
- [ ] Compare behavior, not just patch text. Record per entry: retain, adapt, or
  remove; upstream evidence; candidate commit; and regression results.
- [ ] On an isolated sync branch, drop a local exception only after its regression
  passes using the upstream replacement. A patch conflict is not proof of a fix.
- [ ] If upstream is only partially equivalent, retain the necessary portion and
  document its revised scope. Materially new core behavior needs separate review.
- [ ] Run the affected upstream tests and combined FastDB acceptance. For storage
  changes include upgrade/restore and faults; for WASI include installed browsers.
- [ ] Update this register, UPSTREAM.md, proposal status and release provenance in
  the sync commit. Preserve the regression even when the local patch is removed.

The scalar-error patch and retired WASI patch remain byte-for-byte available
in their review files. Their SHA-256 values are respectively
`953e88426f9c5e5ea52a3f41251c7e64bc0b2bb06c70557f58ff596454c68b0a`
and `b6771224bade4381592906cbc16962add85819f1881e56860bc02a10a992d881`.
The frontend WASI wiring patch is
`68adbabbe59c6c2b6b160aa3b12f046e8af17bef2aad6f790d1bddb9b9efa0a1`.
