//! Phase 3 planning, evaluation, lowering, and atomic execution.

use crate::catalog::{
    self, CapabilityRequirement, CatalogSnapshot, CatalogState, IndexDefinition, IndexKind,
    TableDefinition, TableKind, BUILTIN_GRAPH_ENCODING_VERSION, BUILTIN_GRAPH_PROVIDER,
    BUILTIN_GRAPH_PROVIDER_VERSION,
};
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
use std::sync::RwLockReadGuard;
use turso_fastdb_parser::{
    BinaryOperator, CreateData, Expr, ExprKind, IndexKindSyntax, ProjectionList, RecordIdPart,
    RecordIdPartKind, ReturnKind, Span, Statement, TableKindSyntax, TableMode, Target,
};

#[derive(Debug, Clone)]
struct Candidate {
    encoded_rid: String,
    id: RecordId,
    document: BTreeMap<String, Value>,
    endpoints: Option<(RecordId, RecordId)>,
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
                Statement::Relate(statement) => run_relate(conn, execution, statement, params),
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
                Statement::Explain(statement) => run_explain(conn, execution, statement, params)
                    .map(StatementExecution::read_only),
                Statement::RemoveIndex(statement) => {
                    run_remove_index(conn, execution, statement).map(StatementExecution::read_only)
                }
                Statement::RebuildIndex(statement) => {
                    run_rebuild_index(conn, execution, statement).map(StatementExecution::read_only)
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
        endpoints: None,
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
        if table.kind == TableKind::Relation {
            return Err(FastDbError::Schema(
                "relation records must be created with RELATE".into(),
            ));
        }
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

fn run_relate(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::RelateStatement,
    params: &Params,
) -> Result<StatementExecution> {
    let from = resolve_relate_endpoint(&statement.from, params)?;
    let to = resolve_relate_endpoint(&statement.to, params)?;
    let relation_name = statement.relation.value.clone();
    let edge_id = RecordId::new(&relation_name, uuid::Uuid::now_v7());
    let empty = BTreeMap::new();
    let context = EvalContext {
        document: &empty,
        id: &edge_id,
        endpoints: Some((&from, &to)),
        params,
    };
    let mut document = match &statement.data {
        None => BTreeMap::new(),
        Some(CreateData::Content(expression)) => {
            let Value::Object(document) = eval::evaluate(expression, &context)?.into_projection()
            else {
                return Err(FastDbError::Schema(
                    "RELATE CONTENT must evaluate to an object".into(),
                ));
            };
            document
        }
        Some(CreateData::Set(assignments)) => {
            let evaluated = evaluate_assignments(assignments, &context)?;
            let mut document = BTreeMap::new();
            apply_assignments(&mut document, evaluated)?;
            document
        }
    };
    reject_stored_edge_fields(&document)?;

    let catalogs_missing = !catalog_for_read(conn, execution)?
        .snapshot()
        .is_some_and(|snapshot| {
            snapshot.tables.contains_key(&from.table)
                && snapshot.tables.contains_key(&to.table)
                && snapshot.tables.contains_key(&relation_name)
        });
    let value = with_create_mutation(conn, execution, catalogs_missing, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        for endpoint in [&from, &to] {
            if !snapshot.tables.contains_key(&endpoint.table) {
                register_normal_table(
                    conn,
                    snapshot,
                    &endpoint.table,
                    TableMode::Schemaless,
                    None,
                )?;
            }
            if snapshot.tables[&endpoint.table].kind != TableKind::Normal {
                return Err(FastDbError::Schema(format!(
                    "RELATE endpoint {:?} is not a normal record table",
                    endpoint.table
                )));
            }
        }
        if !snapshot.tables.contains_key(&relation_name) {
            create_relation_table(
                conn,
                snapshot,
                &relation_name,
                TableMode::Schemaless,
                None,
                None,
                None,
                false,
            )?;
        }
        let relation = snapshot.tables[&relation_name].clone();
        if relation.kind != TableKind::Relation {
            return Err(FastDbError::Schema(format!(
                "RELATE middle table {relation_name:?} is not a relation table"
            )));
        }
        let from_table = &snapshot.tables[&from.table];
        let to_table = &snapshot.tables[&to.table];
        if relation
            .relation_in_table_id
            .is_some_and(|expected| expected != from_table.id)
        {
            return Err(FastDbError::Schema(format!(
                "relation {:?} does not accept input table {:?}",
                relation_name, from.table
            )));
        }
        if relation
            .relation_out_table_id
            .is_some_and(|expected| expected != to_table.id)
        {
            return Err(FastDbError::Schema(format!(
                "relation {:?} does not accept output table {:?}",
                relation_name, to.table
            )));
        }
        if relation.relation_enforced {
            if !record_exists(conn, from_table, &from.id)? {
                return Err(FastDbError::Constraint(format!(
                    "enforced relation source record {from} does not exist"
                )));
            }
            if !record_exists(conn, to_table, &to.id)? {
                return Err(FastDbError::Constraint(format!(
                    "enforced relation target record {to} does not exist"
                )));
            }
        }
        schema::validate_document(
            relation.mode == TableMode::Schemafull,
            &relation.fields,
            &mut document,
        )?;
        validate_index_values(&relation, &document)?;
        let hidden = catalog::graph_columns(snapshot, &relation)?
            .into_iter()
            .map(|column| column.physical_name.clone())
            .collect::<Vec<_>>();
        let (insert, bindings) = lower::physical_relation_insert_stmt(
            &relation.physical_name,
            &hidden,
            &encode_rid(&edge_id.id),
            &decode::encode_doc(&document)?,
            &from_table.id.to_hex(),
            &encode_rid(&from.id),
            &to_table.id.to_hex(),
            &encode_rid(&to.id),
        )?;
        conn.exec_bound(insert, bindings)?;
        conn.check_failpoint(Failpoint::AfterGraphEdgeInsert)?;
        Ok(full_edge_value(&edge_id, &from, &to, &document))
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

fn resolve_relate_endpoint(expression: &Expr, params: &Params) -> Result<RecordId> {
    match &expression.kind {
        ExprKind::RecordId(record) => Ok(RecordId::new(
            &record.table.value,
            record_id_value(record.id.clone())?,
        )),
        ExprKind::Parameter(name) => match params.get(name) {
            Some(Value::RecordId(record)) => Ok(record.clone()),
            Some(_) => Err(FastDbError::Schema(format!(
                "RELATE parameter ${name} must contain a record ID"
            ))),
            None => Err(FastDbError::Schema(format!(
                "missing value for parameter ${name}"
            ))),
        },
        _ => Err(FastDbError::Schema(
            "RELATE endpoints must be record IDs".into(),
        )),
    }
}

fn record_exists(conn: &Connection, table: &TableDefinition, id: &RecordIdValue) -> Result<bool> {
    let encoded = encode_rid(id);
    let (statement, bindings) =
        lower::physical_select_stmt(&table.physical_name, Some(&encoded), &[])?;
    Ok(!conn.collect_rows(statement, bindings)?.is_empty())
}

fn run_select(
    conn: &Connection,
    execution: &ExecutionState,
    statement: turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<StatementResult> {
    validate_projection_shapes(&statement.projections)?;
    if let Some(only) = statement.only {
        if !matches!(statement.target, Target::Record(_)) {
            return unsupported(only, "SELECT ONLY requires a record target");
        }
    }
    let (table_name, id) = target_parts(statement.target.clone())?;
    let catalog = catalog_for_read(conn, execution)?;
    let Some(snapshot) = catalog.snapshot() else {
        return Ok(if statement.only.is_some() {
            StatementResult::Value(Value::Null)
        } else {
            StatementResult::Rows(Vec::new())
        });
    };
    let Some(table) = snapshot.tables.get(&table_name) else {
        return Ok(if statement.only.is_some() {
            StatementResult::Value(Value::Null)
        } else {
            StatementResult::Rows(Vec::new())
        });
    };
    let candidates = read_candidates(
        conn,
        snapshot,
        table,
        id.as_ref(),
        statement.condition.as_ref(),
        params,
        !matches!(execution.transaction, TransactionState::Active(_)),
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
        .map(|candidate| {
            project_candidate(conn, snapshot, &candidate, &statement.projections, params)
        })
        .collect::<Result<Vec<_>>>()?;
    if statement.only.is_some() {
        Ok(StatementResult::Value(
            rows.into_iter().next().unwrap_or(Value::Null),
        ))
    } else {
        Ok(StatementResult::Rows(rows))
    }
}

fn validate_projection_shapes(projections: &ProjectionList) -> Result<()> {
    if let ProjectionList::Fields(projections) = projections {
        for projection in projections {
            if projection.alias.is_none()
                && !matches!(projection.expression.kind, ExprKind::FieldPath(_))
            {
                return Err(FastDbError::Schema(
                    "non-field SELECT projections require an AS alias".into(),
                ));
            }
        }
    }
    Ok(())
}

fn run_explain(
    conn: &Connection,
    execution: &ExecutionState,
    statement: turso_fastdb_parser::ExplainStatement,
    params: &Params,
) -> Result<StatementResult> {
    validate_projection_shapes(&statement.select.projections)?;
    let graph_statements = lower_graph_scans_for_explain(conn, execution, &statement.select)?;
    let lowered = lower_select_scan_for_explain(conn, execution, statement.select.clone(), params)?;
    let mut details = crate::connection::explain_statement(conn, lowered)?;
    for graph_statement in graph_statements {
        details.extend(crate::connection::explain_statement(conn, graph_statement)?);
    }
    let rows =
        details
            .into_iter()
            .enumerate()
            .map(|(ordinal, detail)| {
                Ok(Value::Object(BTreeMap::from([
                    (
                        "ordinal".to_string(),
                        Value::Integer(i64::try_from(ordinal).map_err(|_| {
                            FastDbError::Engine("explain row count overflow".into())
                        })?),
                    ),
                    ("detail".to_string(), Value::Str(detail)),
                ])))
            })
            .collect::<Result<Vec<_>>>()?;
    Ok(StatementResult::Rows(rows))
}

fn lower_graph_scans_for_explain(
    conn: &Connection,
    execution: &ExecutionState,
    select: &turso_fastdb_parser::SelectStatement,
) -> Result<Vec<turso_parser::ast::Stmt>> {
    let ProjectionList::Fields(projections) = &select.projections else {
        return Ok(Vec::new());
    };
    let (source_table_name, _) = target_parts(select.target.clone())?;
    let catalog = catalog_for_read(conn, execution)?;
    let snapshot = catalog
        .snapshot()
        .ok_or_else(|| FastDbError::Schema("graph source catalog is absent".into()))?;
    let mut statements = Vec::new();
    for projection in projections {
        let ExprKind::Traversal(traversal) = &projection.expression.kind else {
            continue;
        };
        let mut current_table_name = source_table_name.as_str();
        for hop in &traversal.hops {
            let current = snapshot.tables.get(current_table_name).ok_or_else(|| {
                FastDbError::Schema(format!(
                    "graph traversal endpoint table {current_table_name:?} is not defined"
                ))
            })?;
            let other = snapshot
                .tables
                .get(&hop.endpoint_table.value)
                .ok_or_else(|| {
                    FastDbError::Schema(format!(
                        "graph traversal endpoint table {:?} is not defined",
                        hop.endpoint_table.value
                    ))
                })?;
            let relation = snapshot.tables.get(&hop.relation.value).ok_or_else(|| {
                FastDbError::Schema(format!(
                    "graph traversal relation table {:?} is not defined",
                    hop.relation.value
                ))
            })?;
            if relation.kind != TableKind::Relation {
                return Err(FastDbError::Schema(
                    "graph traversal middle table is not a relation".into(),
                ));
            }
            let hidden = catalog::graph_columns(snapshot, relation)?
                .into_iter()
                .map(|column| column.physical_name.clone())
                .collect::<Vec<_>>();
            let directions: &[bool] = match hop.direction.value {
                turso_fastdb_parser::TraversalDirection::Forward => &[true],
                turso_fastdb_parser::TraversalDirection::Reverse => &[false],
                turso_fastdb_parser::TraversalDirection::Bidirectional => &[true, false],
            };
            for forward in directions {
                let (statement, _) = lower::physical_graph_neighbors_stmt(
                    &relation.physical_name,
                    &hidden,
                    *forward,
                    &current.id.to_hex(),
                    &encode_rid("explain"),
                    &other.id.to_hex(),
                )?;
                statements.push(statement);
            }
            current_table_name = &hop.endpoint_table.value;
        }
    }
    Ok(statements)
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
        let Some(snapshot) = catalog.snapshot() else {
            return Ok(Vec::new());
        };
        let Some(table) = snapshot.tables.get(&table_name).cloned() else {
            return Ok(Vec::new());
        };
        if table.kind == TableKind::Relation
            && assignment_paths.iter().any(|path| {
                path.first()
                    .is_some_and(|segment| matches!(segment.as_str(), "in" | "out"))
            })
        {
            return Err(FastDbError::Schema(
                "edge endpoints `in` and `out` are immutable".into(),
            ));
        }
        let candidates = read_candidates(
            conn,
            snapshot,
            &table,
            id.as_ref(),
            statement.condition.as_ref(),
            params,
            false,
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
            if table.kind == TableKind::Relation {
                reject_stored_edge_fields(&document)?;
            } else {
                reject_stored_id(&document)?;
            }
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
                .map(|(candidate, document)| full_candidate_with_document(&candidate, &document))
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
    let (deleted, cascaded_edges) = data_mutation(conn, execution, || {
        let catalog = catalog_for_read(conn, execution)?;
        let Some(snapshot) = catalog.snapshot() else {
            return Ok((Vec::new(), 0));
        };
        let Some(table) = snapshot.tables.get(&table_name).cloned() else {
            return Ok((Vec::new(), 0));
        };
        let candidates = read_candidates(
            conn,
            snapshot,
            &table,
            id.as_ref(),
            statement.condition.as_ref(),
            params,
            false,
        )?;
        let mut deleted = Vec::new();
        for candidate in candidates {
            if matches_condition(statement.condition.as_ref(), &candidate, params)? {
                deleted.push(candidate);
            }
        }
        conn.check_failpoint(Failpoint::BeforeDeleteMutations)?;
        let mut connected_edges = BTreeSet::new();
        if table.kind == TableKind::Normal {
            for candidate in &deleted {
                for relation in snapshot
                    .tables
                    .values()
                    .filter(|table| table.kind == TableKind::Relation)
                {
                    for encoded_edge in
                        connected_edge_ids(conn, snapshot, relation, &table, &candidate.id.id)?
                    {
                        connected_edges.insert((relation.physical_name.clone(), encoded_edge));
                    }
                }
            }
        }
        for (physical_table, encoded_edge) in &connected_edges {
            let (delete, bindings) =
                lower::physical_delete_by_rid_stmt(physical_table, encoded_edge)?;
            conn.exec_bound(delete, bindings)?;
            conn.check_failpoint(Failpoint::AfterDeleteMutation)?;
        }
        for candidate in &deleted {
            let (delete, bindings) =
                lower::physical_delete_by_rid_stmt(&table.physical_name, &candidate.encoded_rid)?;
            conn.exec_bound(delete, bindings)?;
            conn.check_failpoint(Failpoint::AfterDeleteMutation)?;
        }
        Ok((deleted, connected_edges.len()))
    })?;
    let mutation_count = deleted
        .len()
        .checked_add(cascaded_edges)
        .ok_or_else(|| FastDbError::Engine("cascade mutation count overflowed usize".into()))?;
    if statement.return_clause.is_some() {
        StatementExecution::mutation(
            StatementResult::Rows(
                deleted
                    .into_iter()
                    .map(|candidate| full_candidate_value(&candidate))
                    .collect(),
            ),
            mutation_count,
        )
    } else {
        StatementExecution::mutation(StatementResult::Rows(Vec::new()), mutation_count)
    }
}

fn connected_edge_ids(
    conn: &Connection,
    snapshot: &CatalogSnapshot,
    relation: &TableDefinition,
    endpoint_table: &TableDefinition,
    endpoint_id: &RecordIdValue,
) -> Result<Vec<String>> {
    let hidden = catalog::graph_columns(snapshot, relation)?
        .into_iter()
        .map(|column| column.physical_name.clone())
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for forward in [true, false] {
        let (statement, bindings) = lower::physical_graph_connected_edge_ids_stmt(
            &relation.physical_name,
            &hidden,
            forward,
            &endpoint_table.id.to_hex(),
            &encode_rid(endpoint_id),
        )?;
        for row in conn.collect_rows(statement, bindings)? {
            result.push(value_to_string(
                row.first().unwrap_or(&turso_core::Value::Null),
            )?);
        }
    }
    Ok(result)
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
    conn: &Connection,
    snapshot: &CatalogSnapshot,
    candidate: &Candidate,
    projections: &ProjectionList,
    params: &Params,
) -> Result<Value> {
    if matches!(projections, ProjectionList::All(_)) {
        return Ok(full_candidate_value(candidate));
    }
    let ProjectionList::Fields(projections) = projections else {
        unreachable!()
    };
    let context = candidate_context(candidate, params);
    let mut object = BTreeMap::new();
    for projection in projections {
        let value = if let ExprKind::Traversal(traversal) = &projection.expression.kind {
            traverse_graph(conn, snapshot, &candidate.id, traversal)?
        } else {
            eval::evaluate(&projection.expression, &context)?.into_projection()
        };
        if let Some(alias) = &projection.alias {
            object.insert(alias.value.clone(), value);
        } else {
            let ExprKind::FieldPath(path) = &projection.expression.kind else {
                unreachable!("non-field projections without aliases are prevalidated")
            };
            let path = path
                .segments
                .iter()
                .map(|segment| segment.value.clone())
                .collect::<Vec<_>>();
            crate::path::set_path(&mut object, &path, value)?;
        }
    }
    Ok(Value::Object(object))
}

fn traverse_graph(
    conn: &Connection,
    snapshot: &CatalogSnapshot,
    source: &RecordId,
    traversal: &turso_fastdb_parser::TraversalExpr,
) -> Result<Value> {
    const MAX_FRONTIER: usize = 10_000;
    let mut frontier = vec![source.clone()];
    for hop in &traversal.hops {
        let mut next = Vec::new();
        for endpoint in &frontier {
            let directions: &[bool] = match hop.direction.value {
                turso_fastdb_parser::TraversalDirection::Forward => &[true],
                turso_fastdb_parser::TraversalDirection::Reverse => &[false],
                turso_fastdb_parser::TraversalDirection::Bidirectional => &[true, false],
            };
            for forward in directions {
                let neighbors = graph_neighbors(
                    conn,
                    snapshot,
                    endpoint,
                    &hop.relation.value,
                    &hop.endpoint_table.value,
                    *forward,
                )?;
                if next.len().saturating_add(neighbors.len()) > MAX_FRONTIER {
                    return Err(FastDbError::Schema(format!(
                        "graph traversal frontier exceeds {MAX_FRONTIER} endpoint occurrences"
                    )));
                }
                next.extend(neighbors);
            }
        }
        frontier = next;
    }
    if traversal.materialize {
        let mut values = Vec::new();
        for endpoint in frontier {
            if let Some(value) = materialize_record(conn, snapshot, &endpoint)? {
                values.push(value);
            }
        }
        Ok(Value::Array(values))
    } else {
        Ok(Value::Array(
            frontier.into_iter().map(Value::RecordId).collect(),
        ))
    }
}

fn graph_neighbors(
    conn: &Connection,
    snapshot: &CatalogSnapshot,
    endpoint: &RecordId,
    relation_name: &str,
    other_table_name: &str,
    forward: bool,
) -> Result<Vec<RecordId>> {
    let endpoint_table = snapshot.tables.get(&endpoint.table).ok_or_else(|| {
        FastDbError::Schema(format!(
            "graph endpoint table {:?} is not defined",
            endpoint.table
        ))
    })?;
    let other_table = snapshot.tables.get(other_table_name).ok_or_else(|| {
        FastDbError::Schema(format!(
            "graph traversal endpoint table {other_table_name:?} is not defined"
        ))
    })?;
    let relation = snapshot.tables.get(relation_name).ok_or_else(|| {
        FastDbError::Schema(format!(
            "graph traversal relation table {relation_name:?} is not defined"
        ))
    })?;
    if relation.kind != TableKind::Relation || other_table.kind != TableKind::Normal {
        return Err(FastDbError::Schema(
            "graph traversal requires a relation and a normal endpoint table".into(),
        ));
    }
    let hidden = catalog::graph_columns(snapshot, relation)?
        .into_iter()
        .map(|column| column.physical_name.clone())
        .collect::<Vec<_>>();
    let (statement, bindings) = lower::physical_graph_neighbors_stmt(
        &relation.physical_name,
        &hidden,
        forward,
        &endpoint_table.id.to_hex(),
        &encode_rid(&endpoint.id),
        &other_table.id.to_hex(),
    )?;
    conn.collect_rows(statement, bindings)?
        .into_iter()
        .map(|row| {
            let encoded = value_to_string(row.first().unwrap_or(&turso_core::Value::Null))?;
            Ok(RecordId::new(other_table_name, decode_rid(&encoded)?))
        })
        .collect()
}

fn materialize_record(
    conn: &Connection,
    snapshot: &CatalogSnapshot,
    endpoint: &RecordId,
) -> Result<Option<Value>> {
    let table = snapshot
        .tables
        .get(&endpoint.table)
        .ok_or_else(|| FastDbError::format("graph result references a missing endpoint table"))?;
    let encoded = encode_rid(&endpoint.id);
    let (statement, bindings) =
        lower::physical_select_stmt(&table.physical_name, Some(&encoded), &[])?;
    let Some(row) = conn.collect_rows(statement, bindings)?.into_iter().next() else {
        return Ok(None);
    };
    let json = value_to_string(row.get(1).unwrap_or(&turso_core::Value::Null))?;
    let document = decode::parse_doc(&json)?.into_iter().collect();
    Ok(Some(full_record_value(endpoint, &document)))
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
        endpoints: candidate.endpoints.as_ref().map(|(from, to)| (from, to)),
        params,
    }
}

fn full_record_value(id: &RecordId, document: &BTreeMap<String, Value>) -> Value {
    let mut object = document.clone();
    object.insert("id".into(), Value::RecordId(id.clone()));
    Value::Object(object)
}

fn full_candidate_value(candidate: &Candidate) -> Value {
    full_candidate_with_document(candidate, &candidate.document)
}

fn full_candidate_with_document(
    candidate: &Candidate,
    document: &BTreeMap<String, Value>,
) -> Value {
    if let Some((from, to)) = &candidate.endpoints {
        full_edge_value(&candidate.id, from, to, document)
    } else {
        full_record_value(&candidate.id, document)
    }
}

fn full_edge_value(
    id: &RecordId,
    from: &RecordId,
    to: &RecordId,
    document: &BTreeMap<String, Value>,
) -> Value {
    let mut object = document.clone();
    object.insert("id".into(), Value::RecordId(id.clone()));
    object.insert("in".into(), Value::RecordId(from.clone()));
    object.insert("out".into(), Value::RecordId(to.clone()));
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

fn reject_stored_edge_fields(document: &BTreeMap<String, Value>) -> Result<()> {
    reject_stored_id(document)?;
    if document.contains_key("in") || document.contains_key("out") {
        return Err(FastDbError::Schema(
            "top-level fields `in` and `out` are immutable synthesized edge endpoints".into(),
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

enum CatalogRead<'a> {
    Active(&'a CatalogState),
    Shared(RwLockReadGuard<'a, Option<CatalogState>>),
}

impl CatalogRead<'_> {
    fn snapshot(&self) -> Option<&catalog::CatalogSnapshot> {
        match self {
            Self::Active(catalog) => catalog.snapshot(),
            Self::Shared(catalog) => catalog.as_ref().and_then(CatalogState::snapshot),
        }
    }
}

fn catalog_for_read<'a>(
    conn: &'a Connection,
    execution: &'a ExecutionState,
) -> Result<CatalogRead<'a>> {
    if let TransactionState::Active(active) = &execution.transaction {
        return Ok(CatalogRead::Active(&active.catalog));
    }
    conn.wait_for_catalog()?;
    let catalog = conn
        .coordinator
        .catalog
        .read()
        .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?;
    if catalog.is_none() {
        return Err(FastDbError::Engine(
            "catalog cache was not initialized".into(),
        ));
    }
    Ok(CatalogRead::Shared(catalog))
}

fn read_candidates(
    conn: &Connection,
    snapshot: &CatalogSnapshot,
    table: &TableDefinition,
    id: Option<&RecordIdValue>,
    condition: Option<&Expr>,
    params: &Params,
    allow_cache: bool,
) -> Result<Vec<Candidate>> {
    let predicates = condition
        .map(|condition| safe_pushdowns(condition, params, table))
        .unwrap_or_default();
    let encoded_rid = id.map(encode_rid);
    let hidden = if table.kind == TableKind::Relation {
        Some(
            catalog::graph_columns(snapshot, table)?
                .into_iter()
                .map(|column| column.physical_name.clone())
                .collect::<Vec<_>>(),
        )
    } else {
        None
    };
    let (statement, bindings) = if let Some(hidden) = &hidden {
        lower::physical_relation_select_predicates_stmt(
            &table.physical_name,
            hidden,
            encoded_rid.as_deref(),
            &predicates,
        )?
    } else {
        lower::physical_select_predicates_stmt(
            &table.physical_name,
            encoded_rid.as_deref(),
            &predicates,
        )?
    };
    conn.collect_select_candidates(
        statement,
        bindings,
        &table.physical_name,
        encoded_rid.is_some(),
        &predicates,
        allow_cache && table.kind == TableKind::Normal,
    )
    .map_err(stored_value_error)?
    .into_iter()
    .map(|row| {
        let encoded_rid = value_to_string(row.first().unwrap_or(&turso_core::Value::Null))
            .map_err(stored_value_error)?;
        let id = RecordId::new(&table.logical_name, decode_rid(&encoded_rid)?);
        let json = value_to_string(row.get(1).unwrap_or(&turso_core::Value::Null))
            .map_err(stored_value_error)?;
        let document = decode::parse_doc(&json)?.into_iter().collect();
        let endpoints = if table.kind == TableKind::Relation {
            let in_table = crate::names::CatalogId::from_hex(&value_to_string(
                row.get(2).unwrap_or(&turso_core::Value::Null),
            )?)?;
            let in_rid = decode_rid(&value_to_string(
                row.get(3).unwrap_or(&turso_core::Value::Null),
            )?)?;
            let out_table = crate::names::CatalogId::from_hex(&value_to_string(
                row.get(4).unwrap_or(&turso_core::Value::Null),
            )?)?;
            let out_rid = decode_rid(&value_to_string(
                row.get(5).unwrap_or(&turso_core::Value::Null),
            )?)?;
            Some((
                RecordId::new(logical_table_name(snapshot, in_table)?, in_rid),
                RecordId::new(logical_table_name(snapshot, out_table)?, out_rid),
            ))
        } else {
            None
        };
        Ok(Candidate {
            encoded_rid,
            id,
            document,
            endpoints,
        })
    })
    .collect()
}

fn logical_table_name(snapshot: &CatalogSnapshot, id: crate::names::CatalogId) -> Result<&str> {
    snapshot
        .tables
        .values()
        .find(|table| table.id == id)
        .map(|table| table.logical_name.as_str())
        .ok_or_else(|| FastDbError::format("edge endpoint references a missing table catalog"))
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
    if table_was_missing {
        return schema_mutation(conn, body);
    }

    // An existing-table CREATE is a data mutation. It needs the catalog to
    // validate and lower the record, but it must not republish an unchanged
    // catalog generation (which would invalidate every prepared SELECT after
    // every insert).
    // Keep the schema mutex through commit so a concurrent standalone DEFINE
    // cannot publish a stricter schema after this validation snapshot but
    // before the record mutation.
    let _schema_guard = conn
        .coordinator
        .schema_mutex
        .lock()
        .map_err(|_| FastDbError::Transaction("database schema mutex is poisoned".into()))?;
    conn.wait_for_catalog()?;
    let mut catalog = conn
        .coordinator
        .catalog
        .read()
        .map_err(|_| FastDbError::Transaction("catalog cache lock is poisoned".into()))?
        .clone()
        .ok_or_else(|| FastDbError::Engine("catalog cache was not initialized".into()))?;
    data_mutation(conn, execution, || body(&mut catalog))
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
        match &statement.kind {
            TableKindSyntax::Normal { .. } => {
                register_normal_table(
                    conn,
                    snapshot,
                    &statement.name.value,
                    statement.mode.value,
                    Some(definition.clone()),
                )?;
            }
            TableKindSyntax::Relation(relation) => {
                for endpoint in [relation.input.as_ref(), relation.output.as_ref()]
                    .into_iter()
                    .flatten()
                {
                    if endpoint.value == statement.name.value {
                        return Err(FastDbError::Schema(
                            "a relation table cannot constrain an endpoint to itself".into(),
                        ));
                    }
                    if !snapshot.tables.contains_key(&endpoint.value) {
                        register_normal_table(
                            conn,
                            snapshot,
                            &endpoint.value,
                            TableMode::Schemaless,
                            None,
                        )?;
                    }
                    if snapshot.tables[&endpoint.value].kind != TableKind::Normal {
                        return Err(FastDbError::Schema(format!(
                            "relation endpoint table {:?} is itself a relation",
                            endpoint.value
                        )));
                    }
                }
                let input = relation
                    .input
                    .as_ref()
                    .map(|endpoint| snapshot.tables[&endpoint.value].id);
                let output = relation
                    .output
                    .as_ref()
                    .map(|endpoint| snapshot.tables[&endpoint.value].id);
                create_relation_table(
                    conn,
                    snapshot,
                    &statement.name.value,
                    statement.mode.value,
                    Some(definition.clone()),
                    input,
                    output,
                    relation.enforced.is_some(),
                )?;
            }
        }
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn register_normal_table(
    conn: &Connection,
    snapshot: &mut CatalogSnapshot,
    logical_name: &str,
    mode: TableMode,
    definition: Option<String>,
) -> Result<()> {
    let table = catalog::allocate_table(logical_name, mode, definition)?;
    catalog::persist_table(conn, &table)?;
    conn.check_failpoint(Failpoint::AfterCatalogRow)?;
    conn.exec_bound(lower::physical_table_ddl(&table.physical_name)?, vec![])?;
    conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
    snapshot.tables.insert(logical_name.to_string(), table);
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn create_relation_table(
    conn: &Connection,
    snapshot: &mut CatalogSnapshot,
    logical_name: &str,
    mode: TableMode,
    definition: Option<String>,
    input: Option<crate::names::CatalogId>,
    output: Option<crate::names::CatalogId>,
    enforced: bool,
) -> Result<()> {
    let mut table =
        catalog::allocate_relation_table(logical_name, mode, definition, input, output, enforced)?;
    let hidden = catalog::allocate_graph_hidden_columns(table.id);
    let forward_columns = hidden.iter().collect::<Vec<_>>();
    let reverse_columns = vec![&hidden[2], &hidden[3], &hidden[0], &hidden[1]];
    let forward = catalog::allocate_graph_index("__graph_forward", &forward_columns, "forward")?;
    let reverse = catalog::allocate_graph_index("__graph_reverse", &reverse_columns, "reverse")?;

    catalog::persist_table(conn, &table)?;
    conn.check_failpoint(Failpoint::AfterCatalogRow)?;
    for column in &hidden {
        catalog::persist_hidden_column(conn, column)?;
    }
    conn.check_failpoint(Failpoint::AfterGraphHiddenCatalog)?;
    if !snapshot.capabilities.contains_key(BUILTIN_GRAPH_PROVIDER) {
        catalog::persist_graph_capability(conn)?;
    }
    conn.exec_bound(
        lower::physical_relation_table_ddl(
            &table.physical_name,
            &hidden
                .iter()
                .map(|column| column.physical_name.clone())
                .collect::<Vec<_>>(),
        )?,
        vec![],
    )?;
    conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
    for index in [&forward, &reverse] {
        catalog::persist_index(conn, &table, index)?;
        let provider = crate::provider::index_provider(index)?;
        conn.exec_bound(
            provider.create_statement(index, &table.physical_name)?,
            vec![],
        )?;
        conn.check_failpoint(if index.options_json.contains("forward") {
            Failpoint::AfterGraphForwardIndex
        } else {
            Failpoint::AfterGraphReverseIndex
        })?;
    }

    table.indexes.insert(forward.logical_name.clone(), forward);
    table.indexes.insert(reverse.logical_name.clone(), reverse);
    for column in hidden {
        snapshot.hidden_columns.insert(column.id, column);
    }
    snapshot.capabilities.insert(
        BUILTIN_GRAPH_PROVIDER.to_string(),
        CapabilityRequirement {
            provider: BUILTIN_GRAPH_PROVIDER.to_string(),
            min_provider_version: BUILTIN_GRAPH_PROVIDER_VERSION,
            min_encoding_version: BUILTIN_GRAPH_ENCODING_VERSION,
        },
    );
    snapshot.tables.insert(logical_name.to_string(), table);
    Ok(())
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
        if table.kind == TableKind::Relation
            && rule
                .path
                .first()
                .is_some_and(|segment| matches!(segment.as_str(), "in" | "out"))
        {
            return Err(FastDbError::Schema(
                "synthesized edge endpoints `in` and `out` cannot be declared".into(),
            ));
        }
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
    match &statement.kind {
        IndexKindSyntax::Btree => {}
        IndexKindSyntax::Fulltext { span, .. } => {
            return unsupported(*span, "FULLTEXT indexes are unavailable until Phase 8")
        }
        IndexKindSyntax::Provider { span, .. } => {
            return unsupported(
                *span,
                "provider-specific indexes are unavailable in Phase 6",
            )
        }
    }
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
        if table.kind == TableKind::Relation
            && index.paths.iter().any(|path| {
                path.first()
                    .is_some_and(|segment| matches!(segment.as_str(), "in" | "out"))
            })
        {
            return Err(FastDbError::Schema(
                "synthesized edge endpoints use mandatory internal adjacency indexes".into(),
            ));
        }
        if table.indexes.contains_key(&statement.name.value) {
            return Err(FastDbError::Constraint(format!(
                "index {:?} is already defined on table {:?}",
                statement.name.value, statement.table.value
            )));
        }
        validate_existing_index(conn, table, &index)?;
        conn.check_failpoint(Failpoint::AfterIndexValidation)?;
        conn.exec_bound(
            crate::provider::index_provider(&index)?
                .create_statement(&index, &table.physical_name)?,
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

fn resolve_index_for_maintenance(
    conn: &Connection,
    execution: &ExecutionState,
    table_name: &str,
    index_name: &str,
) -> Result<IndexDefinition> {
    let catalog = catalog_for_read(conn, execution)?;
    let table = catalog
        .snapshot()
        .and_then(|snapshot| snapshot.tables.get(table_name))
        .ok_or_else(|| FastDbError::Schema(format!("table {table_name:?} is not defined")))?;
    let index = table.indexes.get(index_name).cloned().ok_or_else(|| {
        FastDbError::Schema(format!(
            "index {index_name:?} is not defined on table {table_name:?}"
        ))
    })?;
    if index.kind == IndexKind::GraphAdjacency {
        return Err(FastDbError::Schema(
            "graph adjacency indexes are internal and cannot be addressed by logical maintenance"
                .into(),
        ));
    }
    Ok(index)
}

fn run_remove_index(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::IndexMaintenanceStatement,
) -> Result<StatementResult> {
    let table_name = statement.table.value;
    let index_name = statement.name.value;
    let resolved = resolve_index_for_maintenance(conn, execution, &table_name, &index_name)?;
    with_schema_mutation(conn, execution, |state| {
        let table = ready_snapshot_mut(state)?
            .tables
            .get_mut(&table_name)
            .ok_or_else(|| FastDbError::Schema(format!("table {table_name:?} is not defined")))?;
        let current = table.indexes.get(&index_name).ok_or_else(|| {
            FastDbError::Schema(format!(
                "index {index_name:?} is not defined on table {table_name:?}"
            ))
        })?;
        if current.id != resolved.id {
            return Err(FastDbError::Transaction(
                "index changed while waiting for the schema lease".into(),
            ));
        }
        conn.exec_bound(
            crate::provider::index_provider(&resolved)?.drop_statement(&resolved)?,
            vec![],
        )?;
        conn.check_failpoint(Failpoint::AfterIndexRemovePhysical)?;
        catalog::remove_index(conn, &resolved)?;
        conn.check_failpoint(Failpoint::AfterIndexRemoveCatalog)?;
        table.indexes.remove(&index_name);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_rebuild_index(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::IndexMaintenanceStatement,
) -> Result<StatementResult> {
    let table_name = statement.table.value;
    let index_name = statement.name.value;
    let resolved = resolve_index_for_maintenance(conn, execution, &table_name, &index_name)?;
    with_schema_mutation(conn, execution, |state| {
        let current = ready_snapshot_mut(state)?
            .tables
            .get(&table_name)
            .and_then(|table| table.indexes.get(&index_name))
            .ok_or_else(|| {
                FastDbError::Schema(format!(
                    "index {index_name:?} is not defined on table {table_name:?}"
                ))
            })?;
        if current.id != resolved.id {
            return Err(FastDbError::Transaction(
                "index changed while waiting for the schema lease".into(),
            ));
        }
        conn.exec_bound(
            crate::provider::index_provider(&resolved)?.rebuild_statement(&resolved)?,
            vec![],
        )?;
        conn.check_failpoint(Failpoint::AfterIndexRebuild)
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
    conn.coordinator.publish_catalog_generation();
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
        if index.kind == IndexKind::GraphAdjacency {
            continue;
        }
        let _ = index_key(index, document)?;
    }
    Ok(())
}

fn index_key(
    index: &IndexDefinition,
    document: &BTreeMap<String, Value>,
) -> Result<Option<String>> {
    if index.kind != IndexKind::Btree {
        return Err(FastDbError::Engine(
            "document index-key evaluation received a provider-owned index".into(),
        ));
    }
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

fn lower_select_scan_for_explain(
    conn: &Connection,
    execution: &ExecutionState,
    statement: turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<turso_parser::ast::Stmt> {
    eval::validate_parameter_references(&Statement::Select(statement.clone()), params)?;
    let (table_name, id) = target_parts(statement.target)?;
    let catalog = catalog_for_read(conn, execution)?;
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

#[cfg(feature = "testing")]
pub(crate) fn lowered_select_for_explain(
    conn: &Connection,
    statement: turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<turso_parser::ast::Stmt> {
    let execution = ExecutionState {
        transaction: TransactionState::Idle,
    };
    lower_select_scan_for_explain(conn, &execution, statement, params)
}
