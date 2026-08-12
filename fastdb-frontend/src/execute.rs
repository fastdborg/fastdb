//! Capability-gated statement execution and transaction ownership.

use crate::catalog;
use crate::connection::{value_to_string, Connection};
use crate::decode;
use crate::error::{FastDbError, Result};
use crate::lower;
use crate::names::{decode_rid, encode_rid};
use crate::test_failpoints::Failpoint;
use crate::{ExecutionResult, Record, RecordId, Value as FValue};
use turso_fastdb_parser::{
    BinaryOperator, CreateData, Expr, ExprKind, ProjectionList, RecordIdPartKind, Span, Statement,
    Target,
};

struct ExecutableCreate {
    table: String,
    id: String,
    field: String,
    value: String,
}

struct ExecutableSelect {
    table: String,
    id: Option<String>,
    filter: Option<ExecutablePredicate>,
}

struct ExecutableDelete {
    table: String,
    id: String,
}

struct ExecutablePredicate {
    field: String,
    value: String,
}

enum ExecutableStatement {
    Create(ExecutableCreate),
    Select(ExecutableSelect),
    Delete(ExecutableDelete),
}

/// Execute a parsed statement only after proving it belongs to the existing
/// storage-feasibility capability set.
pub fn run_statement(conn: &Connection, stmt: Statement, source: &str) -> Result<ExecutionResult> {
    match capability_gate(stmt, source)? {
        ExecutableStatement::Create(create) => run_create(conn, create),
        ExecutableStatement::Select(select) => run_select(conn, select),
        ExecutableStatement::Delete(delete) => run_delete(conn, delete),
    }
}

fn capability_gate(statement: Statement, source: &str) -> Result<ExecutableStatement> {
    let span = statement.span();
    let executable = match statement {
        Statement::Create(create) => {
            if create.only.is_some() || create.return_clause.is_some() {
                None
            } else {
                let (table, id) = bare_record_target(create.target)?;
                match create.data {
                    CreateData::Set(mut assignments) if assignments.len() == 1 => {
                        let assignment = assignments.pop().expect("length checked above");
                        let field = single_field(assignment.path)?;
                        let value = phase0_string_expression(assignment.value, source)?;
                        Some(ExecutableStatement::Create(ExecutableCreate {
                            table,
                            id,
                            field,
                            value,
                        }))
                    }
                    CreateData::Set(_) | CreateData::Content(_) => None,
                }
            }
        }
        Statement::Select(select) => {
            if !matches!(select.projections, ProjectionList::All(_))
                || select.only.is_some()
                || !select.order_by.is_empty()
                || select.limit.is_some()
                || select.start.is_some()
            {
                None
            } else {
                match (select.target, select.condition) {
                    (Target::Record(record), None) => {
                        let RecordIdPartKind::Bare(id) = record.id.kind else {
                            return unsupported_execution(span);
                        };
                        Some(ExecutableStatement::Select(ExecutableSelect {
                            table: record.table.value,
                            id: Some(id),
                            filter: None,
                        }))
                    }
                    (Target::Table(table), Some(condition)) => {
                        let predicate = equality_string_predicate(condition, source)?;
                        Some(ExecutableStatement::Select(ExecutableSelect {
                            table: table.name.value,
                            id: None,
                            filter: Some(predicate),
                        }))
                    }
                    _ => None,
                }
            }
        }
        Statement::Delete(delete) => {
            if delete.condition.is_some() || delete.return_clause.is_some() {
                None
            } else {
                let (table, id) = bare_record_target(delete.target)?;
                Some(ExecutableStatement::Delete(ExecutableDelete { table, id }))
            }
        }
        Statement::Update(_)
        | Statement::DefineTable(_)
        | Statement::DefineField(_)
        | Statement::DefineIndex(_)
        | Statement::Begin(_)
        | Statement::Commit(_)
        | Statement::Cancel(_) => None,
    };
    executable.map_or_else(|| unsupported_execution(span), Ok)
}

fn bare_record_target(target: Target) -> Result<(String, String)> {
    let Target::Record(record) = target else {
        return unsupported_execution(target.span());
    };
    let RecordIdPartKind::Bare(id) = record.id.kind else {
        return unsupported_execution(record.id.span);
    };
    Ok((record.table.value, id))
}

fn single_field(path: turso_fastdb_parser::FieldPath) -> Result<String> {
    if path.segments.len() != 1 {
        return unsupported_execution(path.span);
    }
    Ok(path
        .segments
        .into_iter()
        .next()
        .expect("length checked above")
        .value)
}

fn phase0_string_expression(expression: Expr, source: &str) -> Result<String> {
    let span = expression.span;
    let ExprKind::String(value) = expression.kind else {
        return unsupported_execution(span);
    };
    if source.as_bytes().get(span.offset) != Some(&b'\'') {
        return unsupported_execution(span);
    }
    Ok(value)
}

fn equality_string_predicate(expression: Expr, source: &str) -> Result<ExecutablePredicate> {
    let span = expression.span;
    let ExprKind::Binary {
        left,
        operator,
        right,
    } = expression.kind
    else {
        return unsupported_execution(span);
    };
    if operator.value != BinaryOperator::Equal {
        return unsupported_execution(operator.span);
    }
    let ExprKind::FieldPath(path) = left.kind else {
        return unsupported_execution(left.span);
    };
    let field = single_field(path)?;
    let value = phase0_string_expression(*right, source)?;
    Ok(ExecutablePredicate { field, value })
}

fn unsupported_execution<T>(span: Span) -> Result<T> {
    Err(FastDbError::UnsupportedSyntax(
        turso_fastdb_parser::ParseError::unsupported(
            "parsed syntax is not executable until its implementation phase",
            span,
        ),
    ))
}

fn with_tx<R>(conn: &Connection, body: impl FnOnce() -> Result<R>) -> Result<R> {
    conn.exec_bound(lower::begin_immediate(), vec![])?;
    let outcome: Result<R> = body().and_then(|result| {
        conn.check_failpoint(Failpoint::CommitFailure)?;
        conn.exec_bound(lower::commit(), vec![]).map(|()| result)
    });
    match outcome {
        Ok(result) => Ok(result),
        Err(error) => match conn
            .check_failpoint(Failpoint::RollbackFailure)
            .and_then(|()| conn.exec_bound(lower::rollback(), vec![]))
        {
            Ok(()) => Err(error),
            Err(rollback_error) => {
                // A failed commit may have already cleared the transaction.
                // Probe typed transaction state without matching error text.
                match conn.exec_bound(lower::begin_immediate(), vec![]) {
                    Ok(()) => match conn.exec_bound(lower::rollback(), vec![]) {
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

fn run_create(conn: &Connection, create: ExecutableCreate) -> Result<ExecutionResult> {
    let database_id = format!("{:032x}", rand::random::<u128>());

    with_tx(conn, || {
        if catalog::catalog_exists(conn, catalog::META_TABLE)? {
            catalog::ensure_catalog_compatible(conn)?;
        } else {
            catalog::bootstrap_catalog(conn, &database_id)?;
            conn.check_failpoint(Failpoint::AfterBootstrap)?;
        }

        let resolved = match catalog::resolve_table(conn, &create.table)? {
            Some(resolved) => resolved,
            None => {
                let resolved = catalog::allocate_table(&create.table)?;
                catalog::insert_catalog_row(conn, &resolved)?;
                conn.check_failpoint(Failpoint::AfterCatalogRow)?;
                catalog::create_physical_table(conn, &resolved)?;
                conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
                resolved
            }
        };

        let encoded_rid = encode_rid(&create.id);
        let (insert, bindings) = lower::physical_insert_stmt(
            &resolved.physical_name,
            &encoded_rid,
            &create.field,
            &create.value,
        )?;
        let mut statement = conn.prepare_bound(insert, bindings)?;
        conn.check_failpoint(Failpoint::AfterRecordPrepare)?;
        statement.run_ignore_rows()?;
        conn.check_failpoint(Failpoint::AfterRecordInsert)?;

        let record = Record::new(RecordId::new(&create.table, create.id))
            .with_field(create.field, FValue::Str(create.value));
        Ok(ExecutionResult {
            records: vec![record],
        })
    })
}

fn run_select(conn: &Connection, select: ExecutableSelect) -> Result<ExecutionResult> {
    if !catalog::catalog_exists(conn, catalog::META_TABLE)? {
        return Ok(ExecutionResult { records: vec![] });
    }
    catalog::ensure_catalog_compatible(conn)?;
    let Some(resolved) = catalog::resolve_table(conn, &select.table)? else {
        return Ok(ExecutionResult { records: vec![] });
    };

    let records = match (select.id, select.filter) {
        (Some(id), None) => {
            let encoded = encode_rid(&id);
            let (statement, bindings) =
                lower::physical_select_by_rid_stmt(&resolved.physical_name, &encoded)?;
            decode_rows(conn, statement, bindings, &resolved.logical)?
        }
        (None, Some(predicate)) => {
            let path = lower::canonical_field_path(&predicate.field)?;
            let (statement, bindings) = lower::physical_select_by_field_stmt(
                &resolved.physical_name,
                &path,
                &predicate.value,
            )?;
            decode_rows(conn, statement, bindings, &resolved.logical)?
        }
        _ => unreachable!("capability gate creates exactly one SELECT mode"),
    };
    Ok(ExecutionResult { records })
}

fn run_delete(conn: &Connection, delete: ExecutableDelete) -> Result<ExecutionResult> {
    if !catalog::catalog_exists(conn, catalog::META_TABLE)? {
        return Ok(ExecutionResult { records: vec![] });
    }
    catalog::ensure_catalog_compatible(conn)?;
    let Some(resolved) = catalog::resolve_table(conn, &delete.table)? else {
        return Ok(ExecutionResult { records: vec![] });
    };
    let encoded = encode_rid(&delete.id);
    let (statement, bindings) =
        lower::physical_delete_by_rid_stmt(&resolved.physical_name, &encoded)?;
    conn.exec_bound(statement, bindings)?;
    Ok(ExecutionResult { records: vec![] })
}

fn decode_rows(
    conn: &Connection,
    statement: turso_parser::ast::Stmt,
    bindings: lower::Bindings,
    table: &str,
) -> Result<Vec<Record>> {
    let rows = conn.collect_rows(statement, bindings)?;
    let mut records = Vec::with_capacity(rows.len());
    for row in rows {
        let rid = value_to_string(row.first().unwrap_or(&turso_core::Value::Null))?;
        let json = value_to_string(row.get(1).unwrap_or(&turso_core::Value::Null))?;
        let id = decode_rid(&rid)?;
        let fields = decode::parse_doc(&json)?;
        records.push(Record {
            id: RecordId::new(table, id),
            fields,
        });
    }
    Ok(records)
}
