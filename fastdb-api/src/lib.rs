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
pub use turso_fastdb::decode::{
    DatetimeValue, DecimalValue, DurationValue, FileValue, RangeBound, RangeValue, RegexValue,
    SetValue, TableValue,
};
pub use turso_fastdb::{CheckReport, RecordId, RecordIdValue, StatementResult, Value};

use error::Result;
use futures::channel::oneshot;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

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

const MAX_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_OUTPUT_ROWS: usize = 100_000;
const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const MAX_GRAPH_HOPS: usize = 16;
const MAX_VECTOR_DIMENSIONS: usize = 65_536;
const MAX_FTS_QUERY_BYTES: usize = 64 * 1024;

/// Bounded resources for one request. Builder methods may only select values
/// within the documented hard ceilings; validation happens before execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceLimits {
    timeout: Duration,
    output_rows: usize,
    output_bytes: usize,
    graph_hops: usize,
    vector_dimensions: usize,
    fts_query_bytes: usize,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            output_rows: 10_000,
            output_bytes: 16 * 1024 * 1024,
            graph_hops: 8,
            vector_dimensions: MAX_VECTOR_DIMENSIONS,
            fts_query_bytes: 4 * 1024,
        }
    }
}

impl ResourceLimits {
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn with_output_rows(mut self, rows: usize) -> Self {
        self.output_rows = rows;
        self
    }

    pub fn with_output_bytes(mut self, bytes: usize) -> Self {
        self.output_bytes = bytes;
        self
    }

    pub fn with_graph_hops(mut self, hops: usize) -> Self {
        self.graph_hops = hops;
        self
    }

    pub fn with_vector_dimensions(mut self, dimensions: usize) -> Self {
        self.vector_dimensions = dimensions;
        self
    }

    pub fn with_fts_query_bytes(mut self, bytes: usize) -> Self {
        self.fts_query_bytes = bytes;
        self
    }

    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn output_rows(&self) -> usize {
        self.output_rows
    }

    pub fn output_bytes(&self) -> usize {
        self.output_bytes
    }

    pub fn graph_hops(&self) -> usize {
        self.graph_hops
    }

    pub fn vector_dimensions(&self) -> usize {
        self.vector_dimensions
    }

    pub fn fts_query_bytes(&self) -> usize {
        self.fts_query_bytes
    }

    fn validate(&self) -> Result<()> {
        for (name, value, ceiling) in [
            ("output rows", self.output_rows, MAX_OUTPUT_ROWS),
            ("output bytes", self.output_bytes, MAX_OUTPUT_BYTES),
            ("graph hops", self.graph_hops, MAX_GRAPH_HOPS),
            (
                "vector dimensions",
                self.vector_dimensions,
                MAX_VECTOR_DIMENSIONS,
            ),
            ("FTS query bytes", self.fts_query_bytes, MAX_FTS_QUERY_BYTES),
        ] {
            if value == 0 || value > ceiling {
                return Err(Error::new(
                    ErrorCategory::Schema,
                    format!("{name} limit must be in 1..={ceiling}"),
                ));
            }
        }
        if self.timeout.is_zero() || self.timeout > MAX_TIMEOUT {
            return Err(Error::new(
                ErrorCategory::Schema,
                "timeout must be greater than zero and no more than 300 seconds",
            ));
        }
        Ok(())
    }
}

/// Per-request execution options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryOptions {
    limits: ResourceLimits,
}

impl QueryOptions {
    pub fn with_resource_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    pub fn resource_limits(&self) -> &ResourceLimits {
        &self.limits
    }
}

/// Metadata-only request event. It never contains source or parameter values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryEvent {
    pub operation: &'static str,
    pub elapsed: Duration,
    pub mutation_count: u64,
    pub output_rows: usize,
    pub output_bytes: usize,
    pub error_category: Option<ErrorCategory>,
}

pub type EventHook = Arc<dyn Fn(&QueryEvent) + Send + Sync + 'static>;

/// Builder for a local FastDB database.
#[derive(Clone)]
pub struct Builder {
    path: PathBuf,
    event_hook: Option<EventHook>,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Builder")
            .field("path", &self.path)
            .field(
                "event_hook",
                &self.event_hook.as_ref().map(|_| "configured"),
            )
            .finish()
    }
}

impl Builder {
    pub fn new_local(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_owned(),
            event_hook: None,
        }
    }

    pub fn new_memory() -> Self {
        Self::new_local(":memory:")
    }

    pub fn event_hook(mut self, hook: EventHook) -> Self {
        self.event_hook = Some(hook);
        self
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
        Ok(Database {
            inner: Arc::new(DatabaseInner {
                frontend: inner,
                lifecycle: Mutex::new(DatabaseLifecycle::default()),
                event_hook: self.event_hook,
            }),
        })
    }
}

/// Open database handle. Every call to [`Self::connect`] creates a worker.
#[derive(Clone)]
pub struct Database {
    inner: Arc<DatabaseInner>,
}

#[derive(Default)]
struct DatabaseLifecycle {
    closed: bool,
    connections: usize,
}

struct DatabaseInner {
    frontend: turso_fastdb::Database,
    lifecycle: Mutex<DatabaseLifecycle>,
    event_hook: Option<EventHook>,
}

impl std::fmt::Debug for Database {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Database { .. }")
    }
}

impl Database {
    pub fn connect(&self) -> Result<Connection> {
        {
            let mut lifecycle = self.inner.lifecycle.lock().map_err(|_| {
                Error::new(ErrorCategory::Engine, "database lifecycle lock is poisoned")
            })?;
            if lifecycle.closed {
                return Err(Error::new(ErrorCategory::Engine, "database is closed"));
            }
            lifecycle.connections = lifecycle.connections.checked_add(1).ok_or_else(|| {
                Error::new(
                    ErrorCategory::Engine,
                    "database connection count overflowed",
                )
            })?;
        }
        let (request_sender, request_receiver) = mpsc::channel();
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let database = self.inner.frontend.clone();
        let active = Arc::new(ActiveStatement::default());
        let worker_active = active.clone();
        let event_hook = self.inner.event_hook.clone();
        let join = std::thread::Builder::new()
            .name("fastdb-connection".into())
            .spawn(move || match database.connect() {
                Ok(connection) => {
                    let _ = ready_sender.send(Ok(()));
                    run_worker(connection, request_receiver, worker_active, event_hook);
                }
                Err(error) => {
                    let _ = ready_sender.send(Err(Error::from_frontend(error)));
                }
            })
            .map_err(|error| {
                self.release_connection();
                Error::new(ErrorCategory::Io, error.to_string())
            })?;
        if let Err(error) = ready_receiver
            .recv()
            .map_err(|_| Error::new(ErrorCategory::Engine, "connection worker stopped"))?
        {
            self.release_connection();
            let _ = join.join();
            return Err(error);
        }
        Ok(Connection {
            sender: Some(request_sender),
            join: Some(join),
            interrupt: InterruptHandle { active },
            database: Some(self.inner.clone()),
        })
    }

    pub async fn check(&self) -> Result<CheckReport> {
        self.ensure_open()?;
        let database = self.inner.frontend.clone();
        run_database_io("database check worker stopped", move || {
            database.check().map_err(Error::from_frontend)
        })
        .await
    }

    pub async fn backup_to(&self, destination: impl AsRef<Path>) -> Result<CheckReport> {
        self.ensure_open()?;
        let destination = destination.as_ref().to_owned();
        let database = self.inner.frontend.clone();
        run_database_io("database backup worker stopped", move || {
            database
                .backup_to(destination)
                .map_err(Error::from_frontend)
        })
        .await
    }

    pub async fn rebuild_index(&self, table: &str, index: &str) -> Result<()> {
        self.ensure_open()?;
        let table = table.to_owned();
        let index = index.to_owned();
        let database = self.inner.frontend.clone();
        run_database_io("index rebuild worker stopped", move || {
            database
                .rebuild_index(&table, &index)
                .map_err(Error::from_frontend)
        })
        .await
    }

    pub async fn close(&self) -> Result<()> {
        let mut lifecycle = self.inner.lifecycle.lock().map_err(|_| {
            Error::new(ErrorCategory::Engine, "database lifecycle lock is poisoned")
        })?;
        if lifecycle.closed {
            return Ok(());
        }
        if lifecycle.connections != 0 {
            return Err(Error::new(
                ErrorCategory::Transaction,
                "database still has open connections",
            ));
        }
        lifecycle.closed = true;
        Ok(())
    }

    fn ensure_open(&self) -> Result<()> {
        let lifecycle = self.inner.lifecycle.lock().map_err(|_| {
            Error::new(ErrorCategory::Engine, "database lifecycle lock is poisoned")
        })?;
        if lifecycle.closed {
            Err(Error::new(ErrorCategory::Engine, "database is closed"))
        } else {
            Ok(())
        }
    }

    fn release_connection(&self) {
        release_database_connection(&self.inner);
    }
}

/// Serialized asynchronous FastDB connection.
pub struct Connection {
    sender: Option<mpsc::Sender<WorkerRequest>>,
    join: Option<JoinHandle<()>>,
    interrupt: InterruptHandle,
    database: Option<Arc<DatabaseInner>>,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Connection { .. }")
    }
}

impl Connection {
    pub async fn query(&self, source: &str, params: Params) -> Result<QueryResponse> {
        self.query_with_options(source, params, QueryOptions::default())
            .await
    }

    pub async fn query_with_options(
        &self,
        source: &str,
        params: Params,
        options: QueryOptions,
    ) -> Result<QueryResponse> {
        match self
            .request(source, params, options, RequestKind::Query, false)
            .await?
        {
            WorkerResponse::Query(response) => Ok(response),
            WorkerResponse::Summary(_) | WorkerResponse::Unit => {
                Err(Error::new(ErrorCategory::Engine, "invalid worker response"))
            }
        }
    }

    pub async fn execute(&self, source: &str, params: Params) -> Result<ExecutionSummary> {
        self.execute_with_options(source, params, QueryOptions::default())
            .await
    }

    pub async fn execute_with_options(
        &self,
        source: &str,
        params: Params,
        options: QueryOptions,
    ) -> Result<ExecutionSummary> {
        match self
            .request(source, params, options, RequestKind::Execute, false)
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
        self.release_database();
        result
    }

    async fn request(
        &self,
        source: &str,
        params: Params,
        options: QueryOptions,
        kind: RequestKind,
        guarded: bool,
    ) -> Result<WorkerResponse> {
        let (reply, receiver) = oneshot::channel();
        self.send(WorkerRequest::Request {
            kind,
            source: source.to_owned(),
            params,
            options,
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

    fn release_database(&mut self) {
        if let Some(database) = self.database.take() {
            release_database_connection(&database);
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
        self.release_database();
    }
}

/// Guard owning one explicit transaction on a connection.
pub struct Transaction<'connection> {
    connection: &'connection mut Connection,
    active: bool,
}

impl Transaction<'_> {
    pub async fn query(&mut self, source: &str, params: Params) -> Result<QueryResponse> {
        self.query_with_options(source, params, QueryOptions::default())
            .await
    }

    pub async fn query_with_options(
        &mut self,
        source: &str,
        params: Params,
        options: QueryOptions,
    ) -> Result<QueryResponse> {
        let result = self
            .connection
            .request(source, params, options, RequestKind::Query, true)
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
        self.execute_with_options(source, params, QueryOptions::default())
            .await
    }

    pub async fn execute_with_options(
        &mut self,
        source: &str,
        params: Params,
        options: QueryOptions,
    ) -> Result<ExecutionSummary> {
        let result = self
            .connection
            .request(source, params, options, RequestKind::Execute, true)
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
        options: QueryOptions,
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

struct RequestInput {
    kind: RequestKind,
    source: String,
    params: Params,
    options: QueryOptions,
    guarded: bool,
}

fn run_worker(
    connection: turso_fastdb::Connection,
    requests: mpsc::Receiver<WorkerRequest>,
    active: Arc<ActiveStatement>,
    event_hook: Option<EventHook>,
) {
    for request in requests {
        match request {
            WorkerRequest::Request {
                kind,
                source,
                params,
                options,
                guarded,
                reply,
            } => {
                if reply.is_canceled() {
                    continue;
                }
                let result = run_request(
                    &connection,
                    &active,
                    event_hook.as_ref(),
                    RequestInput {
                        kind,
                        source,
                        params,
                        options,
                        guarded,
                    },
                );
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
    event_hook: Option<&EventHook>,
    request: RequestInput,
) -> Result<WorkerResponse> {
    let RequestInput {
        kind,
        source,
        params,
        options,
        guarded,
    } = request;
    let started = Instant::now();
    if guarded && contains_transaction_control(&source) {
        let original = Error::new(
            ErrorCategory::Transaction,
            "transaction-control source is not allowed inside a transaction guard",
        );
        let error = cleanup_guard(connection, original);
        emit_event(
            event_hook,
            kind,
            started.elapsed(),
            0,
            (0, 0),
            Some(error.category()),
        );
        return Err(error);
    }
    if let Err(error) = options
        .limits
        .validate()
        .and_then(|()| validate_request_limits(&source, &params, &options.limits))
    {
        let error = if guarded {
            cleanup_guard(connection, error)
        } else {
            error
        };
        emit_event(
            event_hook,
            kind,
            started.elapsed(),
            0,
            (0, 0),
            Some(error.category()),
        );
        return Err(error);
    }
    active.requested.store(false, Ordering::SeqCst);
    *active
        .engine
        .lock()
        .expect("active statement mutex poisoned") = Some(connection.native().clone());
    let frontend_params = params.into_iter().collect();
    connection
        .native()
        .set_query_timeout(options.limits.timeout);
    let result = connection
        .execute_with_params(&source, &frontend_params)
        .map_err(Error::from_frontend);
    connection.native().set_query_timeout(Duration::ZERO);
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
        Err(error) => {
            let error = if guarded {
                cleanup_guard(connection, error)
            } else {
                error
            };
            emit_event(
                event_hook,
                kind,
                started.elapsed(),
                0,
                (0, 0),
                Some(error.category()),
            );
            return Err(error);
        }
    };
    let response = QueryResponse {
        statements: result.statements,
        mutation_count: result.mutation_count,
    };
    let usage = match response_usage(&response) {
        Ok(usage) => usage,
        Err(error) => {
            emit_event(
                event_hook,
                kind,
                started.elapsed(),
                response.mutation_count,
                (0, 0),
                Some(error.category()),
            );
            return Err(error);
        }
    };
    let limited = if usage.0 > options.limits.output_rows {
        Err(Error::new(
            ErrorCategory::Constraint,
            "request exceeded its output row limit",
        ))
    } else if usage.1 > options.limits.output_bytes {
        Err(Error::new(
            ErrorCategory::Constraint,
            "request exceeded its output byte limit",
        ))
    } else {
        Ok(())
    };
    if let Err(error) = limited {
        emit_event(
            event_hook,
            kind,
            started.elapsed(),
            response.mutation_count,
            usage,
            Some(error.category()),
        );
        return if guarded {
            Err(cleanup_guard(connection, error))
        } else {
            Err(error)
        };
    }
    emit_event(
        event_hook,
        kind,
        started.elapsed(),
        response.mutation_count,
        usage,
        None,
    );
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

async fn run_database_io<T: Send + 'static>(
    stopped: &'static str,
    operation: impl FnOnce() -> Result<T> + Send + 'static,
) -> Result<T> {
    let (sender, receiver) = oneshot::channel();
    std::thread::Builder::new()
        .name("fastdb-maintenance".into())
        .spawn(move || {
            let _ = sender.send(operation());
        })
        .map_err(|error| Error::new(ErrorCategory::Io, error.to_string()))?;
    receiver
        .await
        .map_err(|_| Error::new(ErrorCategory::Engine, stopped))?
}

fn release_database_connection(database: &Arc<DatabaseInner>) {
    if let Ok(mut lifecycle) = database.lifecycle.lock() {
        lifecycle.connections = lifecycle.connections.saturating_sub(1);
    }
}

fn validate_request_limits(source: &str, params: &Params, limits: &ResourceLimits) -> Result<()> {
    let mut cursor = turso_fastdb_parser::StatementCursor::new(source);
    while let Some(statement) = cursor
        .next_statement()
        .map_err(|error| Error::from_frontend(turso_fastdb::FastDbError::from(error)))?
    {
        validate_statement_limits(&statement, params, limits)?;
    }
    Ok(())
}

fn validate_statement_limits(
    statement: &turso_fastdb_parser::Statement,
    params: &Params,
    limits: &ResourceLimits,
) -> Result<()> {
    use turso_fastdb_parser::{CreateData, ProjectionList, SchemaTypeKind, Statement};

    let mut expressions = Vec::new();
    match statement {
        Statement::Create(statement) => match &statement.data {
            CreateData::Content(expression) => expressions.push(expression),
            CreateData::Set(assignments) => {
                expressions.extend(assignments.iter().map(|assignment| &assignment.value));
            }
        },
        Statement::Relate(statement) => {
            expressions.extend([&statement.from, &statement.to]);
            if let Some(data) = &statement.data {
                match data {
                    CreateData::Content(expression) => expressions.push(expression),
                    CreateData::Set(assignments) => {
                        expressions.extend(assignments.iter().map(|assignment| &assignment.value))
                    }
                }
            }
        }
        Statement::Select(statement) => {
            if let ProjectionList::Fields(projections) = &statement.projections {
                expressions.extend(projections.iter().map(|projection| &projection.expression));
            }
            expressions.extend(statement.condition.iter());
        }
        Statement::Update(statement) => {
            expressions.extend(
                statement
                    .assignments
                    .iter()
                    .map(|assignment| &assignment.value),
            );
            expressions.extend(statement.condition.iter());
        }
        Statement::Delete(statement) => expressions.extend(statement.condition.iter()),
        Statement::DefineField(statement) => {
            let mut ty = &statement.ty.kind;
            while let SchemaTypeKind::Option(inner) = ty {
                ty = &inner.kind;
            }
            if let SchemaTypeKind::FixedFloatArray(dimension) = ty {
                let dimension = usize::try_from(dimension.value).unwrap_or(usize::MAX);
                check_vector_dimension(dimension, limits)?;
            }
        }
        Statement::DefineIndex(statement) => {
            if let turso_fastdb_parser::IndexKindSyntax::Provider { options, .. } = &statement.kind
            {
                expressions.extend(options.iter().map(|option| &option.value));
            }
        }
        Statement::Explain(statement) => {
            let nested = Statement::Select(statement.select.clone());
            return validate_statement_limits(&nested, params, limits);
        }
        Statement::DefineTable(_)
        | Statement::DefineAnalyzer(_)
        | Statement::RemoveIndex(_)
        | Statement::RebuildIndex(_)
        | Statement::Begin(_)
        | Statement::Commit(_)
        | Statement::Cancel(_) => {}
    }
    for expression in expressions {
        validate_expression_limits(expression, params, limits)?;
    }
    Ok(())
}

fn validate_expression_limits(
    expression: &turso_fastdb_parser::Expr,
    params: &Params,
    limits: &ResourceLimits,
) -> Result<()> {
    use turso_fastdb_parser::{Accessor, BinaryOperator, ExprKind};

    match &expression.kind {
        ExprKind::Array(values) => {
            for value in values {
                validate_expression_limits(value, params, limits)?;
            }
        }
        ExprKind::Object(fields) => {
            for field in fields {
                validate_expression_limits(&field.value, params, limits)?;
            }
        }
        ExprKind::Access { target, accessor } => {
            validate_expression_limits(target, params, limits)?;
            match accessor {
                Accessor::Index(index) => validate_expression_limits(index, params, limits)?,
                Accessor::Slice { start, end, .. } => {
                    if let Some(start) = start {
                        validate_expression_limits(start, params, limits)?;
                    }
                    if let Some(end) = end {
                        validate_expression_limits(end, params, limits)?;
                    }
                }
                Accessor::Field(_) | Accessor::Last(_) => {}
            }
        }
        ExprKind::Cast { value, .. } => {
            validate_expression_limits(value, params, limits)?;
        }
        ExprKind::Range(range) => {
            if let Some(start) = &range.start {
                validate_expression_limits(start, params, limits)?;
            }
            if let Some(end) = &range.end {
                validate_expression_limits(end, params, limits)?;
            }
        }
        ExprKind::FunctionCall { name, arguments } => {
            let function = name
                .iter()
                .map(|segment| segment.value.to_ascii_lowercase())
                .collect::<Vec<_>>()
                .join("::");
            if function.starts_with("vector::") {
                for argument in arguments {
                    check_vector_expression(argument, params, limits)?;
                }
            }
            if function == "fts_match" {
                if let Some(query) = arguments.last() {
                    check_fts_query(query, params, limits)?;
                }
            }
            for argument in arguments {
                validate_expression_limits(argument, params, limits)?;
            }
        }
        ExprKind::Knn(knn) => {
            check_vector_expression(&knn.query, params, limits)?;
            validate_expression_limits(&knn.field, params, limits)?;
            validate_expression_limits(&knn.query, params, limits)?;
        }
        ExprKind::Closure(closure) => {
            validate_expression_limits(&closure.body, params, limits)?;
        }
        ExprKind::Traversal(traversal) => {
            if traversal.hops.len() > limits.graph_hops {
                return Err(Error::new(
                    ErrorCategory::Constraint,
                    "request exceeded its graph hop limit",
                ));
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => {
            validate_expression_limits(operand, params, limits)?;
        }
        ExprKind::Binary {
            left,
            operator,
            right,
        } => {
            if matches!(operator.value, BinaryOperator::FtsMatch(_)) {
                check_fts_query(right, params, limits)?;
            }
            validate_expression_limits(left, params, limits)?;
            validate_expression_limits(right, params, limits)?;
        }
        ExprKind::None
        | ExprKind::Null
        | ExprKind::NamespacedValue { .. }
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Parameter(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_) => {}
    }
    Ok(())
}

fn check_vector_expression(
    expression: &turso_fastdb_parser::Expr,
    params: &Params,
    limits: &ResourceLimits,
) -> Result<()> {
    use turso_fastdb_parser::ExprKind;
    let dimension = match &expression.kind {
        ExprKind::Array(values) => Some(values.len()),
        ExprKind::Parameter(name) => match params.get(name) {
            Some(Value::Array(values)) => Some(values.len()),
            _ => None,
        },
        ExprKind::Parenthesized(inner) => {
            return check_vector_expression(inner, params, limits);
        }
        _ => None,
    };
    if let Some(dimension) = dimension {
        check_vector_dimension(dimension, limits)?;
    }
    Ok(())
}

fn check_vector_dimension(dimension: usize, limits: &ResourceLimits) -> Result<()> {
    if dimension > limits.vector_dimensions {
        Err(Error::new(
            ErrorCategory::Constraint,
            "request exceeded its vector dimension limit",
        ))
    } else {
        Ok(())
    }
}

fn check_fts_query(
    expression: &turso_fastdb_parser::Expr,
    params: &Params,
    limits: &ResourceLimits,
) -> Result<()> {
    use turso_fastdb_parser::ExprKind;
    let bytes = match &expression.kind {
        ExprKind::String(value) => Some(value.len()),
        ExprKind::Parameter(name) => match params.get(name) {
            Some(Value::Str(value)) => Some(value.len()),
            _ => None,
        },
        ExprKind::Parenthesized(inner) => return check_fts_query(inner, params, limits),
        _ => None,
    };
    if bytes.is_some_and(|bytes| bytes > limits.fts_query_bytes) {
        Err(Error::new(
            ErrorCategory::Constraint,
            "request exceeded its FTS query byte limit",
        ))
    } else {
        Ok(())
    }
}

fn response_usage(response: &QueryResponse) -> Result<(usize, usize)> {
    let mut rows = 0_usize;
    let mut bytes = 0_usize;
    for statement in &response.statements {
        match statement {
            StatementResult::None => {}
            StatementResult::Rows(values) => {
                rows = rows.checked_add(values.len()).ok_or_else(usage_overflow)?;
                for value in values {
                    bytes = bytes
                        .checked_add(value_size(value)?)
                        .ok_or_else(usage_overflow)?;
                }
            }
            StatementResult::Value(value) => {
                rows = rows.checked_add(1).ok_or_else(usage_overflow)?;
                bytes = bytes
                    .checked_add(value_size(value)?)
                    .ok_or_else(usage_overflow)?;
            }
        }
    }
    Ok((rows, bytes))
}

fn value_size(value: &Value) -> Result<usize> {
    let size = match value {
        Value::None | Value::Null => 1,
        Value::Bool(_) => 1,
        Value::Integer(_) | Value::Float(_) => 8,
        Value::Decimal(value) => value.to_canonical().len(),
        Value::Str(value) => value.len(),
        Value::Bytes(value) => value.len(),
        Value::Duration(_) | Value::Datetime(_) => 12,
        Value::Uuid(_) => 16,
        Value::Array(values) => values.iter().try_fold(0_usize, |total, value| {
            total
                .checked_add(value_size(value)?)
                .ok_or_else(usage_overflow)
        })?,
        Value::Object(values) => values.iter().try_fold(0_usize, |total, (key, value)| {
            total
                .checked_add(key.len())
                .and_then(|total| total.checked_add(value_size(value).ok()?))
                .ok_or_else(usage_overflow)
        })?,
        Value::Set(values) => values.as_slice().iter().try_fold(0_usize, |total, value| {
            total
                .checked_add(value_size(value)?)
                .ok_or_else(usage_overflow)
        })?,
        Value::Range(value) => {
            [value.start(), value.end()]
                .into_iter()
                .try_fold(0_usize, |total, bound| {
                    let size = match bound {
                        RangeBound::Unbounded => 1,
                        RangeBound::Included(value) | RangeBound::Excluded(value) => {
                            value_size(value)?
                        }
                    };
                    total.checked_add(size).ok_or_else(usage_overflow)
                })?
        }
        Value::Regex(value) => value.as_str().len(),
        Value::RecordId(value) => {
            value.table.len()
                + match &value.id {
                    RecordIdValue::String(value) => value.len(),
                    RecordIdValue::Integer(_) => 8,
                    RecordIdValue::Uuid(_) => 16,
                    RecordIdValue::Array(values) => {
                        values.iter().try_fold(0_usize, |total, value| {
                            total
                                .checked_add(value_size(value)?)
                                .ok_or_else(usage_overflow)
                        })?
                    }
                    RecordIdValue::Object(values) => {
                        values.iter().try_fold(0_usize, |total, (key, value)| {
                            total
                                .checked_add(key.len())
                                .and_then(|total| total.checked_add(value_size(value).ok()?))
                                .ok_or_else(usage_overflow)
                        })?
                    }
                }
        }
        Value::Table(value) => value.as_str().len(),
        Value::File(value) => value.as_str().len(),
    };
    Ok(size)
}

fn usage_overflow() -> Error {
    Error::new(ErrorCategory::Constraint, "request output size overflowed")
}

fn emit_event(
    hook: Option<&EventHook>,
    kind: RequestKind,
    elapsed: Duration,
    mutation_count: u64,
    usage: (usize, usize),
    error_category: Option<ErrorCategory>,
) {
    let Some(hook) = hook else {
        return;
    };
    let event = QueryEvent {
        operation: match kind {
            RequestKind::Query => "query",
            RequestKind::Execute => "execute",
        },
        elapsed,
        mutation_count,
        output_rows: usage.0,
        output_bytes: usage.1,
        error_category,
    };
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| hook(&event)));
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
                options: QueryOptions::default(),
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
        let worker =
            std::thread::spawn(move || run_worker(connection, receiver, worker_active, None));
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
        let worker =
            std::thread::spawn(move || run_worker(connection, receiver, worker_active, None));

        let (reply, dropped) = oneshot::channel();
        sender
            .send(WorkerRequest::Request {
                kind: RequestKind::Execute,
                source: "UPDATE item SET n=n+1 RETURN NONE".into(),
                params: Params::new(),
                options: QueryOptions::default(),
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
