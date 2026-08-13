#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{
    params, Builder, Connection as FastConnection, ExecutionSummary, QueryResponse,
    StatementResult, Value as FastValue,
};
use futures::channel::oneshot;
use futures::executor::block_on;
use serde_json::json;
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;
use std::time::Instant;
use tempfile::TempDir;
use turso_core::{
    storage::database::DatabaseFile, Database, DatabaseOpts, OpenFlags, OpenOptions, SqliteDialect,
    Value,
};
use turso_fastdb::names::{decode_rid, encode_rid};
use turso_fastdb::{parse_doc, RecordId};

const ENGINE_SHA: &str = "977383ff40edc44ef410af062ed0d2322252a869";
const DOCUMENT_PAYLOAD_BYTES: usize = 1024;

struct Config {
    samples: usize,
    records: usize,
    output: Option<PathBuf>,
    gate: bool,
}

impl Config {
    fn from_args() -> Result<Self, String> {
        let mut config = Self {
            samples: 200,
            records: 10_000,
            output: None,
            gate: true,
        };
        let mut arguments = std::env::args().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.as_str() {
                "--samples" => {
                    config.samples = arguments
                        .next()
                        .ok_or("--samples requires a positive integer")?
                        .parse()
                        .map_err(|_| "--samples requires a positive integer")?;
                }
                "--records" => {
                    config.records = arguments
                        .next()
                        .ok_or("--records requires a positive integer")?
                        .parse()
                        .map_err(|_| "--records requires a positive integer")?;
                }
                "--output" => {
                    config.output = Some(PathBuf::from(
                        arguments.next().ok_or("--output requires a path")?,
                    ));
                }
                "--no-gate" => config.gate = false,
                _ => return Err(format!("unknown argument {argument:?}")),
            }
        }
        if config.samples == 0 || config.records < 2 {
            return Err("samples must be positive and records must be at least two".into());
        }
        Ok(config)
    }
}

struct Distribution {
    samples_ns: Vec<u64>,
    p50_ns: u64,
    p95_ns: u64,
    p99_ns: u64,
}

impl Distribution {
    fn new(samples_ns: Vec<u64>) -> Self {
        let mut sorted = samples_ns.clone();
        sorted.sort_unstable();
        Self {
            p50_ns: percentile(&sorted, 50),
            p95_ns: percentile(&sorted, 95),
            p99_ns: percentile(&sorted, 99),
            samples_ns,
        }
    }

    fn json(&self) -> serde_json::Value {
        json!({
            "samples_ns": self.samples_ns,
            "p50_ns": self.p50_ns,
            "p95_ns": self.p95_ns,
            "p99_ns": self.p99_ns,
        })
    }
}

fn percentile(sorted: &[u64], percent: usize) -> u64 {
    let rank = (sorted.len() * percent).div_ceil(100).saturating_sub(1);
    sorted[rank.min(sorted.len() - 1)]
}

fn ratio(left: u64, right: u64) -> f64 {
    left as f64 / right.max(1) as f64
}

fn elapsed_ns(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

struct NativeBackend {
    connection: Arc<turso_core::Connection>,
    point: turso_core::Statement,
    filter: turso_core::Statement,
    insert: turso_core::Statement,
}

impl NativeBackend {
    fn seed(path: &str, records: usize) -> Self {
        let connection = native_open(path);
        connection
            .prepare("CREATE TABLE t (rid TEXT PRIMARY KEY, doc BLOB) STRICT")
            .unwrap()
            .run_ignore_rows()
            .unwrap();
        connection
            .prepare("CREATE INDEX ix_score ON t(json_extract(doc, '$.score'))")
            .unwrap()
            .run_ignore_rows()
            .unwrap();
        connection
            .prepare("BEGIN IMMEDIATE")
            .unwrap()
            .run_ignore_rows()
            .unwrap();
        let mut insert = connection
            .prepare(
                "INSERT INTO t (rid, doc) VALUES \
                 (?1, jsonb(json_object('score', ?2, 'name', ?3, \
                                        'active', json(?4), 'payload', ?5)))",
            )
            .unwrap();
        let payload = document_payload();
        for index in 0..records {
            bind_text(
                &mut insert,
                1,
                encode_rid(format!("r{index}")).expect("benchmark RID must encode"),
            );
            bind_integer(&mut insert, 2, i64::try_from(index).unwrap());
            bind_text(&mut insert, 3, format!("person-{index}"));
            bind_text(&mut insert, 4, "true".into());
            bind_text(&mut insert, 5, payload.clone());
            insert.run_ignore_rows().unwrap();
            insert.reset().unwrap();
            insert.clear_bindings();
        }
        connection
            .prepare("COMMIT")
            .unwrap()
            .run_ignore_rows()
            .unwrap();
        let point = connection
            .prepare("SELECT rid, json(doc) FROM t WHERE rid=?1")
            .unwrap();
        let filter = connection
            .prepare("SELECT rid, json(doc) FROM t WHERE json_extract(doc, '$.score')=?1")
            .unwrap();
        let insert = connection
            .prepare(
                "INSERT INTO t (rid, doc) VALUES \
                 (?1, jsonb(json_object('score', ?2, 'name', ?3, \
                                        'active', json(?4), 'payload', ?5)))",
            )
            .unwrap();
        Self {
            connection,
            point,
            filter,
            insert,
        }
    }

    fn point(&mut self, id: &str) -> QueryResponse {
        bind_text(
            &mut self.point,
            1,
            encode_rid(id).expect("benchmark RID must encode"),
        );
        let mut result = None;
        self.point
            .run_with_row_callback(|row| {
                result = Some(materialize(&row.get::<String>(0)?, &row.get::<String>(1)?));
                Ok(())
            })
            .unwrap();
        self.point.reset().unwrap();
        self.point.clear_bindings();
        query_response(result.expect("point benchmark target exists"))
    }

    fn filter(&mut self, score: i64) -> QueryResponse {
        bind_integer(&mut self.filter, 1, score);
        let mut result = None;
        self.filter
            .run_with_row_callback(|row| {
                result = Some(materialize(&row.get::<String>(0)?, &row.get::<String>(1)?));
                Ok(())
            })
            .unwrap();
        self.filter.reset().unwrap();
        self.filter.clear_bindings();
        query_response(result.expect("indexed benchmark target exists"))
    }

    fn insert(&mut self, index: usize) {
        let id = uuid::Uuid::now_v7();
        bind_text(
            &mut self.insert,
            1,
            encode_rid(id).expect("benchmark RID must encode"),
        );
        bind_integer(&mut self.insert, 2, i64::try_from(index).unwrap());
        bind_text(&mut self.insert, 3, format!("person-{index}"));
        bind_text(&mut self.insert, 4, "true".into());
        bind_text(&mut self.insert, 5, document_payload());
        self.insert.run_ignore_rows().unwrap();
        self.insert.reset().unwrap();
        self.insert.clear_bindings();
    }

    fn close(self) {
        drop(self.point);
        drop(self.filter);
        drop(self.insert);
        self.connection.close().unwrap();
    }
}

struct NativeClient {
    sender: mpsc::Sender<NativeRequest>,
    join: JoinHandle<()>,
}

enum NativeRequest {
    Point {
        id: String,
        reply: oneshot::Sender<QueryResponse>,
    },
    Filter {
        score: i64,
        reply: oneshot::Sender<QueryResponse>,
    },
    Insert {
        index: usize,
        reply: oneshot::Sender<ExecutionSummary>,
    },
    Close {
        reply: oneshot::Sender<()>,
    },
}

impl NativeClient {
    fn seed(path: String, records: usize) -> Self {
        let (sender, receiver) = mpsc::channel();
        let join = std::thread::Builder::new()
            .name("fastdb-benchmark-native".into())
            .spawn(move || {
                let mut backend = NativeBackend::seed(&path, records);
                for request in receiver {
                    match request {
                        NativeRequest::Point { id, reply } => {
                            let _ = reply.send(backend.point(&id));
                        }
                        NativeRequest::Filter { score, reply } => {
                            let _ = reply.send(backend.filter(score));
                        }
                        NativeRequest::Insert { index, reply } => {
                            backend.insert(index);
                            let _ = reply.send(ExecutionSummary {
                                statement_count: 1,
                                mutation_count: 1,
                            });
                        }
                        NativeRequest::Close { reply } => {
                            backend.close();
                            let _ = reply.send(());
                            return;
                        }
                    }
                }
                backend.close();
            })
            .unwrap();
        Self { sender, join }
    }

    async fn point(&self, id: &str) -> QueryResponse {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(NativeRequest::Point {
                id: id.to_owned(),
                reply,
            })
            .unwrap();
        receiver.await.unwrap()
    }

    async fn filter(&self, score: i64) -> QueryResponse {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(NativeRequest::Filter { score, reply })
            .unwrap();
        receiver.await.unwrap()
    }

    async fn insert(&self, index: usize) -> ExecutionSummary {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(NativeRequest::Insert { index, reply })
            .unwrap();
        receiver.await.unwrap()
    }

    async fn close(self) {
        let (reply, receiver) = oneshot::channel();
        self.sender.send(NativeRequest::Close { reply }).unwrap();
        receiver.await.unwrap();
        self.join.join().unwrap();
    }
}

fn bind_text(statement: &mut turso_core::Statement, index: usize, value: String) {
    statement
        .bind_at(
            NonZeroUsize::new(index).expect("indexes are one-based"),
            Value::build_text(value),
        )
        .unwrap();
}

fn bind_integer(statement: &mut turso_core::Statement, index: usize, value: i64) {
    statement
        .bind_at(
            NonZeroUsize::new(index).expect("indexes are one-based"),
            Value::from_i64(value),
        )
        .unwrap();
}

fn materialize(rid: &str, document: &str) -> FastValue {
    let mut object = parse_doc(document)
        .unwrap()
        .into_iter()
        .collect::<fastdb::Object>();
    object.insert(
        "id".into(),
        FastValue::RecordId(RecordId::new("person", decode_rid(rid).unwrap())),
    );
    FastValue::Object(object)
}

fn query_response(value: FastValue) -> QueryResponse {
    QueryResponse {
        statements: vec![StatementResult::Rows(vec![value])],
        mutation_count: 0,
    }
}

fn native_open(path: &str) -> Arc<turso_core::Connection> {
    let io = Database::io_for_path(path).unwrap();
    let flags = OpenFlags::default();
    let file = io.open_file(path, flags, true).unwrap();
    let database_file = Arc::new(DatabaseFile::new(file));
    let options = OpenOptions::new(Arc::new(SqliteDialect))
        .storage(database_file)
        .flags(flags)
        .db_opts(DatabaseOpts::default());
    Database::open(io, path, options)
        .unwrap()
        .connect()
        .unwrap()
}

fn path_string(path: &Path) -> String {
    path.to_str().expect("temporary paths are UTF-8").to_owned()
}

fn document_payload() -> String {
    "x".repeat(DOCUMENT_PAYLOAD_BYTES)
}

async fn seed_fast(path: &Path, records: usize) -> FastConnection {
    let database = Builder::new_local(path).build().await.unwrap();
    let mut connection = database.connect().unwrap();
    let mut transaction = connection.transaction().await.unwrap();
    transaction
        .execute(
            "DEFINE TABLE person SCHEMAFULL; \
             DEFINE FIELD score ON person TYPE int; \
             DEFINE FIELD name ON person TYPE string; \
             DEFINE FIELD active ON person TYPE bool; \
             DEFINE FIELD payload ON person TYPE string; \
             DEFINE INDEX by_score ON person FIELDS score",
            params! {},
        )
        .await
        .unwrap();
    for chunk in (0..records).collect::<Vec<_>>().chunks(200) {
        let source = chunk
            .iter()
            .map(|index| {
                format!(
                    "CREATE person:r{index} SET score={index}, name='person-{index}', \
                     active=true, payload=$payload RETURN NONE"
                )
            })
            .collect::<Vec<_>>()
            .join(";");
        transaction
            .execute(&source, params! { "payload" => document_payload() })
            .await
            .unwrap();
    }
    transaction.commit().await.unwrap();
    connection
}

fn assert_one_row(response: &fastdb::QueryResponse) {
    let Some(StatementResult::Rows(rows)) = response.statements.first() else {
        panic!("benchmark SELECT must return rows")
    };
    assert_eq!(rows.len(), 1);
    black_box(rows);
}

async fn run(config: Config) -> Result<bool, String> {
    let directory = TempDir::new().map_err(|error| error.to_string())?;
    let fast_path = directory.path().join("fast.fastdb");
    let native_path = directory.path().join("native.fastdb");
    let fast = seed_fast(&fast_path, config.records).await;
    let native = NativeClient::seed(path_string(&native_path), config.records);
    let point_id = format!("r{}", config.records / 2);
    let point_source = format!("SELECT * FROM person:{point_id}");
    let score = i64::try_from(config.records / 2).unwrap();

    let warmup = config.samples.clamp(50, 200);
    for sample in 0..warmup {
        assert_one_row(&fast.query(&point_source, params! {}).await.unwrap());
        assert_one_row(&native.point(&point_id).await);
        assert_one_row(
            &fast
                .query(
                    "SELECT * FROM person WHERE score=$score",
                    params! { "score" => score },
                )
                .await
                .unwrap(),
        );
        assert_one_row(&native.filter(score).await);
        let summary = fast
            .execute(
                "CREATE person SET score=$score, name=$name, active=true, \
                 payload=$payload RETURN NONE",
                params! {
                    "score" => i64::try_from(config.records + sample).unwrap(),
                    "name" => format!("person-{}", config.records + sample),
                    "payload" => document_payload(),
                },
            )
            .await
            .unwrap();
        assert_eq!(summary.mutation_count, 1);
        black_box(native.insert(config.records + sample).await);
    }

    let mut fast_point = Vec::with_capacity(config.samples);
    let mut native_point = Vec::with_capacity(config.samples);
    let mut fast_filter = Vec::with_capacity(config.samples);
    let mut native_filter = Vec::with_capacity(config.samples);
    let mut fast_write = Vec::with_capacity(config.samples);
    let mut native_write = Vec::with_capacity(config.samples);
    for sample in 0..config.samples {
        let start = Instant::now();
        assert_one_row(&fast.query(&point_source, params! {}).await.unwrap());
        fast_point.push(elapsed_ns(start));

        let start = Instant::now();
        assert_one_row(&native.point(&point_id).await);
        native_point.push(elapsed_ns(start));

        let start = Instant::now();
        assert_one_row(
            &fast
                .query(
                    "SELECT * FROM person WHERE score=$score",
                    params! { "score" => score },
                )
                .await
                .unwrap(),
        );
        fast_filter.push(elapsed_ns(start));

        let start = Instant::now();
        assert_one_row(&native.filter(score).await);
        native_filter.push(elapsed_ns(start));

        let start = Instant::now();
        let summary = fast
            .execute(
                "CREATE person SET score=$score, name=$name, active=true, \
                 payload=$payload RETURN NONE",
                params! {
                    "score" => i64::try_from(config.records + warmup + sample).unwrap(),
                    "name" => format!("person-{}", config.records + warmup + sample),
                    "payload" => document_payload(),
                },
            )
            .await
            .unwrap();
        assert_eq!(summary.mutation_count, 1);
        fast_write.push(elapsed_ns(start));

        let start = Instant::now();
        let summary = native.insert(config.records + warmup + sample).await;
        assert_eq!(summary.mutation_count, 1);
        native_write.push(elapsed_ns(start));
    }

    fast.close().await.unwrap();
    native.close().await;
    let fast_size = std::fs::metadata(&fast_path)
        .map_err(|error| error.to_string())?
        .len();
    let native_size = std::fs::metadata(&native_path)
        .map_err(|error| error.to_string())?
        .len();

    let fast_point = Distribution::new(fast_point);
    let native_point = Distribution::new(native_point);
    let fast_filter = Distribution::new(fast_filter);
    let native_filter = Distribution::new(native_filter);
    let fast_write = Distribution::new(fast_write);
    let native_write = Distribution::new(native_write);
    let point_p50_ratio = ratio(fast_point.p50_ns, native_point.p50_ns);
    let point_p99_ratio = ratio(fast_point.p99_ns, native_point.p99_ns);
    let filter_p95_ratio = ratio(fast_filter.p95_ns, native_filter.p95_ns);
    let write_p95_ratio = ratio(fast_write.p95_ns, native_write.p95_ns);
    let storage_ratio = fast_size as f64 / native_size.max(1) as f64;
    let passed = point_p50_ratio <= 1.5
        && point_p99_ratio <= 2.0
        && filter_p95_ratio <= 2.0
        && write_p95_ratio <= 2.0
        && storage_ratio <= 1.5;

    let report = json!({
        "format": "fastdb-phase5-benchmark-v1",
        "engine_sha": ENGINE_SHA,
        "target": {"os": std::env::consts::OS, "arch": std::env::consts::ARCH},
        "configuration": {
            "samples": config.samples,
            "warmup_iterations": warmup,
            "seed_records": config.records,
            "durability": "stable WAL / full",
            "cache_policy": "warm parse and prepared SELECT caches",
            "document_payload_bytes": DOCUMENT_PAYLOAD_BYTES,
            "result_materialization": "typed record ID plus complete decoded document",
        },
        "workloads": {
            "point_read": {"fastdb": fast_point.json(), "native": native_point.json()},
            "indexed_filter": {"fastdb": fast_filter.json(), "native": native_filter.json()},
            "write": {"fastdb": fast_write.json(), "native": native_write.json()},
        },
        "storage": {"fastdb_bytes": fast_size, "native_bytes": native_size},
        "gates": {
            "point_read_p50_ratio": point_p50_ratio,
            "point_read_p99_ratio": point_p99_ratio,
            "indexed_filter_p95_ratio": filter_p95_ratio,
            "write_p95_ratio": write_p95_ratio,
            "checkpointed_storage_ratio": storage_ratio,
            "passed": passed,
        },
    });
    let rendered = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    if let Some(output) = config.output {
        std::fs::write(output, format!("{rendered}\n")).map_err(|error| error.to_string())?;
    } else {
        println!("{rendered}");
    }
    Ok(passed || !config.gate)
}

fn main() -> std::process::ExitCode {
    let config = match Config::from_args() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return std::process::ExitCode::from(2);
        }
    };
    match block_on(run(config)) {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => {
            eprintln!("one or more release benchmark ratios exceeded their gate");
            std::process::ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("benchmark failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
