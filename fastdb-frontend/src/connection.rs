//! FastDB database/connection wrapper and the single engine seam.
//!
//! Opens the engine with `SqliteDialect` (no custom dialect), following the
//! PostgreSQL frontend's open pattern. All engine access goes through
//! [`Connection`], which prepares directly-constructed Turso AST via
//! `Connection::prepare_translated_stmt_with_options`. FastDB input never
//! reaches the SQLite parser.

use crate::error::{FastDbError, Result};
use crate::execute;
use crate::test_failpoints::{Failpoint, Failpoints};
use crate::{Params, QueryResponse, StatementResult};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock, Weak};
use turso_core::Value;
use turso_parser::ast::Stmt;

/// An open FastDB database. Phase 0 uses one connection and one writer.
#[derive(Clone)]
pub struct Database {
    db: Arc<turso_core::Database>,
    coordinator: Arc<Coordinator>,
}

pub(crate) struct Coordinator {
    pub(crate) schema_mutex: Mutex<()>,
    pub(crate) catalog: RwLock<Option<crate::catalog::CatalogState>>,
    schema_lease: Mutex<Option<u64>>,
    schema_lease_changed: Condvar,
    next_connection_id: AtomicU64,
}

impl Coordinator {
    fn new() -> Self {
        Self {
            schema_mutex: Mutex::new(()),
            catalog: RwLock::new(None),
            schema_lease: Mutex::new(None),
            schema_lease_changed: Condvar::new(),
            next_connection_id: AtomicU64::new(1),
        }
    }

    fn connection_id(&self) -> u64 {
        self.next_connection_id.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn wait_for_catalog(&self, connection_id: u64) -> Result<()> {
        let mut owner = self
            .schema_lease
            .lock()
            .map_err(|_| FastDbError::Transaction("schema lease lock is poisoned".into()))?;
        while owner.is_some_and(|owner| owner != connection_id) {
            owner = self
                .schema_lease_changed
                .wait(owner)
                .map_err(|_| FastDbError::Transaction("schema lease lock is poisoned".into()))?;
        }
        Ok(())
    }

    pub(crate) fn acquire_schema_lease(&self, connection_id: u64) -> Result<()> {
        let mut owner = self
            .schema_lease
            .lock()
            .map_err(|_| FastDbError::Transaction("schema lease lock is poisoned".into()))?;
        while owner.is_some_and(|owner| owner != connection_id) {
            owner = self
                .schema_lease_changed
                .wait(owner)
                .map_err(|_| FastDbError::Transaction("schema lease lock is poisoned".into()))?;
        }
        *owner = Some(connection_id);
        Ok(())
    }

    pub(crate) fn release_schema_lease(&self, connection_id: u64) {
        if let Ok(mut owner) = self.schema_lease.lock() {
            if *owner == Some(connection_id) {
                *owner = None;
                self.schema_lease_changed.notify_all();
            }
        }
    }
}

static COORDINATORS: OnceLock<Mutex<HashMap<PathBuf, Weak<Coordinator>>>> = OnceLock::new();

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database").finish_non_exhaustive()
    }
}

impl Database {
    /// Open (or create) a file-backed database at `path`.
    pub fn open(path: &str) -> Result<Self> {
        let io = turso_core::Database::io_for_path(path)?;
        Self::open_with_io_inner(path, io)
    }

    fn open_with_io_inner(path: &str, io: Arc<dyn turso_core::IO>) -> Result<Self> {
        let flags = turso_core::OpenFlags::default();
        let file = io.open_file(path, flags, true)?;
        let db_file = Arc::new(turso_core::storage::database::DatabaseFile::new(file));
        let opts = turso_core::OpenOptions::new(Arc::new(turso_core::SqliteDialect))
            .storage(db_file)
            .flags(flags)
            .db_opts(turso_core::DatabaseOpts::default());
        let db = turso_core::Database::open(io, path, opts)?;
        let coordinator = coordinator_for_path(path)?;
        let database = Self { db, coordinator };
        database.initialize_catalog()?;
        Ok(database)
    }

    /// Open using a caller-supplied I/O implementation. This is exposed only
    /// for deterministic failure testing at real WAL completion boundaries.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn open_with_io(path: &str, io: Arc<dyn turso_core::IO>) -> Result<Self> {
        Self::open_with_io_inner(path, io)
    }

    /// Open a private in-memory database (used by unit tests).
    pub fn open_memory() -> Result<Self> {
        Self::open(":memory:")
    }

    /// Connect a single FastDB connection.
    pub fn connect(&self) -> Result<Connection> {
        let conn = self.db.connect()?;
        Ok(Connection::new(conn, self.coordinator.clone()))
    }

    fn initialize_catalog(&self) -> Result<()> {
        let connection = Connection::new(self.db.connect()?, self.coordinator.clone());
        let _schema_guard =
            self.coordinator.schema_mutex.lock().map_err(|_| {
                FastDbError::Transaction("database schema mutex is poisoned".into())
            })?;
        let loaded = crate::catalog::load_and_validate(&connection)?;
        let mut cached = self
            .coordinator
            .catalog
            .write()
            .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?;
        if let Some(existing) = cached.as_ref() {
            if existing != &loaded {
                return Err(FastDbError::Format(
                    "database catalog changed incompatibly across open wrappers".into(),
                ));
            }
        } else {
            *cached = Some(loaded);
        }
        Ok(())
    }
}

fn coordinator_for_path(path: &str) -> Result<Arc<Coordinator>> {
    if path == ":memory:" {
        return Ok(Arc::new(Coordinator::new()));
    }
    let key = normalized_database_path(path)?;
    let registry = COORDINATORS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut registry = registry.lock().map_err(|_| {
        FastDbError::Transaction("database coordinator registry is poisoned".into())
    })?;
    if let Some(coordinator) = registry.get(&key).and_then(Weak::upgrade) {
        return Ok(coordinator);
    }
    let coordinator = Arc::new(Coordinator::new());
    registry.insert(key, Arc::downgrade(&coordinator));
    Ok(coordinator)
}

fn normalized_database_path(path: &str) -> Result<PathBuf> {
    let path = Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute
        .parent()
        .ok_or_else(|| FastDbError::Io("database path has no parent".into()))?;
    let file_name = absolute
        .file_name()
        .ok_or_else(|| FastDbError::Io("database path has no file name".into()))?;
    let normalized_parent = parent
        .canonicalize()
        .unwrap_or_else(|_| parent.to_path_buf());
    Ok(normalized_parent.join(file_name))
}

/// A FastDB connection wrapping one Turso connection. Not `Send`/`Sync` in
/// Phase 0: one connection, one writer.
pub struct Connection {
    conn: Arc<turso_core::Connection>,
    pub(crate) coordinator: Arc<Coordinator>,
    failpoints: Failpoints,
    connection_id: u64,
    execution: Mutex<ExecutionState>,
}

pub(crate) struct ExecutionState {
    pub(crate) transaction: TransactionState,
}

pub(crate) enum TransactionState {
    Idle,
    Active(ActiveTransaction),
    Poisoned,
    Broken,
}

pub(crate) struct ActiveTransaction {
    pub(crate) catalog: crate::catalog::CatalogState,
    pub(crate) schema_changed: bool,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

impl Connection {
    pub(crate) fn new(conn: Arc<turso_core::Connection>, coordinator: Arc<Coordinator>) -> Self {
        let connection_id = coordinator.connection_id();
        Self {
            conn,
            coordinator,
            failpoints: Failpoints::default(),
            connection_id,
            execution: Mutex::new(ExecutionState {
                transaction: TransactionState::Idle,
            }),
        }
    }

    /// Execute one or more FastDB statements with no named parameters.
    pub fn execute(&self, source: &str) -> Result<QueryResponse> {
        self.execute_with_params(source, &Params::new())
    }

    /// Execute one or more statements with named value bindings.
    pub fn execute_with_params(&self, source: &str, params: &Params) -> Result<QueryResponse> {
        let mut execution = self.execution.lock().map_err(|_| {
            FastDbError::Transaction("connection execution lock is poisoned".into())
        })?;
        if matches!(execution.transaction, TransactionState::Broken) {
            return Err(FastDbError::Transaction(
                "connection transaction state is broken; close and reopen it".into(),
            ));
        }
        if let Err(error) = crate::validate_params(params) {
            return Err(self.poison_after_error(&mut execution, error));
        }

        let mut cursor = turso_fastdb_parser::StatementCursor::new(source);
        let mut statements = Vec::new();
        loop {
            let statement = match cursor.next_statement() {
                Ok(Some(statement)) => statement,
                Ok(None) => break,
                Err(error) => {
                    let error = FastDbError::from(error);
                    return Err(self.poison_after_error(&mut execution, error));
                }
            };
            match execute::run_statement(self, &mut execution, statement, source, params) {
                Ok(result) => statements.push(result),
                Err(error) => return Err(self.poison_after_error(&mut execution, error)),
            }
        }
        Ok(QueryResponse::new(statements))
    }

    pub(crate) fn begin_explicit(&self, state: &mut ExecutionState) -> Result<StatementResult> {
        match state.transaction {
            TransactionState::Idle => {}
            TransactionState::Active(_) => {
                return Err(FastDbError::Transaction(
                    "an explicit transaction is already active".into(),
                ))
            }
            TransactionState::Poisoned => {
                return Err(FastDbError::Transaction(
                    "transaction is poisoned; CANCEL is required".into(),
                ))
            }
            TransactionState::Broken => {
                return Err(FastDbError::Transaction(
                    "connection transaction state is broken; close and reopen it".into(),
                ))
            }
        }
        self.coordinator.wait_for_catalog(self.connection_id)?;
        self.exec_bound(crate::lower::begin_immediate(), vec![])?;
        let catalog = self
            .coordinator
            .catalog
            .read()
            .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?
            .clone()
            .ok_or_else(|| FastDbError::Engine("catalog cache was not initialized".into()))?;
        state.transaction = TransactionState::Active(ActiveTransaction {
            catalog,
            schema_changed: false,
        });
        Ok(StatementResult::None)
    }

    pub(crate) fn commit_explicit(&self, state: &mut ExecutionState) -> Result<StatementResult> {
        let TransactionState::Active(active) = &state.transaction else {
            return Err(match state.transaction {
                TransactionState::Poisoned => {
                    FastDbError::Transaction("transaction is poisoned; CANCEL is required".into())
                }
                TransactionState::Broken => FastDbError::Transaction(
                    "connection transaction state is broken; close and reopen it".into(),
                ),
                TransactionState::Idle => {
                    FastDbError::Transaction("no explicit transaction is active".into())
                }
                TransactionState::Active(_) => unreachable!(),
            });
        };
        let schema_changed = active.schema_changed;
        let candidate = schema_changed.then(|| active.catalog.clone());
        self.check_failpoint(Failpoint::CommitFailure)?;
        self.exec_bound(crate::lower::commit(), vec![])?;
        if let Some(candidate) = candidate {
            let publication = self.coordinator.catalog.write().map(|mut cache| {
                *cache = Some(candidate);
            });
            if publication.is_err() {
                state.transaction = TransactionState::Broken;
                self.coordinator.release_schema_lease(self.connection_id);
                return Err(FastDbError::Transaction(
                    "commit succeeded but catalog publication failed; close and reopen the connection"
                        .into(),
                ));
            }
        }
        state.transaction = TransactionState::Idle;
        self.coordinator.release_schema_lease(self.connection_id);
        Ok(StatementResult::None)
    }

    pub(crate) fn cancel_explicit(&self, state: &mut ExecutionState) -> Result<StatementResult> {
        match state.transaction {
            TransactionState::Active(_) => {
                let rollback = self
                    .check_failpoint(Failpoint::RollbackFailure)
                    .and_then(|()| self.exec_bound(crate::lower::rollback(), vec![]));
                self.coordinator.release_schema_lease(self.connection_id);
                match rollback {
                    Ok(()) => state.transaction = TransactionState::Idle,
                    Err(error) => {
                        state.transaction = TransactionState::Broken;
                        return Err(FastDbError::Transaction(format!(
                            "explicit transaction rollback failed: {error}"
                        )));
                    }
                }
            }
            TransactionState::Poisoned => state.transaction = TransactionState::Idle,
            TransactionState::Idle => {
                return Err(FastDbError::Transaction(
                    "no explicit transaction is active".into(),
                ))
            }
            TransactionState::Broken => {
                return Err(FastDbError::Transaction(
                    "connection transaction state is broken; close and reopen it".into(),
                ))
            }
        }
        Ok(StatementResult::None)
    }

    fn poison_after_error(&self, state: &mut ExecutionState, error: FastDbError) -> FastDbError {
        if !matches!(state.transaction, TransactionState::Active(_)) {
            return error;
        }
        let rollback = self
            .check_failpoint(Failpoint::RollbackFailure)
            .and_then(|()| self.exec_bound(crate::lower::rollback(), vec![]));
        self.coordinator.release_schema_lease(self.connection_id);
        match rollback {
            Ok(()) => {
                state.transaction = TransactionState::Poisoned;
                error
            }
            Err(rollback_error) => {
                state.transaction = TransactionState::Broken;
                FastDbError::Transaction(format!(
                    "transaction failed and rollback cleanup failed; original: {error}; rollback: {rollback_error}"
                ))
            }
        }
    }

    pub(crate) fn wait_for_catalog(&self) -> Result<()> {
        self.coordinator.wait_for_catalog(self.connection_id)
    }

    pub(crate) fn acquire_schema_lease(&self) -> Result<()> {
        self.coordinator.acquire_schema_lease(self.connection_id)
    }

    /// The underlying Turso connection (test-only diagnostics: PRAGMA,
    /// EXPLAIN QUERY PLAN, integrity checks). Never used to run FastDB input.
    #[doc(hidden)]
    pub fn native(&self) -> &Arc<turso_core::Connection> {
        &self.conn
    }

    // ---- crate-private engine runner ----

    pub(crate) fn prepare_translated(&self, stmt: Stmt) -> Result<turso_core::Statement> {
        let s = self.conn.prepare_translated_stmt_with_options(
            stmt,
            crate::lower::TRANSLATED_INPUT,
            &turso_core::PrepareOptions::default(),
        )?;
        Ok(s)
    }

    /// Prepare, bind, and run a statement that returns no rows.
    pub(crate) fn exec_bound(&self, stmt: Stmt, bindings: crate::lower::Bindings) -> Result<()> {
        let mut s = self.prepare_translated(stmt)?;
        bind_all(&mut s, &bindings)?;
        s.run_ignore_rows()?;
        Ok(())
    }

    /// Prepare and bind a statement, returning it un-run so a failpoint can
    /// fire before execution.
    pub(crate) fn prepare_bound(
        &self,
        stmt: Stmt,
        bindings: crate::lower::Bindings,
    ) -> Result<turso_core::Statement> {
        let mut s = self.prepare_translated(stmt)?;
        bind_all(&mut s, &bindings)?;
        Ok(s)
    }

    /// Check a deterministic failpoint. No-op unless armed via the `testing`
    /// feature. Used by `execute.rs` to drive the real rollback path.
    pub(crate) fn check_failpoint(&self, fp: Failpoint) -> Result<()> {
        self.failpoints.check(fp)
    }

    pub(crate) fn with_transaction<R>(&self, body: impl FnOnce() -> Result<R>) -> Result<R> {
        self.exec_bound(crate::lower::begin_immediate(), vec![])?;
        let outcome = body().and_then(|result| {
            self.check_failpoint(Failpoint::CommitFailure)?;
            self.exec_bound(crate::lower::commit(), vec![])
                .map(|()| result)
        });
        match outcome {
            Ok(result) => Ok(result),
            Err(error) => match self
                .check_failpoint(Failpoint::RollbackFailure)
                .and_then(|()| self.exec_bound(crate::lower::rollback(), vec![]))
            {
                Ok(()) => Err(error),
                Err(rollback_error) => {
                    match self.exec_bound(crate::lower::begin_immediate(), vec![]) {
                        Ok(()) => match self.exec_bound(crate::lower::rollback(), vec![]) {
                            Ok(()) => Err(error),
                            Err(probe_rollback) => Err(FastDbError::Transaction(format!(
                            "transaction failed; original: {error}; rollback reported: \
                             {rollback_error}; cleanup probe rollback also failed: {probe_rollback}"
                        ))),
                        },
                        Err(probe) => Err(FastDbError::Transaction(format!(
                            "transaction failed; original: {error}; rollback also failed: \
                         {rollback_error}; clean-state probe failed: {probe}"
                        ))),
                    }
                }
            },
        }
    }

    /// Arm a deterministic failpoint (test-only).
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn arm_failpoint(&self, fp: Failpoint) {
        self.failpoints.arm(fp);
    }

    /// Disarm all failpoints (test-only).
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn disarm_all_failpoints(&self) {
        self.failpoints.disarm_all();
    }

    /// Reload and validate the persisted catalog into the shared cache.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn reload_catalog(&self) -> Result<()> {
        let _schema_guard =
            self.coordinator.schema_mutex.lock().map_err(|_| {
                FastDbError::Transaction("database schema mutex is poisoned".into())
            })?;
        let loaded = crate::catalog::load_and_validate(self)?;
        let mut cache = self
            .coordinator
            .catalog
            .write()
            .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?;
        *cache = Some(loaded);
        Ok(())
    }

    /// Return the shared immutable catalog snapshot for test assertions.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn catalog_state(&self) -> Result<crate::catalog::CatalogState> {
        self.coordinator
            .catalog
            .read()
            .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?
            .clone()
            .ok_or_else(|| FastDbError::Engine("catalog cache was not initialized".into()))
    }

    /// Explain the actual canonical composite filter lowering.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn explain_filters(
        &self,
        logical_table: &str,
        filters: &[(&str, crate::Value)],
    ) -> Result<Vec<String>> {
        let state = self.catalog_state()?;
        let table = state
            .snapshot()
            .and_then(|snapshot| snapshot.tables.get(logical_table))
            .ok_or_else(|| {
                FastDbError::Engine(format!("table {logical_table:?} is not registered"))
            })?;
        let filters = filters
            .iter()
            .map(|(field, value)| Ok((crate::path::canonical_path([*field])?, value.clone())))
            .collect::<Result<Vec<_>>>()?;
        let (statement, _) =
            crate::lower::physical_select_stmt(&table.physical_name, None, &filters)?;
        explain_statement(self, statement)
    }

    /// Explain the candidate scan built for a parameterized FastDB SELECT.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn explain_query_with_params(
        &self,
        source: &str,
        params: &crate::Params,
    ) -> Result<Vec<String>> {
        crate::validate_params(params)?;
        let statement = turso_fastdb_parser::parse_one(source)?;
        let turso_fastdb_parser::Statement::Select(statement) = statement else {
            return Err(FastDbError::Schema(
                "explain_query_with_params requires SELECT".into(),
            ));
        };
        let statement = crate::execute::lowered_select_for_explain(self, statement, params)?;
        explain_statement(self, statement)
    }

    /// Test-only: install the canonical non-unique expression index on a
    /// top-level field of a logical table, returning the opaque index name.
    /// Resolves the table through the catalog and reuses the same canonical
    /// JSON expression builder as the filter lowering. Not a public API.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn create_field_index(&self, logical_table: &str, field: &str) -> Result<String> {
        let logical_index = "phase0_field_index";
        self.execute(&format!(
            "DEFINE INDEX {logical_index} ON TABLE {logical_table} FIELDS {field}"
        ))?;
        let state = self
            .coordinator
            .catalog
            .read()
            .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?;
        let snapshot = state
            .as_ref()
            .and_then(crate::catalog::CatalogState::snapshot)
            .ok_or_else(|| {
                FastDbError::Engine("catalog disappeared after index definition".into())
            })?;
        snapshot
            .tables
            .get(logical_table)
            .and_then(|table| table.indexes.get(logical_index))
            .map(|index| index.physical_name.clone())
            .ok_or_else(|| FastDbError::Engine("index disappeared after definition".into()))
    }

    /// Test-only: explain the actual lowered FastDB filter statement. The
    /// public engine API cannot prepare an `ExplainQueryPlan(Stmt)` directly,
    /// so this helper renders that command and first proves that reparsing it
    /// produces the structurally identical AST before asking the engine for
    /// the plan.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn explain_field_filter(&self, logical_table: &str, field: &str) -> Result<Vec<String>> {
        let state = self
            .coordinator
            .catalog
            .read()
            .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?;
        let resolved = state
            .as_ref()
            .and_then(crate::catalog::CatalogState::snapshot)
            .and_then(|snapshot| snapshot.tables.get(logical_table))
            .ok_or_else(|| {
                FastDbError::Engine(format!("table {logical_table:?} is not registered"))
            })?;
        let path = crate::path::canonical_path([field])?;
        // The exact translated statement the filter lowering builds.
        let (select_stmt, _bindings) = crate::lower::physical_select_stmt(
            &resolved.physical_name,
            None,
            &[(path, crate::Value::Str("x".into()))],
        )?;
        explain_statement(self, select_stmt)
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        if let Ok(state) = self.execution.get_mut() {
            if matches!(
                state.transaction,
                TransactionState::Active(_) | TransactionState::Broken
            ) {
                let _ = self.exec_bound(crate::lower::rollback(), vec![]);
            }
        }
        self.coordinator.release_schema_lease(self.connection_id);
    }
}

#[cfg(feature = "testing")]
fn explain_statement(connection: &Connection, select_stmt: Stmt) -> Result<Vec<String>> {
    let explain_cmd = turso_parser::ast::Cmd::ExplainQueryPlan(select_stmt);
    let explain_sql = explain_cmd.to_string();
    let mut reparsed = turso_parser::parser::Parser::new(explain_sql.as_bytes())
        .next()
        .transpose()
        .map_err(|e| FastDbError::Engine(format!("failed to reparse explain AST: {e}")))?
        .ok_or_else(|| FastDbError::Engine("empty rendered explain command".into()))?;
    strip_parser_implicit_result_names(&mut reparsed);
    if reparsed != explain_cmd {
        return Err(FastDbError::Engine(format!(
            "rendered explain command did not round-trip to the lowered AST; \
                 expected {explain_cmd:?}, got {reparsed:?}"
        )));
    }
    // EXPLAIN QUERY PLAN does not execute, so `?1` needs no binding.
    let mut stmt = connection.conn.prepare(&explain_sql)?;
    let mut plans = Vec::new();
    stmt.run_with_row_callback(|row| {
        plans.push(row.get::<String>(3)?);
        Ok(())
    })?;
    Ok(plans)
}

impl Connection {
    /// Prepare, bind, and run a statement, collecting every row's raw values.
    pub(crate) fn collect_rows(
        &self,
        stmt: Stmt,
        bindings: crate::lower::Bindings,
    ) -> Result<Vec<Vec<Value>>> {
        let mut s = self.prepare_translated(stmt)?;
        bind_all(&mut s, &bindings)?;
        let mut rows: Vec<Vec<Value>> = Vec::new();
        s.run_with_row_callback(|row| {
            rows.push(row.get_values().cloned().collect());
            Ok(())
        })?;
        Ok(rows)
    }
}

/// The SQLite parser annotates unaliased result expressions with their source
/// text. Directly constructed AST omits that display-only metadata; it does
/// not affect planning, so remove it before the test-only round-trip check.
#[cfg(feature = "testing")]
fn strip_parser_implicit_result_names(cmd: &mut turso_parser::ast::Cmd) {
    use turso_parser::ast::{As, Cmd, OneSelect, ResultColumn, Stmt};

    let stmt = match cmd {
        Cmd::Stmt(stmt) | Cmd::Explain(stmt) | Cmd::ExplainQueryPlan(stmt) => stmt,
    };
    let Stmt::Select(select) = stmt else {
        return;
    };
    let OneSelect::Select { columns, .. } = &mut select.body.select else {
        return;
    };
    for column in columns {
        let ResultColumn::Expr(_, alias) = column else {
            continue;
        };
        if matches!(alias, Some(As::ImplicitColumnName(_))) {
            *alias = None;
        }
    }
}

fn bind_all(stmt: &mut turso_core::Statement, bindings: &[Value]) -> Result<()> {
    for (i, v) in bindings.iter().enumerate() {
        let idx = NonZeroUsize::new(i + 1).expect("nonzero bind index");
        stmt.bind_at(idx, v.clone())?;
    }
    Ok(())
}

/// Convert a single engine [`Value`] to a `String` (text) or error.
pub(crate) fn value_to_string(v: &Value) -> Result<String> {
    match v {
        Value::Text(t) => Ok(t.as_str().to_string()),
        other => Err(FastDbError::Engine(format!(
            "expected text value, got {other:?}"
        ))),
    }
}
