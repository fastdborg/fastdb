//! Shared helpers for FastDB integration tests: native (test-only) engine
//! inspection. These use the Turso connection directly to run PRAGMA and
//! internal catalog queries — never to run FastDB user input.

// Different test binaries use different subsets of these helpers.
#![allow(dead_code)]

use std::sync::Arc;
use turso_core::{Connection, Value};

/// Run `PRAGMA integrity_check` and return the result string.
pub fn integrity_check(conn: &Arc<Connection>) -> String {
    let mut stmt = conn.prepare("PRAGMA integrity_check").unwrap();
    let mut out = String::new();
    stmt.run_with_row_callback(|row| {
        out = row.get::<String>(0).unwrap_or_default();
        Ok(())
    })
    .unwrap();
    out
}

/// Run a SQL query on the native connection, returning rows of stringified
/// values (text as-is, integers decimalized, blobs as `<blob>`).
pub fn native_rows(conn: &Arc<Connection>, sql: &str) -> Vec<Vec<String>> {
    let mut stmt = conn.prepare(sql).unwrap();
    let mut rows = Vec::new();
    stmt.run_with_row_callback(|row| {
        rows.push(row.get_values().map(value_to_string).collect());
        Ok(())
    })
    .unwrap();
    rows
}

/// Run a statement that returns no rows.
pub fn native_exec(conn: &Arc<Connection>, sql: &str) {
    let mut stmt = conn.prepare(sql).unwrap();
    stmt.run_ignore_rows().unwrap();
}

/// Read the opaque physical table name registered for a logical table.
/// Returns `None` if the logical name is not registered.
pub fn physical_name_for(conn: &Arc<Connection>, logical: &str) -> Option<String> {
    // logical name bound, not interpolated, by the FastDB path; here we are
    // running a known internal value for inspection only.
    let sql = format!("SELECT physical_name FROM __fastdb_tables WHERE logical_name = '{logical}'");
    native_rows(conn, &sql).into_iter().next().and_then(|r| r.into_iter().next())
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Text(t) => t.as_str().to_string(),
        Value::Numeric(turso_core::Numeric::Integer(i)) => i.to_string(),
        Value::Numeric(turso_core::Numeric::Float(f)) => f.to_string(),
        Value::Blob(_) => "<blob>".to_string(),
        Value::Null => "<null>".to_string(),
    }
}
