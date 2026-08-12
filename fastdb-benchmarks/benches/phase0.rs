#![allow(dead_code)]
//! P0.10 — FastDB vs native Turso comparison benchmark.
//!
//! Both paths run on the same pinned engine, the same physical schema
//! (`rid TEXT PRIMARY KEY, doc BLOB`), the same JSONB functions
//! (`jsonb(json_object(...))` / `json(doc)` / `json_extract`), bound values,
//! and full-durability file-backed WAL.
//!
//! - FastDB path: the full `Connection::execute` (parse, catalog resolution,
//!   direct-AST lowering, prepare, bind, execute, decode).
//! - Native path: the bare engine via SQL text, with the statement prepared
//!   once and re-bound per iteration — the pure-engine baseline.
//!
//! The ratio therefore isolates the FastDB frontend overhead over the engine.
//! Run with: `cargo bench -p turso_fastdb_benchmarks --bench phase0`.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::num::NonZeroUsize;
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use turso_core::{
    storage::database::DatabaseFile, Database, DatabaseOpts, OpenFlags, OpenOptions, SqliteDialect,
    Value,
};
use turso_fastdb::{Connection as FdbConn, Database as FdbDb};

// --------------------------- helpers ---------------------------

fn fresh_file(dir: &TempDir, name: &str) -> String {
    dir.path().join(name).to_str().unwrap().to_string()
}

fn native_open(path: &str) -> Arc<turso_core::Connection> {
    let io = Database::io_for_path(path).unwrap();
    let flags = OpenFlags::default();
    let file = io.open_file(path, flags, true).unwrap();
    let db_file = Arc::new(DatabaseFile::new(file));
    let opts = OpenOptions::new(Arc::new(SqliteDialect))
        .storage(db_file)
        .flags(flags)
        .db_opts(DatabaseOpts::default());
    let db = Database::open(io, path, opts).unwrap();
    db.connect().unwrap()
}

/// A native (bare-engine) harness over a fixed physical table `t`.
struct Native {
    conn: Arc<turso_core::Connection>,
    insert: turso_core::Statement,
    select_rid: turso_core::Statement,
    select_filter: turso_core::Statement,
    delete: turso_core::Statement,
}

impl Native {
    /// Open, create the physical table, the expression index, and seed `n`
    /// records. Prepared statements are created once.
    fn setup(path: &str, n: usize, with_index: bool) -> Self {
        let conn = native_open(path);
        conn.prepare("CREATE TABLE t (rid TEXT PRIMARY KEY, doc BLOB) STRICT")
            .unwrap()
            .run_ignore_rows()
            .unwrap();
        if with_index {
            conn.prepare("CREATE INDEX ix_name ON t(json_extract(doc, '$.name'))")
                .unwrap()
                .run_ignore_rows()
                .unwrap();
        }
        let mut seed = conn
            .prepare("INSERT INTO t (rid, doc) VALUES (?1, jsonb(json_object('name', ?2)))")
            .unwrap();
        for i in 0..n {
            seed.bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(format!("rid{i}")),
            )
            .unwrap();
            seed.bind_at(
                NonZeroUsize::new(2).unwrap(),
                Value::build_text(format!("rec{i}")),
            )
            .unwrap();
            seed.run_ignore_rows().unwrap();
            seed.reset().unwrap();
        }
        let insert = conn
            .prepare("INSERT INTO t (rid, doc) VALUES (?1, jsonb(json_object('name', ?2)))")
            .unwrap();
        let select_rid = conn
            .prepare("SELECT rid, json(doc) FROM t WHERE rid = ?1")
            .unwrap();
        let select_filter = conn
            .prepare("SELECT rid, json(doc) FROM t WHERE json_extract(doc, '$.name') = ?1")
            .unwrap();
        let delete = conn.prepare("DELETE FROM t WHERE rid = ?1").unwrap();
        Self {
            conn,
            insert,
            select_rid,
            select_filter,
            delete,
        }
    }

    fn create(&mut self, key: &str, name: &str) {
        self.insert
            .bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(key.to_string()),
            )
            .unwrap();
        self.insert
            .bind_at(
                NonZeroUsize::new(2).unwrap(),
                Value::build_text(name.to_string()),
            )
            .unwrap();
        self.insert.run_ignore_rows().unwrap();
        self.insert.reset().unwrap();
    }

    /// Read by rid, materializing the doc (serde_json decode) like FastDB.
    /// Returns the number of decoded fields so the work is real, not a count.
    fn read_by_rid(&mut self, key: &str) -> usize {
        self.select_rid
            .bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(key.to_string()),
            )
            .unwrap();
        let mut fields = 0;
        self.select_rid
            .run_with_row_callback(|row| {
                let _rid = row.get::<String>(0).unwrap_or_default();
                let doc = row.get::<String>(1).unwrap_or_default();
                fields = materialize_field_count(&doc);
                Ok(())
            })
            .unwrap();
        self.select_rid.reset().unwrap();
        fields
    }

    /// Filter by name, materializing each matching doc. Returns the total
    /// decoded field count across matches (not a row count).
    fn filter(&mut self, name: &str) -> usize {
        self.select_filter
            .bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(name.to_string()),
            )
            .unwrap();
        let mut total = 0;
        self.select_filter
            .run_with_row_callback(|row| {
                let _rid = row.get::<String>(0).unwrap_or_default();
                let doc = row.get::<String>(1).unwrap_or_default();
                total += materialize_field_count(&doc);
                Ok(())
            })
            .unwrap();
        self.select_filter.reset().unwrap();
        total
    }

    fn delete(&mut self, key: &str) {
        self.delete
            .bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(key.to_string()),
            )
            .unwrap();
        self.delete.run_ignore_rows().unwrap();
        self.delete.reset().unwrap();
    }
}

/// Parse a `json(doc)` string and return its field count, mirroring the
/// serde_json materialization the FastDB decode path does.
fn materialize_field_count(json: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(json)
        .ok()
        .and_then(|v| v.as_object().map(|o| o.len()))
        .unwrap_or(0)
}

fn fdb_seed(path: &str, n: usize, with_index: bool) -> FdbConn {
    let db = FdbDb::open(path).unwrap();
    let conn = db.connect().unwrap();
    for i in 0..n {
        conn.execute(&format!("CREATE person:rec{i} SET name = 'rec{i}';"))
            .unwrap();
    }
    if with_index {
        conn.create_field_index("person", "name").unwrap();
    }
    conn
}

fn cold_create(c: &mut Criterion) {
    let mut g = c.benchmark_group("cold_create");
    g.bench_function("fastdb", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().unwrap();
                let path = fresh_file(&dir, "c.fastdb");
                let db = FdbDb::open(&path).unwrap();
                (dir, db.connect().unwrap())
            },
            |(_dir, conn)| {
                let r = conn
                    .execute("CREATE person:first SET name = 'Tobie';")
                    .unwrap();
                assert_eq!(r.records.len(), 1);
            },
            BatchSize::SmallInput,
        )
    });
    g.bench_function("native", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().unwrap();
                let path = fresh_file(&dir, "c.fastdb");
                (dir, native_open(&path))
            },
            |(_dir, conn)| {
                conn.prepare("CREATE TABLE t (rid TEXT PRIMARY KEY, doc BLOB) STRICT")
                    .unwrap()
                    .run_ignore_rows()
                    .unwrap();
                let mut s = conn
                    .prepare("INSERT INTO t (rid, doc) VALUES (?1, jsonb(json_object('name', ?2)))")
                    .unwrap();
                s.bind_at(
                    NonZeroUsize::new(1).unwrap(),
                    Value::build_text("rid".to_string()),
                )
                .unwrap();
                s.bind_at(
                    NonZeroUsize::new(2).unwrap(),
                    Value::build_text("Tobie".to_string()),
                )
                .unwrap();
                s.run_ignore_rows().unwrap();
            },
            BatchSize::SmallInput,
        )
    });
    g.finish();
}

fn steady_create(c: &mut Criterion) {
    let mut g = c.benchmark_group("steady_create");
    let dir = tempdir().unwrap();
    let fdb = fdb_seed(&fresh_file(&dir, "s.fastdb"), 1, false);
    let native = Native::setup(&fresh_file(&dir, "sn.fastdb"), 1, false);
    let mut i: u64 = 1_000_000;
    g.bench_function("fastdb", |b| {
        b.iter(|| {
            i += 1;
            let sql = format!("CREATE person:k{i} SET name = 'v{i}';");
            let r = fdb.execute(&sql).unwrap();
            assert_eq!(r.records.len(), 1);
        })
    });
    let mut native = native;
    let mut j: u64 = 1_000_000;
    g.bench_function("native", |b| {
        b.iter(|| {
            j += 1;
            native.create(&format!("rid{j}"), &format!("v{j}"));
        })
    });
    g.finish();
}

fn point_read(c: &mut Criterion) {
    let mut g = c.benchmark_group("point_read");
    let dir = tempdir().unwrap();
    let fdb = fdb_seed(&fresh_file(&dir, "r.fastdb"), 1, false);
    let mut native = Native::setup(&fresh_file(&dir, "rn.fastdb"), 1, false);
    g.bench_function("fastdb", |b| {
        b.iter(|| {
            let r = fdb.execute("SELECT * FROM person:rec0;").unwrap();
            assert_eq!(r.records.len(), 1);
        })
    });
    g.bench_function("native", |b| {
        b.iter(|| {
            let fields = native.read_by_rid("rid0");
            assert_eq!(fields, 1, "decoded the one field");
        })
    });
    g.finish();
}

fn indexed_filter(c: &mut Criterion) {
    let mut g = c.benchmark_group("indexed_filter");
    let dir = tempdir().unwrap();
    let fdb = fdb_seed(&fresh_file(&dir, "f.fastdb"), 50, true);
    let mut native = Native::setup(&fresh_file(&dir, "fn.fastdb"), 50, true);
    g.bench_function("fastdb", |b| {
        b.iter(|| {
            let r = fdb
                .execute("SELECT * FROM person WHERE name = 'rec25';")
                .unwrap();
            assert_eq!(r.records.len(), 1);
        })
    });
    g.bench_function("native", |b| {
        b.iter(|| {
            let n = native.filter("rec25");
            assert_eq!(n, 1);
        })
    });
    g.finish();
}

fn delete_op(c: &mut Criterion) {
    let mut g = c.benchmark_group("delete");
    // Delete-only timing: setup creates the target record (untimed); the
    // routine times only the delete.
    let fdb = fdb_seed(&fresh_file(&tempdir().unwrap(), "d.fastdb"), 0, false);
    let k = std::cell::Cell::new(0u64);
    g.bench_function("fastdb", |b| {
        b.iter_batched(
            || {
                let n = k.get();
                k.set(n + 1);
                fdb.execute(&format!("CREATE person:del{n} SET name = 'del{n}';"))
                    .unwrap();
                n
            },
            |n| {
                let r = fdb.execute(&format!("DELETE person:del{n};")).unwrap();
                assert!(r.records.is_empty());
            },
            BatchSize::SmallInput,
        )
    });
    let native = std::cell::RefCell::new(Native::setup(
        &fresh_file(&tempdir().unwrap(), "dn.fastdb"),
        0,
        false,
    ));
    let j = std::cell::Cell::new(0u64);
    g.bench_function("native", |b| {
        b.iter_batched(
            || {
                let n = j.get();
                j.set(n + 1);
                native.borrow_mut().create(&format!("rid{n}"), "del");
                n
            },
            |n| {
                native.borrow_mut().delete(&format!("rid{n}"));
            },
            BatchSize::SmallInput,
        )
    });
    g.finish();
}

criterion_group!(
    benches,
    cold_create,
    steady_create,
    point_read,
    indexed_filter,
    delete_op
);
criterion_main!(benches);
