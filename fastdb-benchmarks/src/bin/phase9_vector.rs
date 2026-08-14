#![forbid(unsafe_code)]
#![deny(warnings)]

use serde_json::json;
use std::collections::BTreeMap;
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tempfile::tempdir;
use turso_core::{
    storage::database::DatabaseFile, Database as EngineDatabase, DatabaseOpts, OpenFlags,
    OpenOptions, SqliteDialect, Value as EngineValue,
};
use turso_fastdb::{Database, RecordId, StatementResult, Value};

const ENGINE_SHA: &str = "977383ff40edc44ef410af062ed0d2322252a869";

fn main() {
    let (samples, records, output) = arguments();
    let directory = tempdir().unwrap();
    let fastdb_path = directory.path().join("vector.fastdb");
    let native_path = directory.path().join("vector-native.db");

    let database = Database::open(fastdb_path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE point:seed SET embedding = [0,0]; DEFINE FIELD embedding ON point TYPE array<float, 2>; BEGIN")
        .unwrap();
    for index in 0..records {
        let (x, y) = coordinates(index);
        connection
            .execute(&format!(
                "CREATE point:r{index} SET embedding = [{x},{y}] RETURN NONE"
            ))
            .unwrap();
    }
    connection.execute("COMMIT").unwrap();

    let native = native_open(native_path.to_str().unwrap());
    native
        .prepare("CREATE TABLE point(rid TEXT PRIMARY KEY,doc BLOB NOT NULL,embedding BLOB) STRICT")
        .unwrap()
        .run_ignore_rows()
        .unwrap();
    native
        .prepare("BEGIN IMMEDIATE")
        .unwrap()
        .run_ignore_rows()
        .unwrap();
    let mut insert = native
        .prepare("INSERT INTO point VALUES(?1,jsonb(?2),?3)")
        .unwrap();
    insert_native(&mut insert, "seed", 0.0, 0.0);
    for index in 0..records {
        let (x, y) = coordinates(index);
        insert_native(&mut insert, &format!("r{index}"), x, y);
    }
    native.prepare("COMMIT").unwrap().run_ignore_rows().unwrap();
    drop(insert);

    let query_blob = vector64(1.0, -1.0);
    let mut native_query = native
        .prepare(
            "SELECT rid,vector_distance_l2(embedding,?1) AS distance FROM point \
             WHERE embedding IS NOT NULL ORDER BY vector_distance_l2(embedding,?1),rid LIMIT 10",
        )
        .unwrap();
    let fastdb_source = "SELECT id, vector::distance::knn() AS distance FROM point \
         WHERE embedding <|10,EUCLIDEAN|> [1,-1]";
    for _ in 0..10 {
        black_box(connection.execute(fastdb_source).unwrap());
        black_box(run_native(&mut native_query, &query_blob));
    }
    let mut fastdb_samples = Vec::with_capacity(samples);
    let mut native_samples = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let response = connection.execute(fastdb_source).unwrap();
        assert!(
            matches!(&response.statements[0], StatementResult::Rows(rows) if rows.len() == 10.min(records + 1))
        );
        black_box(response);
        fastdb_samples.push(elapsed_ns(start));

        let start = Instant::now();
        assert_eq!(
            run_native(&mut native_query, &query_blob),
            10.min(records + 1)
        );
        native_samples.push(elapsed_ns(start));
    }
    let fastdb_p95 = percentile(&fastdb_samples, 95);
    let native_p95 = percentile(&native_samples, 95);
    drop(native_query);
    connection.close().unwrap();
    native.close().unwrap();
    let fastdb_bytes = std::fs::metadata(&fastdb_path).unwrap().len();
    let native_bytes = std::fs::metadata(&native_path).unwrap().len();
    let result = json!({
        "engine_sha": ENGINE_SHA,
        "samples": samples,
        "records": records + 1,
        "k": 10,
        "fastdb": {"samples_ns": fastdb_samples, "p95_ns": fastdb_p95},
        "native": {"samples_ns": native_samples, "p95_ns": native_p95},
        "p95_ratio": fastdb_p95 as f64 / native_p95.max(1) as f64,
        "storage": {
            "fastdb_bytes": fastdb_bytes,
            "native_bytes": native_bytes,
            "ratio": fastdb_bytes as f64 / native_bytes.max(1) as f64,
        },
        "provisional_phase12_ceiling": 2.0,
    });
    let encoded = serde_json::to_vec_pretty(&result).unwrap();
    if let Some(output) = output {
        std::fs::write(output, encoded).unwrap();
    } else {
        println!("{}", String::from_utf8(encoded).unwrap());
    }
}

fn coordinates(index: usize) -> (f64, f64) {
    ((index % 101) as f64 / 10.0, (index % 97) as f64 / -10.0)
}

fn vector64(x: f64, y: f64) -> Vec<u8> {
    let mut value = Vec::with_capacity(17);
    value.extend_from_slice(&x.to_le_bytes());
    value.extend_from_slice(&y.to_le_bytes());
    value.push(2);
    value
}

fn native_open(path: &str) -> Arc<turso_core::Connection> {
    let io = EngineDatabase::io_for_path(path).unwrap();
    let flags = OpenFlags::default();
    let file = io.open_file(path, flags, true).unwrap();
    let options = OpenOptions::new(Arc::new(SqliteDialect))
        .storage(Arc::new(DatabaseFile::new(file)))
        .flags(flags)
        .db_opts(DatabaseOpts::default().with_index_method(true));
    EngineDatabase::open(io, path, options)
        .unwrap()
        .connect()
        .unwrap()
}

fn insert_native(statement: &mut turso_core::Statement, rid: &str, x: f64, y: f64) {
    bind(statement, 1, EngineValue::build_text(rid.to_owned()));
    bind(
        statement,
        2,
        EngineValue::build_text(format!("{{\"embedding\":[{x},{y}]}}")),
    );
    bind(statement, 3, EngineValue::from_blob(vector64(x, y)));
    statement.run_ignore_rows().unwrap();
    statement.reset().unwrap();
    statement.clear_bindings();
}

fn run_native(statement: &mut turso_core::Statement, query: &[u8]) -> usize {
    bind(statement, 1, EngineValue::from_blob(query.to_vec()));
    let mut rows = Vec::new();
    statement
        .run_with_row_callback(|row| {
            rows.push(Value::Object(BTreeMap::from([
                (
                    "id".to_string(),
                    Value::RecordId(RecordId::new("point", row.get::<String>(0)?)),
                ),
                ("distance".to_string(), Value::Float(row.get::<f64>(1)?)),
            ])));
            Ok(())
        })
        .unwrap();
    statement.reset().unwrap();
    statement.clear_bindings();
    black_box(rows).len()
}

fn bind(statement: &mut turso_core::Statement, index: usize, value: EngineValue) {
    statement
        .bind_at(NonZeroUsize::new(index).unwrap(), value)
        .unwrap();
}

fn arguments() -> (usize, usize, Option<PathBuf>) {
    let mut samples = 100;
    let mut records = 1_000;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--samples" => samples = args.next().unwrap().parse().unwrap(),
            "--records" => records = args.next().unwrap().parse().unwrap(),
            "--output" => output = Some(PathBuf::from(args.next().unwrap())),
            _ => panic!("unknown argument {arg:?}"),
        }
    }
    assert!(samples > 0 && records > 0);
    (samples, records, output)
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

fn percentile(samples: &[u64], percent: usize) -> u64 {
    let mut samples = samples.to_vec();
    samples.sort_unstable();
    let rank = (samples.len() * percent).div_ceil(100).saturating_sub(1);
    samples[rank.min(samples.len() - 1)]
}
