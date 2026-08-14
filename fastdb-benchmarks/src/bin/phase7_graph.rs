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
use turso_fastdb::catalog::{GraphColumnRole, HiddenColumnRole, TableKind};
use turso_fastdb::names::encode_rid;
use turso_fastdb::{Database, RecordId, StatementResult, Value};

const ENGINE_SHA: &str = "977383ff40edc44ef410af062ed0d2322252a869";

fn main() {
    let (samples, edges, output) = arguments();
    let directory = tempdir().unwrap();
    let path = directory.path().join("graph.fastdb");
    let native_path = directory.path().join("native.db");
    let database = Database::open(path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE person:hub CONTENT {}; DEFINE TABLE links TYPE RELATION FROM person TO post",
        )
        .unwrap();
    for index in 0..edges {
        connection
            .execute(&format!(
                "CREATE post:r{index} SET n={index} RETURN NONE; \
                 RELATE person:hub->links->post:r{index} RETURN NONE"
            ))
            .unwrap();
    }

    let state = connection.catalog_state().unwrap();
    let snapshot = state.snapshot().unwrap();
    let relation = &snapshot.tables["links"];
    assert_eq!(relation.kind, TableKind::Relation);
    let hidden = GraphColumnRole::ALL.map(|role| {
        snapshot
            .hidden_columns
            .values()
            .find(|column| {
                column.table_id == relation.id && column.role == HiddenColumnRole::Graph(role)
            })
            .unwrap()
    });
    let person_id = snapshot.tables["person"].id.to_hex();
    let post_id = snapshot.tables["post"].id.to_hex();
    let query = format!(
        "SELECT {out_rid} FROM {table} WHERE {in_table}=?1 AND {in_rid}=?2 AND {out_table}=?3",
        out_rid = hidden[3].physical_name,
        table = relation.physical_name,
        in_table = hidden[0].physical_name,
        in_rid = hidden[1].physical_name,
        out_table = hidden[2].physical_name,
    );
    drop(state);
    let mut native = connection.native().prepare(&query).unwrap();

    let source = "SELECT ->links->post AS ids FROM person:hub";
    for _ in 0..20 {
        black_box(connection.execute(source).unwrap());
        black_box(run_native(&mut native, &person_id, &post_id));
    }
    let mut fastdb_samples = Vec::with_capacity(samples);
    let mut native_samples = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let response = connection.execute(source).unwrap();
        assert_eq!(projected_count(&response.statements[0]), edges);
        black_box(response);
        fastdb_samples.push(elapsed_ns(start));

        let start = Instant::now();
        let response = run_native(&mut native, &person_id, &post_id);
        assert_eq!(projected_count(&response), edges);
        black_box(response);
        native_samples.push(elapsed_ns(start));
    }
    let fastdb_p95 = percentile(&fastdb_samples, 95);
    let native_p95 = percentile(&native_samples, 95);
    drop(native);
    connection.close().unwrap();
    seed_native_storage(native_path.to_str().unwrap(), edges);
    let fastdb_bytes = std::fs::metadata(&path).unwrap().len();
    let native_bytes = std::fs::metadata(&native_path).unwrap().len();
    let storage_ratio = fastdb_bytes as f64 / native_bytes.max(1) as f64;
    let result = json!({
        "engine_sha": ENGINE_SHA,
        "samples": samples,
        "edges": edges,
        "fastdb": {"samples_ns": fastdb_samples, "p95_ns": fastdb_p95},
        "native": {"samples_ns": native_samples, "p95_ns": native_p95},
        "p95_ratio": fastdb_p95 as f64 / native_p95.max(1) as f64,
        "storage": {
            "fastdb_bytes": fastdb_bytes,
            "native_bytes": native_bytes,
            "ratio": storage_ratio,
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

fn seed_native_storage(path: &str, edges: usize) {
    let connection = native_open(path);
    for ddl in [
        "CREATE TABLE person(rid TEXT PRIMARY KEY,doc BLOB NOT NULL) STRICT",
        "CREATE TABLE post(rid TEXT PRIMARY KEY,doc BLOB NOT NULL) STRICT",
        "CREATE TABLE links(rid TEXT PRIMARY KEY,doc BLOB NOT NULL,in_table TEXT NOT NULL,in_rid TEXT NOT NULL,out_table TEXT NOT NULL,out_rid TEXT NOT NULL) STRICT",
        "CREATE INDEX links_forward ON links(in_table,in_rid,out_table,out_rid)",
        "CREATE INDEX links_reverse ON links(out_table,out_rid,in_table,in_rid)",
    ] {
        connection.prepare(ddl).unwrap().run_ignore_rows().unwrap();
    }
    connection
        .prepare("BEGIN IMMEDIATE")
        .unwrap()
        .run_ignore_rows()
        .unwrap();
    connection
        .prepare("INSERT INTO person VALUES('v1:s:3:hub',jsonb('{}'))")
        .unwrap()
        .run_ignore_rows()
        .unwrap();
    let mut post = connection
        .prepare("INSERT INTO post VALUES(?1,jsonb(json_object('n',?2)))")
        .unwrap();
    let mut edge = connection
        .prepare("INSERT INTO links VALUES(?1,jsonb('{}'),?2,'v1:s:3:hub',?3,?4)")
        .unwrap();
    let person_table = "00000000000000000000000000000001";
    let post_table = "00000000000000000000000000000002";
    for index in 0..edges {
        let post_rid = encode_rid(format!("r{index}")).expect("benchmark RID must encode");
        bind_text(&mut post, 1, &post_rid);
        post.bind_at(
            NonZeroUsize::new(2).unwrap(),
            EngineValue::from_i64(i64::try_from(index).unwrap()),
        )
        .unwrap();
        post.run_ignore_rows().unwrap();
        post.reset().unwrap();
        post.clear_bindings();

        bind_text(
            &mut edge,
            1,
            &encode_rid(format!("e{index}")).expect("benchmark RID must encode"),
        );
        bind_text(&mut edge, 2, person_table);
        bind_text(&mut edge, 3, post_table);
        bind_text(&mut edge, 4, &post_rid);
        edge.run_ignore_rows().unwrap();
        edge.reset().unwrap();
        edge.clear_bindings();
    }
    connection
        .prepare("COMMIT")
        .unwrap()
        .run_ignore_rows()
        .unwrap();
    drop(post);
    drop(edge);
    connection.close().unwrap();
}

fn native_open(path: &str) -> Arc<turso_core::Connection> {
    let io = EngineDatabase::io_for_path(path).unwrap();
    let flags = OpenFlags::default();
    let file = io.open_file(path, flags, true).unwrap();
    let database_file = Arc::new(DatabaseFile::new(file));
    let options = OpenOptions::new(Arc::new(SqliteDialect))
        .storage(database_file)
        .flags(flags)
        .db_opts(DatabaseOpts::default());
    EngineDatabase::open(io, path, options)
        .unwrap()
        .connect()
        .unwrap()
}

fn arguments() -> (usize, usize, Option<PathBuf>) {
    let mut samples = 100;
    let mut edges = 200;
    let mut output = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--samples" => samples = args.next().unwrap().parse().unwrap(),
            "--edges" => edges = args.next().unwrap().parse().unwrap(),
            "--output" => output = Some(PathBuf::from(args.next().unwrap())),
            _ => panic!("unknown argument {arg:?}"),
        }
    }
    assert!(samples > 0 && edges > 0);
    (samples, edges, output)
}

fn run_native(
    statement: &mut turso_core::Statement,
    person_id: &str,
    post_id: &str,
) -> StatementResult {
    bind_text(statement, 1, person_id);
    bind_text(
        statement,
        2,
        &encode_rid("hub").expect("benchmark RID must encode"),
    );
    bind_text(statement, 3, post_id);
    let mut values = Vec::new();
    statement
        .run_with_row_callback(|row| {
            let encoded = row.get::<String>(0)?;
            let id = turso_fastdb::names::decode_rid(&encoded).unwrap();
            values.push(Value::RecordId(RecordId::new("post", id)));
            Ok(())
        })
        .unwrap();
    statement.reset().unwrap();
    statement.clear_bindings();
    StatementResult::Rows(vec![Value::Object(std::collections::BTreeMap::from([(
        "ids".into(),
        Value::Array(values),
    )]))])
}

fn projected_count(result: &StatementResult) -> usize {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    let Value::Object(row) = &rows[0] else {
        panic!("expected object")
    };
    let Value::Array(values) = &row["ids"] else {
        panic!("expected array")
    };
    values.len()
}

fn bind_text(statement: &mut turso_core::Statement, index: usize, value: &str) {
    statement
        .bind_at(
            NonZeroUsize::new(index).unwrap(),
            EngineValue::build_text(value.to_string()),
        )
        .unwrap();
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
