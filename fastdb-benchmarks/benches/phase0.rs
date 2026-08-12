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
//!   once and re-bound per iteration. It uses the same canonical rid codec,
//!   document decoder, and public result types as FastDB.
//!
//! The ratio therefore isolates the FastDB frontend overhead over the engine.
//! Run with: `cargo bench -p turso_fastdb_benchmarks --bench phase0`.

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::sync::Arc;
use tempfile::{tempdir, TempDir};
use turso_core::{
    storage::database::DatabaseFile, Database, DatabaseOpts, OpenFlags, OpenOptions, SqliteDialect,
    Value,
};
use turso_fastdb::names::{decode_rid, encode_rid};
use turso_fastdb::{
    parse_doc, Connection as FdbConn, Database as FdbDb, ExecutionResult, Record, RecordId,
    RecordIdValue, Value as FdbValue,
};

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
                Value::build_text(encode_rid(format!("rec{i}"))),
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

    fn create(&mut self, id: &str, name: &str) -> Record {
        let encoded_rid = encode_rid(id);
        self.insert
            .bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(encoded_rid),
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
        Record::new(RecordId::new("person", id)).with_field("name", FdbValue::Str(name.to_string()))
    }

    /// Read by rid and construct the same typed record as FastDB.
    fn read_by_rid(&mut self, id: &str) -> Option<Record> {
        self.select_rid
            .bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(encode_rid(id)),
            )
            .unwrap();
        let mut record = None;
        self.select_rid
            .run_with_row_callback(|row| {
                let rid = row.get::<String>(0).unwrap_or_default();
                let doc = row.get::<String>(1).unwrap_or_default();
                record = Some(materialize_record(&rid, &doc));
                Ok(())
            })
            .unwrap();
        self.select_rid.reset().unwrap();
        record
    }

    /// Filter by name, materializing the same typed records as FastDB.
    fn filter(&mut self, name: &str) -> Vec<Record> {
        self.select_filter
            .bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(name.to_string()),
            )
            .unwrap();
        let mut records = Vec::new();
        self.select_filter
            .run_with_row_callback(|row| {
                let rid = row.get::<String>(0).unwrap_or_default();
                let doc = row.get::<String>(1).unwrap_or_default();
                records.push(materialize_record(&rid, &doc));
                Ok(())
            })
            .unwrap();
        self.select_filter.reset().unwrap();
        records
    }

    fn delete(&mut self, id: &str) -> ExecutionResult {
        self.delete
            .bind_at(
                NonZeroUsize::new(1).unwrap(),
                Value::build_text(encode_rid(id)),
            )
            .unwrap();
        self.delete.run_ignore_rows().unwrap();
        self.delete.reset().unwrap();
        ExecutionResult {
            records: Vec::new(),
        }
    }
}

/// Decode the canonical rid and JSON document into the public FastDB result
/// types, matching the frontend's result materialization.
fn materialize_record(rid: &str, json: &str) -> Record {
    Record {
        id: RecordId::new("person", decode_rid(rid).unwrap()),
        fields: parse_doc(json).unwrap(),
    }
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
                    .execute("CREATE person:first SET name = 'Tracy';")
                    .unwrap();
                assert_eq!(r.records.len(), 1);
                black_box(r);
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
                    Value::build_text(encode_rid("first")),
                )
                .unwrap();
                s.bind_at(
                    NonZeroUsize::new(2).unwrap(),
                    Value::build_text("Tracy".to_string()),
                )
                .unwrap();
                s.run_ignore_rows().unwrap();
                let record = Record::new(RecordId::new("person", "first"))
                    .with_field("name", FdbValue::Str("Tracy".to_string()));
                assert_eq!(record.id.id, "first");
                black_box(record);
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
            black_box(r);
        })
    });
    let mut native = native;
    let mut j: u64 = 1_000_000;
    g.bench_function("native", |b| {
        b.iter(|| {
            j += 1;
            let record = native.create(&format!("k{j}"), &format!("v{j}"));
            assert_eq!(record.id.id, RecordIdValue::String(format!("k{j}")));
            black_box(record);
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
            black_box(r);
        })
    });
    g.bench_function("native", |b| {
        b.iter(|| {
            let record = native.read_by_rid("rec0").expect("seed record");
            assert_eq!(record.fields.len(), 1);
            black_box(record);
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
            black_box(r);
        })
    });
    g.bench_function("native", |b| {
        b.iter(|| {
            let records = native.filter("rec25");
            assert_eq!(records.len(), 1);
            black_box(records);
        })
    });
    g.finish();
}

fn delete_op(c: &mut Criterion) {
    let mut g = c.benchmark_group("delete");
    // Delete-only timing: setup creates the target record (untimed); the
    // routine times only the delete.
    let fdb_dir = tempdir().unwrap();
    let fdb = fdb_seed(&fresh_file(&fdb_dir, "d.fastdb"), 0, false);
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
                black_box(r);
            },
            BatchSize::SmallInput,
        )
    });
    let native_dir = tempdir().unwrap();
    let native = std::cell::RefCell::new(Native::setup(
        &fresh_file(&native_dir, "dn.fastdb"),
        0,
        false,
    ));
    let j = std::cell::Cell::new(0u64);
    g.bench_function("native", |b| {
        b.iter_batched(
            || {
                let n = j.get();
                j.set(n + 1);
                let record = native.borrow_mut().create(&format!("del{n}"), "del");
                black_box(record);
                n
            },
            |n| {
                let result = native.borrow_mut().delete(&format!("del{n}"));
                assert!(result.records.is_empty());
                black_box(result);
            },
            BatchSize::SmallInput,
        )
    });

    let last_fdb = k.get().checked_sub(1).unwrap();
    assert!(
        fdb.execute(&format!("SELECT * FROM person:del{last_fdb};"))
            .unwrap()
            .records
            .is_empty(),
        "last FastDB delete persisted"
    );
    let last_native = j.get().checked_sub(1).unwrap();
    assert!(
        native
            .borrow_mut()
            .read_by_rid(&format!("del{last_native}"))
            .is_none(),
        "last native delete persisted"
    );
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
