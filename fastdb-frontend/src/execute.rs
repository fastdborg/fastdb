//! Phase 3 planning, evaluation, lowering, and atomic execution.

use crate::catalog::{self, CatalogState, IndexDefinition, TableDefinition};
use crate::connection::{value_to_string, Connection, ExecutionState, TransactionState};
use crate::decode::{self, RecordIdValue};
use crate::error::{ErrorCategory, FastDbError, Result};
use crate::eval::{self, EvalContext, EvalValue};
use crate::lower::{self, PredicateOperator};
use crate::names::{decode_rid, encode_rid};
use crate::schema::{self, FieldRule, FieldType};
use crate::test_failpoints::Failpoint;
use crate::{Params, RecordId, StatementResult, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use turso_fastdb_parser::{
    BinaryOperator, CreateData, Expr, ExprKind, ProjectionList, RecordIdPart, RecordIdPartKind,
    ReturnKind, Span, Statement, TableMode, Target,
};

#[derive(Debug, Clone)]
struct Candidate {
    encoded_rid: String,
    id: RecordId,
    document: BTreeMap<String, Value>,
}

pub(crate) struct StatementExecution {
    pub(crate) result: StatementResult,
    pub(crate) mutation_count: u64,
}

impl StatementExecution {
    fn read_only(result: StatementResult) -> Self {
        Self {
            result,
            mutation_count: 0,
        }
    }

    fn mutation(result: StatementResult, mutation_count: usize) -> Result<Self> {
        Ok(Self {
            result,
            mutation_count: u64::try_from(mutation_count).map_err(|_| {
                FastDbError::Engine("statement mutation count overflowed u64".into())
            })?,
        })
    }
}

pub(crate) fn run_statement(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: Statement,
    source: &str,
    params: &Params,
) -> Result<StatementExecution> {
    if matches!(execution.transaction, TransactionState::Poisoned)
        && !matches!(statement, Statement::Cancel(_))
    {
        return Err(FastDbError::Transaction(
            "transaction is poisoned; CANCEL is required".into(),
        ));
    }
    if matches!(execution.transaction, TransactionState::Broken) {
        return Err(FastDbError::Transaction(
            "connection transaction state is broken; close and reopen it".into(),
        ));
    }

    match statement {
        Statement::Begin(_) => conn
            .begin_explicit(execution)
            .map(StatementExecution::read_only),
        Statement::Commit(_) => conn
            .commit_explicit(execution)
            .map(StatementExecution::read_only),
        Statement::Cancel(_) => conn
            .cancel_explicit(execution)
            .map(StatementExecution::read_only),
        statement => {
            eval::validate_parameter_references(&statement, params)?;
            match statement {
                Statement::Create(statement) => run_create(conn, execution, statement, params),
                Statement::Select(statement) => run_select(conn, execution, statement, params)
                    .map(StatementExecution::read_only),
                Statement::Update(statement) => run_update(conn, execution, statement, params),
                Statement::Delete(statement) => run_delete(conn, execution, statement, params),
                Statement::DefineTable(statement) => {
                    run_define_table(conn, execution, statement, source)
                        .map(StatementExecution::read_only)
                }
                Statement::DefineField(statement) => {
                    run_define_field(conn, execution, statement, source)
                        .map(StatementExecution::read_only)
                }
                Statement::DefineIndex(statement) => {
                    run_define_index(conn, execution, statement, source)
                        .map(StatementExecution::read_only)
                }
                Statement::Begin(_) | Statement::Commit(_) | Statement::Cancel(_) => {
                    unreachable!("transaction statements were handled above")
                }
            }
        }
    }
}

fn run_create(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::CreateStatement,
    params: &Params,
) -> Result<StatementExecution> {
    let (table_name, parsed_id) = target_parts(statement.target)?;
    let id_value = parsed_id.unwrap_or_else(|| RecordIdValue::Uuid(uuid::Uuid::now_v7()));
    let id = RecordId::new(table_name.clone(), id_value.clone());
    let empty = BTreeMap::new();
    let context = EvalContext {
        document: &empty,
        id: &id,
        params,
    };
    let mut document = match &statement.data {
        CreateData::Content(expression) => {
            let value = eval::evaluate(expression, &context)?.into_projection();
            let Value::Object(document) = value else {
                return Err(FastDbError::Schema(
                    "CREATE CONTENT must evaluate to an object".into(),
                ));
            };
            document
        }
        CreateData::Set(assignments) => {
            let evaluated = evaluate_assignments(assignments, &context)?;
            let mut document = BTreeMap::new();
            apply_assignments(&mut document, evaluated)?;
            document
        }
    };
    reject_stored_id(&document)?;
    let encoded_rid = encode_rid(&id_value);
    let table_was_missing = !catalog_for_read(conn, execution)?
        .snapshot()
        .is_some_and(|snapshot| snapshot.tables.contains_key(&table_name));

    let value = with_create_mutation(conn, execution, table_was_missing, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        if !snapshot.tables.contains_key(&table_name) {
            let table = catalog::allocate_table(&table_name, TableMode::Schemaless, None)?;
            catalog::persist_table(conn, &table)?;
            conn.check_failpoint(Failpoint::AfterCatalogRow)?;
            conn.exec_bound(lower::physical_table_ddl(&table.physical_name)?, vec![])?;
            conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
            snapshot.tables.insert(table_name.clone(), table);
        }
        let table = snapshot
            .tables
            .get(&table_name)
            .expect("table inserted or already present");
        schema::validate_document(
            table.mode == TableMode::Schemafull,
            &table.fields,
            &mut document,
        )?;
        validate_index_values(table, &document)?;
        let (insert, bindings) = lower::physical_insert_content_stmt(
            &table.physical_name,
            &encoded_rid,
            &decode::encode_doc(&document)?,
        )?;
        let mut prepared = conn.prepare_bound(insert, bindings)?;
        conn.check_failpoint(Failpoint::AfterRecordPrepare)?;
        contextual_constraint(
            prepared.run_ignore_rows().map_err(FastDbError::from),
            "record ID already exists or violates a declared unique index",
        )?;
        conn.check_failpoint(Failpoint::AfterRecordInsert)?;
        Ok(full_record_value(&id, &document))
    })?;

    let returned = match statement.return_clause.map(|clause| clause.kind.value) {
        Some(ReturnKind::None) => None,
        Some(ReturnKind::Before) => Some(Value::Null),
        Some(ReturnKind::After) | None => Some(value),
    };
    let result = if statement.only.is_some() {
        StatementResult::Value(returned.unwrap_or(Value::Null))
    } else {
        StatementResult::Rows(returned.into_iter().collect())
    };
    StatementExecution::mutation(result, 1)
}

fn run_select(
    conn: &Connection,
    execution: &ExecutionState,
    statement: turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<StatementResult> {
    if let Some(only) = statement.only {
        if !matches!(statement.target, Target::Record(_)) {
            return unsupported(only, "SELECT ONLY requires a record target");
        }
    }
    let (table_name, id) = target_parts(statement.target.clone())?;
    let catalog = catalog_for_read(conn, execution)?;
    let Some(table) = catalog
        .snapshot()
        .and_then(|snapshot| snapshot.tables.get(&table_name))
    else {
        return Ok(if statement.only.is_some() {
            StatementResult::Value(Value::Null)
        } else {
            StatementResult::Rows(Vec::new())
        });
    };
    let candidates = read_candidates(
        conn,
        table,
        id.as_ref(),
        statement.condition.as_ref(),
        params,
    )?;
    let mut matched = Vec::new();
    for candidate in candidates {
        if matches_condition(statement.condition.as_ref(), &candidate, params)? {
            matched.push(candidate);
        }
    }
    let mut candidates = matched;

    if !statement.order_by.is_empty() {
        let mut keyed = candidates
            .into_iter()
            .map(|candidate| {
                let context = candidate_context(&candidate, params);
                let keys = statement
                    .order_by
                    .iter()
                    .map(|term| {
                        eval::evaluate(
                            &Expr::new(ExprKind::FieldPath(term.path.clone()), term.path.span),
                            &context,
                        )
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok((candidate, keys))
            })
            .collect::<Result<Vec<_>>>()?;
        keyed.sort_by(|(_, left), (_, right)| {
            for ((left, right), term) in left.iter().zip(right).zip(&statement.order_by) {
                let ordering = eval::compare_values(left, right);
                if ordering != Ordering::Equal {
                    return match term.direction.value {
                        turso_fastdb_parser::OrderDirection::Ascending => ordering,
                        turso_fastdb_parser::OrderDirection::Descending => ordering.reverse(),
                    };
                }
            }
            Ordering::Equal
        });
        candidates = keyed.into_iter().map(|(candidate, _)| candidate).collect();
    }

    let start = statement.start.as_ref().map_or(0, |value| {
        usize::try_from(value.value).unwrap_or(usize::MAX)
    });
    let limit = statement.limit.as_ref().map_or(usize::MAX, |value| {
        usize::try_from(value.value).unwrap_or(usize::MAX)
    });
    let rows = candidates
        .into_iter()
        .skip(start)
        .take(limit)
        .map(|candidate| project_candidate(&candidate, &statement.projections, params))
        .collect::<Result<Vec<_>>>()?;
    if statement.only.is_some() {
        Ok(StatementResult::Value(
            rows.into_iter().next().unwrap_or(Value::Null),
        ))
    } else {
        Ok(StatementResult::Rows(rows))
    }
}

fn run_update(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::UpdateStatement,
    params: &Params,
) -> Result<StatementExecution> {
    let assignment_paths = statement
        .assignments
        .iter()
        .map(|assignment| assignment_path(&assignment.path))
        .collect::<Result<Vec<_>>>()?;
    let (table_name, id) = target_parts(statement.target.clone())?;
    let updates = data_mutation(conn, execution, || {
        let catalog = catalog_for_read(conn, execution)?;
        let Some(table) = catalog
            .snapshot()
            .and_then(|snapshot| snapshot.tables.get(&table_name))
            .cloned()
        else {
            return Ok(Vec::new());
        };
        let candidates = read_candidates(
            conn,
            &table,
            id.as_ref(),
            statement.condition.as_ref(),
            params,
        )?;
        let mut updates = Vec::new();
        for candidate in candidates {
            if !matches_condition(statement.condition.as_ref(), &candidate, params)? {
                continue;
            }
            let context = candidate_context(&candidate, params);
            let values = statement
                .assignments
                .iter()
                .map(|assignment| eval::evaluate(&assignment.value, &context))
                .collect::<Result<Vec<_>>>()?;
            let mut document = candidate.document.clone();
            apply_assignments(
                &mut document,
                assignment_paths.iter().cloned().zip(values).collect(),
            )?;
            reject_stored_id(&document)?;
            schema::validate_document(
                table.mode == TableMode::Schemafull,
                &table.fields,
                &mut document,
            )?;
            validate_index_values(&table, &document)?;
            updates.push((candidate, document));
        }

        conn.check_failpoint(Failpoint::BeforeUpdateMutations)?;
        for (candidate, document) in &updates {
            let (update, bindings) = lower::physical_update_doc_stmt(
                &table.physical_name,
                &candidate.encoded_rid,
                &decode::encode_doc(document)?,
            )?;
            contextual_constraint(
                conn.exec_bound(update, bindings),
                "UPDATE violates a declared unique index",
            )?;
            conn.check_failpoint(Failpoint::AfterUpdateMutation)?;
        }
        Ok(updates)
    })?;
    if matches!(
        statement.return_clause.map(|clause| clause.kind.value),
        Some(ReturnKind::None)
    ) {
        return StatementExecution::mutation(StatementResult::Rows(Vec::new()), updates.len());
    }
    let mutation_count = updates.len();
    StatementExecution::mutation(
        StatementResult::Rows(
            updates
                .into_iter()
                .map(|(candidate, document)| full_record_value(&candidate.id, &document))
                .collect(),
        ),
        mutation_count,
    )
}

fn run_delete(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DeleteStatement,
    params: &Params,
) -> Result<StatementExecution> {
    let (table_name, id) = target_parts(statement.target.clone())?;
    let deleted = data_mutation(conn, execution, || {
        let catalog = catalog_for_read(conn, execution)?;
        let Some(table) = catalog
            .snapshot()
            .and_then(|snapshot| snapshot.tables.get(&table_name))
            .cloned()
        else {
            return Ok(Vec::new());
        };
        let candidates = read_candidates(
            conn,
            &table,
            id.as_ref(),
            statement.condition.as_ref(),
            params,
        )?;
        let mut deleted = Vec::new();
        for candidate in candidates {
            if matches_condition(statement.condition.as_ref(), &candidate, params)? {
                deleted.push(candidate);
            }
        }
        conn.check_failpoint(Failpoint::BeforeDeleteMutations)?;
        for candidate in &deleted {
            let (delete, bindings) =
                lower::physical_delete_by_rid_stmt(&table.physical_name, &candidate.encoded_rid)?;
            conn.exec_bound(delete, bindings)?;
            conn.check_failpoint(Failpoint::AfterDeleteMutation)?;
        }
        Ok(deleted)
    })?;
    if statement.return_clause.is_some() {
        let mutation_count = deleted.len();
        StatementExecution::mutation(
            StatementResult::Rows(
                deleted
                    .into_iter()
                    .map(|candidate| full_record_value(&candidate.id, &candidate.document))
                    .collect(),
            ),
            mutation_count,
        )
    } else {
        StatementExecution::mutation(StatementResult::Rows(Vec::new()), deleted.len())
    }
}

fn evaluate_assignments(
    assignments: &[turso_fastdb_parser::Assignment],
    context: &EvalContext<'_>,
) -> Result<Vec<(Vec<String>, EvalValue)>> {
    assignments
        .iter()
        .map(|assignment| {
            Ok((
                assignment_path(&assignment.path)?,
                eval::evaluate(&assignment.value, context)?,
            ))
        })
        .collect()
}

fn assignment_path(path: &turso_fastdb_parser::FieldPath) -> Result<Vec<String>> {
    let (path, _) = crate::path::parser_path(path)?;
    if path.first().is_some_and(|segment| segment == "id") {
        return Err(FastDbError::Schema(
            "top-level field `id` is read-only and cannot be assigned".into(),
        ));
    }
    Ok(path)
}

fn apply_assignments(
    document: &mut BTreeMap<String, Value>,
    assignments: Vec<(Vec<String>, EvalValue)>,
) -> Result<()> {
    for (path, value) in assignments {
        match value {
            EvalValue::Missing => crate::path::remove_path(document, &path)?,
            EvalValue::Present(value) => crate::path::set_path(document, &path, value)?,
        }
    }
    Ok(())
}

fn project_candidate(
    candidate: &Candidate,
    projections: &ProjectionList,
    params: &Params,
) -> Result<Value> {
    if matches!(projections, ProjectionList::All(_)) {
        return Ok(full_record_value(&candidate.id, &candidate.document));
    }
    let ProjectionList::Fields(projections) = projections else {
        unreachable!()
    };
    let context = candidate_context(candidate, params);
    let mut object = BTreeMap::new();
    for projection in projections {
        let value = eval::evaluate(
            &Expr::new(
                ExprKind::FieldPath(projection.path.clone()),
                projection.path.span,
            ),
            &context,
        )?
        .into_projection();
        if let Some(alias) = &projection.alias {
            object.insert(alias.value.clone(), value);
        } else {
            let path = projection
                .path
                .segments
                .iter()
                .map(|segment| segment.value.clone())
                .collect::<Vec<_>>();
            crate::path::set_path(&mut object, &path, value)?;
        }
    }
    Ok(Value::Object(object))
}

fn matches_condition(
    condition: Option<&Expr>,
    candidate: &Candidate,
    params: &Params,
) -> Result<bool> {
    let Some(condition) = condition else {
        return Ok(true);
    };
    Ok(eval::evaluate(condition, &candidate_context(candidate, params))?.truthy())
}

fn candidate_context<'a>(candidate: &'a Candidate, params: &'a Params) -> EvalContext<'a> {
    EvalContext {
        document: &candidate.document,
        id: &candidate.id,
        params,
    }
}

fn full_record_value(id: &RecordId, document: &BTreeMap<String, Value>) -> Value {
    let mut object = document.clone();
    object.insert("id".into(), Value::RecordId(id.clone()));
    Value::Object(object)
}

fn reject_stored_id(document: &BTreeMap<String, Value>) -> Result<()> {
    if document.contains_key("id") {
        return Err(FastDbError::Schema(
            "top-level field `id` is reserved and synthesized from the record ID".into(),
        ));
    }
    Ok(())
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

fn catalog_for_read(conn: &Connection, execution: &ExecutionState) -> Result<CatalogState> {
    if let TransactionState::Active(active) = &execution.transaction {
        return Ok(active.catalog.clone());
    }
    conn.wait_for_catalog()?;
    conn.coordinator
        .catalog
        .read()
        .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?
        .clone()
        .ok_or_else(|| FastDbError::Engine("catalog cache was not initialized".into()))
}

fn read_candidates(
    conn: &Connection,
    table: &TableDefinition,
    id: Option<&RecordIdValue>,
    condition: Option<&Expr>,
    params: &Params,
) -> Result<Vec<Candidate>> {
    let predicates = condition
        .map(|condition| safe_pushdowns(condition, params, table))
        .unwrap_or_default();
    let encoded_rid = id.map(encode_rid);
    let (statement, bindings) = lower::physical_select_predicates_stmt(
        &table.physical_name,
        encoded_rid.as_deref(),
        &predicates,
    )?;
    conn.collect_rows(statement, bindings)
        .map_err(stored_value_error)?
        .into_iter()
        .map(|row| {
            let encoded_rid = value_to_string(row.first().unwrap_or(&turso_core::Value::Null))
                .map_err(stored_value_error)?;
            let id = RecordId::new(&table.logical_name, decode_rid(&encoded_rid)?);
            let json = value_to_string(row.get(1).unwrap_or(&turso_core::Value::Null))
                .map_err(stored_value_error)?;
            let document = decode::parse_doc(&json)?.into_iter().collect();
            Ok(Candidate {
                encoded_rid,
                id,
                document,
            })
        })
        .collect()
}

fn safe_pushdowns(
    expression: &Expr,
    params: &Params,
    table: &TableDefinition,
) -> Vec<(String, PredicateOperator, Value)> {
    match &expression.kind {
        ExprKind::Parenthesized(inner) => safe_pushdowns(inner, params, table),
        ExprKind::Binary {
            left,
            operator,
            right,
        } if operator.value == BinaryOperator::And => {
            let mut result = safe_pushdowns(left, params, table);
            result.extend(safe_pushdowns(right, params, table));
            result
        }
        ExprKind::Binary {
            left,
            operator,
            right,
        } if operator.value == BinaryOperator::Equal => {
            scalar_comparison_pushdown(left, right, PredicateOperator::Equal, params, table, false)
                .or_else(|| {
                    scalar_comparison_pushdown(
                        right,
                        left,
                        PredicateOperator::Equal,
                        params,
                        table,
                        false,
                    )
                })
                .into_iter()
                .collect()
        }
        ExprKind::Binary {
            left,
            operator,
            right,
        } if matches!(
            operator.value,
            BinaryOperator::Less
                | BinaryOperator::LessEqual
                | BinaryOperator::Greater
                | BinaryOperator::GreaterEqual
        ) =>
        {
            let direct = match operator.value {
                BinaryOperator::Less => PredicateOperator::Less,
                BinaryOperator::LessEqual => PredicateOperator::LessEqual,
                BinaryOperator::Greater => PredicateOperator::Greater,
                BinaryOperator::GreaterEqual => PredicateOperator::GreaterEqual,
                _ => unreachable!(),
            };
            let reversed = match direct {
                PredicateOperator::Less => PredicateOperator::Greater,
                PredicateOperator::LessEqual => PredicateOperator::GreaterEqual,
                PredicateOperator::Greater => PredicateOperator::Less,
                PredicateOperator::GreaterEqual => PredicateOperator::LessEqual,
                PredicateOperator::Equal => PredicateOperator::Equal,
            };
            scalar_comparison_pushdown(left, right, direct, params, table, true)
                .or_else(|| scalar_comparison_pushdown(right, left, reversed, params, table, true))
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

fn scalar_comparison_pushdown(
    path: &Expr,
    value: &Expr,
    operator: PredicateOperator,
    params: &Params,
    table: &TableDefinition,
    require_declared_type: bool,
) -> Option<(String, PredicateOperator, Value)> {
    let ExprKind::FieldPath(path) = &path.kind else {
        return None;
    };
    if path.segments.first()?.value == "id" {
        return None;
    }
    let value = scalar_expression_value(value, params)?;
    if matches!(value, Value::Null) || !value.is_indexable_scalar() {
        return None;
    }
    let (_, path) = crate::path::parser_path(path).ok()?;
    if require_declared_type {
        let rule = table.fields.get(&path)?;
        if !rule.required || !range_type_matches(&rule.ty, &value) {
            return None;
        }
    }
    Some((path, operator, value))
}

fn range_type_matches(ty: &FieldType, value: &Value) -> bool {
    matches!(
        (ty, value),
        (FieldType::Bool, Value::Bool(_))
            | (FieldType::Int, Value::Integer(_))
            | (FieldType::Float, Value::Float(_))
            | (FieldType::Number, Value::Integer(_) | Value::Float(_))
            | (FieldType::String, Value::Str(_))
    )
}

fn scalar_expression_value(expression: &Expr, params: &Params) -> Option<Value> {
    match &expression.kind {
        ExprKind::Null => Some(Value::Null),
        ExprKind::Bool(value) => Some(Value::Bool(*value)),
        ExprKind::Integer(value) => Some(Value::Integer(*value)),
        ExprKind::Float(value) if value.is_finite() => Some(Value::Float(*value)),
        ExprKind::String(value) => Some(Value::Str(value.clone())),
        ExprKind::Parameter(name) => params.get(name).cloned(),
        ExprKind::Parenthesized(inner) => scalar_expression_value(inner, params),
        _ => None,
    }
}

fn data_mutation<R>(
    conn: &Connection,
    execution: &ExecutionState,
    body: impl FnOnce() -> Result<R>,
) -> Result<R> {
    if matches!(execution.transaction, TransactionState::Active(_)) {
        body()
    } else {
        conn.with_transaction(body)
    }
}

fn with_create_mutation<R>(
    conn: &Connection,
    execution: &mut ExecutionState,
    table_was_missing: bool,
    body: impl FnOnce(&mut CatalogState) -> Result<R>,
) -> Result<R> {
    if matches!(execution.transaction, TransactionState::Active(_)) {
        if table_was_missing {
            conn.acquire_schema_lease()?;
        }
        let TransactionState::Active(active) = &mut execution.transaction else {
            unreachable!()
        };
        if table_was_missing {
            active.schema_changed = true;
        }
        return body(&mut active.catalog);
    }
    conn.wait_for_catalog()?;
    schema_mutation(conn, body)
}

fn with_schema_mutation<R>(
    conn: &Connection,
    execution: &mut ExecutionState,
    body: impl FnOnce(&mut CatalogState) -> Result<R>,
) -> Result<R> {
    if matches!(execution.transaction, TransactionState::Active(_)) {
        conn.acquire_schema_lease()?;
        let TransactionState::Active(active) = &mut execution.transaction else {
            unreachable!()
        };
        active.schema_changed = true;
        return body(&mut active.catalog);
    }
    conn.wait_for_catalog()?;
    schema_mutation(conn, body)
}

fn run_define_table(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineTableStatement,
    source: &str,
) -> Result<StatementResult> {
    let definition = source_slice(source, statement.span)?.to_string();
    with_schema_mutation(conn, execution, |state| {
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
    Ok(StatementResult::None)
}

fn run_define_field(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineFieldStatement,
    source: &str,
) -> Result<StatementResult> {
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
    with_schema_mutation(conn, execution, |state| {
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
    Ok(StatementResult::None)
}

fn run_define_index(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineIndexStatement,
    source: &str,
) -> Result<StatementResult> {
    let definition = source_slice(source, statement.span)?.to_string();
    let paths = statement
        .fields
        .iter()
        .map(|path| crate::path::parser_path(path).map(|(segments, _)| segments))
        .collect::<Result<Vec<_>>>()?;
    if paths
        .iter()
        .any(|path| path.first().is_some_and(|segment| segment == "id"))
    {
        return Err(FastDbError::Schema(
            "the synthesized `id` path cannot be indexed".into(),
        ));
    }
    let index = catalog::allocate_index(
        &statement.name.value,
        paths,
        statement.unique.is_some(),
        definition,
    )?;
    with_schema_mutation(conn, execution, |state| {
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
    Ok(StatementResult::None)
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

#[cfg(feature = "testing")]
pub(crate) fn lowered_select_for_explain(
    conn: &Connection,
    statement: turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<turso_parser::ast::Stmt> {
    eval::validate_parameter_references(&Statement::Select(statement.clone()), params)?;
    let (table_name, id) = target_parts(statement.target)?;
    conn.wait_for_catalog()?;
    let catalog = conn
        .coordinator
        .catalog
        .read()
        .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?
        .clone()
        .ok_or_else(|| FastDbError::Engine("catalog cache was not initialized".into()))?;
    let table = catalog
        .snapshot()
        .and_then(|snapshot| snapshot.tables.get(&table_name))
        .ok_or_else(|| FastDbError::Schema(format!("table {table_name:?} is not defined")))?;
    let predicates = statement
        .condition
        .as_ref()
        .map(|condition| safe_pushdowns(condition, params, table))
        .unwrap_or_default();
    lower::physical_select_predicates_stmt(
        &table.physical_name,
        id.as_ref().map(encode_rid).as_deref(),
        &predicates,
    )
    .map(|(statement, _)| statement)
}
