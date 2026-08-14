---
name: mvcc
description: Experimental MVCC snapshot isolation, recovery, GC, checkpointing, memory behavior, and production limitations
---
# MVCC Guide (Experimental)

Multi-Version Concurrency Control. **Work in progress, not production-ready.**

**CRITICAL**: Ignore MVCC when debugging unless the bug is MVCC-specific.

## Enabling MVCC

```sql
PRAGMA journal_mode = 'mvcc';
```

Runtime configuration, not a compile-time feature flag. Per-database setting.

## How It Works

Standard WAL: single version per page, readers see snapshot at read mark time.

MVCC: multiple row versions, snapshot isolation. Each transaction sees consistent snapshot at begin time.

### Key Differences from WAL

| Aspect | WAL | MVCC |
|--------|-----|------|
| Write granularity | Every commit writes full pages | Affected rows only
| Readers/Writers | Don't block each other | Don't block each other |
| Persistence | `.db-wal` | `.db-log` (logical log) |
| Isolation | Snapshot (page-level) | Snapshot (row-level) |

### Versioning

Each row version tracks:
- `begin` - timestamp when visible
- `end` - timestamp when deleted/replaced
- `btree_resident` - existed before MVCC enabled

## Architecture

```
Database
  └─ mv_store: MvStore
      ├─ rows: SkipMap<RowID, Vec<RowVersion>>
      ├─ txs: SkipMap<TxID, Transaction>
      ├─ Storage (.db-log file)
      └─ CheckpointStateMachine
```

**Per-connection**: `mv_tx` tracks current MVCC transaction.

**Shared**: `MvStore` with lock-free `crossbeam_skiplist` structures.

## Key Files

- `core/mvcc/mod.rs` - Module overview
- `core/mvcc/database/mod.rs` - Main implementation (~3000 lines)
- `core/mvcc/cursor.rs` - Merged MVCC + B-tree cursor
- `core/mvcc/persistent_storage/logical_log.rs` - Disk format
- `core/mvcc/database/checkpoint_state_machine.rs` - Checkpoint logic

## Checkpointing

Flushes row versions to B-tree periodically.

```sql
PRAGMA mvcc_checkpoint_threshold = <pages>;
```

Process: acquire lock → begin pager txn → write rows → commit → truncate log → fsync → release.

## Recovery and Garbage Collection

The current source implements logical-log restart recovery, durable checkpoint
watermarks, interrupted-checkpoint reconciliation, and fail-closed corruption
handling. See `docs/internals/mvcc/RECOVERY_SEMANTICS.md`.

Checkpoint and inline GC reclaim versions using the active-reader low-water
mark and the durable checkpoint boundary. See
`docs/internals/mvcc/GC.md`. Use the memory benchmark's `update-churn` profile
when evaluating sustained growth.

## Current Production Limitations

- The upstream manual still labels MVCC experimental and not production-ready,
  and warns that queries may be incorrect or panic.
- The supported truncate checkpoint is stop-the-world and blocks both readers
  and writers. Passive checkpointing is separately experimental and has
  documented snapshot/GC constraints.
- Startup eagerly loads database state into memory; sustained long-reader and
  large-database memory bounds are not a production guarantee.
- Fundamental concurrency/cursor/rowid tests remain ignored for known or
  intermittent failures, including non-overlapping concurrent writes and an
  MVCC cursor model test.
- `core/mvcc/mod.rs` still records phantom/read-skew/write-skew gaps beyond the
  implemented snapshot behavior.

FastDB Phase 11 rejected the retained pin, `v0.8.0-pre.4`, and upstream `main`
at `069b5431e86779d70df3940711bb61f8601db069`. Do not enable MVCC in FastDB
without a new exact-SHA qualification audit.

## Testing

```bash
# Run MVCC-specific tests
cargo test mvcc

# TCL tests with MVCC
make test-mvcc
```

Use `#[turso_macros::test(mvcc)]` attribute for MVCC-enabled tests.

```rust
#[turso_macros::test(mvcc)]
fn test_something() {
    // runs with MVCC enabled
}
```

## References

- `core/mvcc/mod.rs` documents data anomalies (dirty reads, lost updates, etc.)
- Snapshot isolation vs serializability: MVCC provides the former, not the latter
- `docs/phase11-mvcc-audit.md` records FastDB's exact-SHA production gate
