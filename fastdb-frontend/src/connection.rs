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

impl Database {
    /// Open (or create) a file-backed database at `path`.
    pub fn open(path: &str) -> Result<Self> {
        let io = turso_core::Database::io_for_path(path)?;
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

impl Connection {
    pub(crate) fn new(conn: Arc<turso_core::Connection>) -> Self {
        Self {
            conn,
            failpoints: Failpoints::default(),
        }
    }

    /// Parse and execute one FastDB statement end-to-end.
    pub fn execute(&self, sql: &str) -> Result<ExecutionResult> {
        let stmt = turso_fastdb_parser::parse(sql)?;
        execute::run_statement(self, stmt)
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
        let resolved = crate::catalog::resolve_table(self, logical_table)?
            .ok_or_else(|| FastDbError::Engine(format!("table {logical_table:?} is not registered")))?;
        let idx_name = crate::names::physical_index_name(crate::names::TableId::new_random());
        let path = crate::lower::canonical_field_path(field)?;
        let stmt = crate::lower::physical_name_index_ddl(&idx_name, &resolved.physical_name, &path)?;
        self.exec_bound(stmt, vec![])?;
        Ok(idx_name)
    }

    /// Test-only: run `EXPLAIN QUERY PLAN` over the exact translated
    /// predicate shape used by the equality filter, returning the plan
    /// detail strings. The opaque table name and canonical path are the same
    /// validated values the filter lowering uses; the literal value does not
    /// affect the plan.
    #[cfg(feature = "testing")]
    #[doc(hidden)]
    pub fn explain_field_filter(&self, logical_table: &str, field: &str) -> Result<Vec<String>> {
        let resolved = crate::catalog::resolve_table(self, logical_table)?
            .ok_or_else(|| FastDbError::Engine(format!("table {logical_table:?} is not registered")))?;
        let path = crate::lower::canonical_field_path(field)?;
        let sql = format!(
            "EXPLAIN QUERY PLAN SELECT rid, json(doc) FROM {} WHERE json_extract(doc, '{}') = 'x'",
            resolved.physical_name, path
        );
        let mut stmt = self.conn.prepare(&sql)?;
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
