//! Phase-2 capability gate and atomic catalog/schema/data execution.

use crate::catalog::{self, CatalogState, IndexDefinition, TableDefinition};
use crate::connection::{value_to_string, Connection};
use crate::decode::{self, RecordIdValue};
use crate::error::{ErrorCategory, FastDbError, Result};
use crate::lower;
use crate::names::{decode_rid, encode_rid};
use crate::schema::{self, FieldRule, FieldType};
use crate::test_failpoints::Failpoint;
use crate::{ExecutionResult, Record, RecordId, Value};
use std::collections::{BTreeMap, BTreeSet};
use turso_fastdb_parser::{
    BinaryOperator, CreateData, Expr, ExprKind, ProjectionList, RecordIdPart, RecordIdPartKind,
    Span, Statement, TableMode, Target, UnaryOperator,
};

#[derive(Debug)]
struct CreatePlan {
    table: String,
    id: Option<RecordIdValue>,
    data: CreatePlanData,
}

#[derive(Debug)]
enum CreatePlanData {
    Content(BTreeMap<String, Value>),
    Set {
        path: Vec<String>,
        path_key: String,
        value: Value,
    },
}

#[derive(Debug)]
struct SelectPlan {
    table: String,
    id: Option<RecordIdValue>,
    filters: Vec<(Vec<String>, String, Value)>,
}

#[derive(Debug)]
struct DeletePlan {
    table: String,
    id: RecordIdValue,
}

enum ExecutableStatement {
    Create(CreatePlan),
    Select(SelectPlan),
    Delete(DeletePlan),
    DefineTable(turso_fastdb_parser::DefineTableStatement),
    DefineField(turso_fastdb_parser::DefineFieldStatement),
    DefineIndex(turso_fastdb_parser::DefineIndexStatement),
}

pub fn run_statement(
    conn: &Connection,
    statement: Statement,
    source: &str,
) -> Result<ExecutionResult> {
    match capability_gate(statement)? {
        ExecutableStatement::Create(plan) => run_create(conn, plan),
        ExecutableStatement::Select(plan) => run_select(conn, plan),
        ExecutableStatement::Delete(plan) => run_delete(conn, plan),
        ExecutableStatement::DefineTable(statement) => run_define_table(conn, statement, source),
        ExecutableStatement::DefineField(statement) => run_define_field(conn, statement, source),
        ExecutableStatement::DefineIndex(statement) => run_define_index(conn, statement, source),
    }
}

fn capability_gate(statement: Statement) -> Result<ExecutableStatement> {
    let span = statement.span();
    match statement {
        Statement::Create(statement) => {
            if let Some(span) = statement
                .only
                .or_else(|| statement.return_clause.map(|value| value.span))
            {
                return unsupported(span, "ONLY and RETURN remain deferred to Phase 3");
            }
            let (table, id) = target_parts(statement.target)?;
            let data = match statement.data {
                CreateData::Content(expression) => {
                    let expression_span = expression.span;
                    let Value::Object(object) = constant_value(expression)? else {
                        return unsupported(
                            expression_span,
                            "CREATE CONTENT requires a constant object",
                        );
                    };
                    CreatePlanData::Content(object)
                }
                CreateData::Set(mut assignments) if assignments.len() == 1 => {
                    let assignment = assignments.pop().expect("one assignment checked");
                    let (path, path_key) = crate::path::parser_path(&assignment.path)?;
                    if path.first().is_some_and(|segment| segment == "id") {
                        return Err(FastDbError::Schema(
                            "top-level field `id` is reserved and cannot be assigned".into(),
                        ));
                    }
                    CreatePlanData::Set {
                        path,
                        path_key,
                        value: constant_value(assignment.value)?,
                    }
                }
                CreateData::Set(assignments) => {
                    let span = assignments
                        .first()
                        .map_or(span, |assignment| assignment.span);
                    return unsupported(span, "Phase 2 CREATE SET requires exactly one assignment");
                }
            };
            Ok(ExecutableStatement::Create(CreatePlan { table, id, data }))
        }
        Statement::Select(statement) => {
            if !matches!(statement.projections, ProjectionList::All(_)) {
                return unsupported(span, "projections remain deferred to Phase 3");
            }
            if let Some(span) = statement.only {
                return unsupported(span, "ONLY remains deferred to Phase 3");
            }
            if let Some(order) = statement.order_by.first() {
                return unsupported(order.span, "ORDER BY remains deferred to Phase 3");
            }
            if let Some(limit) = statement.limit {
                return unsupported(limit.span, "LIMIT remains deferred to Phase 3");
            }
            if let Some(start) = statement.start {
                return unsupported(start.span, "START remains deferred to Phase 3");
            }
            let (table, id) = target_parts(statement.target)?;
            let filters = statement
                .condition
                .map(parse_predicates)
                .transpose()?
                .unwrap_or_default();
            Ok(ExecutableStatement::Select(SelectPlan {
                table,
                id,
                filters,
            }))
        }
        Statement::Delete(statement) => {
            if let Some(condition) = statement.condition {
                return unsupported(condition.span, "DELETE WHERE remains deferred to Phase 3");
            }
            if let Some(return_clause) = statement.return_clause {
                return unsupported(
                    return_clause.span,
                    "DELETE RETURN remains deferred to Phase 3",
                );
            }
            let Target::Record(record) = statement.target else {
                return unsupported(
                    statement.target.span(),
                    "Phase 2 DELETE requires one record target",
                );
            };
            let RecordIdPartKind::Bare(id) = record.id.kind else {
                return unsupported(
                    record.id.span,
                    "Phase 2 preserves the bare-ID Phase 0 DELETE shape",
                );
            };
            Ok(ExecutableStatement::Delete(DeletePlan {
                table: record.table.value,
                id: RecordIdValue::String(id),
            }))
        }
        Statement::DefineTable(statement) => Ok(ExecutableStatement::DefineTable(statement)),
        Statement::DefineField(statement) => Ok(ExecutableStatement::DefineField(statement)),
        Statement::DefineIndex(statement) => Ok(ExecutableStatement::DefineIndex(statement)),
        Statement::Update(statement) => {
            unsupported(statement.span, "UPDATE remains deferred to Phase 3")
        }
        Statement::Begin(statement)
        | Statement::Commit(statement)
        | Statement::Cancel(statement) => unsupported(
            statement.span,
            "explicit transactions remain deferred to Phase 3",
        ),
    }
}

fn target_parts(target: Target) -> Result<(String, Option<RecordIdValue>)> {
    match target {
        Target::Table(table) => Ok((table.name.value, None)),
        Target::Record(record) => Ok((record.table.value, Some(record_id_value(record.id)?))),
    }
}

fn record_id_value(value: RecordIdPart) -> Result<RecordIdValue> {
    Ok(match value.kind {
        RecordIdPartKind::Bare(value) | RecordIdPartKind::Quoted(value) => {
            RecordIdValue::String(value)
        }
        RecordIdPartKind::Integer(value) => RecordIdValue::Integer(value),
        RecordIdPartKind::Uuid(value) => RecordIdValue::Uuid(value),
    })
}

fn constant_value(expression: Expr) -> Result<Value> {
    let span = expression.span;
    match expression.kind {
        ExprKind::Null => Ok(Value::Null),
        ExprKind::Bool(value) => Ok(Value::Bool(value)),
        ExprKind::Integer(value) => Ok(Value::Integer(value)),
        ExprKind::Float(value) if value.is_finite() => Ok(Value::Float(value)),
        ExprKind::Float(_) => Err(FastDbError::Schema("non-finite constant float".into())),
        ExprKind::String(value) => Ok(Value::Str(value)),
        ExprKind::Array(values) => values
            .into_iter()
            .map(constant_value)
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        ExprKind::Object(fields) => {
            let mut object = BTreeMap::new();
            for field in fields {
                let key = match field.key.kind {
                    turso_fastdb_parser::ObjectKeyKind::Identifier(key)
                    | turso_fastdb_parser::ObjectKeyKind::String(key) => key,
                };
                object.insert(key, constant_value(field.value)?);
            }
            Ok(Value::Object(object))
        }
        ExprKind::RecordId(record) => Ok(Value::RecordId(RecordId::new(
            record.table.value,
            record_id_value(record.id)?,
        ))),
        ExprKind::Parenthesized(value) => constant_value(*value),
        ExprKind::Unary { operator, operand } => {
            let value = constant_value(*operand)?;
            match (operator.value, value) {
                (UnaryOperator::Plus, Value::Integer(value)) => Ok(Value::Integer(value)),
                (UnaryOperator::Plus, Value::Float(value)) => Ok(Value::Float(value)),
                (UnaryOperator::Minus, Value::Integer(value)) => value
                    .checked_neg()
                    .map(Value::Integer)
                    .ok_or_else(|| FastDbError::Schema("integer unary negation overflow".into())),
                (UnaryOperator::Minus, Value::Float(value)) if (-value).is_finite() => {
                    Ok(Value::Float(-value))
                }
                (UnaryOperator::Not, _) => unsupported(
                    operator.span,
                    "NOT is not a Phase 2 constant-value operator",
                ),
                _ => unsupported(operator.span, "numeric unary signs require a number"),
            }
        }
        ExprKind::Parameter(_) => unsupported(span, "parameters remain deferred to Phase 3"),
        ExprKind::FieldPath(_) => unsupported(span, "field references are not constant values"),
        ExprKind::Binary { operator, .. } => unsupported(
            operator.span,
            "binary expressions are not constant values in Phase 2",
        ),
    }
}

fn parse_predicates(expression: Expr) -> Result<Vec<(Vec<String>, String, Value)>> {
    match expression.kind {
        ExprKind::Parenthesized(inner) => parse_predicates(*inner),
        ExprKind::Binary {
            left,
            operator,
            right,
        } if operator.value == BinaryOperator::And => {
            let mut predicates = parse_predicates(*left)?;
            predicates.extend(parse_predicates(*right)?);
            Ok(predicates)
        }
        ExprKind::Binary {
            left,
            operator,
            right,
        } if operator.value == BinaryOperator::Equal => {
            let ExprKind::FieldPath(path) = left.kind else {
                return unsupported(left.span, "Phase 2 predicates require path = scalar");
            };
            let (segments, key) = crate::path::parser_path(&path)?;
            let right_span = right.span;
            let value = constant_value(*right)?;
            if !value.is_indexable_scalar() {
                return unsupported(right_span, "Phase 2 predicates require a scalar constant");
            }
            Ok(vec![(segments, key, value)])
        }
        ExprKind::Binary { operator, .. } => unsupported(
            operator.span,
            "Phase 2 predicates support only equality joined by AND",
        ),
        _ => unsupported(expression.span, "Phase 2 predicates require path = scalar"),
    }
}

fn run_create(conn: &Connection, mut plan: CreatePlan) -> Result<ExecutionResult> {
    let id = plan
        .id
        .take()
        .unwrap_or_else(|| RecordIdValue::Uuid(uuid::Uuid::now_v7()));
    let mut document = match &plan.data {
        CreatePlanData::Content(document) => document.clone(),
        CreatePlanData::Set { path, value, .. } => {
            let mut document = BTreeMap::new();
            crate::path::set_path(&mut document, path, value.clone())?;
            document
        }
    };
    if document.contains_key("id") {
        return Err(FastDbError::Schema(
            "top-level field `id` is reserved and synthesized from the record ID".into(),
        ));
    }
    let encoded_rid = encode_rid(&id);

    let record = schema_mutation(conn, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        if !snapshot.tables.contains_key(&plan.table) {
            let table = catalog::allocate_table(&plan.table, TableMode::Schemaless, None)?;
            catalog::persist_table(conn, &table)?;
            conn.check_failpoint(Failpoint::AfterCatalogRow)?;
            conn.exec_bound(lower::physical_table_ddl(&table.physical_name)?, vec![])?;
            conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
            snapshot.tables.insert(plan.table.clone(), table);
        }
        let table = snapshot
            .tables
            .get(&plan.table)
            .expect("table inserted or already present");
        schema::validate_document(
            table.mode == TableMode::Schemafull,
            &table.fields,
            &mut document,
        )?;
        validate_index_values(table, &document)?;

        let (insert, bindings) = match &plan.data {
            CreatePlanData::Content(_) => lower::physical_insert_content_stmt(
                &table.physical_name,
                &encoded_rid,
                &decode::encode_doc(&document)?,
            )?,
            CreatePlanData::Set { path_key, path, .. } => {
                let normalized = crate::path::get_path(&document, path)
                    .expect("SET path was inserted before validation");
                let encoded =
                    serde_json::to_string(&decode::encode_value(normalized)?).map_err(|error| {
                        FastDbError::Engine(format!("failed to encode SET value: {error}"))
                    })?;
                lower::physical_insert_set_stmt(
                    &table.physical_name,
                    &encoded_rid,
                    path_key,
                    &encoded,
                )?
            }
        };
        let mut statement = conn.prepare_bound(insert, bindings)?;
        conn.check_failpoint(Failpoint::AfterRecordPrepare)?;
        contextual_constraint(
            statement.run_ignore_rows().map_err(FastDbError::from),
            "record ID already exists or violates a declared unique index",
        )?;
        conn.check_failpoint(Failpoint::AfterRecordInsert)?;
        Ok(Record {
            id: RecordId::new(plan.table.clone(), id.clone()),
            fields: document.clone().into_iter().collect(),
        })
    })?;
    Ok(ExecutionResult {
        records: vec![record],
    })
}

fn run_select(conn: &Connection, plan: SelectPlan) -> Result<ExecutionResult> {
    let state = conn
        .coordinator
        .catalog
        .read()
        .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?;
    let Some(snapshot) = state.as_ref().and_then(CatalogState::snapshot) else {
        return Ok(ExecutionResult { records: vec![] });
    };
    let Some(table) = snapshot.tables.get(&plan.table) else {
        return Ok(ExecutionResult { records: vec![] });
    };
    let rid = plan.id.as_ref().map(encode_rid);
    let filters = plan
        .filters
        .iter()
        .map(|(_, key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    let (statement, bindings) =
        lower::physical_select_stmt(&table.physical_name, rid.as_deref(), &filters)?;
    let records = decode_rows(conn, statement, bindings, &table.logical_name)?;
    Ok(ExecutionResult { records })
}

fn run_delete(conn: &Connection, plan: DeletePlan) -> Result<ExecutionResult> {
    let state = conn
        .coordinator
        .catalog
        .read()
        .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?;
    let Some(table) = state
        .as_ref()
        .and_then(CatalogState::snapshot)
        .and_then(|snapshot| snapshot.tables.get(&plan.table))
    else {
        return Ok(ExecutionResult { records: vec![] });
    };
    let (statement, bindings) =
        lower::physical_delete_by_rid_stmt(&table.physical_name, &encode_rid(&plan.id))?;
    conn.exec_bound(statement, bindings)?;
    Ok(ExecutionResult { records: vec![] })
}

fn run_define_table(
    conn: &Connection,
    statement: turso_fastdb_parser::DefineTableStatement,
    source: &str,
) -> Result<ExecutionResult> {
    let definition = source_slice(source, statement.span)?.to_string();
    schema_mutation(conn, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        if snapshot.tables.contains_key(&statement.name.value) {
            return Err(FastDbError::Constraint(format!(
                "table {:?} is already defined",
                statement.name.value
            )));
        }
        let table = catalog::allocate_table(
            &statement.name.value,
            statement.mode.value,
            Some(definition.clone()),
        )?;
        catalog::persist_table(conn, &table)?;
        conn.check_failpoint(Failpoint::AfterCatalogRow)?;
        conn.exec_bound(lower::physical_table_ddl(&table.physical_name)?, vec![])?;
        conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
        snapshot.tables.insert(statement.name.value.clone(), table);
        Ok(())
    })?;
    Ok(ExecutionResult { records: vec![] })
}

fn run_define_field(
    conn: &Connection,
    statement: turso_fastdb_parser::DefineFieldStatement,
    source: &str,
) -> Result<ExecutionResult> {
    let definition = source_slice(source, statement.span)?.to_string();
    let (path, path_key) = crate::path::parser_path(&statement.path)?;
    if path.first().is_some_and(|segment| segment == "id") {
        return Err(FastDbError::Schema(
            "the synthesized `id` path cannot be declared".into(),
        ));
    }
    let ty = FieldType::from_parser(&statement.ty);
    let rule = FieldRule {
        path,
        path_key: path_key.clone(),
        required: ty.required(),
        ty,
        definition,
    };
    schema_mutation(conn, |state| {
        let snapshot = ready_snapshot_mut(state)?;
        let table = snapshot
            .tables
            .get_mut(&statement.table.value)
            .ok_or_else(|| {
                FastDbError::Schema(format!("table {:?} is not defined", statement.table.value))
            })?;
        if table.fields.contains_key(&path_key) {
            return Err(FastDbError::Constraint(format!(
                "field {path_key} is already defined on table {:?}",
                statement.table.value
            )));
        }
        schema::validate_field_relationships(table.fields.values(), &rule)?;
        let mut candidate_fields = table.fields.clone();
        candidate_fields.insert(path_key.clone(), rule.clone());
        let mut rows = read_documents(conn, table)?;
        for (_, document) in &mut rows {
            schema::validate_document(
                table.mode == TableMode::Schemafull,
                &candidate_fields,
                document,
            )?;
        }
        conn.check_failpoint(Failpoint::AfterFieldValidation)?;
        for (rid, document) in rows {
            let (update, bindings) = lower::physical_update_doc_stmt(
                &table.physical_name,
                &rid,
                &decode::encode_doc(&document)?,
            )?;
            conn.exec_bound(update, bindings)?;
        }
        catalog::persist_field(conn, table, &rule)?;
        conn.check_failpoint(Failpoint::AfterFieldCatalogRow)?;
        table.fields.insert(path_key.clone(), rule.clone());
        Ok(())
    })?;
    Ok(ExecutionResult { records: vec![] })
}

fn run_define_index(
    conn: &Connection,
    statement: turso_fastdb_parser::DefineIndexStatement,
    source: &str,
) -> Result<ExecutionResult> {
    let definition = source_slice(source, statement.span)?.to_string();
    let paths = statement
        .fields
        .iter()
        .map(|path| crate::path::parser_path(path).map(|(segments, _)| segments))
        .collect::<Result<Vec<_>>>()?;
    let index = catalog::allocate_index(
        &statement.name.value,
        paths,
        statement.unique.is_some(),
        definition,
    )?;
    schema_mutation(conn, |state| {
        let snapshot = ready_snapshot_mut(state)?;
        let table = snapshot
            .tables
            .get_mut(&statement.table.value)
            .ok_or_else(|| {
                FastDbError::Schema(format!("table {:?} is not defined", statement.table.value))
            })?;
        if table.indexes.contains_key(&statement.name.value) {
            return Err(FastDbError::Constraint(format!(
                "index {:?} is already defined on table {:?}",
                statement.name.value, statement.table.value
            )));
        }
        validate_existing_index(conn, table, &index)?;
        conn.check_failpoint(Failpoint::AfterIndexValidation)?;
        conn.exec_bound(
            lower::physical_index_ddl(
                &index.physical_name,
                &table.physical_name,
                &index.path_keys,
                index.unique,
            )?,
            vec![],
        )
        .map_err(|error| logical_index_constraint(error, &index.logical_name))?;
        conn.check_failpoint(Failpoint::AfterIndexPhysicalDdl)?;
        catalog::persist_index(conn, table, &index)?;
        conn.check_failpoint(Failpoint::AfterIndexCatalogRow)?;
        table
            .indexes
            .insert(statement.name.value.clone(), index.clone());
        Ok(())
    })?;
    Ok(ExecutionResult { records: vec![] })
}

fn schema_mutation<R>(
    conn: &Connection,
    body: impl FnOnce(&mut CatalogState) -> Result<R>,
) -> Result<R> {
    let _schema_guard = conn
        .coordinator
        .schema_mutex
        .lock()
        .map_err(|_| FastDbError::Transaction("database schema mutex is poisoned".into()))?;
    let mut cache = conn
        .coordinator
        .catalog
        .write()
        .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?;
    let mut candidate = cache
        .as_ref()
        .cloned()
        .ok_or_else(|| FastDbError::Engine("catalog cache was not initialized".into()))?;
    let result = conn.with_transaction(|| body(&mut candidate))?;
    *cache = Some(candidate);
    Ok(result)
}

fn ensure_snapshot<'a>(
    conn: &Connection,
    state: &'a mut CatalogState,
) -> Result<&'a mut catalog::CatalogSnapshot> {
    if matches!(state, CatalogState::Empty) {
        let snapshot = catalog::bootstrap(conn)?;
        conn.check_failpoint(Failpoint::AfterBootstrap)?;
        *state = CatalogState::Ready(snapshot);
    }
    ready_snapshot_mut(state)
}

fn ready_snapshot_mut(state: &mut CatalogState) -> Result<&mut catalog::CatalogSnapshot> {
    match state {
        CatalogState::Ready(snapshot) => Ok(snapshot),
        CatalogState::Empty => Err(FastDbError::Schema(
            "schema statement requires an existing FastDB catalog and table".into(),
        )),
    }
}

fn read_documents(
    conn: &Connection,
    table: &TableDefinition,
) -> Result<Vec<(String, BTreeMap<String, Value>)>> {
    let statement = lower::physical_all_rows_stmt(&table.physical_name)?;
    conn.collect_rows(statement, vec![])
        .map_err(stored_value_error)?
        .into_iter()
        .map(|row| {
            let rid = value_to_string(row.first().unwrap_or(&turso_core::Value::Null))
                .map_err(stored_value_error)?;
            decode_rid(&rid)?;
            let json = value_to_string(row.get(1).unwrap_or(&turso_core::Value::Null))
                .map_err(stored_value_error)?;
            let document = decode::parse_doc(&json)?.into_iter().collect();
            Ok((rid, document))
        })
        .collect()
}

fn validate_existing_index(
    conn: &Connection,
    table: &TableDefinition,
    index: &IndexDefinition,
) -> Result<()> {
    let mut unique_values = BTreeSet::new();
    for (_, document) in read_documents(conn, table)? {
        if let Some(key) = index_key(index, &document)? {
            if index.unique && !unique_values.insert(key) {
                return Err(FastDbError::Constraint(format!(
                    "unique index {:?} has duplicate existing values",
                    index.logical_name
                )));
            }
        }
    }
    Ok(())
}

fn validate_index_values(
    table: &TableDefinition,
    document: &BTreeMap<String, Value>,
) -> Result<()> {
    for index in table.indexes.values() {
        let _ = index_key(index, document)?;
    }
    Ok(())
}

fn index_key(
    index: &IndexDefinition,
    document: &BTreeMap<String, Value>,
) -> Result<Option<String>> {
    let mut components = Vec::with_capacity(index.paths.len());
    for (path, path_key) in index.paths.iter().zip(&index.path_keys) {
        let Some(value) = crate::path::get_path(document, path) else {
            return Ok(None);
        };
        if matches!(value, Value::Null) {
            return Ok(None);
        }
        if !value.is_indexable_scalar() {
            return Err(FastDbError::Schema(format!(
                "indexed path {path_key} contains a non-scalar value"
            )));
        }
        let component = match value {
            Value::Bool(value) => format!("b:{value}"),
            Value::Integer(value) => format!("i:{value}"),
            Value::Float(value) => format!("f:{:016x}", value.to_bits()),
            Value::Str(value) => format!("s:{}:{value}", value.len()),
            Value::Null | Value::Array(_) | Value::Object(_) | Value::RecordId(_) => {
                unreachable!("null returned early and non-scalars were rejected")
            }
        };
        components.push(component);
    }
    Ok(Some(components.join("|")))
}

fn decode_rows(
    conn: &Connection,
    statement: turso_parser::ast::Stmt,
    bindings: lower::Bindings,
    table: &str,
) -> Result<Vec<Record>> {
    conn.collect_rows(statement, bindings)
        .map_err(stored_value_error)?
        .into_iter()
        .map(|row| {
            let rid = value_to_string(row.first().unwrap_or(&turso_core::Value::Null))
                .map_err(stored_value_error)?;
            let id = decode_rid(&rid)?;
            let json = value_to_string(row.get(1).unwrap_or(&turso_core::Value::Null))
                .map_err(stored_value_error)?;
            Ok(Record {
                id: RecordId::new(table, id),
                fields: decode::parse_doc(&json)?,
            })
        })
        .collect()
}

fn source_slice(source: &str, span: Span) -> Result<&str> {
    source
        .get(span.offset..span.end())
        .ok_or_else(|| FastDbError::Engine("parser span lies outside source".into()))
}

fn contextual_constraint<T>(result: Result<T>, message: &'static str) -> Result<T> {
    result.map_err(|error| {
        if error.category() == ErrorCategory::Constraint {
            FastDbError::Constraint(message.into())
        } else {
            error
        }
    })
}

fn logical_index_constraint(error: FastDbError, logical_index: &str) -> FastDbError {
    if error.category() == ErrorCategory::Constraint {
        FastDbError::Constraint(format!(
            "index {logical_index:?} conflicts with existing values"
        ))
    } else {
        error
    }
}

fn stored_value_error(error: FastDbError) -> FastDbError {
    match error.category() {
        ErrorCategory::Engine | ErrorCategory::Constraint => {
            FastDbError::Format("stored record contains malformed FastDB data".into())
        }
        _ => error,
    }
}

fn unsupported<T>(span: Span, message: &'static str) -> Result<T> {
    Err(FastDbError::UnsupportedSyntax(
        turso_fastdb_parser::ParseError::unsupported(message, span),
    ))
}
