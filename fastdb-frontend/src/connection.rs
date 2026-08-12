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
use crate::ExecutionResult;
use std::num::NonZeroUsize;
use std::sync::Arc;
use turso_core::Value;
use turso_parser::ast::Stmt;

/// An open FastDB database. Phase 0 uses one connection and one writer.
pub struct Database {
    db: Arc<turso_core::Database>,
}

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
        Ok(Self { db })
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
        Ok(Connection::new(conn))
    }
}

/// A FastDB connection wrapping one Turso connection. Not `Send`/`Sync` in
/// Phase 0: one connection, one writer.
pub struct Connection {
    conn: Arc<turso_core::Connection>,
    failpoints: Failpoints,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection").finish_non_exhaustive()
    }
}

impl Connection {
    pub(crate) fn new(conn: Arc<turso_core::Connection>) -> Self {
        Self {
            conn,
            failpoints: Failpoints::default(),
        }
    }

    /// Parse and execute one FastDB statement end-to-end.
    pub fn execute(&self, sql: &str) -> Result<ExecutionResult> {
        let stmt = turso_fastdb_parser::parse_one(sql)?;
        execute::run_statement(self, stmt, sql)
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

    /// Test-only: install the canonical non-unique expression index on a
    /// top-level field of a logical table, returning the opaque index name.
    /// Resolves the table through the catalog and reuses the same canonical
    /// JSON expression builder as the filter lowering. Not a public API.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn create_field_index(&self, logical_table: &str, field: &str) -> Result<String> {
        if !crate::catalog::catalog_exists(self, crate::catalog::META_TABLE)? {
            return Err(FastDbError::Engine(format!(
                "table {logical_table:?} is not registered"
            )));
        }
        crate::catalog::ensure_catalog_compatible(self)?;
        let resolved = crate::catalog::resolve_table(self, logical_table)?.ok_or_else(|| {
            FastDbError::Engine(format!("table {logical_table:?} is not registered"))
        })?;
        let idx_name = crate::names::physical_index_name(crate::names::TableId::new_random());
        let path = crate::lower::canonical_field_path(field)?;
        let stmt =
            crate::lower::physical_name_index_ddl(&idx_name, &resolved.physical_name, &path)?;
        self.exec_bound(stmt, vec![])?;
        Ok(idx_name)
    }

    /// Test-only: explain the actual lowered FastDB filter statement. The
    /// public engine API cannot prepare an `ExplainQueryPlan(Stmt)` directly,
    /// so this helper renders that command and first proves that reparsing it
    /// produces the structurally identical AST before asking the engine for
    /// the plan.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn explain_field_filter(&self, logical_table: &str, field: &str) -> Result<Vec<String>> {
        if !crate::catalog::catalog_exists(self, crate::catalog::META_TABLE)? {
            return Err(FastDbError::Engine(format!(
                "table {logical_table:?} is not registered"
            )));
        }
        crate::catalog::ensure_catalog_compatible(self)?;
        let resolved = crate::catalog::resolve_table(self, logical_table)?.ok_or_else(|| {
            FastDbError::Engine(format!("table {logical_table:?} is not registered"))
        })?;
        let path = crate::lower::canonical_field_path(field)?;
        // The exact translated statement the filter lowering builds.
        let (select_stmt, _bindings) =
            crate::lower::physical_select_by_field_stmt(&resolved.physical_name, &path, "x")?;
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
        let mut stmt = self.conn.prepare(&explain_sql)?;
        let mut plans = Vec::new();
        stmt.run_with_row_callback(|row| {
            plans.push(row.get::<String>(3)?);
            Ok(())
        })?;
        Ok(plans)
    }

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

/// Convert a single engine [`Value`] to an `i64` (integer) or error.
pub(crate) fn value_to_i64(v: &Value) -> Result<i64> {
    match v {
        Value::Numeric(turso_core::Numeric::Integer(i)) => Ok(*i),
        other => Err(FastDbError::Engine(format!(
            "expected integer value, got {other:?}"
        ))),
    }
}
