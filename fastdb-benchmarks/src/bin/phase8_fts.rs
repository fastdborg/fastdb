#![forbid(unsafe_code)]
#![deny(warnings)]

use serde_json::json;
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
use turso_fastdb::names::{decode_rid, encode_rid};
use turso_fastdb::{parse_doc, Database, RecordId, StatementResult, Value};

const ENGINE_SHA: &str = "977383ff40edc44ef410af062ed0d2322252a869";

fn main() {
    let (samples, records, output) = arguments();
    let directory = tempdir().unwrap();
    let fastdb_path = directory.path().join("fts.fastdb");
    let native_path = directory.path().join("fts-native.db");

    let database = Database::open(fastdb_path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE doc:seed SET text = 'needle seed'; \
             CREATE INDEX text_idx ON doc USING fts (text) WITH (tokenizer = 'whitespace'); \
             BEGIN",
        )
        .unwrap();
    for index in 0..records {
        let term = if index % 100 == 0 { "needle" } else { "other" };
        connection
            .execute(&format!(
                "CREATE doc:r{index} SET text = '{term} document {index}' RETURN NONE"
            ))
            .unwrap();
    }
    connection.execute("COMMIT").unwrap();

    let native = native_open(native_path.to_str().unwrap());
    native
        .prepare("CREATE TABLE doc(rid TEXT PRIMARY KEY,doc BLOB NOT NULL,body TEXT) STRICT")
        .unwrap()
        .run_ignore_rows()
        .unwrap();
    native
        .prepare("CREATE INDEX text_idx ON doc USING fts(body) WITH(tokenizer='whitespace')")
        .unwrap()
        .run_ignore_rows()
        .unwrap();
    native
        .prepare("BEGIN IMMEDIATE")
        .unwrap()
        .run_ignore_rows()
        .unwrap();
    let mut insert = native
        .prepare("INSERT INTO doc VALUES(?1,jsonb(json_object('text',?2)),?2)")
        .unwrap();
    bind_text(
        &mut insert,
        1,
        &encode_rid("seed").expect("benchmark RID must encode"),
    );
    bind_text(&mut insert, 2, "needle seed");
    insert.run_ignore_rows().unwrap();
    insert.reset().unwrap();
    insert.clear_bindings();
    for index in 0..records {
        let term = if index % 100 == 0 { "needle" } else { "other" };
        bind_text(
            &mut insert,
            1,
            &encode_rid(format!("r{index}")).expect("benchmark RID must encode"),
        );
        bind_text(&mut insert, 2, &format!("{term} document {index}"));
        insert.run_ignore_rows().unwrap();
        insert.reset().unwrap();
        insert.clear_bindings();
    }
    native.prepare("COMMIT").unwrap().run_ignore_rows().unwrap();
    drop(insert);
    let mut native_query = native
        .prepare(
            "SELECT rid,json(doc),fts_score(body,?1) FROM doc \
             WHERE fts_match(body,?1) LIMIT 10001",
        )
        .unwrap();

    let fastdb_source = "SELECT id, text, fts_score(text, 'needle') AS score FROM doc \
         WHERE fts_match(text, 'needle')";
    for _ in 0..10 {
        black_box(connection.execute(fastdb_source).unwrap());
        black_box(run_native(&mut native_query));
    }
    let mut fastdb_samples = Vec::with_capacity(samples);
    let mut native_samples = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let response = connection.execute(fastdb_source).unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected FTS rows")
        };
        assert_eq!(rows.len(), records.div_ceil(100) + 1);
        black_box(response);
        fastdb_samples.push(elapsed_ns(start));

        let start = Instant::now();
        let count = run_native(&mut native_query);
        assert_eq!(count, records.div_ceil(100) + 1);
        black_box(count);
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
        "records": records,
        "matched_rows": records.div_ceil(100) + 1,
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

fn run_native(statement: &mut turso_core::Statement) -> usize {
    bind_text(statement, 1, "needle");
    let mut rows = Vec::new();
    statement
        .run_with_row_callback(|row| {
            let id = decode_rid(&row.get::<String>(0)?).unwrap();
            let mut object = parse_doc(&row.get::<String>(1)?).unwrap();
            object.push(("id".into(), Value::RecordId(RecordId::new("doc", id))));
            object.push(("score".into(), Value::Float(row.get::<f64>(2)?)));
            rows.push(Value::Object(object.into_iter().collect()));
            Ok(())
        })
        .unwrap();
    statement.reset().unwrap();
    statement.clear_bindings();
    black_box(rows).len()
}

fn bind_text(statement: &mut turso_core::Statement, index: usize, value: &str) {
    statement
        .bind_at(
            NonZeroUsize::new(index).unwrap(),
            EngineValue::build_text(value.to_owned()),
        )
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
