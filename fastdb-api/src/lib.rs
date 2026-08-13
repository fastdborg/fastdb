//! Runtime-neutral asynchronous embedded API for FastDB.
//!
//! Each [`Connection`] owns one dedicated worker thread. Complete requests are
//! serialized through that worker; dropping a queued future skips it, while
//! dropping an in-flight future never changes execution semantics.

#![forbid(unsafe_code)]
#![deny(warnings)]

mod error;
pub mod json;

pub use error::{Error, ErrorCategory, SourceSpan};
pub use turso_fastdb::{RecordId, RecordIdValue, StatementResult, Value};

use error::Result;
use futures::channel::oneshot;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

/// Deterministically ordered FastDB object fields.
pub type Object = BTreeMap<String, Value>;

/// Owned named bindings. Names omit the `$` source prefix.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Params(BTreeMap<String, Value>);

impl Params {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<Value>) -> Option<Value> {
        self.0.insert(name.into(), value.into())
    }

    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.0.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<(String, Value)> for Params {
    fn from_iter<T: IntoIterator<Item = (String, Value)>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl IntoIterator for Params {
    type Item = (String, Value);
    type IntoIter = std::collections::btree_map::IntoIter<String, Value>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Construct [`Params`] with values converted through [`Value::from`].
#[macro_export]
macro_rules! params {
    () => {
        $crate::Params::new()
    };
    ($($name:expr => $value:expr),+ $(,)?) => {{
        let mut bindings = $crate::Params::new();
        $(bindings.insert($name, $value);)+
        bindings
    }};
}

/// Ordered statement results for one successfully completed request.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResponse {
    pub statements: Vec<StatementResult>,
    pub mutation_count: u64,
}

/// Aggregate result for a request whose rows were intentionally discarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExecutionSummary {
    pub statement_count: usize,
    pub mutation_count: u64,
}

/// Builder for a local FastDB database.
#[derive(Debug, Clone)]
pub struct Builder {
    path: PathBuf,
}

impl Builder {
    pub fn new_local(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
        }
    }

    pub fn new_memory() -> Self {
        Self::new_local(":memory:")
    }

    pub async fn build(self) -> Result<Database> {
        let path = path_string(&self.path)?;
        let (sender, receiver) = oneshot::channel();
        std::thread::Builder::new()
            .name("fastdb-open".into())
            .spawn(move || {
                let result = turso_fastdb::Database::open(&path).map_err(Error::from_frontend);
                let _ = sender.send(result);
            })
            .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
        let inner = receiver
            .await
            .map_err(|_| Error::new(ErrorCategory::Engine, "database open worker stopped"))??;
        Ok(Database { inner })
    }
}

/// Open database handle. Every call to [`Self::connect`] creates a worker.
#[derive(Clone)]
pub struct Database {
    inner: turso_fastdb::Database,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Database { .. }")
    }
}

impl Database {
    pub fn connect(&self) -> Result<Connection> {
        let (request_sender, request_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let database = self.inner.clone();
        let active = Arc::new(ActiveStatement::default());
        let worker_active = active.clone();
        let join = std::thread::Builder::new()
            .name("fastdb-connection".into())
            .spawn(move || match database.connect() {
                Ok(connection) => {
                    let _ = ready_sender.send(Ok(()));
                    run_worker(connection, request_receiver, worker_active);
                }
                Err(error) => {
                    let _ = ready_sender.send(Err(Error::from_frontend(error)));
                }
            })
            .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
        ready_receiver
            .recv()
            .map_err(|_| Error::new(ErrorCategory::Engine, "connection worker stopped"))??;
        Ok(Connection {
            sender: Some(request_sender),
            join: Some(join),
            interrupt: InterruptHandle { active },
        })
    }
}

/// Serialized asynchronous FastDB connection.
pub struct Connection {
    sender: Option<mpsc::Sender<WorkerRequest>>,
    join: Option<JoinHandle<()>>,
    interrupt: InterruptHandle,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Connection { .. }")
    }
}

impl Connection {
    pub async fn query(&self, source: &str, params: Params) -> Result<QueryResponse> {
        match self
            .request(source, params, RequestKind::Query, false)
            .await?
        {
            WorkerResponse::Query(response) => Ok(response),
            WorkerResponse::Summary(_) | WorkerResponse::Unit => {
                Err(Error::new(ErrorCategory::Engine, "invalid worker response"))
            }
        }
    }

    pub async fn execute(&self, source: &str, params: Params) -> Result<ExecutionSummary> {
        match self
            .request(source, params, RequestKind::Execute, false)
            .await?
        {
            WorkerResponse::Summary(summary) => Ok(summary),
            WorkerResponse::Query(_) | WorkerResponse::Unit => {
                Err(Error::new(ErrorCategory::Engine, "invalid worker response"))
            }
        }
    }

    pub async fn transaction(&mut self) -> Result<Transaction<'_>> {
        self.control(Control::Begin).await?;
        Ok(Transaction {
            connection: self,
            active: true,
        })
    }

    pub fn interrupt_handle(&self) -> InterruptHandle {
        self.interrupt.clone()
    }

    pub async fn close(mut self) -> Result<()> {
        let result = self.control(Control::Close).await;
        self.sender.take();
        self.join_worker();
        result
    }

    async fn request(
        &self,
        source: &str,
        params: Params,
        kind: RequestKind,
        guarded: bool,
    ) -> Result<WorkerResponse> {
        let (reply, receiver) = oneshot::channel();
        self.send(WorkerRequest::Request {
            kind,
            source: source.to_owned(),
            params,
            guarded,
            reply,
        })?;
        receiver
            .await
            .map_err(|_| Error::new(ErrorCategory::Engine, "connection worker stopped"))?
    }

    async fn control(&self, control: Control) -> Result<()> {
        let (reply, receiver) = oneshot::channel();
        self.send(WorkerRequest::Control { control, reply })?;
        match receiver
            .await
            .map_err(|_| Error::new(ErrorCategory::Engine, "connection worker stopped"))??
        {
            WorkerResponse::Unit => Ok(()),
            WorkerResponse::Query(_) | WorkerResponse::Summary(_) => {
                Err(Error::new(ErrorCategory::Engine, "invalid worker response"))
            }
        }
    }

    fn send(&self, request: WorkerRequest) -> Result<()> {
        self.sender
            .as_ref()
            .ok_or_else(|| Error::new(ErrorCategory::Engine, "connection is closed"))?
            .send(request)
            .map_err(|_| Error::new(ErrorCategory::Engine, "connection worker stopped"))
    }

    fn queue_rollback(&self) {
        let (reply, _receiver) = oneshot::channel();
        let _ = self.send(WorkerRequest::Control {
            control: Control::Rollback,
            reply,
        });
    }

    fn join_worker(&mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            let (reply, _receiver) = oneshot::channel();
            let _ = sender.send(WorkerRequest::Control {
                control: Control::Close,
                reply,
            });
        }
        self.join_worker();
    }
}

/// Guard owning one explicit transaction on a connection.
pub struct Transaction<'connection> {
    connection: &'connection mut Connection,
    active: bool,
}

impl Transaction<'_> {
    pub async fn query(&mut self, source: &str, params: Params) -> Result<QueryResponse> {
        let result = self
            .connection
            .request(source, params, RequestKind::Query, true)
            .await;
        if result.is_err() {
            self.active = false;
        }
        match result? {
            WorkerResponse::Query(response) => Ok(response),
            WorkerResponse::Summary(_) | WorkerResponse::Unit => {
                Err(Error::new(ErrorCategory::Engine, "invalid worker response"))
            }
        }
    }

    pub async fn execute(&mut self, source: &str, params: Params) -> Result<ExecutionSummary> {
        let result = self
            .connection
            .request(source, params, RequestKind::Execute, true)
            .await;
        if result.is_err() {
            self.active = false;
        }
        match result? {
            WorkerResponse::Summary(summary) => Ok(summary),
            WorkerResponse::Query(_) | WorkerResponse::Unit => {
                Err(Error::new(ErrorCategory::Engine, "invalid worker response"))
            }
        }
    }

    pub async fn commit(mut self) -> Result<()> {
        self.active = false;
        self.connection.control(Control::Commit).await
    }

    pub async fn rollback(mut self) -> Result<()> {
        self.active = false;
        self.connection.control(Control::Rollback).await
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if self.active {
            self.connection.queue_rollback();
            self.active = false;
        }
    }
}

/// Cloneable cooperative interruption handle for the active engine statement.
#[derive(Clone)]
pub struct InterruptHandle {
    active: Arc<ActiveStatement>,
}

impl std::fmt::Debug for InterruptHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("InterruptHandle { .. }")
    }
}

impl InterruptHandle {
    pub fn interrupt(&self) {
        let active = self
            .active
            .engine
            .lock()
            .expect("active statement mutex poisoned");
        if let Some(connection) = active.as_ref() {
            self.active.requested.store(true, Ordering::SeqCst);
            connection.interrupt();
        }
    }
}

#[derive(Default)]
struct ActiveStatement {
    engine: Mutex<Option<Arc<turso_core::Connection>>>,
    requested: AtomicBool,
}

enum WorkerRequest {
    Request {
        kind: RequestKind,
        source: String,
        params: Params,
        guarded: bool,
        reply: oneshot::Sender<Result<WorkerResponse>>,
    },
    Control {
        control: Control,
        reply: oneshot::Sender<Result<WorkerResponse>>,
    },
}

#[derive(Clone, Copy)]
enum RequestKind {
    Query,
    Execute,
}

#[derive(Clone, Copy)]
enum Control {
    Begin,
    Commit,
    Rollback,
    Close,
}

enum WorkerResponse {
    Query(QueryResponse),
    Summary(ExecutionSummary),
    Unit,
}

fn run_worker(
    connection: turso_fastdb::Connection,
    requests: mpsc::Receiver<WorkerRequest>,
    active: Arc<ActiveStatement>,
) {
    for request in requests {
        match request {
            WorkerRequest::Request {
                kind,
                source,
                params,
                guarded,
                reply,
            } => {
                if reply.is_canceled() {
                    continue;
                }
                let result = run_request(&connection, &active, kind, source, params, guarded);
                let _ = reply.send(result);
            }
            WorkerRequest::Control { control, reply } => {
                let close = matches!(control, Control::Close);
                let result = run_control(&connection, control).map(|()| WorkerResponse::Unit);
                let _ = reply.send(result);
                if close {
                    return;
                }
            }
        }
    }
    let _ = connection.close();
}

fn run_request(
    connection: &turso_fastdb::Connection,
    active: &ActiveStatement,
    kind: RequestKind,
    source: String,
    params: Params,
    guarded: bool,
) -> Result<WorkerResponse> {
    if guarded && contains_transaction_control(&source) {
        let original = Error::new(
            ErrorCategory::Transaction,
            "transaction-control source is not allowed inside a transaction guard",
        );
        return Err(cleanup_guard(connection, original));
    }
    active.requested.store(false, Ordering::SeqCst);
    *active
        .engine
        .lock()
        .expect("active statement mutex poisoned") = Some(connection.native().clone());
    let frontend_params = params.into_iter().collect();
    let result = connection
        .execute_with_params(&source, &frontend_params)
        .map_err(Error::from_frontend);
    *active
        .engine
        .lock()
        .expect("active statement mutex poisoned") = None;
    let result = match result {
        Err(_) if active.requested.swap(false, Ordering::SeqCst) => {
            Err(Error::new(ErrorCategory::Engine, "operation interrupted"))
        }
        result => result,
    };
    let result = match result {
        Ok(response) => response,
        Err(error) if guarded => return Err(cleanup_guard(connection, error)),
        Err(error) => return Err(error),
    };
    let response = QueryResponse {
        statements: result.statements,
        mutation_count: result.mutation_count,
    };
    Ok(match kind {
        RequestKind::Query => WorkerResponse::Query(response),
        RequestKind::Execute => WorkerResponse::Summary(ExecutionSummary {
            statement_count: response.statements.len(),
            mutation_count: response.mutation_count,
        }),
    })
}

fn cleanup_guard(connection: &turso_fastdb::Connection, original: Error) -> Error {
    match connection.execute("CANCEL") {
        Ok(_) => original,
        Err(cleanup) => Error::transaction_cleanup(original, Error::from_frontend(cleanup)),
    }
}

fn run_control(connection: &turso_fastdb::Connection, control: Control) -> Result<()> {
    let source = match control {
        Control::Begin => "BEGIN",
        Control::Commit => "COMMIT",
        Control::Rollback => "CANCEL",
        Control::Close => return connection.close().map_err(Error::from_frontend),
    };
    match connection.execute(source) {
        Ok(_) => Ok(()),
        Err(error) if matches!(control, Control::Commit) => {
            let original = Error::from_frontend(error);
            Err(cleanup_guard(connection, original))
        }
        Err(error) => Err(Error::from_frontend(error)),
    }
}

fn contains_transaction_control(source: &str) -> bool {
    let mut cursor = turso_fastdb_parser::StatementCursor::new(source);
    loop {
        match cursor.next_statement() {
            Ok(Some(
                turso_fastdb_parser::Statement::Begin(_)
                | turso_fastdb_parser::Statement::Commit(_)
                | turso_fastdb_parser::Statement::Cancel(_),
            )) => return true,
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => return false,
        }
    }
}

fn path_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::new(ErrorCategory::Io, "database path is not valid UTF-8"))
}

#[cfg(test)]
mod worker_tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn p4_api_008_dropped_queued_request_is_skipped() {
        let database = turso_fastdb::Database::open_memory().unwrap();
        let connection = database.connect().unwrap();
        let (sender, receiver) = mpsc::channel();
        let active = Arc::new(ActiveStatement::default());

        let (reply, dropped) = oneshot::channel();
        drop(dropped);
        sender
            .send(WorkerRequest::Request {
                kind: RequestKind::Execute,
                source: "CREATE item:skipped SET n=1".into(),
                params: Params::new(),
                guarded: false,
                reply,
            })
            .unwrap();
        let (close_reply, close_receiver) = oneshot::channel();
        sender
            .send(WorkerRequest::Control {
                control: Control::Close,
                reply: close_reply,
            })
            .unwrap();

        let worker_active = active.clone();
        let worker = std::thread::spawn(move || run_worker(connection, receiver, worker_active));
        assert!(matches!(
            block_on(close_receiver).unwrap().unwrap(),
            WorkerResponse::Unit
        ));
        worker.join().unwrap();

        let verify = database.connect().unwrap();
        let response = verify.execute("SELECT * FROM item:skipped").unwrap();
        assert_eq!(response.statements, vec![StatementResult::Rows(vec![])]);
    }

    #[test]
    fn p4_api_009_dropped_in_flight_request_runs_to_completion() {
        let database = turso_fastdb::Database::open_memory().unwrap();
        let seed = database.connect().unwrap();
        seed.execute("BEGIN").unwrap();
        for batch in 0..16 {
            let source = (0..240)
                .map(|offset| {
                    let id = batch * 240 + offset;
                    format!("CREATE item:r{id} SET n=1 RETURN NONE")
                })
                .collect::<Vec<_>>()
                .join(";");
            seed.execute(&source).unwrap();
        }
        seed.execute("COMMIT").unwrap();
        drop(seed);

        let connection = database.connect().unwrap();
        let (sender, receiver) = mpsc::channel();
        let active = Arc::new(ActiveStatement::default());
        let worker_active = active.clone();
        let worker = std::thread::spawn(move || run_worker(connection, receiver, worker_active));

        let (reply, dropped) = oneshot::channel();
        sender
            .send(WorkerRequest::Request {
                kind: RequestKind::Execute,
                source: "UPDATE item SET n=n+1 RETURN NONE".into(),
                params: Params::new(),
                guarded: false,
                reply,
            })
            .unwrap();
        loop {
            if active.engine.lock().unwrap().is_some() {
                break;
            }
            std::thread::yield_now();
        }
        drop(dropped);

        let (close_reply, close_receiver) = oneshot::channel();
        sender
            .send(WorkerRequest::Control {
                control: Control::Close,
                reply: close_reply,
            })
            .unwrap();
        block_on(close_receiver).unwrap().unwrap();
        worker.join().unwrap();

        let verify = database.connect().unwrap();
        let response = verify.execute("SELECT n FROM item:r1").unwrap();
        assert_eq!(
            response.statements,
            vec![StatementResult::Rows(vec![Value::Object(BTreeMap::from(
                [("n".into(), Value::Integer(2)),]
            ))])]
        );
    }
}
