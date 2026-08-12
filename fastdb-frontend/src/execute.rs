//! Statement execution: the transaction owner and the per-statement
//! algorithms. CREATE runs inside one `BEGIN IMMEDIATE` transaction so
//! catalog bootstrap, table registration, physical DDL, and the record
//! insert commit or roll back together. SELECT/DELETE are read/autocommit.

use crate::catalog;
use crate::connection::{value_to_string, Connection};
use crate::decode;
use crate::error::{FastDbError, Result};
use crate::lower;
use crate::names::{decode_rid, encode_rid};
use crate::test_failpoints::Failpoint;
use crate::{ExecutionResult, Record, RecordId, Value as FValue};
use turso_fastdb_parser::{
    CreateStatement, DeleteStatement, Predicate, SelectStatement, Statement,
};

/// Parse + execute one statement. Dispatched from [`Connection::execute`].
pub fn run_statement(conn: &Connection, stmt: Statement) -> Result<ExecutionResult> {
    match stmt {
        Statement::Create(c) => run_create(conn, c),
        Statement::Select(s) => run_select(conn, s),
        Statement::Delete(d) => run_delete(conn, d),
    }
}

/// `BEGIN` … body … `COMMIT`, rolling back on any error. The original typed
/// error is preserved on rollback success; a rollback failure is surfaced.
fn with_tx<R>(conn: &Connection, body: impl FnOnce() -> Result<R>) -> Result<R> {
    conn.exec_bound(lower::begin_immediate(), vec![])?;
    match body() {
        Ok(r) => {
            conn.exec_bound(lower::commit(), vec![])?;
            Ok(r)
        }
        Err(e) => {
            if let Err(rb) = conn.exec_bound(lower::rollback(), vec![]) {
                return Err(FastDbError::Transaction(format!(
                    "rollback failed after error; original: {e}; rollback: {rb}"
                )));
            }
            Err(e)
        }
    }
}

fn run_create(conn: &Connection, c: CreateStatement) -> Result<ExecutionResult> {
    let logical = c.target.table.value.clone();
    let id_part = c.target.id.expect("parser guarantees CREATE id").value;
    let field = c.assignment.field.value.clone();
    let value = c.assignment.value.value.clone();
    let database_id = format!("{:032x}", rand::random::<u128>());

    with_tx(conn, || {
        if catalog::catalog_exists(conn, catalog::META_TABLE)? {
            let v = catalog::read_format_version(conn)?;
            if v != catalog::FORMAT_VERSION {
                return Err(FastDbError::Format(format!(
                    "unknown Phase 0 format version {v}; only 0 is supported"
                )));
            }
        } else {
            catalog::bootstrap_catalog(conn, &database_id)?;
            conn.check_failpoint(Failpoint::AfterBootstrap)?;
        }

        let resolved = match catalog::resolve_table(conn, &logical)? {
            Some(r) => r,
            None => {
                let r = catalog::allocate_table(&logical)?;
                catalog::insert_catalog_row(conn, &r)?;
                conn.check_failpoint(Failpoint::AfterCatalogRow)?;
                catalog::create_physical_table(conn, &r)?;
                conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
                r
            }
        };

        let encoded_rid = encode_rid(&id_part);
        let (ins, bindings) =
            lower::physical_insert_stmt(&resolved.physical_name, &encoded_rid, &field, &value)?;
        let mut stmt = conn.prepare_bound(ins, bindings)?;
        conn.check_failpoint(Failpoint::AfterRecordPrepare)?;
        stmt.run_ignore_rows()?;
        conn.check_failpoint(Failpoint::AfterRecordInsert)?;

        let record =
            Record::new(RecordId::new(&logical, id_part)).with_field(field, FValue::Str(value));
        Ok(ExecutionResult {
            records: vec![record],
        })
    })
}

fn run_select(conn: &Connection, s: SelectStatement) -> Result<ExecutionResult> {
    let logical = s.target.table.value.clone();
    // A read of a new/empty database must not create catalog or user objects.
    if !catalog::catalog_exists(conn, catalog::META_TABLE)? {
        return Ok(ExecutionResult { records: vec![] });
    }
    let Some(resolved) = catalog::resolve_table(conn, &logical)? else {
        return Ok(ExecutionResult { records: vec![] });
    };

    let records = match s.filter {
        None => {
            let id_part = s
                .target
                .id
                .expect("parser guarantees record-select id")
                .value;
            let encoded = encode_rid(&id_part);
            let (stmt, bindings) =
                lower::physical_select_by_rid_stmt(&resolved.physical_name, &encoded)?;
            decode_rows(conn, stmt, bindings, &resolved.logical)?
        }
        Some(Predicate::StringEquals { field, value, .. }) => {
            let path = lower::canonical_field_path(&field.value)?;
            let (stmt, bindings) =
                lower::physical_select_by_field_stmt(&resolved.physical_name, &path, &value.value)?;
            decode_rows(conn, stmt, bindings, &resolved.logical)?
        }
    };
    Ok(ExecutionResult { records })
}

fn run_delete(conn: &Connection, d: DeleteStatement) -> Result<ExecutionResult> {
    let logical = d.target.table.value.clone();
    if !catalog::catalog_exists(conn, catalog::META_TABLE)? {
        return Ok(ExecutionResult { records: vec![] });
    }
    let Some(resolved) = catalog::resolve_table(conn, &logical)? else {
        return Ok(ExecutionResult { records: vec![] });
    };
    let id_part = d.target.id.expect("parser guarantees DELETE id").value;
    let encoded = encode_rid(&id_part);
    let (stmt, bindings) = lower::physical_delete_by_rid_stmt(&resolved.physical_name, &encoded)?;
    conn.exec_bound(stmt, bindings)?;
    Ok(ExecutionResult { records: vec![] })
}

/// Run a `SELECT rid, json(doc) ...` and decode each row into a [`Record`].
fn decode_rows(
    conn: &Connection,
    stmt: turso_parser::ast::Stmt,
    bindings: lower::Bindings,
    table: &str,
) -> Result<Vec<Record>> {
    let rows = conn.collect_rows(stmt, bindings)?;
    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let rid = value_to_string(row.first().unwrap_or(&turso_core::Value::Null))?;
        let json = value_to_string(row.get(1).unwrap_or(&turso_core::Value::Null))?;
        let id = decode_rid(&rid)?;
        let fields = decode::parse_doc(&json)?;
        out.push(Record {
            id: RecordId::new(table, id),
            fields,
        });
    }
    Ok(out)
}
