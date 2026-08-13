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
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock, Weak};
use turso_core::Value;
use turso_parser::ast::Stmt;

/// Result of the supported catalog/provider/engine integrity path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub format_version: i64,
    pub tables: usize,
    pub indexes: usize,
    pub fts_indexes: usize,
    pub vector_fields: usize,
    pub pinned_fts_exception: bool,
}

/// An open FastDB database. Phase 0 uses one connection and one writer.
#[derive(Clone)]
pub struct Database {
    db: Arc<turso_core::Database>,
    coordinator: Arc<Coordinator>,
    path: PathBuf,
}

pub(crate) struct Coordinator {
    pub(crate) schema_mutex: Mutex<()>,
    pub(crate) catalog: RwLock<Option<crate::catalog::CatalogState>>,
    maintenance: RwLock<()>,
    schema_lease: Mutex<Option<u64>>,
    schema_lease_changed: Condvar,
    next_connection_id: AtomicU64,
    catalog_generation: AtomicU64,
    active_transactions: AtomicU64,
}

impl Coordinator {
    fn new() -> Self {
        Self {
            schema_mutex: Mutex::new(()),
            catalog: RwLock::new(None),
            maintenance: RwLock::new(()),
            schema_lease: Mutex::new(None),
            schema_lease_changed: Condvar::new(),
            next_connection_id: AtomicU64::new(1),
            catalog_generation: AtomicU64::new(0),
            active_transactions: AtomicU64::new(0),
        }
    }

    fn connection_id(&self) -> u64 {
        self.next_connection_id.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn catalog_generation(&self) -> u64 {
        self.catalog_generation.load(Ordering::Acquire)
    }

    pub(crate) fn publish_catalog_generation(&self) {
        self.catalog_generation.fetch_add(1, Ordering::AcqRel);
    }

    fn ensure_no_active_transactions(&self) -> Result<()> {
        if self.active_transactions.load(Ordering::Acquire) == 0 {
            Ok(())
        } else {
            Err(FastDbError::Transaction(
                "database maintenance requires all explicit transactions to finish".into(),
            ))
        }
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
        Self::open_with_io_inner(path, io, None)
    }

    fn open_with_io_inner(
        path: &str,
        io: Arc<dyn turso_core::IO>,
        catalog_failpoint: Option<Failpoint>,
    ) -> Result<Self> {
        let flags = turso_core::OpenFlags::default();
        let file = io.open_file(path, flags, true)?;
        let db_file = Arc::new(turso_core::storage::database::DatabaseFile::new(file));
        let opts = turso_core::OpenOptions::new(Arc::new(turso_core::SqliteDialect))
            .storage(db_file)
            .flags(flags)
            .db_opts(turso_core::DatabaseOpts::default().with_index_method(true));
        let db = turso_core::Database::open(io, path, opts)?;
        let coordinator = coordinator_for_path(path)?;
        let stored_path = if path == ":memory:" {
            PathBuf::from(path)
        } else {
            normalized_database_path(path)?
        };
        let database = Self {
            db,
            coordinator,
            path: stored_path,
        };
        database.initialize_catalog(catalog_failpoint)?;
        Ok(database)
    }

    /// Open using a caller-supplied I/O implementation. This is exposed only
    /// for deterministic failure testing at real WAL completion boundaries.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn open_with_io(path: &str, io: Arc<dyn turso_core::IO>) -> Result<Self> {
        Self::open_with_io_inner(path, io, None)
    }

    /// Open with one catalog failpoint armed before format migration starts.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn open_with_catalog_failpoint(path: &str, failpoint: Failpoint) -> Result<Self> {
        let io = turso_core::Database::io_for_path(path)?;
        Self::open_with_io_inner(path, io, Some(failpoint))
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

    /// Validate catalogs, provider-derived state, physical objects, and the
    /// pinned engine integrity result through one supported path.
    pub fn check(&self) -> Result<CheckReport> {
        let _maintenance = self
            .coordinator
            .maintenance
            .write()
            .map_err(|_| FastDbError::Transaction("maintenance lock is poisoned".into()))?;
        self.coordinator.ensure_no_active_transactions()?;
        let connection = Connection::new(self.db.connect()?, self.coordinator.clone());
        let state = crate::catalog::load_and_validate(&connection)?;
        let (tables, indexes, fts_indexes, vector_fields) = match &state {
            crate::catalog::CatalogState::Empty => (0, 0, 0, 0),
            crate::catalog::CatalogState::Ready(snapshot) => {
                let indexes = snapshot
                    .tables
                    .values()
                    .map(|table| table.indexes.len())
                    .sum();
                let fts_indexes = snapshot
                    .tables
                    .values()
                    .flat_map(|table| table.indexes.values())
                    .filter(|index| index.provider == crate::catalog::Provider::BuiltinFts)
                    .count();
                let vector_fields = snapshot
                    .hidden_columns
                    .values()
                    .filter(|column| column.provider == crate::catalog::Provider::BuiltinVector)
                    .count();
                (snapshot.tables.len(), indexes, fts_indexes, vector_fields)
            }
        };
        let allowed_fts_diagnostics = match &state {
            crate::catalog::CatalogState::Empty => BTreeSet::new(),
            crate::catalog::CatalogState::Ready(snapshot) => snapshot
                .tables
                .values()
                .flat_map(|table| table.indexes.values())
                .filter(|index| index.provider == crate::catalog::Provider::BuiltinFts)
                .map(|index| {
                    format!(
                        "wrong # of entries in index __turso_internal_fts_dir_{}_key",
                        index.physical_name
                    )
                })
                .collect(),
        };
        let mut statement = connection.conn.prepare("PRAGMA integrity_check")?;
        let mut diagnostics = Vec::new();
        statement.run_with_row_callback(|row| {
            diagnostics.push(row.get::<String>(0)?);
            Ok(())
        })?;
        if diagnostics.is_empty() {
            return Err(FastDbError::Format(
                "engine integrity check returned no result".into(),
            ));
        }
        let mut pinned_fts_exception = false;
        for diagnostic in diagnostics {
            if diagnostic == "ok" {
                continue;
            }
            if allowed_fts_diagnostics.contains(&diagnostic) {
                pinned_fts_exception = true;
                continue;
            }
            return Err(FastDbError::Format(format!(
                "engine integrity check failed: {diagnostic}"
            )));
        }
        connection.close()?;
        Ok(CheckReport {
            format_version: crate::catalog::FORMAT_VERSION,
            tables,
            indexes,
            fts_indexes,
            vector_fields,
            pinned_fts_exception,
        })
    }

    /// Create one checkpointed, validated backup without overwriting a path.
    pub fn backup_to(&self, destination: impl AsRef<Path>) -> Result<CheckReport> {
        self.backup_to_inner(destination.as_ref(), None)
    }

    /// Test-only entry point for proving that a durable temporary copy is not
    /// published when backup is interrupted before validation.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn backup_to_with_failpoint(
        &self,
        destination: impl AsRef<Path>,
        failpoint: Failpoint,
    ) -> Result<CheckReport> {
        self.backup_to_inner(destination.as_ref(), Some(failpoint))
    }

    fn backup_to_inner(
        &self,
        destination: &Path,
        failpoint: Option<Failpoint>,
    ) -> Result<CheckReport> {
        if self.path == Path::new(":memory:") {
            return Err(FastDbError::Io(
                "an in-memory database cannot be backed up to a file".into(),
            ));
        }
        let source = self.path.clone();
        let destination_normalized = normalized_output_path(destination)?;
        if source == destination_normalized {
            return Err(FastDbError::Io(
                "backup destination must differ from the source".into(),
            ));
        }
        if destination.exists() {
            return Err(FastDbError::Io("backup destination already exists".into()));
        }
        let parent = destination
            .parent()
            .ok_or_else(|| FastDbError::Io("backup destination has no parent directory".into()))?;
        let file_name = destination
            .file_name()
            .ok_or_else(|| FastDbError::Io("backup destination has no file name".into()))?;
        let temporary = parent.join(format!(
            ".{}.fastdb-backup-{}.tmp",
            file_name.to_string_lossy(),
            uuid::Uuid::new_v4()
        ));
        let _maintenance = self
            .coordinator
            .maintenance
            .write()
            .map_err(|_| FastDbError::Transaction("maintenance lock is poisoned".into()))?;
        self.coordinator.ensure_no_active_transactions()?;
        let result = (|| {
            let connection = Connection::new(self.db.connect()?, self.coordinator.clone());
            connection
                .conn
                .checkpoint(turso_core::CheckpointMode::Truncate {
                    upper_bound_inclusive: None,
                })?;
            std::fs::copy(&source, &temporary)?;
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&temporary)?
                .sync_all()?;
            if failpoint == Some(Failpoint::AfterBackupCopy) {
                return Err(FastDbError::Transaction(
                    "injected failure: AfterBackupCopy".into(),
                ));
            }
            connection.close()?;
            let temporary_text = temporary
                .to_str()
                .ok_or_else(|| FastDbError::Io("backup path is not valid UTF-8".into()))?;
            let report = Database::open(temporary_text)?.check()?;
            std::fs::rename(&temporary, destination)?;
            sync_parent(parent)?;
            Ok(report)
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    }

    /// Rebuild one catalog-resolved index without constructing FastDB source.
    pub fn rebuild_index(&self, table: &str, index: &str) -> Result<()> {
        let _maintenance = self
            .coordinator
            .maintenance
            .write()
            .map_err(|_| FastDbError::Transaction("maintenance lock is poisoned".into()))?;
        self.coordinator.ensure_no_active_transactions()?;
        let _schema =
            self.coordinator.schema_mutex.lock().map_err(|_| {
                FastDbError::Transaction("database schema mutex is poisoned".into())
            })?;
        let catalog = self
            .coordinator
            .catalog
            .read()
            .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?
            .clone()
            .ok_or_else(|| FastDbError::Engine("catalog cache was not initialized".into()))?;
        let snapshot = match catalog {
            crate::catalog::CatalogState::Ready(snapshot) => snapshot,
            crate::catalog::CatalogState::Empty => {
                return Err(FastDbError::Schema("database has no indexes".into()))
            }
        };
        let definition = snapshot
            .tables
            .get(table)
            .and_then(|table| table.indexes.get(index))
            .ok_or_else(|| {
                FastDbError::Schema(format!("index {index:?} is not defined on table {table:?}"))
            })?;
        let connection = Connection::new(self.db.connect()?, self.coordinator.clone());
        connection.with_transaction(|| {
            connection.exec_bound(
                crate::provider::index_provider(definition)?.rebuild_statement(definition)?,
                vec![],
            )
        })?;
        connection.close()
    }

    fn initialize_catalog(&self, catalog_failpoint: Option<Failpoint>) -> Result<()> {
        let connection = Connection::new(self.db.connect()?, self.coordinator.clone());
        #[cfg(feature = "testing")]
        if let Some(failpoint) = catalog_failpoint {
            connection.failpoints.arm(failpoint);
        }
        #[cfg(not(feature = "testing"))]
        debug_assert!(catalog_failpoint.is_none());
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
            self.coordinator.publish_catalog_generation();
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

fn normalized_output_path(path: &Path) -> Result<PathBuf> {
    let text = path
        .to_str()
        .ok_or_else(|| FastDbError::Io("output path is not valid UTF-8".into()))?;
    normalized_database_path(text)
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> Result<()> {
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> Result<()> {
    Ok(())
}

/// A FastDB connection wrapping one Turso connection. Not `Send`/`Sync` in
/// Phase 0: one connection, one writer.
pub struct Connection {
    conn: Arc<turso_core::Connection>,
    pub(crate) coordinator: Arc<Coordinator>,
    failpoints: Failpoints,
    connection_id: u64,
    execution: Mutex<ExecutionState>,
    parse_cache: Mutex<ParseCache>,
    prepared_select_cache: Mutex<PreparedSelectCache>,
}

const PARSE_CACHE_MAX_ENTRIES: usize = 128;
const PARSE_CACHE_MAX_BYTES: usize = 4 * 1024 * 1024;
const PARSE_CACHE_MAX_SOURCE_BYTES: usize = 64 * 1024;
const PREPARED_SELECT_CACHE_MAX_ENTRIES: usize = 64;

#[derive(Default)]
struct ParseCache {
    entries: VecDeque<ParsedSource>,
    source_bytes: usize,
    hits: u64,
    misses: u64,
}

struct ParsedSource {
    source: String,
    statements: Vec<turso_fastdb_parser::Statement>,
}

impl ParseCache {
    fn get(&mut self, source: &str) -> Option<Vec<turso_fastdb_parser::Statement>> {
        if source.len() > PARSE_CACHE_MAX_SOURCE_BYTES {
            self.misses += 1;
            return None;
        }
        let Some(position) = self.entries.iter().position(|entry| entry.source == source) else {
            self.misses += 1;
            return None;
        };
        let entry = self
            .entries
            .remove(position)
            .expect("parse cache position came from the same deque");
        let statements = entry.statements.clone();
        self.entries.push_back(entry);
        self.hits += 1;
        Some(statements)
    }

    fn insert(&mut self, source: &str, statements: Vec<turso_fastdb_parser::Statement>) {
        if source.len() > PARSE_CACHE_MAX_SOURCE_BYTES {
            return;
        }
        if let Some(position) = self.entries.iter().position(|entry| entry.source == source) {
            let previous = self
                .entries
                .remove(position)
                .expect("parse cache position came from the same deque");
            self.source_bytes -= previous.source.len();
        }
        self.source_bytes += source.len();
        self.entries.push_back(ParsedSource {
            source: source.to_owned(),
            statements,
        });
        while self.entries.len() > PARSE_CACHE_MAX_ENTRIES
            || self.source_bytes > PARSE_CACHE_MAX_BYTES
        {
            let evicted = self
                .entries
                .pop_front()
                .expect("an over-limit parse cache cannot be empty");
            self.source_bytes -= evicted.source.len();
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.source_bytes = 0;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PreparedSelectKey {
    catalog_generation: u64,
    physical_table: String,
    uses_rid: bool,
    predicates: Vec<(String, crate::lower::PredicateOperator, ScalarKind)>,
    provider_plan: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScalarKind {
    Null,
    Bool,
    Integer,
    Float,
    String,
}

struct PreparedSelectEntry {
    key: PreparedSelectKey,
    statement: turso_core::Statement,
}

#[derive(Default)]
struct PreparedSelectCache {
    generation: u64,
    entries: VecDeque<PreparedSelectEntry>,
    hits: u64,
    misses: u64,
}

impl PreparedSelectCache {
    fn sync_generation(&mut self, generation: u64) {
        if self.generation != generation {
            self.entries.clear();
            self.generation = generation;
        }
    }

    fn take(&mut self, key: &PreparedSelectKey) -> Option<turso_core::Statement> {
        let Some(position) = self.entries.iter().position(|entry| &entry.key == key) else {
            self.misses += 1;
            return None;
        };
        self.hits += 1;
        self.entries.remove(position).map(|entry| entry.statement)
    }

    fn insert(&mut self, key: PreparedSelectKey, statement: turso_core::Statement) {
        if self.entries.len() == PREPARED_SELECT_CACHE_MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries
            .push_back(PreparedSelectEntry { key, statement });
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
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
    pub(crate) dirty_fts_tables: BTreeSet<crate::names::CatalogId>,
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
            parse_cache: Mutex::new(ParseCache::default()),
            prepared_select_cache: Mutex::new(PreparedSelectCache::default()),
        }
    }

    /// Execute one or more FastDB statements with no named parameters.
    pub fn execute(&self, source: &str) -> Result<QueryResponse> {
        self.execute_with_params(source, &Params::new())
    }

    /// Execute one or more statements with named value bindings.
    pub fn execute_with_params(&self, source: &str, params: &Params) -> Result<QueryResponse> {
        let _maintenance = self
            .coordinator
            .maintenance
            .read()
            .map_err(|_| FastDbError::Transaction("maintenance lock is poisoned".into()))?;
        let mut execution = self.execution.lock().map_err(|_| {
            FastDbError::Transaction("connection execution lock is poisoned".into())
        })?;
        if matches!(execution.transaction, TransactionState::Broken) {
            return Err(FastDbError::Transaction(
                "connection transaction state is broken; close and reopen it".into(),
            ));
        }
        if let Err(error) = crate::validate_params(params) {
            self.invalidate_caches();
            return Err(self.poison_after_error(&mut execution, error));
        }

        let mut statements = Vec::new();
        let mut mutation_count = 0_u64;
        let cached = self
            .parse_cache
            .lock()
            .map_err(|_| FastDbError::Engine("parse cache lock is poisoned".into()))?
            .get(source);
        let mut parsed_for_cache = Vec::new();
        let mut cursor = cached
            .is_none()
            .then(|| turso_fastdb_parser::StatementCursor::new(source));
        let mut cached = cached.unwrap_or_default().into_iter();
        loop {
            let statement = if let Some(cursor) = cursor.as_mut() {
                match cursor.next_statement() {
                    Ok(Some(statement)) => {
                        parsed_for_cache.push(statement.clone());
                        Some(statement)
                    }
                    Ok(None) => None,
                    Err(error) => {
                        self.invalidate_caches();
                        let error = FastDbError::from(error);
                        return Err(self.poison_after_error(&mut execution, error));
                    }
                }
            } else {
                cached.next()
            };
            let Some(statement) = statement else {
                break;
            };
            match execute::run_statement(self, &mut execution, statement, source, params) {
                Ok(result) => {
                    statements.push(result.result);
                    mutation_count = mutation_count
                        .checked_add(result.mutation_count)
                        .ok_or_else(|| {
                            FastDbError::Engine("request mutation count overflowed u64".into())
                        })?;
                }
                Err(error) => {
                    self.invalidate_caches();
                    return Err(self.poison_after_error(&mut execution, error));
                }
            }
        }
        if cursor.is_some() {
            self.parse_cache
                .lock()
                .map_err(|_| FastDbError::Engine("parse cache lock is poisoned".into()))?
                .insert(source, parsed_for_cache);
        }
        Ok(QueryResponse::new(statements, mutation_count))
    }

    /// Cooperatively interrupt the currently active engine statement.
    pub fn interrupt(&self) {
        self.conn.interrupt();
    }

    /// Close this connection and request the engine's clean-shutdown checkpoint.
    pub fn close(&self) -> Result<()> {
        self.invalidate_caches();
        if let Ok(mut execution) = self.execution.lock() {
            if matches!(execution.transaction, TransactionState::Active(_)) {
                self.coordinator
                    .active_transactions
                    .fetch_sub(1, Ordering::AcqRel);
                execution.transaction = TransactionState::Broken;
                self.coordinator.release_schema_lease(self.connection_id);
            }
        }
        self.conn.close().map_err(FastDbError::from)
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
        self.invalidate_prepared_cache();
        self.exec_bound(crate::lower::begin_immediate(), vec![])?;
        self.coordinator
            .active_transactions
            .fetch_add(1, Ordering::AcqRel);
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
            dirty_fts_tables: BTreeSet::new(),
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
            self.coordinator.publish_catalog_generation();
        }
        state.transaction = TransactionState::Idle;
        self.coordinator
            .active_transactions
            .fetch_sub(1, Ordering::AcqRel);
        self.coordinator.release_schema_lease(self.connection_id);
        self.invalidate_prepared_cache();
        Ok(StatementResult::None)
    }

    pub(crate) fn cancel_explicit(&self, state: &mut ExecutionState) -> Result<StatementResult> {
        match state.transaction {
            TransactionState::Active(_) => {
                let rollback = self
                    .check_failpoint(Failpoint::RollbackFailure)
                    .and_then(|()| self.exec_bound(crate::lower::rollback(), vec![]));
                self.coordinator.release_schema_lease(self.connection_id);
                self.coordinator
                    .active_transactions
                    .fetch_sub(1, Ordering::AcqRel);
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
        self.invalidate_prepared_cache();
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
        self.coordinator
            .active_transactions
            .fetch_sub(1, Ordering::AcqRel);
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
        self.coordinator.publish_catalog_generation();
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

    /// Install a test-only hidden-column provider fixture. Both names are
    /// opaque IDs; no logical identifier or source text reaches Turso.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn install_test_provider(&self, encoded_document: &str) -> Result<TestProviderHandle> {
        let table = crate::names::physical_table_name(crate::names::CatalogId::new_random());
        let hidden_column =
            crate::names::physical_hidden_column_name(crate::names::CatalogId::new_random());
        let derived = crate::provider::derive_test_hidden_value(encoded_document)?;
        self.with_transaction(|| {
            self.exec_bound(
                crate::lower::test_provider_table_ddl(&table, &hidden_column)?,
                vec![],
            )?;
            let (insert, bindings) = crate::lower::test_provider_insert_stmt(
                &table,
                &hidden_column,
                encoded_document,
                derived,
            )?;
            self.exec_bound(insert, bindings)
        })?;
        Ok(TestProviderHandle {
            table,
            hidden_column,
        })
    }

    /// Update a test provider's document and derived state in one real
    /// transaction, with a failure boundary between the physical writes.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn write_test_provider(
        &self,
        handle: &TestProviderHandle,
        encoded_document: &str,
    ) -> Result<()> {
        let derived = crate::provider::derive_test_hidden_value(encoded_document)?;
        self.with_transaction(|| {
            let (update, bindings) =
                crate::lower::test_provider_update_document_stmt(&handle.table, encoded_document)?;
            self.exec_bound(update, bindings)?;
            self.check_failpoint(Failpoint::AfterTestProviderDocument)?;
            let (update, bindings) = crate::lower::test_provider_update_hidden_stmt(
                &handle.table,
                &handle.hidden_column,
                derived,
            )?;
            self.exec_bound(update, bindings)
        })
    }

    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn read_test_provider(&self, handle: &TestProviderHandle) -> Result<(String, i64)> {
        let rows = self.collect_rows(
            crate::lower::test_provider_select_stmt(&handle.table, &handle.hidden_column)?,
            vec![],
        )?;
        let row = rows
            .first()
            .filter(|_| rows.len() == 1)
            .ok_or_else(|| FastDbError::Engine("test provider row is missing".into()))?;
        match row.as_slice() {
            [turso_core::Value::Text(document), turso_core::Value::Numeric(turso_core::Numeric::Integer(derived))] => {
                Ok((document.as_str().to_string(), *derived))
            }
            _ => Err(FastDbError::Engine(
                "test provider returned an unexpected row shape".into(),
            )),
        }
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

pub(crate) fn explain_statement(connection: &Connection, select_stmt: Stmt) -> Result<Vec<String>> {
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

    pub(crate) fn collect_select_candidates(
        &self,
        stmt: Stmt,
        bindings: crate::lower::Bindings,
        physical_table: &str,
        uses_rid: bool,
        predicates: &[(String, crate::lower::PredicateOperator, crate::Value)],
        allow_cache: bool,
    ) -> Result<Vec<Vec<Value>>> {
        self.collect_prepared_select(
            stmt,
            bindings,
            physical_table,
            uses_rid,
            predicates,
            allow_cache,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn collect_vector_candidates(
        &self,
        stmt: Stmt,
        bindings: crate::lower::Bindings,
        physical_table: &str,
        uses_rid: bool,
        predicates: &[(String, crate::lower::PredicateOperator, crate::Value)],
        allow_cache: bool,
        plan_key: String,
    ) -> Result<Vec<Vec<Value>>> {
        self.collect_prepared_select(
            stmt,
            bindings,
            physical_table,
            uses_rid,
            predicates,
            allow_cache,
            Some(plan_key),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn collect_prepared_select(
        &self,
        stmt: Stmt,
        bindings: crate::lower::Bindings,
        physical_table: &str,
        uses_rid: bool,
        predicates: &[(String, crate::lower::PredicateOperator, crate::Value)],
        allow_cache: bool,
        provider_plan: Option<String>,
    ) -> Result<Vec<Vec<Value>>> {
        if !allow_cache {
            return self.collect_rows(stmt, bindings);
        }
        let generation = self.coordinator.catalog_generation();
        let key = PreparedSelectKey {
            catalog_generation: generation,
            physical_table: physical_table.to_owned(),
            uses_rid,
            predicates: predicates
                .iter()
                .map(|(path, operator, value)| Ok((path.clone(), *operator, scalar_kind(value)?)))
                .collect::<Result<Vec<_>>>()?,
            provider_plan,
        };
        let cached_statement = {
            let mut cache = self.prepared_select_cache.lock().map_err(|_| {
                FastDbError::Engine("prepared SELECT cache lock is poisoned".into())
            })?;
            cache.sync_generation(generation);
            cache.take(&key)
        };
        let mut statement = match cached_statement {
            Some(statement) => statement,
            None => match self.prepare_translated(stmt) {
                Ok(statement) => statement,
                Err(error) => {
                    self.invalidate_caches();
                    return Err(error);
                }
            },
        };
        statement.clear_bindings();
        if let Err(error) = bind_all(&mut statement, &bindings) {
            self.invalidate_caches();
            return Err(error);
        }
        let mut rows = Vec::new();
        if let Err(error) = statement.run_with_row_callback(|row| {
            rows.push(row.get_values().cloned().collect());
            Ok(())
        }) {
            self.invalidate_caches();
            return Err(error.into());
        }
        if let Err(error) = statement.reset() {
            self.invalidate_caches();
            return Err(error.into());
        }
        statement.clear_bindings();
        let mut cache = self
            .prepared_select_cache
            .lock()
            .map_err(|_| FastDbError::Engine("prepared SELECT cache lock is poisoned".into()))?;
        cache.sync_generation(self.coordinator.catalog_generation());
        if cache.generation == generation {
            cache.insert(key, statement);
        }
        Ok(rows)
    }

    fn invalidate_caches(&self) {
        if let Ok(mut cache) = self.parse_cache.lock() {
            cache.clear();
        }
        if let Ok(mut cache) = self.prepared_select_cache.lock() {
            cache.clear();
        }
    }

    fn invalidate_prepared_cache(&self) {
        if let Ok(mut cache) = self.prepared_select_cache.lock() {
            cache.clear();
        }
    }

    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn cache_stats(&self) -> Result<CacheStats> {
        let parse = self
            .parse_cache
            .lock()
            .map_err(|_| FastDbError::Engine("parse cache lock is poisoned".into()))?;
        let prepared = self
            .prepared_select_cache
            .lock()
            .map_err(|_| FastDbError::Engine("prepared SELECT cache lock is poisoned".into()))?;
        Ok(CacheStats {
            parse_entries: parse.entries.len(),
            parse_source_bytes: parse.source_bytes,
            parse_hits: parse.hits,
            parse_misses: parse.misses,
            prepared_entries: prepared.entries.len(),
            prepared_hits: prepared.hits,
            prepared_misses: prepared.misses,
        })
    }
}

#[cfg(feature = "testing")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheStats {
    pub parse_entries: usize,
    pub parse_source_bytes: usize,
    pub parse_hits: u64,
    pub parse_misses: u64,
    pub prepared_entries: usize,
    pub prepared_hits: u64,
    pub prepared_misses: u64,
}

#[cfg(feature = "testing")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[doc(hidden)]
pub struct TestProviderHandle {
    table: String,
    hidden_column: String,
}

fn scalar_kind(value: &crate::Value) -> Result<ScalarKind> {
    match value {
        crate::Value::Null => Ok(ScalarKind::Null),
        crate::Value::Bool(_) => Ok(ScalarKind::Bool),
        crate::Value::Integer(_) => Ok(ScalarKind::Integer),
        crate::Value::Float(_) => Ok(ScalarKind::Float),
        crate::Value::Str(_) => Ok(ScalarKind::String),
        crate::Value::None
        | crate::Value::Decimal(_)
        | crate::Value::Bytes(_)
        | crate::Value::Duration(_)
        | crate::Value::Datetime(_)
        | crate::Value::Uuid(_)
        | crate::Value::Array(_)
        | crate::Value::Object(_)
        | crate::Value::Set(_)
        | crate::Value::Range(_)
        | crate::Value::Regex(_)
        | crate::Value::RecordId(_)
        | crate::Value::Table(_)
        | crate::Value::File(_) => Err(FastDbError::Engine(
            "prepared SELECT cache received a non-scalar predicate".into(),
        )),
    }
}

/// The SQLite parser annotates unaliased result expressions with their source
/// text. Directly constructed AST omits that display-only metadata; it does
/// not affect planning, so remove it before the test-only round-trip check.
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
