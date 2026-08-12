//! P0.8 — Atomicity and failure injection.
//!
//! For each deterministic failpoint: start from a new empty file, force the
//! `CREATE` to fail, drop/reopen, and prove the whole transaction (catalog
//! bootstrap, table registration, physical DDL, and the record) rolled
//! back together — leaving an empty schema and `integrity_check == ok` —
//! and that a subsequent `CREATE` then succeeds. Also: a duplicate explicit
//! id is a constraint error and preserves the original record.

#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::io::ErrorKind;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use turso_core::io::{FileId, FileSyncType};
use turso_core::{
    Buffer, Clock, Completion, CompletionError, File, MemoryIO, MonotonicInstant, OpenFlags,
    WallClockInstant, IO,
};
use turso_fastdb::{Database, ErrorCategory, Failpoint, Value};

/// An in-memory Turso I/O backend that can fail the next WAL sync completion.
/// Writes still pass through the real pager/WAL commit state machine; only the
/// durability completion is replaced with an I/O error.
struct WalSyncFaultIo {
    inner: Arc<MemoryIO>,
    fail_next_wal_sync: Arc<AtomicBool>,
    failures: Arc<AtomicUsize>,
}

impl WalSyncFaultIo {
    fn new() -> Self {
        Self {
            inner: Arc::new(MemoryIO::new()),
            fail_next_wal_sync: Arc::new(AtomicBool::new(false)),
            failures: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn arm_next_wal_sync(&self) {
        self.fail_next_wal_sync.store(true, Ordering::SeqCst);
    }

    fn failure_count(&self) -> usize {
        self.failures.load(Ordering::SeqCst)
    }
}

impl Clock for WalSyncFaultIo {
    fn current_time_monotonic(&self) -> MonotonicInstant {
        self.inner.current_time_monotonic()
    }

    fn current_time_wall_clock(&self) -> WallClockInstant {
        self.inner.current_time_wall_clock()
    }
}

impl IO for WalSyncFaultIo {
    fn open_file(
        &self,
        path: &str,
        flags: OpenFlags,
        direct: bool,
    ) -> turso_core::Result<Arc<dyn File>> {
        let inner = self.inner.open_file(path, flags, direct)?;
        Ok(Arc::new(WalSyncFaultFile {
            path: path.to_string(),
            inner,
            fail_next_wal_sync: self.fail_next_wal_sync.clone(),
            failures: self.failures.clone(),
        }))
    }

    fn remove_file(&self, path: &str) -> turso_core::Result<()> {
        self.inner.remove_file(path)
    }

    fn step(&self) -> turso_core::Result<()> {
        self.inner.step()
    }

    fn file_id(&self, path: &str) -> turso_core::Result<FileId> {
        self.inner.file_id(path)
    }

    fn fill_bytes(&self, dest: &mut [u8]) {
        self.inner.fill_bytes(dest);
    }

    fn generate_random_number(&self) -> i64 {
        self.inner.generate_random_number()
    }
}

struct WalSyncFaultFile {
    path: String,
    inner: Arc<dyn File>,
    fail_next_wal_sync: Arc<AtomicBool>,
    failures: Arc<AtomicUsize>,
}

impl File for WalSyncFaultFile {
    fn lock_file(&self, exclusive: bool) -> turso_core::Result<()> {
        self.inner.lock_file(exclusive)
    }

    fn unlock_file(&self) -> turso_core::Result<()> {
        self.inner.unlock_file()
    }

    fn pread(&self, pos: u64, c: Completion) -> turso_core::Result<Completion> {
        self.inner.pread(pos, c)
    }

    fn pwrite(
        &self,
        pos: u64,
        buffer: Arc<Buffer>,
        c: Completion,
    ) -> turso_core::Result<Completion> {
        self.inner.pwrite(pos, buffer, c)
    }

    fn pwritev(
        &self,
        pos: u64,
        buffers: Vec<Arc<Buffer>>,
        c: Completion,
    ) -> turso_core::Result<Completion> {
        self.inner.pwritev(pos, buffers, c)
    }

    fn sync(&self, c: Completion, sync_type: FileSyncType) -> turso_core::Result<Completion> {
        if self.path.ends_with("-wal") && self.fail_next_wal_sync.swap(false, Ordering::SeqCst) {
            self.failures.fetch_add(1, Ordering::SeqCst);
            c.error(CompletionError::IOError(
                ErrorKind::Other,
                "fastdb_test_wal_sync",
            ));
            return Ok(c);
        }
        self.inner.sync(c, sync_type)
    }

    fn size(&self) -> turso_core::Result<u64> {
        self.inner.size()
    }

    fn truncate(&self, len: u64, c: Completion) -> turso_core::Result<Completion> {
        self.inner.truncate(len, c)
    }

    fn has_hole(&self, pos: usize, len: usize) -> turso_core::Result<bool> {
        self.inner.has_hole(pos, len)
    }

    fn punch_hole(&self, pos: usize, len: usize) -> turso_core::Result<()> {
        self.inner.punch_hole(pos, len)
    }
}

fn fresh_db() -> (TempDir, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.fastdb");
    let s = path.to_str().unwrap().to_string();
    (dir, s)
}

/// After a rolled-back first CREATE the schema must be empty (no catalog
/// tables, no physical table, no record) and the database must be consistent.
fn assert_empty_after_rollback(path: &str) {
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    let native = conn.native();
    let tables = common::native_rows(native, "SELECT name FROM sqlite_schema WHERE type='table'");
    assert!(
        tables.is_empty(),
        "rolled-back transaction left tables behind: {tables:?}"
    );
    assert_eq!(
        common::integrity_check(native),
        "ok",
        "integrity_check after rollback"
    );
    let r = conn.execute("SELECT * FROM person:tobie;").unwrap();
    assert!(r.records.is_empty(), "no record after rollback");
}

fn then_create_succeeds(path: &str) {
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    let r = conn
        .execute("CREATE person:tobie SET name = 'Tobie';")
        .unwrap();
    assert_eq!(r.records.len(), 1, "subsequent non-failing CREATE succeeds");
}

/// Arm `fp`, run a CREATE that must fail, reopen, prove clean rollback, then
/// prove a fresh CREATE succeeds.
fn injected_fail_round(fp: Failpoint) {
    let (_dir, path) = fresh_db();
    {
        let db = Database::open(&path).unwrap();
        let conn = db.connect().unwrap();
        conn.arm_failpoint(fp);
        let err = conn
            .execute("CREATE person:tobie SET name = 'Tobie';")
            .unwrap_err();
        assert_eq!(
            err.category(),
            ErrorCategory::Transaction,
            "failpoint {fp:?}: expected Transaction error, got: {err}"
        );
    }
    assert_empty_after_rollback(&path);
    then_create_succeeds(&path);
}

#[test]
fn atomic_001_fail_after_bootstrap() {
    injected_fail_round(Failpoint::AfterBootstrap);
}
#[test]
fn atomic_002_fail_after_catalog_row() {
    injected_fail_round(Failpoint::AfterCatalogRow);
}
#[test]
fn atomic_003_fail_after_physical_ddl() {
    injected_fail_round(Failpoint::AfterPhysicalDdl);
}
#[test]
fn atomic_004_fail_after_record_prepare() {
    injected_fail_round(Failpoint::AfterRecordPrepare);
}
#[test]
fn atomic_005_fail_after_record_insert() {
    injected_fail_round(Failpoint::AfterRecordInsert);
}
#[test]
fn atomic_006_fail_at_commit() {
    // A COMMIT-time failure must still enter the rollback path: the schema
    // ends up empty and a subsequent CREATE succeeds.
    injected_fail_round(Failpoint::CommitFailure);
}

#[test]
fn atomic_007_real_wal_sync_failure_rolls_back_and_connection_recovers() {
    let io = Arc::new(WalSyncFaultIo::new());
    let path = "commit-sync-failure.fastdb";

    {
        let db = Database::open_with_io(path, io.clone()).unwrap();
        let conn = db.connect().unwrap();
        io.arm_next_wal_sync();

        let err = conn
            .execute("CREATE person:tobie SET name = 'Tobie';")
            .unwrap_err();
        assert_eq!(err.category(), ErrorCategory::Io, "{err}");
        assert_eq!(io.failure_count(), 1, "the WAL sync boundary was reached");

        let selected = conn.execute("SELECT * FROM person:tobie;").unwrap();
        assert!(
            selected.records.is_empty(),
            "failed COMMIT must leave no locally visible record"
        );

        let created = conn
            .execute("CREATE person:tobie SET name = 'Tobie';")
            .unwrap();
        assert_eq!(created.records.len(), 1, "connection remains reusable");
    }

    let db = Database::open_with_io(path, io).unwrap();
    let conn = db.connect().unwrap();
    let selected = conn.execute("SELECT * FROM person:tobie;").unwrap();
    assert_eq!(
        selected.records.len(),
        1,
        "successful retry survives reopen"
    );
    assert_eq!(common::integrity_check(conn.native()), "ok");
}

#[test]
fn atomic_008_rollback_failure_preserves_both_errors() {
    let (_dir, path) = fresh_db();
    {
        let db = Database::open(&path).unwrap();
        let conn = db.connect().unwrap();
        conn.arm_failpoint(Failpoint::AfterRecordInsert);
        conn.arm_failpoint(Failpoint::RollbackFailure);

        let err = conn
            .execute("CREATE person:tobie SET name = 'Tobie';")
            .unwrap_err();
        assert_eq!(err.category(), ErrorCategory::Transaction, "{err}");
        let detail = err.to_string();
        assert!(
            detail.contains("original:"),
            "missing original error: {detail}"
        );
        assert!(
            detail.contains("rollback also failed:"),
            "missing rollback error: {detail}"
        );
    }

    // Dropping the unusable connection invokes the engine's transaction
    // cleanup. Reopen must not expose any of the failed first mutation.
    assert_empty_after_rollback(&path);
    then_create_succeeds(&path);
}

#[test]
fn duplicate_explicit_id_is_constraint_and_preserves_original() {
    let (_dir, path) = fresh_db();
    {
        let db = Database::open(&path).unwrap();
        let conn = db.connect().unwrap();
        let r = conn
            .execute("CREATE person:tobie SET name = 'Tobie';")
            .unwrap();
        assert_eq!(
            r.records[0].fields,
            vec![("name".to_string(), Value::Str("Tobie".to_string()))]
        );
        let err = conn
            .execute("CREATE person:tobie SET name = 'Other';")
            .unwrap_err();
        assert_eq!(
            err.category(),
            ErrorCategory::Constraint,
            "duplicate id: {err}"
        );
    }
    // Reopen: exactly one unchanged record; the failed create did not persist.
    let db = Database::open(&path).unwrap();
    let conn = db.connect().unwrap();
    let r = conn
        .execute("SELECT * FROM person WHERE name = 'Tobie';")
        .unwrap();
    assert_eq!(r.records.len(), 1);
    let r = conn
        .execute("SELECT * FROM person WHERE name = 'Other';")
        .unwrap();
    assert!(r.records.is_empty(), "failed create must not persist");
    assert_eq!(common::integrity_check(conn.native()), "ok");
}
