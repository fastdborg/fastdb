//! Deterministic process failure before selected synchronous filesystem operations.
use crate::{Database, Parameters, Value};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use turso_core::io::{
    clock::{Clock, MonotonicInstant, WallClockInstant},
    FileSyncType, UnixIO,
};
use turso_core::{Buffer, Completion, File, OpenFlags, IO};

struct StopIo {
    inner: UnixIO,
    armed: Arc<AtomicBool>,
    phase: String,
    operation: String,
}
struct StopFile {
    inner: Arc<dyn File>,
    armed: Arc<AtomicBool>,
    selected: bool,
    operation: String,
}
impl StopFile {
    fn stop(&self, operation: &str) {
        if self.selected && self.operation == operation && self.armed.load(Ordering::SeqCst) {
            std::process::exit(73);
        }
    }
}
impl File for StopFile {
    fn lock_file(&self, exclusive: bool) -> turso_core::Result<()> {
        self.inner.lock_file(exclusive)
    }
    fn unlock_file(&self) -> turso_core::Result<()> {
        self.inner.unlock_file()
    }
    fn pread(&self, pos: u64, c: Completion) -> turso_core::Result<Completion> {
        self.inner.pread(pos, c)
    }
    fn pwrite(&self, pos: u64, b: Arc<Buffer>, c: Completion) -> turso_core::Result<Completion> {
        self.stop("write");
        self.inner.pwrite(pos, b, c)
    }
    fn sync(&self, c: Completion, kind: FileSyncType) -> turso_core::Result<Completion> {
        self.stop("sync");
        self.inner.sync(c, kind)
    }
    fn size(&self) -> turso_core::Result<u64> {
        self.inner.size()
    }
    fn truncate(&self, len: u64, c: Completion) -> turso_core::Result<Completion> {
        self.inner.truncate(len, c)
    }
}
impl Clock for StopIo {
    fn current_time_monotonic(&self) -> MonotonicInstant {
        self.inner.current_time_monotonic()
    }
    fn current_time_wall_clock(&self) -> WallClockInstant {
        self.inner.current_time_wall_clock()
    }
}
impl IO for StopIo {
    fn open_file(
        &self,
        path: &str,
        flags: OpenFlags,
        direct: bool,
    ) -> turso_core::Result<Arc<dyn File>> {
        Ok(Arc::new(StopFile {
            inner: self.inner.open_file(path, flags, direct)?,
            armed: self.armed.clone(),
            selected: if self.phase == "commit" {
                path.ends_with("-wal")
            } else {
                path.ends_with("app.db")
            },
            operation: self.operation.clone(),
        }))
    }
    fn remove_file(&self, path: &str) -> turso_core::Result<()> {
        self.inner.remove_file(path)
    }
}
fn q(c: &crate::Connection, sql: &str) -> crate::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn io_stop_child() {
    let Ok(path) = std::env::var("FASTDB_IO_STOP_PATH") else {
        return;
    };
    let phase = std::env::var("FASTDB_IO_STOP_PHASE").unwrap();
    let armed = Arc::new(AtomicBool::new(false));
    let io = Arc::new(StopIo {
        inner: UnixIO::new().unwrap(),
        armed: armed.clone(),
        phase: phase.clone(),
        operation: std::env::var("FASTDB_IO_STOP_OP").unwrap(),
    });
    let db = Database {
        engine: turso_core::Database::open_file(io, &path).unwrap(),
    };
    let c = db.connect().unwrap();
    q(&c, "PRAGMA synchronous=FULL");
    q(&c, "BEGIN");
    q(&c, "UPDATE items SET n=2");
    q(&c, "INSERT INTO events VALUES(2)");
    if phase == "commit" {
        armed.store(true, Ordering::SeqCst);
    }
    q(&c, "COMMIT");
    if phase == "checkpoint" {
        armed.store(true, Ordering::SeqCst);
    }
    q(&c, "PRAGMA wal_checkpoint(TRUNCATE)");
    panic!("selected I/O boundary was not reached");
}
#[test]
fn commit_checkpoint_io_boundaries_recover_atomic_state() {
    for phase in ["commit", "checkpoint"] {
        for operation in ["write", "sync"] {
            let dir = std::env::temp_dir().join(format!(
                "fastdb-io-stop-{}-{phase}-{operation}",
                std::process::id()
            ));
            std::fs::create_dir(&dir).unwrap();
            let path = dir.join("app.db");
            {
                let db = Database::open(path.to_str().unwrap()).unwrap();
                let c = db.connect().unwrap();
                q(&c, "CREATE TABLE items");
                q(&c, "CREATE UNIQUE INDEX item_n ON items(n)");
                q(&c, "INSERT INTO items {id:items:a,n:1}");
                q(&c, "CREATE TABLE events(n INTEGER PRIMARY KEY)");
                q(&c, "INSERT INTO events VALUES(1)");
                q(&c, "PRAGMA wal_checkpoint(TRUNCATE)");
            }
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "recovery_io::io_stop_child", "--nocapture"])
                .env("FASTDB_IO_STOP_PATH", &path)
                .env("FASTDB_IO_STOP_PHASE", phase)
                .env("FASTDB_IO_STOP_OP", operation)
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("I/O child timeout");
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            assert_eq!(
                status.code(),
                Some(73),
                "{phase}/{operation}: boundary must be reached"
            );
            for _ in 0..2 {
                let db = Database::open(path.to_str().unwrap()).unwrap();
                let c = db.connect().unwrap();
                let n = q(&c, "SELECT n FROM items").rows[0][0].clone();
                assert!(n == Value::Integer(1) || n == Value::Integer(2));
                if phase == "checkpoint" {
                    assert_eq!(n, Value::Integer(2));
                }
                assert_eq!(q(&c, "SELECT count(*) FROM events").rows, vec![vec![n]]);
                c.check_collection_integrity("items", crate::IntegrityLimits::default())
                    .unwrap();
                assert_eq!(
                    q(&c, "PRAGMA integrity_check").rows,
                    vec![vec![Value::String("ok".into())]]
                );
            }
            std::fs::remove_dir_all(dir).unwrap();
        }
    }
}
