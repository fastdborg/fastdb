use crate::common::{compute_dbhash, do_flush, maybe_setup_tracing, TempDatabase};
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use turso_core::{Connection, LimboError, Result};

#[allow(clippy::arc_with_non_send_sync)]
#[turso_macros::test]
fn test_wal_checkpoint_result(tmp_db: TempDatabase) -> Result<()> {
    maybe_setup_tracing();
    let conn = tmp_db.connect_limbo();
    conn.execute("CREATE TABLE t1 (id text);")?;

    let res = execute_and_get_strings(&conn, "pragma journal_mode;")?;
    assert_eq!(res, vec!["wal"]);

    conn.execute("insert into t1(id) values (1), (2);")?;
    do_flush(&conn, &tmp_db).unwrap();
    conn.execute("select * from t1;")?;
    do_flush(&conn, &tmp_db).unwrap();

    // hash BEFORE checkpoint
    let hash_before = compute_dbhash(&tmp_db);

    // checkpoint result should return > 0 num pages now as database has data
    let res = execute_and_get_ints(&conn, "pragma wal_checkpoint;")?;
    println!("'pragma wal_checkpoint;' returns: {res:?}");
    assert_eq!(res.len(), 3);
    assert_eq!(res[0], 0); // checkpoint successfully
    assert!(res[1] > 0); // num pages in wal
    assert!(res[2] > 0); // num pages checkpointed successfully

    do_flush(&conn, &tmp_db).unwrap();

    // hash AFTER checkpoint - must be identical
    let hash_after = compute_dbhash(&tmp_db);
    assert_eq!(
        hash_before.hash, hash_after.hash,
        "checkpoint changed database content!!!!!!"
    );

    Ok(())
}

#[allow(clippy::arc_with_non_send_sync)]
#[turso_macros::test]
fn test_truncate_checkpoint_not_busy_after_rollback(tmp_db: TempDatabase) -> Result<()> {
    maybe_setup_tracing();
    let conn = tmp_db.connect_limbo();
    conn.execute("CREATE TABLE t(x);")?;
    conn.execute("INSERT INTO t VALUES (randomblob(10 * 4096 + 0));")?;
    conn.execute("INSERT INTO t VALUES (randomblob(10 * 4096 + 1));")?;
    conn.execute("INSERT INTO t VALUES (randomblob(10 * 4096 + 2));")?;

    conn.execute("BEGIN;")?;
    conn.execute("INSERT INTO t VALUES (1);")?;
    conn.execute("ROLLBACK;")?;

    let checkpoint = execute_and_get_ints(&conn, "PRAGMA wal_checkpoint(TRUNCATE);")?;
    assert_eq!(
        checkpoint,
        vec![0, 0, 0],
        "truncate checkpoint should not return busy after rollback"
    );
    Ok(())
}

#[allow(clippy::arc_with_non_send_sync)]
#[turso_macros::test]
fn test_savepoint_rollback_after_cache_spill_preserves_wal_pages(
    tmp_db: TempDatabase,
) -> Result<()> {
    maybe_setup_tracing();
    let conn = tmp_db.connect_limbo();

    conn.execute("PRAGMA page_size=512;")?;
    conn.execute("PRAGMA journal_mode=WAL;")?;
    conn.execute("PRAGMA cache_size=50;")?;

    conn.execute("CREATE TABLE filler(x INTEGER PRIMARY KEY);")?;
    conn.execute("INSERT INTO filler(x) VALUES (1),(2),(3),(4),(5),(6),(7),(8),(9),(10);")?;

    conn.execute("BEGIN;")?;
    conn.execute("CREATE TABLE t(k INTEGER PRIMARY KEY, v TEXT NOT NULL);")?;
    conn.execute("CREATE TABLE b(k INTEGER PRIMARY KEY, v TEXT NOT NULL);")?;
    conn.execute(
        "INSERT INTO t(v)
         SELECT printf('%0*d', 4000, f1.x*1000 + f2.x)
         FROM filler f1, filler f2
         LIMIT 25;",
    )?;
    conn.execute(
        "UPDATE t
         SET v = printf('%0*d', 4000, k + 10000000)
         WHERE k BETWEEN 1 AND 23;",
    )?;
    conn.execute("SAVEPOINT s1;")?;
    conn.execute(
        "INSERT INTO b(v)
         SELECT printf('%0*d', 4000, f1.x*1000 + f2.x)
         FROM filler f1, filler f2
         LIMIT 40;",
    )?;
    conn.execute("ROLLBACK TO s1;")?;
    conn.execute("RELEASE s1;")?;
    conn.execute("COMMIT;")?;

    let res = execute_and_get_strings(&conn, "PRAGMA integrity_check;")?;
    assert_eq!(res, vec!["ok"]);

    Ok(())
}

#[allow(clippy::arc_with_non_send_sync)]
#[turso_macros::test]
fn test_wal_cacheflush_savepoint_rollback_preserves_pre_savepoint_indexed_rows(
    tmp_db: TempDatabase,
) -> Result<()> {
    maybe_setup_tracing();
    let conn = tmp_db.connect_limbo();

    conn.execute("PRAGMA page_size=512;")?;
    conn.execute("PRAGMA journal_mode=WAL;")?;
    conn.execute("PRAGMA cache_size=200;")?;
    conn.execute("PRAGMA data_sync_retry=1;")?;

    conn.execute(
        "CREATE TABLE shy_rain_806 (
            hot_cave_669 BLOB NOT NULL,
            blue_wind_834 TEXT,
            hard_star_751 NUMERIC UNIQUE,
            angry_stone_202 INTEGER,
            slow_sun_632 NUMERIC NOT NULL,
            fast_fish_819 REAL PRIMARY KEY,
            happy_lake_980 REAL NOT NULL,
            brave_fish_935 NUMERIC NOT NULL
        );",
    )?;

    for i in 0..260 {
        conn.execute(format!(
            "INSERT INTO shy_rain_806
             (hot_cave_669, blue_wind_834, hard_star_751, angry_stone_202,
              slow_sun_632, fast_fish_819, happy_lake_980, brave_fish_935)
             VALUES (x'73656564', 'seed_{i}', {}, {}, {}, {:.2}, {:.2}, {});",
            10_000 + i,
            20_000 + i,
            30_000 + i,
            i as f64 + 0.25,
            i as f64 + 0.75,
            40_000 + i,
        ))?;
    }

    conn.execute("BEGIN;")?;
    conn.execute(
        "UPDATE shy_rain_806
         SET hot_cave_669 = x'75706461746564',
             blue_wind_834 = 'updated',
             hard_star_751 = hard_star_751 + 100000,
             angry_stone_202 = angry_stone_202 + 1,
             happy_lake_980 = happy_lake_980 + 1.0
         WHERE fast_fish_819 BETWEEN 180.25 AND 240.25;",
    )?;
    conn.execute("SAVEPOINT sp_55;")?;
    for completion in conn.cacheflush()? {
        tmp_db.io.wait_for_completion(completion)?;
    }
    conn.execute(
        "UPDATE shy_rain_806
         SET blue_wind_834 = 'rolled_back',
             hard_star_751 = hard_star_751 + 50000
         WHERE fast_fish_819 = 204.25;",
    )?;
    conn.execute("ROLLBACK TO sp_55;")?;
    conn.execute("RELEASE sp_55;")?;
    conn.execute("COMMIT;")?;

    let res = execute_and_get_strings(&conn, "PRAGMA integrity_check;")?;
    assert_eq!(res, vec!["ok"]);
    let count = execute_and_get_ints(&conn, "SELECT COUNT(*) FROM shy_rain_806;")?;
    assert_eq!(count, vec![260]);
    let updated = execute_and_get_ints(
        &conn,
        "SELECT COUNT(*) FROM shy_rain_806 WHERE hard_star_751 >= 110000;",
    )?;
    assert_eq!(updated, vec![61]);

    Ok(())
}

#[test]
#[ignore = "ignored for now because it's flaky"]
fn test_wal_1_writer_1_reader() -> Result<()> {
    maybe_setup_tracing();
    let tmp_db = Arc::new(Mutex::new(TempDatabase::new("test_wal.db")));
    let db = tmp_db.lock().unwrap().limbo_database();

    {
        let conn = db.connect().unwrap();
        match conn.query("CREATE TABLE t (id)")? {
            Some(ref mut rows) => {
                rows.run_with_row_callback(|_| Ok(())).unwrap();
            }
            None => todo!(),
        }
        do_flush(&conn, tmp_db.lock().unwrap().deref()).unwrap();
    }
    let rows = Arc::new(std::sync::Mutex::new(0));
    let rows_ = rows.clone();
    const ROWS_WRITE: usize = 1000;
    let tmp_db_w = db.clone();
    let writer_thread = std::thread::spawn(move || {
        let conn = tmp_db_w.connect().unwrap();
        for i in 0..ROWS_WRITE {
            conn.execute(format!("INSERT INTO t values({i})").as_str())
                .unwrap();
            let mut rows = rows_.lock().unwrap();
            *rows += 1;
        }
    });
    let rows_ = rows.clone();
    let reader_thread = std::thread::spawn(move || {
        let conn = db.connect().unwrap();
        loop {
            let rows = *rows_.lock().unwrap();
            let mut i = 0;
            match conn.query("SELECT * FROM t") {
                Ok(Some(ref mut rows)) => {
                    rows.run_with_row_callback(|row| {
                        let id = row.get::<i64>(0).unwrap();
                        assert_eq!(id, i);
                        i += 1;
                        Ok(())
                    })
                    .unwrap();
                }
                Ok(None) => {}
                Err(err) => {
                    eprintln!("{err}");
                }
            }
            if rows == ROWS_WRITE {
                break;
            }
        }
    });

    writer_thread.join().unwrap();
    reader_thread.join().unwrap();
    Ok(())
}

/// Execute a statement and get strings result
pub(crate) fn execute_and_get_strings(conn: &Arc<Connection>, sql: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(sql)?;
    let mut result = Vec::new();

    stmt.run_with_row_callback(|row| {
        for el in row.get_values() {
            result.push(format!("{el}"));
        }
        Ok(())
    })?;
    Ok(result)
}

/// Execute a statement and get integers
pub(crate) fn execute_and_get_ints(conn: &Arc<Connection>, sql: &str) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare(sql)?;
    let mut result = Vec::new();

    stmt.run_with_row_callback(|row| {
        for value in row.get_values() {
            let out = match value {
                turso_core::Value::Numeric(turso_core::Numeric::Integer(i)) => i,
                _ => {
                    return Err(LimboError::ConversionError(format!(
                        "cannot convert {value} to int"
                    )))
                }
            };
            result.push(*out);
        }
        Ok(())
    })?;

    Ok(result)
}

#[test]
fn test_wal_read_lock_released_on_conn_drop() {
    maybe_setup_tracing();
    let tmp_db = TempDatabase::new("test_wal_read_lock_released.db");
    let db = tmp_db.limbo_database();

    // Setup: create table and insert data so WAL has content
    let setup_conn = db.connect().unwrap();
    setup_conn
        .execute("CREATE TABLE t (id integer primary key)")
        .unwrap();
    setup_conn.execute("INSERT INTO t VALUES (1)").unwrap();

    let conn1 = db.connect().unwrap();
    let conn2 = db.connect().unwrap();

    // conn1 starts a read transaction and panics while holding the read lock
    let join_result = std::thread::spawn(move || {
        conn1.execute("BEGIN").unwrap();
        conn1.execute("SELECT * FROM t").unwrap();
        panic!("intentional panic while holding read tx");
    })
    .join();
    assert!(join_result.is_err(), "conn1 thread should panic");

    // TRUNCATE checkpoint requires that there be no readers - this would hang/fail if read lock wasn't released
    let res = conn2.pragma_update("wal_checkpoint", "TRUNCATE").unwrap();
    let row = res.first().unwrap();
    let truncate_succeeded = row.first().unwrap() == &turso_core::Value::from_i64(0)
        && row.get(1).unwrap() == &turso_core::Value::from_i64(0)
        && row.get(2).unwrap() == &turso_core::Value::from_i64(0);

    // Expect full truncate, i.e. 0 0 0 result.
    assert!(
        truncate_succeeded,
        "truncate should have succeeded, got checkpoint result: {res:?}"
    );
}

#[test]
fn test_wal_write_lock_released_on_conn_drop() {
    maybe_setup_tracing();
    let tmp_db = TempDatabase::new("test_wal_write_lock_released.db");
    let db = tmp_db.limbo_database();

    let conn1 = db.connect().unwrap();
    let conn2 = db.connect().unwrap();

    let join_result = std::thread::spawn(move || {
        conn1.execute("BEGIN IMMEDIATE").unwrap();
        panic!("intentional panic while holding write tx");
    })
    .join();
    assert!(join_result.is_err(), "conn1 thread should panic");

    conn2.set_busy_handler(Some(Box::new(move |_| {
        panic!("Got busy, this should not happen");
    })));

    conn2
        .execute("CREATE TABLE t (id integer primary key)")
        .unwrap();
}

mod prepared_commit_sync {
    // Read-only engine API probe: records whether a FULL commit syncs its WAL writes.
    use std::sync::{Arc, Mutex};
    use turso_core::io::clock::{MonotonicInstant, WallClockInstant};
    use turso_core::io::{FileId, FileSyncType};
    use turso_core::{Buffer, Clock, Completion, File, MemoryIO, OpenFlags, IO};
    struct TracedIo {
        inner: MemoryIO,
        events: Arc<Mutex<Vec<&'static str>>>,
    }
    struct TracedFile {
        inner: Arc<dyn File>,
        wal: bool,
        events: Arc<Mutex<Vec<&'static str>>>,
    }
    impl Clock for TracedIo {
        fn current_time_monotonic(&self) -> MonotonicInstant {
            self.inner.current_time_monotonic()
        }
        fn current_time_wall_clock(&self) -> WallClockInstant {
            self.inner.current_time_wall_clock()
        }
    }
    impl IO for TracedIo {
        fn open_file(
            &self,
            path: &str,
            flags: OpenFlags,
            direct: bool,
        ) -> turso_core::Result<Arc<dyn File>> {
            Ok(Arc::new(TracedFile {
                inner: self.inner.open_file(path, flags, direct)?,
                wal: path.ends_with("-wal"),
                events: self.events.clone(),
            }))
        }
        fn remove_file(&self, path: &str) -> turso_core::Result<()> {
            self.inner.remove_file(path)
        }
        fn file_id(&self, path: &str) -> turso_core::Result<FileId> {
            self.inner.file_id(path)
        }
    }
    impl File for TracedFile {
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
            b: Arc<Buffer>,
            c: Completion,
        ) -> turso_core::Result<Completion> {
            if self.wal {
                self.events.lock().unwrap().push("write");
            }
            self.inner.pwrite(pos, b, c)
        }
        fn sync(&self, c: Completion, kind: FileSyncType) -> turso_core::Result<Completion> {
            if self.wal {
                self.events.lock().unwrap().push("sync");
            }
            self.inner.sync(c, kind)
        }
        fn size(&self) -> turso_core::Result<u64> {
            self.inner.size()
        }
        fn truncate(&self, length: u64, c: Completion) -> turso_core::Result<Completion> {
            self.inner.truncate(length, c)
        }
    }
    #[test]
    fn full_commit_syncs_prepared_frames_after_reopen() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let io = Arc::new(TracedIo {
            inner: MemoryIO::new(),
            events: events.clone(),
        });
        {
            let db = turso_core::Database::open_file(io.clone(), "probe.db").unwrap();
            let c = db.connect().unwrap();
            c.execute("CREATE TABLE items(n INTEGER)").unwrap();
            c.execute("INSERT INTO items VALUES(1)").unwrap();
            c.execute("PRAGMA wal_checkpoint(TRUNCATE)").unwrap();
        }
        let db = turso_core::Database::open_file(io, "probe.db").unwrap();
        let c = db.connect().unwrap();
        c.execute("PRAGMA synchronous=FULL").unwrap();
        c.execute("PRAGMA wal_autocheckpoint=0").unwrap();
        let mut failures = 0;
        for value in 2..=4 {
            events.lock().unwrap().clear();
            c.execute(format!("INSERT INTO items VALUES({value})"))
                .unwrap();
            let trace = events.lock().unwrap().clone();
            println!(
                "commit {value}, mode {:?}, WAL events: {trace:?}",
                c.get_sync_mode()
            );
            if trace.last() != Some(&"sync") {
                failures += 1;
            }
        }
        assert_eq!(
            failures, 0,
            "FULL must sync after the commit's final WAL write"
        );
    }
}
