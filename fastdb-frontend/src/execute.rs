//! Phase 3 planning, evaluation, lowering, and atomic execution.

use crate::catalog::{
    self, CapabilityRequirement, CatalogSnapshot, CatalogState, IndexDefinition, IndexKind,
    TableDefinition, TableKind, BUILTIN_FTS_PROVIDER, BUILTIN_GRAPH_ENCODING_VERSION,
    BUILTIN_GRAPH_PROVIDER, BUILTIN_GRAPH_PROVIDER_VERSION, BUILTIN_VECTOR_ENCODING_VERSION,
    BUILTIN_VECTOR_PROVIDER, BUILTIN_VECTOR_PROVIDER_VERSION,
};
use crate::connection::{value_to_string, Connection, ExecutionState, TransactionState};
use crate::decode::{self, RecordIdValue};
use crate::error::{ErrorCategory, FastDbError, Result};
use crate::eval::{self, EvalContext, EvalValue};
use crate::lower::{self, PredicateOperator};
use crate::names::{decode_rid, encode_rid};
use crate::schema::{self, FieldRule, FieldType, SchemaExpression};
use crate::test_failpoints::Failpoint;
use crate::{Params, RecordId, StatementResult, Value};
use rand::seq::SliceRandom as _;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::Arc;
use std::sync::RwLockReadGuard;
use std::time::{Duration, Instant};
use turso_fastdb_parser::{
    AssignmentOperator, BinaryOperator, CreateData, Expr, ExprKind, GroupClause,
    IndexDefinitionSurface, IndexKindSyntax, InsertData, ProjectionList, RecordIdPart,
    RecordIdPartKind, ReturnKind, SelectTarget, Span, Statement, TableKindSyntax, TableMode,
    Target, UpdateData,
};

#[derive(Debug, Clone)]
struct Candidate {
    encoded_rid: String,
    id: RecordId,
    document: BTreeMap<String, Value>,
    endpoints: Option<(RecordId, RecordId)>,
    fts: Option<FtsCandidateContext>,
    vector_distance: Option<f64>,
}

#[derive(Debug, Clone)]
struct QueryRow {
    value: Value,
    source: Option<Candidate>,
}

#[derive(Debug, Clone)]
struct ResolvedVectorQuery {
    column: catalog::HiddenColumnDefinition,
    query: turso_core::Value,
    k: u64,
    metric: turso_fastdb_parser::KnnMetric,
    needs_document: bool,
}

#[derive(Debug, Clone)]
struct ResolvedFtsQuery {
    index: IndexDefinition,
    options: catalog::FtsIndexOptions,
    query: String,
}

#[derive(Debug, Clone)]
struct FtsCandidateContext {
    query: ResolvedFtsQuery,
    score: f64,
}

struct CandidateReadOptions<'a> {
    id: Option<&'a RecordIdValue>,
    range: Option<&'a ResolvedRecordRange>,
    condition: Option<&'a Expr>,
    params: &'a Params,
    allow_cache: bool,
    fts: Option<&'a ResolvedFtsQuery>,
    vector: Option<&'a ResolvedVectorQuery>,
}

#[derive(Debug, Clone)]
enum TargetSelector {
    All,
    Record(RecordIdValue),
    Range(ResolvedRecordRange),
}

enum UpdateWork {
    Existing(String, Box<Candidate>),
    Missing(String, RecordIdValue),
}

impl TargetSelector {
    fn id(&self) -> Option<&RecordIdValue> {
        match self {
            Self::Record(id) => Some(id),
            Self::All | Self::Range(_) => None,
        }
    }

    fn range(&self) -> Option<&ResolvedRecordRange> {
        match self {
            Self::Range(range) => Some(range),
            Self::All | Self::Record(_) => None,
        }
    }
}

#[derive(Debug, Clone)]
struct ResolvedRecordRange {
    start: Option<RecordIdValue>,
    end: Option<RecordIdValue>,
    inclusive: bool,
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

const MAX_SCRIPT_STEPS: usize = 100_000;
const MAX_LOOP_ITERATIONS: usize = 10_000;
const MAX_SLEEP: Duration = Duration::from_secs(5);
const MAX_FUNCTION_CALLS: usize = 10_000;
const MAX_FUNCTION_RECURSION: usize = 32;
const MAX_EVENT_INVOCATIONS: usize = 10_000;
const MAX_EVENT_RECURSION: usize = 16;

pub(crate) struct ScriptRuntime {
    bindings: Params,
    request_bindings: Params,
    local_bindings: BTreeSet<String>,
    scopes: Vec<Vec<(String, Option<Value>)>>,
    steps: usize,
    loop_depth: usize,
    function_calls: usize,
    function_depth: usize,
    function_mutations: u64,
    event_invocations: usize,
    event_depth: usize,
    event_mutations: u64,
    active_events: BTreeSet<String>,
    deadline: Option<Instant>,
    cancellation: Option<Arc<AtomicBool>>,
}

impl ScriptRuntime {
    pub(crate) fn new(
        mut bindings: Params,
        request_bindings: Params,
        timeout: Duration,
        cancellation: Option<Arc<AtomicBool>>,
    ) -> Self {
        bindings.extend(request_bindings.clone());
        Self {
            bindings,
            request_bindings,
            local_bindings: BTreeSet::new(),
            scopes: Vec::new(),
            steps: 0,
            loop_depth: 0,
            function_calls: 0,
            function_depth: 0,
            function_mutations: 0,
            event_invocations: 0,
            event_depth: 0,
            event_mutations: 0,
            active_events: BTreeSet::new(),
            deadline: (!timeout.is_zero())
                .then(|| Instant::now().checked_add(timeout))
                .flatten(),
            cancellation,
        }
    }

    fn step(&mut self) -> Result<()> {
        if self.steps == MAX_SCRIPT_STEPS {
            return Err(FastDbError::ResourceLimit(format!(
                "script exceeds {MAX_SCRIPT_STEPS} executed statements"
            )));
        }
        self.steps += 1;
        self.check_deadline()
    }

    fn check_deadline(&self) -> Result<()> {
        if self
            .cancellation
            .as_ref()
            .is_some_and(|cancellation| cancellation.load(AtomicOrdering::SeqCst))
        {
            return Err(FastDbError::Engine(
                "script execution was interrupted".into(),
            ));
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Err(FastDbError::ResourceLimit(
                "script exceeded the request timeout".into(),
            ))
        } else {
            Ok(())
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        let changes = self.scopes.pop().expect("script scope is balanced");
        for (name, previous) in changes.into_iter().rev() {
            if let Some(previous) = previous {
                self.bindings.insert(name, previous);
            } else {
                self.bindings.remove(&name);
            }
        }
    }

    fn bind(&mut self, name: String, value: Value) {
        self.local_bindings.insert(name.clone());
        if let Some(scope) = self.scopes.last_mut() {
            if !scope.iter().any(|(existing, _)| existing == &name) {
                scope.push((name.clone(), self.bindings.get(&name).cloned()));
            }
        }
        self.bindings.insert(name, value);
    }

    fn publish_catalog_parameter(&mut self, name: &str, value: Value) {
        if !self.local_bindings.contains(name) && !self.request_bindings.contains_key(name) {
            self.bindings.insert(name.to_string(), value);
        }
    }

    fn remove_catalog_parameter(&mut self, name: &str) {
        if self.local_bindings.contains(name) {
            return;
        }
        if let Some(value) = self.request_bindings.get(name).cloned() {
            self.bindings.insert(name.to_string(), value);
        } else {
            self.bindings.remove(name);
        }
    }
}

#[derive(Debug)]
enum ScriptFlow {
    Normal,
    Break,
    Continue,
    Return(Value),
}

struct ScriptOutcome {
    execution: StatementExecution,
    flow: ScriptFlow,
}

impl ScriptOutcome {
    fn normal(execution: StatementExecution) -> Self {
        Self {
            execution,
            flow: ScriptFlow::Normal,
        }
    }
}

pub(crate) fn run_statement(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: Statement,
    source: &str,
    script: &mut ScriptRuntime,
) -> Result<StatementExecution> {
    let implicit_frontend_transaction = matches!(execution.transaction, TransactionState::Idle)
        && (statement_invokes_custom_function(&statement)
            || statement_may_fire_events(conn, execution, &statement)?
            || statement_requires_multi_record_transaction(&statement));
    if implicit_frontend_transaction {
        conn.begin_explicit(execution)?;
    }
    let outcome = run_script_statement(conn, execution, statement, source, script);
    let mut outcome = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            if implicit_frontend_transaction {
                let _ = conn.cancel_explicit(execution);
            }
            return Err(error);
        }
    };
    if implicit_frontend_transaction
        && matches!(outcome.flow, ScriptFlow::Break | ScriptFlow::Continue)
    {
        let _ = conn.cancel_explicit(execution);
        return Err(FastDbError::Schema(
            "loop control escaped its enclosing FOR statement".into(),
        ));
    }
    if implicit_frontend_transaction {
        if let Err(error) = conn.commit_explicit(execution) {
            let _ = conn.cancel_explicit(execution);
            return Err(error);
        }
    }
    let function_mutations = std::mem::take(&mut script.function_mutations);
    let event_mutations = std::mem::take(&mut script.event_mutations);
    outcome.execution.mutation_count = outcome
        .execution
        .mutation_count
        .checked_add(function_mutations)
        .and_then(|count| count.checked_add(event_mutations))
        .ok_or_else(|| FastDbError::Engine("nested mutation count overflowed u64".into()))?;
    match outcome.flow {
        ScriptFlow::Normal => Ok(outcome.execution),
        ScriptFlow::Return(value) => Ok(StatementExecution {
            result: StatementResult::Value(value),
            mutation_count: outcome.execution.mutation_count,
        }),
        ScriptFlow::Break => Err(FastDbError::Schema(
            "BREAK is only valid inside a FOR loop".into(),
        )),
        ScriptFlow::Continue => Err(FastDbError::Schema(
            "CONTINUE is only valid inside a FOR loop".into(),
        )),
    }
}

fn statement_may_fire_events(
    conn: &Connection,
    execution: &ExecutionState,
    statement: &Statement,
) -> Result<bool> {
    if !matches!(
        statement,
        Statement::Create(_)
            | Statement::Insert(_)
            | Statement::Relate(_)
            | Statement::Update(_)
            | Statement::Upsert(_)
            | Statement::Delete(_)
    ) {
        return Ok(false);
    }
    Ok(catalog_for_read(conn, execution)?
        .snapshot()
        .is_some_and(|snapshot| {
            !snapshot.views.is_empty()
                || snapshot
                    .tables
                    .values()
                    .any(|table| !table.events.is_empty())
        }))
}

fn statement_requires_multi_record_transaction(statement: &Statement) -> bool {
    match statement {
        Statement::Create(statement) => matches!(
            statement.target,
            Target::RecordRange(_) | Target::Expression(_) | Target::Batch { .. }
        ),
        Statement::Insert(_)
        | Statement::Update(_)
        | Statement::Upsert(_)
        | Statement::Delete(_) => true,
        Statement::DefineTable(statement) => statement.view.is_some(),
        _ => false,
    }
}

fn statement_invokes_custom_function(statement: &Statement) -> bool {
    if matches!(
        statement,
        Statement::DefineFunction(_) | Statement::AlterFunction(_) | Statement::RemoveFunction(_)
    ) {
        return false;
    }
    let block = turso_fastdb_parser::ScriptBlock {
        span: statement.span(),
        statements: vec![statement.clone()],
    };
    !function_dependencies(&block).is_empty()
}

fn run_script_statement(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: Statement,
    source: &str,
    script: &mut ScriptRuntime,
) -> Result<ScriptOutcome> {
    script.step()?;
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
        Statement::Let(statement) => {
            let value = evaluate_script_expression(conn, execution, &statement.value, script)?;
            script.bind(statement.name.value, value);
            Ok(ScriptOutcome::normal(StatementExecution::read_only(
                StatementResult::None,
            )))
        }
        Statement::ScriptReturn(statement) => {
            let value = evaluate_script_expression(conn, execution, &statement.value, script)?;
            Ok(ScriptOutcome {
                execution: StatementExecution::read_only(StatementResult::None),
                flow: ScriptFlow::Return(value),
            })
        }
        Statement::If(statement) => run_script_if(conn, execution, statement, source, script),
        Statement::For(statement) => run_script_for(conn, execution, statement, source, script),
        Statement::Break(_) => {
            if script.loop_depth == 0 {
                return Err(FastDbError::Schema(
                    "BREAK is only valid inside a FOR loop".into(),
                ));
            }
            Ok(ScriptOutcome {
                execution: StatementExecution::read_only(StatementResult::None),
                flow: ScriptFlow::Break,
            })
        }
        Statement::Continue(_) => {
            if script.loop_depth == 0 {
                return Err(FastDbError::Schema(
                    "CONTINUE is only valid inside a FOR loop".into(),
                ));
            }
            Ok(ScriptOutcome {
                execution: StatementExecution::read_only(StatementResult::None),
                flow: ScriptFlow::Continue,
            })
        }
        Statement::Throw(_) => Err(FastDbError::Schema(
            "script THROW aborted execution (payload redacted)".into(),
        )),
        Statement::Sleep(statement) => {
            run_script_sleep(conn, execution, &statement.value, script)?;
            Ok(ScriptOutcome::normal(StatementExecution::read_only(
                StatementResult::None,
            )))
        }
        Statement::DefineParam(statement) => {
            run_define_param(conn, execution, statement, source, script)
                .map(StatementExecution::read_only)
                .map(ScriptOutcome::normal)
        }
        Statement::AlterParam(statement) => {
            run_alter_param(conn, execution, statement, source, script)
                .map(StatementExecution::read_only)
                .map(ScriptOutcome::normal)
        }
        Statement::RemoveParam(statement) => run_remove_param(conn, execution, statement, script)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::DefineFunction(statement) => {
            run_define_function(conn, execution, statement, source)
                .map(StatementExecution::read_only)
                .map(ScriptOutcome::normal)
        }
        Statement::AlterFunction(statement) => run_alter_function(conn, execution, statement)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::RemoveFunction(statement) => run_remove_function(conn, execution, statement)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::DefineEvent(statement) => run_define_event(conn, execution, statement, source)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::AlterEvent(statement) => run_alter_event(conn, execution, statement, source)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::RemoveEvent(statement) => run_remove_event(conn, execution, statement)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::InfoDatabase(_) => run_info_database(conn, execution)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::AlterTable(statement) => run_alter_table(conn, execution, statement)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::RemoveTable(statement) => run_remove_table(conn, execution, statement)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::InfoTable(statement) => run_info_table(conn, execution, statement)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::AlterField(statement) => run_alter_field(conn, execution, statement, source)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::RemoveField(statement) => run_remove_field(conn, execution, statement)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::Begin(_) => conn
            .begin_explicit(execution)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::Commit(_) => conn
            .commit_explicit(execution)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        Statement::Cancel(_) => conn
            .cancel_explicit(execution)
            .map(StatementExecution::read_only)
            .map(ScriptOutcome::normal),
        statement => {
            eval::validate_parameter_references(&statement, &script.bindings)?;
            let params = script.bindings.clone();
            let execution = match statement {
                Statement::Create(statement) => {
                    run_create(conn, execution, statement, &params, script)
                }
                Statement::Insert(statement) => {
                    run_insert(conn, execution, statement, &params, script)
                }
                Statement::Relate(statement) => {
                    run_relate(conn, execution, statement, &params, script)
                }
                Statement::Select(statement) => {
                    run_select(conn, execution, statement, &params, script)
                        .map(StatementExecution::read_only)
                }
                Statement::Update(statement) => {
                    run_update(conn, execution, statement, &params, false, script)
                }
                Statement::Upsert(statement) => {
                    run_update(conn, execution, statement, &params, true, script)
                }
                Statement::Delete(statement) => {
                    run_delete(conn, execution, statement, &params, script)
                }
                Statement::DefineTable(statement) => {
                    run_define_table(conn, execution, statement, source, &params, script)
                        .map(StatementExecution::read_only)
                }
                Statement::DefineField(statement) => {
                    run_define_field(conn, execution, statement, source)
                        .map(StatementExecution::read_only)
                }
                Statement::DefineAnalyzer(statement) => {
                    run_define_analyzer(conn, execution, statement, source)
                        .map(StatementExecution::read_only)
                }
                Statement::DefineIndex(statement) => {
                    run_define_index(conn, execution, statement, source)
                        .map(StatementExecution::read_only)
                }
                Statement::Explain(statement) => {
                    run_explain(conn, execution, statement, &params, script)
                        .map(StatementExecution::read_only)
                }
                Statement::RemoveIndex(statement) => {
                    run_remove_index(conn, execution, statement).map(StatementExecution::read_only)
                }
                Statement::RebuildIndex(statement) => {
                    run_rebuild_index(conn, execution, statement).map(StatementExecution::read_only)
                }
                Statement::Begin(_) | Statement::Commit(_) | Statement::Cancel(_) => {
                    unreachable!("transaction statements were handled above")
                }
                Statement::Let(_)
                | Statement::ScriptReturn(_)
                | Statement::If(_)
                | Statement::For(_)
                | Statement::Break(_)
                | Statement::Continue(_)
                | Statement::Throw(_)
                | Statement::Sleep(_)
                | Statement::DefineParam(_)
                | Statement::AlterParam(_)
                | Statement::RemoveParam(_)
                | Statement::DefineFunction(_)
                | Statement::AlterFunction(_)
                | Statement::RemoveFunction(_)
                | Statement::DefineEvent(_)
                | Statement::AlterEvent(_)
                | Statement::RemoveEvent(_)
                | Statement::InfoDatabase(_)
                | Statement::AlterTable(_)
                | Statement::RemoveTable(_)
                | Statement::InfoTable(_)
                | Statement::AlterField(_)
                | Statement::RemoveField(_) => {
                    unreachable!("script statements were handled above")
                }
            }?;
            Ok(ScriptOutcome::normal(execution))
        }
    }
}

fn evaluate_script_expression(
    conn: &Connection,
    execution: &mut ExecutionState,
    expression: &Expr,
    script: &mut ScriptRuntime,
) -> Result<Value> {
    let functions = catalog_for_read(conn, execution)?
        .snapshot()
        .map(|snapshot| snapshot.functions.clone())
        .unwrap_or_default();
    let mut expression = expression.clone();
    let mut temporary = Params::new();
    let document = BTreeMap::new();
    let id = RecordId::new("__script", "context");
    let context = FunctionExpressionContext {
        document: &document,
        id: &id,
        endpoints: None,
        functions: Some(&functions),
    };
    resolve_custom_function_calls(
        conn,
        execution,
        &mut expression,
        script,
        &mut temporary,
        &context,
    )?;
    let mut params = script.bindings.clone();
    params.extend(temporary);
    eval::evaluate(
        &expression,
        &EvalContext {
            document: &document,
            id: &id,
            endpoints: None,
            params: &params,
            functions: Some(&functions),
            function_calls: None,
            function_depth: 0,
        },
    )
    .map(EvalValue::into_projection)
}

struct FunctionExpressionContext<'a> {
    document: &'a BTreeMap<String, Value>,
    id: &'a RecordId,
    endpoints: Option<(&'a RecordId, &'a RecordId)>,
    functions: Option<&'a BTreeMap<String, catalog::FunctionDefinition>>,
}

fn resolve_custom_function_calls(
    conn: &Connection,
    execution: &mut ExecutionState,
    expression: &mut Expr,
    script: &mut ScriptRuntime,
    temporary: &mut Params,
    context: &FunctionExpressionContext<'_>,
) -> Result<()> {
    match &mut expression.kind {
        ExprKind::FunctionCall { name, arguments } => {
            for argument in arguments.iter_mut() {
                resolve_custom_function_calls(
                    conn, execution, argument, script, temporary, context,
                )?;
            }
            let Some(logical_name) = custom_function_name(name) else {
                return Ok(());
            };
            let mut params = script.bindings.clone();
            params.extend(temporary.clone());
            let values = arguments
                .iter()
                .map(|argument| {
                    eval::evaluate(
                        argument,
                        &EvalContext {
                            document: context.document,
                            id: context.id,
                            endpoints: context.endpoints,
                            params: &params,
                            functions: context.functions,
                            function_calls: None,
                            function_depth: 0,
                        },
                    )
                    .map(EvalValue::into_projection)
                })
                .collect::<Result<Vec<_>>>()?;
            let value = invoke_custom_function(conn, execution, &logical_name, values, script)?;
            let parameter = format!("__fastdb_function_result_{}", temporary.len());
            temporary.insert(parameter.clone(), value);
            expression.kind = ExprKind::Parameter(parameter);
        }
        ExprKind::Array(values) | ExprKind::DestructureList(values) => {
            for value in values {
                resolve_custom_function_calls(conn, execution, value, script, temporary, context)?;
            }
        }
        ExprKind::Object(fields) => {
            for field in fields {
                resolve_custom_function_calls(
                    conn,
                    execution,
                    &mut field.value,
                    script,
                    temporary,
                    context,
                )?;
            }
        }
        ExprKind::Destructure { target, .. }
        | ExprKind::Cast { value: target, .. }
        | ExprKind::Unary {
            operand: target, ..
        }
        | ExprKind::Parenthesized(target) => {
            resolve_custom_function_calls(conn, execution, target, script, temporary, context)?;
        }
        ExprKind::Access { target, accessor } => {
            resolve_custom_function_calls(conn, execution, target, script, temporary, context)?;
            match accessor {
                turso_fastdb_parser::Accessor::Index(index) => {
                    resolve_custom_function_calls(
                        conn, execution, index, script, temporary, context,
                    )?;
                }
                turso_fastdb_parser::Accessor::Slice { start, end, .. } => {
                    if let Some(start) = start {
                        resolve_custom_function_calls(
                            conn, execution, start, script, temporary, context,
                        )?;
                    }
                    if let Some(end) = end {
                        resolve_custom_function_calls(
                            conn, execution, end, script, temporary, context,
                        )?;
                    }
                }
                turso_fastdb_parser::Accessor::Field(_)
                | turso_fastdb_parser::Accessor::Last(_) => {}
            }
        }
        ExprKind::Range(range) => {
            if let Some(start) = &mut range.start {
                resolve_custom_function_calls(conn, execution, start, script, temporary, context)?;
            }
            if let Some(end) = &mut range.end {
                resolve_custom_function_calls(conn, execution, end, script, temporary, context)?;
            }
        }
        ExprKind::Closure(_) => {}
        ExprKind::Knn(knn) => {
            resolve_custom_function_calls(
                conn,
                execution,
                &mut knn.field,
                script,
                temporary,
                context,
            )?;
            resolve_custom_function_calls(
                conn,
                execution,
                &mut knn.query,
                script,
                temporary,
                context,
            )?;
        }
        ExprKind::Binary { left, right, .. } => {
            resolve_custom_function_calls(conn, execution, left, script, temporary, context)?;
            resolve_custom_function_calls(conn, execution, right, script, temporary, context)?;
        }
        ExprKind::None
        | ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Parameter(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_)
        | ExprKind::NamespacedValue { .. }
        | ExprKind::Traversal(_) => {}
    }
    Ok(())
}

fn script_value_truthy(value: &Value) -> bool {
    match value {
        Value::None | Value::Null | Value::Bool(false) | Value::Integer(0) => false,
        Value::Float(value) if *value == 0.0 => false,
        Value::Decimal(value) if value.is_zero() => false,
        Value::Str(value) => !value.is_empty(),
        Value::Bytes(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Set(value) => !value.as_slice().is_empty(),
        _ => true,
    }
}

fn custom_function_name(name: &[turso_fastdb_parser::Identifier]) -> Option<String> {
    (name.len() >= 2 && name[0].value.eq_ignore_ascii_case("fn")).then(|| {
        name.iter()
            .skip(1)
            .map(|segment| segment.value.as_str())
            .collect::<Vec<_>>()
            .join("::")
    })
}

fn expression_invokes_custom_function(expression: &Expr) -> bool {
    let mut dependencies = Vec::new();
    collect_function_expression_dependencies(expression, &mut dependencies);
    !dependencies.is_empty()
}

#[allow(clippy::too_many_arguments)]
fn evaluate_expression_with_custom_functions(
    conn: &Connection,
    execution: &mut ExecutionState,
    document: &BTreeMap<String, Value>,
    id: &RecordId,
    endpoints: Option<(&RecordId, &RecordId)>,
    expression: &Expr,
    params: &Params,
    functions: &BTreeMap<String, catalog::FunctionDefinition>,
    script: &mut ScriptRuntime,
) -> Result<Value> {
    let mut expression = expression.clone();
    let mut temporary = Params::new();
    let context = FunctionExpressionContext {
        document,
        id,
        endpoints,
        functions: Some(functions),
    };
    resolve_custom_function_calls(
        conn,
        execution,
        &mut expression,
        script,
        &mut temporary,
        &context,
    )?;
    let mut params = params.clone();
    params.extend(temporary);
    let calls = std::cell::Cell::new(0);
    eval::evaluate(
        &expression,
        &EvalContext {
            document,
            id,
            endpoints,
            params: &params,
            functions: Some(functions),
            function_calls: Some(&calls),
            function_depth: 0,
        },
    )
    .map(EvalValue::into_projection)
}

fn invoke_custom_function(
    conn: &Connection,
    execution: &mut ExecutionState,
    logical_name: &str,
    arguments: Vec<Value>,
    script: &mut ScriptRuntime,
) -> Result<Value> {
    if script.function_calls == MAX_FUNCTION_CALLS {
        return Err(FastDbError::ResourceLimit(format!(
            "custom function calls exceed {MAX_FUNCTION_CALLS}"
        )));
    }
    if script.function_depth == MAX_FUNCTION_RECURSION {
        return Err(FastDbError::ResourceLimit(format!(
            "custom function recursion exceeds {MAX_FUNCTION_RECURSION}"
        )));
    }
    let function = {
        let catalog = catalog_for_read(conn, execution)?;
        catalog
            .snapshot()
            .and_then(|snapshot| snapshot.functions.get(logical_name))
            .cloned()
            .ok_or_else(|| {
                FastDbError::Schema(format!("function fn::{logical_name} is not defined"))
            })?
    };
    if arguments.len() != function.arguments.len() {
        return Err(FastDbError::Schema(format!(
            "function fn::{logical_name} expects {} arguments but received {}",
            function.arguments.len(),
            arguments.len()
        )));
    }
    let mut arguments = arguments;
    for (argument, value) in function.arguments.iter().zip(&mut arguments) {
        schema::validate_standalone_value(
            &FieldType::parse_canonical(&argument.ty)?,
            value,
            &format!("function argument ${}", argument.name),
        )?;
    }
    script.function_calls += 1;
    script.function_depth += 1;
    script.push_scope();
    for (argument, value) in function.arguments.iter().zip(arguments) {
        script.bind(argument.name.clone(), value);
    }
    let outcome = run_script_block(
        conn,
        execution,
        function.body.clone(),
        &function.definition,
        script,
    );
    script.pop_scope();
    script.function_depth -= 1;
    let outcome = outcome?;
    script.function_mutations = script
        .function_mutations
        .checked_add(outcome.execution.mutation_count)
        .ok_or_else(|| FastDbError::Engine("function mutation count overflowed u64".into()))?;
    match outcome.flow {
        ScriptFlow::Return(value) => Ok(value),
        ScriptFlow::Normal => match outcome.execution.result {
            StatementResult::Value(value) => Ok(value),
            StatementResult::Rows(values) => Ok(Value::Array(values)),
            StatementResult::None => Ok(Value::None),
        },
        ScriptFlow::Break | ScriptFlow::Continue => Err(FastDbError::Schema(
            "function body leaked loop control".into(),
        )),
    }
}

fn run_script_block(
    conn: &Connection,
    execution: &mut ExecutionState,
    block: turso_fastdb_parser::ScriptBlock,
    source: &str,
    script: &mut ScriptRuntime,
) -> Result<ScriptOutcome> {
    script.push_scope();
    let result = (|| {
        let mut result = StatementExecution::read_only(StatementResult::None);
        for statement in block.statements {
            let outcome = run_script_statement(conn, execution, statement, source, script)?;
            result.mutation_count = result
                .mutation_count
                .checked_add(outcome.execution.mutation_count)
                .ok_or_else(|| {
                    FastDbError::Engine("script mutation count overflowed u64".into())
                })?;
            result.result = outcome.execution.result;
            if !matches!(outcome.flow, ScriptFlow::Normal) {
                return Ok(ScriptOutcome {
                    execution: result,
                    flow: outcome.flow,
                });
            }
        }
        Ok(ScriptOutcome::normal(result))
    })();
    script.pop_scope();
    result
}

struct EventInvocation<'a> {
    table_name: &'a str,
    kind: &'static str,
    id: &'a RecordId,
    before: Value,
    after: Value,
    input: Value,
}

fn run_table_events(
    conn: &Connection,
    execution: &mut ExecutionState,
    invocation: EventInvocation<'_>,
    script: &mut ScriptRuntime,
) -> Result<()> {
    refresh_dependent_views(conn, execution, invocation.table_name, script)?;
    let events = catalog_for_read(conn, execution)?
        .snapshot()
        .and_then(|snapshot| snapshot.tables.get(invocation.table_name))
        .map(|table| table.events.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    if !events.is_empty() {
        conn.check_failpoint(Failpoint::BeforeEventActions)?;
    }
    for event in events {
        if script.event_invocations == MAX_EVENT_INVOCATIONS {
            return Err(FastDbError::ResourceLimit(format!(
                "event invocations exceed {MAX_EVENT_INVOCATIONS}"
            )));
        }
        if script.event_depth == MAX_EVENT_RECURSION {
            return Err(FastDbError::ResourceLimit(format!(
                "event recursion exceeds {MAX_EVENT_RECURSION}"
            )));
        }
        let active_key = format!(
            "{}:{}:{}:{}",
            event.id.to_hex(),
            invocation.kind,
            invocation.id.table,
            encode_rid(&invocation.id.id)?
        );
        if !script.active_events.insert(active_key.clone()) {
            return Err(FastDbError::ResourceLimit(format!(
                "event {:?} recursively targeted the same record",
                event.logical_name
            )));
        }
        script.event_invocations += 1;
        script.event_depth += 1;
        script.push_scope();
        script.bind("event".into(), Value::Str(invocation.kind.into()));
        script.bind("before".into(), invocation.before.clone());
        script.bind("after".into(), invocation.after.clone());
        script.bind(
            "value".into(),
            if invocation.kind == "DELETE" {
                invocation.before.clone()
            } else {
                invocation.after.clone()
            },
        );
        script.bind("input".into(), invocation.input.clone());
        let result = (|| {
            if let Some(condition) = &event.condition {
                let value = evaluate_script_expression(conn, execution, condition, script)?;
                if !script_value_truthy(&value) {
                    return Ok(0);
                }
            }
            let outcome = run_script_block(
                conn,
                execution,
                event.action.block.clone(),
                &event.definition,
                script,
            )?;
            if matches!(outcome.flow, ScriptFlow::Break | ScriptFlow::Continue) {
                return Err(FastDbError::Schema(
                    "event action leaked loop control".into(),
                ));
            }
            Ok(outcome.execution.mutation_count)
        })();
        script.pop_scope();
        script.event_depth -= 1;
        script.active_events.remove(&active_key);
        let mutation_count = result?;
        script.event_mutations = script
            .event_mutations
            .checked_add(mutation_count)
            .ok_or_else(|| FastDbError::Engine("event mutation count overflowed u64".into()))?;
    }
    Ok(())
}

const VIEW_INTERNAL_FIELD_PREFIX: &str = "\0fastdb-view:";
const MAX_VIEW_DEPENDENCY_DEPTH: usize = 128;

fn refresh_dependent_views(
    conn: &Connection,
    execution: &mut ExecutionState,
    source_table: &str,
    script: &mut ScriptRuntime,
) -> Result<()> {
    refresh_dependent_views_at_depth(conn, execution, source_table, script, 0)
}

fn refresh_dependent_views_at_depth(
    conn: &Connection,
    execution: &mut ExecutionState,
    source_table: &str,
    script: &mut ScriptRuntime,
    depth: usize,
) -> Result<()> {
    if depth == MAX_VIEW_DEPENDENCY_DEPTH {
        return Err(FastDbError::ResourceLimit(format!(
            "view dependency depth exceeds {MAX_VIEW_DEPENDENCY_DEPTH}"
        )));
    }
    let snapshot = catalog_for_read(conn, execution)?
        .snapshot()
        .cloned()
        .ok_or_else(|| FastDbError::format("view refresh requires a catalog"))?;
    let Some(source_id) = snapshot.tables.get(source_table).map(|table| table.id) else {
        return Ok(());
    };
    let views = snapshot
        .views
        .values()
        .filter(|view| view.dependencies.contains(&source_id))
        .map(|view| view.logical_name.clone())
        .collect::<Vec<_>>();
    for view_name in views {
        refresh_view(conn, execution, &view_name, script)?;
        refresh_dependent_views_at_depth(conn, execution, &view_name, script, depth + 1)?;
    }
    Ok(())
}

fn refresh_view(
    conn: &Connection,
    execution: &mut ExecutionState,
    view_name: &str,
    script: &mut ScriptRuntime,
) -> Result<()> {
    let snapshot = catalog_for_read(conn, execution)?
        .snapshot()
        .cloned()
        .ok_or_else(|| FastDbError::format("view refresh requires a catalog"))?;
    let view = snapshot
        .views
        .get(view_name)
        .cloned()
        .ok_or_else(|| FastDbError::format("view refresh target is missing"))?;
    let table = snapshot
        .tables
        .get(view_name)
        .cloned()
        .ok_or_else(|| FastDbError::format("view physical table is missing"))?;
    let rows = evaluate_view_rows(conn, execution, &snapshot, &view, script)?;
    let functions = snapshot.functions.clone();
    data_mutation(conn, execution, || {
        for (encoded_rid, _) in read_documents(conn, &table)? {
            let (delete, bindings) =
                lower::physical_delete_by_rid_stmt(&table.physical_name, &encoded_rid)?;
            conn.exec_bound(delete, bindings)?;
        }
        conn.check_failpoint(Failpoint::DuringViewRefresh)?;
        for (id_value, mut document) in rows {
            let id = RecordId::new(view_name, id_value.clone());
            reject_stored_id(&document)?;
            normalize_schema_document(
                &table,
                &mut document,
                None,
                &id,
                None,
                &Params::new(),
                &functions,
                true,
            )?;
            validate_index_values(&table, &document)?;
            let hidden = derived_hidden_values(&snapshot, &table, &document)?;
            let (insert, bindings) = lower::physical_insert_document_with_hidden_stmt(
                &table.physical_name,
                &encode_rid(&id_value)?,
                &decode::encode_doc(&document)?,
                &hidden,
            )?;
            contextual_constraint(
                conn.exec_bound(insert, bindings),
                "materialized view produced duplicate record identities or index values",
            )?;
        }
        Ok(())
    })?;
    mark_fts_dirty(execution, view_name);
    Ok(())
}

fn evaluate_view_rows(
    conn: &Connection,
    execution: &mut ExecutionState,
    snapshot: &CatalogSnapshot,
    view: &catalog::ViewDefinition,
    script: &mut ScriptRuntime,
) -> Result<Vec<(RecordIdValue, BTreeMap<String, Value>)>> {
    let source_name = view_source_name(&view.select)?;
    let source = snapshot
        .tables
        .get(source_name)
        .ok_or_else(|| FastDbError::format("view source table is missing"))?;
    if read_documents(conn, source)?
        .into_iter()
        .any(|(_, document)| {
            document
                .keys()
                .any(|key| key.starts_with(VIEW_INTERNAL_FIELD_PREFIX))
        })
    {
        return Err(FastDbError::Constraint(
            "view source document uses a reserved materialization field".into(),
        ));
    }

    let mut select = view.select.clone();
    let span = Span::new(select.span.offset, 0);
    let mut identity_aliases = Vec::new();
    match &select.group {
        Some(GroupClause::By(keys)) => {
            let ProjectionList::Fields(projections) = &mut select.projections else {
                return Err(FastDbError::Schema(
                    "grouped views require explicit projections".into(),
                ));
            };
            for (ordinal, key) in keys.iter().cloned().enumerate() {
                let alias = format!("{VIEW_INTERNAL_FIELD_PREFIX}group:{ordinal}");
                projections.push(turso_fastdb_parser::Projection {
                    span: key.span,
                    expression: key,
                    alias: Some(turso_fastdb_parser::Spanned::new(alias.clone(), span)),
                });
                identity_aliases.push(alias);
            }
        }
        Some(GroupClause::All(_)) => {}
        None => {
            let alias = format!("{VIEW_INTERNAL_FIELD_PREFIX}source-id");
            let id_path = turso_fastdb_parser::FieldPath {
                segments: vec![turso_fastdb_parser::Spanned::new("id".into(), span)],
                span,
            };
            let projection = turso_fastdb_parser::Projection {
                span,
                expression: Expr::new(ExprKind::FieldPath(id_path), span),
                alias: Some(turso_fastdb_parser::Spanned::new(alias.clone(), span)),
            };
            match &mut select.projections {
                ProjectionList::All(_) => {
                    select.projections = ProjectionList::Fields(vec![projection]);
                    select.include_all = true;
                }
                ProjectionList::Fields(projections) => projections.push(projection),
            }
            identity_aliases.push(alias);
        }
    }

    let StatementResult::Rows(values) =
        run_select(conn, execution, select, &Params::new(), script)?
    else {
        return Err(FastDbError::Schema(
            "materialized view SELECT must return rows".into(),
        ));
    };
    if values.len() > 100_000 {
        return Err(FastDbError::ResourceLimit(
            "materialized view exceeds 100,000 rows".into(),
        ));
    }
    values
        .into_iter()
        .map(|value| {
            let Value::Object(mut document) = value else {
                return Err(FastDbError::Schema(
                    "materialized view rows must be objects".into(),
                ));
            };
            let id = match &view.select.group {
                Some(GroupClause::By(_)) => RecordIdValue::Array(
                    identity_aliases
                        .iter()
                        .map(|alias| {
                            document.remove(alias).ok_or_else(|| {
                                FastDbError::Engine("view group identity is missing".into())
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                ),
                Some(GroupClause::All(_)) => RecordIdValue::Array(Vec::new()),
                None => {
                    let Value::RecordId(source_id) =
                        document.remove(&identity_aliases[0]).ok_or_else(|| {
                            FastDbError::Engine("view source identity is missing".into())
                        })?
                    else {
                        return Err(FastDbError::format(
                            "view source identity is not a record ID",
                        ));
                    };
                    source_id.id
                }
            };
            document.remove("id");
            Ok((id, document))
        })
        .collect()
}

pub(crate) fn validate_materialized_views(
    conn: &Connection,
    snapshot: &CatalogSnapshot,
) -> Result<()> {
    if snapshot.views.is_empty() {
        return Ok(());
    }
    let mut execution = ExecutionState {
        transaction: TransactionState::Active(Box::new(crate::connection::ActiveTransaction {
            catalog: CatalogState::Ready(Box::new(snapshot.clone())),
            schema_changed: false,
            dirty_fts_tables: BTreeSet::new(),
        })),
    };
    let mut script = ScriptRuntime::new(Params::new(), Params::new(), Duration::ZERO, None);
    for view in snapshot.views.values() {
        let table = snapshot
            .tables
            .get(&view.logical_name)
            .ok_or_else(|| FastDbError::format("view physical table is missing"))?;
        let mut expected = BTreeMap::new();
        for (id_value, mut document) in
            evaluate_view_rows(conn, &mut execution, snapshot, view, &mut script)?
        {
            let id = RecordId::new(&view.logical_name, id_value.clone());
            normalize_schema_document(
                table,
                &mut document,
                None,
                &id,
                None,
                &Params::new(),
                &snapshot.functions,
                true,
            )?;
            validate_index_values(table, &document)?;
            if expected.insert(encode_rid(&id_value)?, document).is_some() {
                return Err(FastDbError::format(
                    "materialized view definition produces duplicate identities",
                ));
            }
        }
        let actual = read_documents(conn, table)?
            .into_iter()
            .collect::<BTreeMap<_, _>>();
        if actual != expected {
            return Err(FastDbError::format(format!(
                "materialized view {:?} does not match its authoritative sources",
                view.logical_name
            )));
        }
    }
    Ok(())
}

fn view_source_name(select: &turso_fastdb_parser::SelectStatement) -> Result<&str> {
    match &select.target {
        SelectTarget::Target(Target::Table(table)) if select.additional_targets.is_empty() => {
            Ok(&table.name.value)
        }
        _ => Err(FastDbError::Schema(
            "materialized views require exactly one table source".into(),
        )),
    }
}

fn reject_direct_view_write(snapshot: &CatalogSnapshot, table_name: &str) -> Result<()> {
    if snapshot.views.contains_key(table_name) {
        return Err(FastDbError::Constraint(format!(
            "materialized view {table_name:?} is read-only"
        )));
    }
    Ok(())
}

fn run_script_if(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::IfStatement,
    source: &str,
    script: &mut ScriptRuntime,
) -> Result<ScriptOutcome> {
    let mut selected = None;
    for (condition, block) in statement.branches {
        if script_value_truthy(&evaluate_script_expression(
            conn, execution, &condition, script,
        )?) {
            selected = Some(block);
            break;
        }
    }
    let selected = selected.or(statement.otherwise);
    let Some(block) = selected else {
        return Ok(ScriptOutcome::normal(StatementExecution::read_only(
            StatementResult::None,
        )));
    };
    let mut outcome = run_script_block(conn, execution, block, source, script)?;
    if let ScriptFlow::Return(value) = outcome.flow {
        outcome.execution.result = StatementResult::Value(value);
        outcome.flow = ScriptFlow::Normal;
    }
    Ok(outcome)
}

fn run_script_for(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::ForStatement,
    source: &str,
    script: &mut ScriptRuntime,
) -> Result<ScriptOutcome> {
    let iterable = evaluate_script_expression(conn, execution, &statement.iterable, script)?;
    let values = script_iterable_values(iterable)?;
    let mut result = StatementExecution::read_only(StatementResult::None);
    for value in values {
        script.check_deadline()?;
        script.push_scope();
        script.bind(statement.binding.value.clone(), value);
        script.loop_depth += 1;
        let outcome = run_script_block(conn, execution, statement.body.clone(), source, script);
        script.loop_depth -= 1;
        script.pop_scope();
        let outcome = outcome?;
        result.mutation_count = result
            .mutation_count
            .checked_add(outcome.execution.mutation_count)
            .ok_or_else(|| FastDbError::Engine("script mutation count overflowed u64".into()))?;
        match outcome.flow {
            ScriptFlow::Normal | ScriptFlow::Continue => {}
            ScriptFlow::Break => break,
            ScriptFlow::Return(value) => {
                result.result = StatementResult::Value(value);
                break;
            }
        }
    }
    Ok(ScriptOutcome::normal(result))
}

fn script_iterable_values(value: Value) -> Result<Vec<Value>> {
    let values = match value {
        Value::Array(values) => values,
        Value::Set(values) => values.as_slice().to_vec(),
        Value::Range(range) => {
            use crate::decode::RangeBound;
            let start = match range.start() {
                RangeBound::Included(value) => match value.as_ref() {
                    Value::Integer(value) => *value,
                    _ => {
                        return Err(FastDbError::Schema(
                            "FOR ranges require integer bounds".into(),
                        ))
                    }
                },
                RangeBound::Excluded(value) => match value.as_ref() {
                    Value::Integer(value) => value.checked_add(1).ok_or_else(|| {
                        FastDbError::ResourceLimit("FOR range start overflows".into())
                    })?,
                    _ => {
                        return Err(FastDbError::Schema(
                            "FOR ranges require integer bounds".into(),
                        ))
                    }
                },
                _ => {
                    return Err(FastDbError::Schema(
                        "FOR ranges require a bounded integer start".into(),
                    ))
                }
            };
            let end = match range.end() {
                RangeBound::Included(value) => match value.as_ref() {
                    Value::Integer(value) => *value,
                    _ => {
                        return Err(FastDbError::Schema(
                            "FOR ranges require integer bounds".into(),
                        ))
                    }
                },
                RangeBound::Excluded(value) => match value.as_ref() {
                    Value::Integer(value) => value.checked_sub(1).ok_or_else(|| {
                        FastDbError::ResourceLimit("FOR range end overflows".into())
                    })?,
                    _ => {
                        return Err(FastDbError::Schema(
                            "FOR ranges require integer bounds".into(),
                        ))
                    }
                },
                _ => {
                    return Err(FastDbError::Schema(
                        "FOR ranges require a bounded integer end".into(),
                    ))
                }
            };
            if end < start {
                Vec::new()
            } else {
                let count = end
                    .checked_sub(start)
                    .and_then(|distance| distance.checked_add(1))
                    .and_then(|count| usize::try_from(count).ok())
                    .ok_or_else(|| FastDbError::ResourceLimit("FOR range is too large".into()))?;
                if count > MAX_LOOP_ITERATIONS {
                    return Err(FastDbError::ResourceLimit(format!(
                        "FOR loop exceeds {MAX_LOOP_ITERATIONS} iterations"
                    )));
                }
                (start..=end).map(Value::Integer).collect()
            }
        }
        _ => {
            return Err(FastDbError::Schema(
                "FOR requires an array, set, or bounded integer range".into(),
            ))
        }
    };
    if values.len() > MAX_LOOP_ITERATIONS {
        return Err(FastDbError::ResourceLimit(format!(
            "FOR loop exceeds {MAX_LOOP_ITERATIONS} iterations"
        )));
    }
    Ok(values)
}

fn run_script_sleep(
    conn: &Connection,
    execution: &mut ExecutionState,
    expression: &Expr,
    script: &mut ScriptRuntime,
) -> Result<()> {
    let Value::Duration(value) = evaluate_script_expression(conn, execution, expression, script)?
    else {
        return Err(FastDbError::Schema("SLEEP requires a duration".into()));
    };
    let duration = Duration::new(value.seconds(), value.nanoseconds());
    if duration > MAX_SLEEP {
        return Err(FastDbError::ResourceLimit(format!(
            "SLEEP exceeds the {} second limit",
            MAX_SLEEP.as_secs()
        )));
    }
    if script.deadline.is_some_and(|deadline| {
        Instant::now()
            .checked_add(duration)
            .is_none_or(|end| end > deadline)
    }) {
        return Err(FastDbError::ResourceLimit(
            "SLEEP exceeds the remaining request timeout".into(),
        ));
    }
    let started = Instant::now();
    while let Some(remaining) = duration.checked_sub(started.elapsed()) {
        script.check_deadline()?;
        std::thread::park_timeout(remaining.min(Duration::from_millis(10)));
    }
    script.check_deadline()
}

fn run_define_param(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineParamStatement,
    source: &str,
    script: &mut ScriptRuntime,
) -> Result<StatementResult> {
    let value = evaluate_script_expression(conn, execution, &statement.value, script)?;
    let value_source = source_slice(source, statement.value.span)?;
    let definition =
        canonical_parameter_definition(&statement.name.value, value_source, statement.permissions);
    let mut published = None;
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        if let Some(existing) = snapshot.parameters.get(&statement.name.value).cloned() {
            if statement.if_not_exists.is_some() {
                published = Some(existing.value);
                return Ok(());
            }
            if statement.overwrite.is_none() {
                return Err(FastDbError::Constraint(format!(
                    "parameter ${} is already defined",
                    statement.name.value
                )));
            }
            catalog::remove_parameter(conn, &existing)?;
            snapshot.parameters.remove(&statement.name.value);
        }
        let parameter = catalog::allocate_parameter(
            &statement.name.value,
            value.clone(),
            value_source.to_string(),
            statement.permissions,
            definition.clone(),
        )?;
        catalog::persist_parameter(conn, &parameter)?;
        published = Some(parameter.value.clone());
        snapshot
            .parameters
            .insert(statement.name.value.clone(), parameter);
        Ok(())
    })?;
    if let Some(value) = published {
        script.publish_catalog_parameter(&statement.name.value, value);
    }
    Ok(StatementResult::None)
}

fn run_alter_param(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::AlterParamStatement,
    source: &str,
    script: &mut ScriptRuntime,
) -> Result<StatementResult> {
    let evaluated = statement
        .value
        .as_ref()
        .map(|expression| evaluate_script_expression(conn, execution, expression, script))
        .transpose()?;
    let value_source = statement
        .value
        .as_ref()
        .map(|expression| source_slice(source, expression.span).map(str::to_string))
        .transpose()?;
    let mut published = None;
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(existing) = snapshot.parameters.get(&statement.name.value).cloned() else {
            return Err(FastDbError::Schema(format!(
                "parameter ${} is not defined",
                statement.name.value
            )));
        };
        let value = evaluated.clone().unwrap_or_else(|| existing.value.clone());
        let value_source = value_source
            .clone()
            .unwrap_or_else(|| existing.value_source.clone());
        let permissions = statement.permissions.unwrap_or(existing.permissions);
        let definition =
            canonical_parameter_definition(&statement.name.value, &value_source, permissions);
        let replacement = catalog::allocate_parameter(
            &statement.name.value,
            value.clone(),
            value_source,
            permissions,
            definition,
        )?;
        catalog::remove_parameter(conn, &existing)?;
        catalog::persist_parameter(conn, &replacement)?;
        snapshot
            .parameters
            .insert(statement.name.value.clone(), replacement);
        published = Some(value);
        Ok(())
    })?;
    if let Some(value) = published {
        script.publish_catalog_parameter(&statement.name.value, value);
    }
    Ok(StatementResult::None)
}

fn canonical_parameter_definition(
    name: &str,
    value_source: &str,
    permissions: turso_fastdb_parser::SchemaPermissions,
) -> String {
    let permissions = match permissions {
        turso_fastdb_parser::SchemaPermissions::Full => "FULL",
        turso_fastdb_parser::SchemaPermissions::None => "NONE",
    };
    format!("DEFINE PARAM ${name} VALUE {value_source} PERMISSIONS {permissions}")
}

fn run_remove_param(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::RemoveParamStatement,
    script: &mut ScriptRuntime,
) -> Result<StatementResult> {
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(parameter) = snapshot.parameters.get(&statement.name.value).cloned() else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "parameter ${} is not defined",
                statement.name.value
            )));
        };
        catalog::remove_parameter(conn, &parameter)?;
        snapshot.parameters.remove(&statement.name.value);
        Ok(())
    })?;
    script.remove_catalog_parameter(&statement.name.value);
    Ok(StatementResult::None)
}

fn function_logical_name(name: &[turso_fastdb_parser::Identifier]) -> String {
    name.iter()
        .skip(1)
        .map(|segment| segment.value.as_str())
        .collect::<Vec<_>>()
        .join("::")
}

fn run_define_function(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineFunctionStatement,
    source: &str,
) -> Result<StatementResult> {
    validate_function_block(&statement.body)?;
    let logical_name = function_logical_name(&statement.name);
    let arguments = statement
        .arguments
        .iter()
        .map(|argument| catalog::FunctionArgumentDefinition {
            name: argument.name.value.clone(),
            ty: FieldType::from_parser(&argument.ty).canonical(),
        })
        .collect::<Vec<_>>();
    let body_source = source_slice(source, statement.body.span)?
        .trim()
        .to_string();
    let definition = catalog::canonical_function_definition(
        &logical_name,
        &arguments,
        &body_source,
        statement.permissions,
    );
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        validate_function_dependencies(snapshot, &logical_name, arguments.len(), &statement.body)?;
        validate_function_call_sites(snapshot, &logical_name, arguments.len())?;
        if let Some(existing) = snapshot.functions.get(&logical_name).cloned() {
            if statement.if_not_exists.is_some() {
                return Ok(());
            }
            if statement.overwrite.is_none() {
                return Err(FastDbError::Constraint(format!(
                    "function fn::{logical_name} is already defined"
                )));
            }
            catalog::remove_function(conn, &existing)?;
            snapshot.functions.remove(&logical_name);
        }
        let function = catalog::allocate_function(
            &logical_name,
            arguments,
            statement.body.clone(),
            body_source,
            statement.permissions,
            definition,
        )?;
        catalog::persist_function(conn, &function)?;
        snapshot.functions.insert(logical_name.clone(), function);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_alter_function(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::AlterFunctionStatement,
) -> Result<StatementResult> {
    let logical_name = function_logical_name(&statement.name);
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(existing) = snapshot.functions.get(&logical_name).cloned() else {
            return Err(FastDbError::Schema(format!(
                "function fn::{logical_name} is not defined"
            )));
        };
        let mut replacement = existing.clone();
        replacement.permissions = statement.permissions;
        replacement.definition = catalog::canonical_function_definition(
            &logical_name,
            &replacement.arguments,
            &replacement.body_source,
            statement.permissions,
        );
        catalog::remove_function(conn, &existing)?;
        catalog::persist_function(conn, &replacement)?;
        snapshot.functions.insert(logical_name.clone(), replacement);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_remove_function(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::RemoveFunctionStatement,
) -> Result<StatementResult> {
    let logical_name = function_logical_name(&statement.name);
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(function) = snapshot.functions.get(&logical_name).cloned() else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "function fn::{logical_name} is not defined"
            )));
        };
        if let Some(dependent) = snapshot.functions.iter().find_map(|(name, candidate)| {
            (name != &logical_name
                && function_dependencies(&candidate.body)
                    .iter()
                    .any(|(dependency, _)| dependency == &logical_name))
            .then_some(name)
        }) {
            return Err(FastDbError::Constraint(format!(
                "function fn::{logical_name} is required by fn::{dependent}"
            )));
        }
        if let Some((table, event)) = snapshot.tables.values().find_map(|table| {
            table.events.values().find_map(|event| {
                event_uses_function(event, &logical_name)
                    .then_some((table.logical_name.as_str(), event.logical_name.as_str()))
            })
        }) {
            return Err(FastDbError::Constraint(format!(
                "function fn::{logical_name} is required by event {event:?} on table {table:?}"
            )));
        }
        catalog::remove_function(conn, &function)?;
        snapshot.functions.remove(&logical_name);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_define_event(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineEventStatement,
    source: &str,
) -> Result<StatementResult> {
    validate_stored_script_block(&statement.action.block, "events")?;
    let condition_source = statement
        .condition
        .as_ref()
        .map(|condition| {
            source_slice(source, condition.span)
                .map(str::trim)
                .map(str::to_string)
        })
        .transpose()?
        .unwrap_or_else(|| "true".to_string());
    let action_source = source_slice(source, statement.action.span)?
        .trim()
        .to_string();
    let comment = statement
        .comment
        .as_ref()
        .map(|comment| comment.value.clone());
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        validate_event_dependencies(
            snapshot,
            &statement.name.value,
            statement.condition.as_ref(),
            &statement.action.block,
        )?;
        let table = snapshot
            .tables
            .get(&statement.table.value)
            .cloned()
            .ok_or_else(|| {
                FastDbError::Schema(format!("table {:?} is not defined", statement.table.value))
            })?;
        if let Some(existing) = table.events.get(&statement.name.value).cloned() {
            if statement.if_not_exists.is_some() {
                return Ok(());
            }
            if statement.overwrite.is_none() {
                return Err(FastDbError::Constraint(format!(
                    "event {:?} is already defined on table {:?}",
                    statement.name.value, statement.table.value
                )));
            }
            catalog::remove_event(conn, &existing)?;
        }
        let definition = catalog::canonical_event_definition(
            &statement.name.value,
            &statement.table.value,
            &condition_source,
            &action_source,
            comment.as_deref(),
        );
        let canonical = turso_fastdb_parser::parse_one(&definition).map_err(FastDbError::from)?;
        let Statement::DefineEvent(canonical) = canonical else {
            return Err(FastDbError::Engine(
                "canonical event definition has the wrong statement kind".into(),
            ));
        };
        let event = catalog::allocate_event(
            table.id,
            catalog::NewEventDefinition {
                logical_name: statement.name.value.clone(),
                condition: canonical.condition,
                condition_source: condition_source.clone(),
                action: canonical.action,
                action_source: action_source.clone(),
                comment: comment.clone(),
                definition,
            },
        )?;
        catalog::persist_event(conn, &event)?;
        snapshot
            .tables
            .get_mut(&statement.table.value)
            .expect("event table was validated")
            .events
            .insert(statement.name.value.clone(), event);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_alter_event(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::AlterEventStatement,
    source: &str,
) -> Result<StatementResult> {
    if let Some(Some(action)) = &statement.changes.action {
        validate_stored_script_block(&action.block, "events")?;
    }
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(existing) = snapshot
            .tables
            .get(&statement.table.value)
            .and_then(|table| table.events.get(&statement.name.value))
            .cloned()
        else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "event {:?} is not defined on table {:?}",
                statement.name.value, statement.table.value
            )));
        };
        let mut replacement = existing.clone();
        if let Some(condition) = &statement.changes.condition {
            match condition {
                Some(condition) => {
                    replacement.condition_source =
                        source_slice(source, condition.span)?.trim().to_string();
                    replacement.condition = Some(condition.clone());
                }
                None => {
                    replacement.condition = None;
                    replacement.condition_source = "true".to_string();
                }
            }
        }
        if let Some(Some(action)) = &statement.changes.action {
            replacement.action_source = source_slice(source, action.span)?.trim().to_string();
            replacement.action = action.clone();
        }
        // SurrealDB v3.1.5 accepts DROP THEN while retaining the mandatory
        // action. Preserve that characterized behavior instead of inventing
        // an action-less event state.
        if let Some(comment) = &statement.changes.comment {
            replacement.comment = comment.clone();
        }
        validate_event_dependencies(
            snapshot,
            &statement.name.value,
            replacement.condition.as_ref(),
            &replacement.action.block,
        )?;
        replacement.definition = catalog::canonical_event_definition(
            &replacement.logical_name,
            &statement.table.value,
            &replacement.condition_source,
            &replacement.action_source,
            replacement.comment.as_deref(),
        );
        let canonical =
            turso_fastdb_parser::parse_one(&replacement.definition).map_err(FastDbError::from)?;
        let Statement::DefineEvent(canonical) = canonical else {
            return Err(FastDbError::Engine(
                "canonical event definition has the wrong statement kind".into(),
            ));
        };
        replacement.condition = canonical.condition;
        replacement.action = canonical.action;
        catalog::remove_event(conn, &existing)?;
        catalog::persist_event(conn, &replacement)?;
        snapshot
            .tables
            .get_mut(&statement.table.value)
            .expect("event table was validated")
            .events
            .insert(statement.name.value.clone(), replacement);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_remove_event(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::RemoveEventStatement,
) -> Result<StatementResult> {
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(event) = snapshot
            .tables
            .get(&statement.table.value)
            .and_then(|table| table.events.get(&statement.name.value))
            .cloned()
        else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "event {:?} is not defined on table {:?}",
                statement.name.value, statement.table.value
            )));
        };
        catalog::remove_event(conn, &event)?;
        snapshot
            .tables
            .get_mut(&statement.table.value)
            .expect("event table was validated")
            .events
            .remove(&statement.name.value);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn validate_event_dependencies(
    snapshot: &CatalogSnapshot,
    event_name: &str,
    condition: Option<&Expr>,
    action: &turso_fastdb_parser::ScriptBlock,
) -> Result<()> {
    let mut dependencies = function_dependencies(action);
    if let Some(condition) = condition {
        collect_function_expression_dependencies(condition, &mut dependencies);
    }
    dependencies.sort();
    dependencies.dedup();
    for (dependency, arity) in dependencies {
        let expected = snapshot
            .functions
            .get(&dependency)
            .map(|function| function.arguments.len())
            .ok_or_else(|| {
                FastDbError::Schema(format!(
                    "event {event_name:?} references undefined function fn::{dependency}"
                ))
            })?;
        if arity != expected {
            return Err(FastDbError::Schema(format!(
                "event {event_name:?} calls fn::{dependency} with {arity} arguments; {expected} required"
            )));
        }
    }
    Ok(())
}

fn validate_function_block(block: &turso_fastdb_parser::ScriptBlock) -> Result<()> {
    validate_stored_script_block(block, "custom functions")
}

fn validate_stored_script_block(
    block: &turso_fastdb_parser::ScriptBlock,
    owner: &'static str,
) -> Result<()> {
    for statement in &block.statements {
        match statement {
            Statement::Begin(_) | Statement::Commit(_) | Statement::Cancel(_) => {
                return Err(FastDbError::Schema(format!(
                    "{owner} cannot control transactions"
                )));
            }
            Statement::If(statement) => {
                for (_, block) in &statement.branches {
                    validate_stored_script_block(block, owner)?;
                }
                if let Some(block) = &statement.otherwise {
                    validate_stored_script_block(block, owner)?;
                }
            }
            Statement::For(statement) => validate_stored_script_block(&statement.body, owner)?,
            _ => {}
        }
    }
    Ok(())
}

fn validate_function_dependencies(
    snapshot: &CatalogSnapshot,
    logical_name: &str,
    argument_count: usize,
    body: &turso_fastdb_parser::ScriptBlock,
) -> Result<()> {
    for (dependency, arity) in function_dependencies(body) {
        let expected = if dependency == logical_name {
            argument_count
        } else {
            snapshot
                .functions
                .get(&dependency)
                .map(|function| function.arguments.len())
                .ok_or_else(|| {
                    FastDbError::Schema(format!(
                        "function fn::{logical_name} references undefined function fn::{dependency}"
                    ))
                })?
        };
        if arity != expected {
            return Err(FastDbError::Schema(format!(
                "function fn::{logical_name} calls fn::{dependency} with {arity} arguments; {expected} required"
            )));
        }
    }
    Ok(())
}

fn validate_function_call_sites(
    snapshot: &CatalogSnapshot,
    logical_name: &str,
    argument_count: usize,
) -> Result<()> {
    for (name, function) in &snapshot.functions {
        if name == logical_name {
            continue;
        }
        for (dependency, arity) in function_dependencies(&function.body) {
            if dependency == logical_name && arity != argument_count {
                return Err(FastDbError::Constraint(format!(
                    "changing fn::{logical_name} would invalidate fn::{name}"
                )));
            }
        }
    }
    for table in snapshot.tables.values() {
        for event in table.events.values() {
            let mut dependencies = function_dependencies(&event.action.block);
            if let Some(condition) = &event.condition {
                collect_function_expression_dependencies(condition, &mut dependencies);
            }
            for (_, arity) in dependencies
                .into_iter()
                .filter(|(dependency, _)| dependency == logical_name)
            {
                if arity != argument_count {
                    return Err(FastDbError::Constraint(format!(
                        "changing fn::{logical_name} would invalidate event {:?} on table {:?}",
                        event.logical_name, table.logical_name
                    )));
                }
            }
        }
    }
    Ok(())
}

fn event_uses_function(event: &catalog::EventDefinition, logical_name: &str) -> bool {
    let mut dependencies = function_dependencies(&event.action.block);
    if let Some(condition) = &event.condition {
        collect_function_expression_dependencies(condition, &mut dependencies);
    }
    dependencies
        .into_iter()
        .any(|(dependency, _)| dependency == logical_name)
}

fn function_dependencies(block: &turso_fastdb_parser::ScriptBlock) -> Vec<(String, usize)> {
    let mut dependencies = Vec::new();
    collect_function_block_dependencies(block, &mut dependencies);
    dependencies.sort();
    dependencies.dedup();
    dependencies
}

fn collect_function_block_dependencies(
    block: &turso_fastdb_parser::ScriptBlock,
    dependencies: &mut Vec<(String, usize)>,
) {
    for statement in &block.statements {
        match statement {
            Statement::Let(statement) => {
                collect_function_expression_dependencies(&statement.value, dependencies)
            }
            Statement::ScriptReturn(statement)
            | Statement::Throw(statement)
            | Statement::Sleep(statement) => {
                collect_function_expression_dependencies(&statement.value, dependencies)
            }
            Statement::If(statement) => {
                for (condition, block) in &statement.branches {
                    collect_function_expression_dependencies(condition, dependencies);
                    collect_function_block_dependencies(block, dependencies);
                }
                if let Some(block) = &statement.otherwise {
                    collect_function_block_dependencies(block, dependencies);
                }
            }
            Statement::For(statement) => {
                collect_function_expression_dependencies(&statement.iterable, dependencies);
                collect_function_block_dependencies(&statement.body, dependencies);
            }
            Statement::Create(statement) => {
                if let Target::Expression(expression) = &statement.target {
                    collect_function_expression_dependencies(expression, dependencies);
                }
                if let Some(data) = &statement.data {
                    collect_create_data_function_dependencies(data, dependencies);
                }
            }
            Statement::Insert(statement) => {
                match &statement.data {
                    InsertData::Expression(expression) => {
                        collect_function_expression_dependencies(expression, dependencies)
                    }
                    InsertData::Values { rows, .. } => {
                        for expression in rows.iter().flatten() {
                            collect_function_expression_dependencies(expression, dependencies);
                        }
                    }
                }
                for assignment in &statement.on_duplicate {
                    collect_function_expression_dependencies(&assignment.value, dependencies);
                }
            }
            Statement::Relate(statement) => {
                collect_function_expression_dependencies(&statement.from, dependencies);
                collect_function_expression_dependencies(&statement.to, dependencies);
                if let Some(data) = &statement.data {
                    collect_create_data_function_dependencies(data, dependencies);
                }
            }
            Statement::Select(statement) => {
                if let ProjectionList::Fields(projections) = &statement.projections {
                    for projection in projections {
                        collect_function_expression_dependencies(
                            &projection.expression,
                            dependencies,
                        );
                    }
                }
                if let Some(condition) = &statement.condition {
                    collect_function_expression_dependencies(condition, dependencies);
                }
            }
            Statement::Update(statement) | Statement::Upsert(statement) => {
                match &statement.data {
                    UpdateData::Content(expression)
                    | UpdateData::Merge(expression)
                    | UpdateData::Patch(expression)
                    | UpdateData::Replace(expression) => {
                        collect_function_expression_dependencies(expression, dependencies)
                    }
                    UpdateData::Set(assignments) => {
                        for assignment in assignments {
                            collect_function_expression_dependencies(
                                &assignment.value,
                                dependencies,
                            );
                        }
                    }
                    UpdateData::Unset(_) => {}
                }
                if let Some(condition) = &statement.condition {
                    collect_function_expression_dependencies(condition, dependencies);
                }
            }
            Statement::Delete(statement) => {
                if let Some(condition) = &statement.condition {
                    collect_function_expression_dependencies(condition, dependencies);
                }
            }
            Statement::DefineParam(statement) => {
                collect_function_expression_dependencies(&statement.value, dependencies)
            }
            Statement::AlterParam(statement) => {
                if let Some(value) = &statement.value {
                    collect_function_expression_dependencies(value, dependencies);
                }
            }
            Statement::DefineFunction(statement) => {
                collect_function_block_dependencies(&statement.body, dependencies)
            }
            _ => {}
        }
    }
}

fn collect_create_data_function_dependencies(
    data: &CreateData,
    dependencies: &mut Vec<(String, usize)>,
) {
    match data {
        CreateData::Content(expression) => {
            collect_function_expression_dependencies(expression, dependencies)
        }
        CreateData::Set(assignments) => {
            for assignment in assignments {
                collect_function_expression_dependencies(&assignment.value, dependencies);
            }
        }
    }
}

fn collect_function_expression_dependencies(
    expression: &Expr,
    dependencies: &mut Vec<(String, usize)>,
) {
    match &expression.kind {
        ExprKind::FunctionCall { name, arguments } => {
            if let Some(name) = custom_function_name(name) {
                dependencies.push((name, arguments.len()));
            }
            for argument in arguments {
                collect_function_expression_dependencies(argument, dependencies);
            }
        }
        ExprKind::Array(values) | ExprKind::DestructureList(values) => {
            for value in values {
                collect_function_expression_dependencies(value, dependencies);
            }
        }
        ExprKind::Object(fields) => {
            for field in fields {
                collect_function_expression_dependencies(&field.value, dependencies);
            }
        }
        ExprKind::Destructure { target, .. }
        | ExprKind::Cast { value: target, .. }
        | ExprKind::Unary {
            operand: target, ..
        }
        | ExprKind::Parenthesized(target) => {
            collect_function_expression_dependencies(target, dependencies)
        }
        ExprKind::Access { target, accessor } => {
            collect_function_expression_dependencies(target, dependencies);
            match accessor {
                turso_fastdb_parser::Accessor::Index(index) => {
                    collect_function_expression_dependencies(index, dependencies)
                }
                turso_fastdb_parser::Accessor::Slice { start, end, .. } => {
                    if let Some(start) = start {
                        collect_function_expression_dependencies(start, dependencies);
                    }
                    if let Some(end) = end {
                        collect_function_expression_dependencies(end, dependencies);
                    }
                }
                turso_fastdb_parser::Accessor::Field(_)
                | turso_fastdb_parser::Accessor::Last(_) => {}
            }
        }
        ExprKind::Range(range) => {
            if let Some(start) = &range.start {
                collect_function_expression_dependencies(start, dependencies);
            }
            if let Some(end) = &range.end {
                collect_function_expression_dependencies(end, dependencies);
            }
        }
        ExprKind::Closure(closure) => {
            collect_function_expression_dependencies(&closure.body, dependencies)
        }
        ExprKind::Knn(knn) => {
            collect_function_expression_dependencies(&knn.field, dependencies);
            collect_function_expression_dependencies(&knn.query, dependencies);
        }
        ExprKind::Binary { left, right, .. } => {
            collect_function_expression_dependencies(left, dependencies);
            collect_function_expression_dependencies(right, dependencies);
        }
        ExprKind::None
        | ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Parameter(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_)
        | ExprKind::NamespacedValue { .. }
        | ExprKind::Traversal(_) => {}
    }
}

pub(crate) fn validate_schema_expression_safety(expression: &Expr) -> Result<()> {
    match &expression.kind {
        ExprKind::FunctionCall { name, arguments } => {
            let segments = name
                .iter()
                .map(|segment| segment.value.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if matches!(segments.first().map(String::as_str), Some("fn" | "rand"))
                || matches!(
                    segments.as_slice(),
                    [family, function, ..]
                        if (family == "time" && function == "now")
                            || (family == "uuid" && matches!(function.as_str(), "v4" | "v7"))
                )
            {
                return Err(FastDbError::Schema(
                    "schema expressions must be deterministic and cannot invoke custom functions"
                        .into(),
                ));
            }
            for argument in arguments {
                validate_schema_expression_safety(argument)?;
            }
        }
        ExprKind::Array(values) | ExprKind::DestructureList(values) => {
            for value in values {
                validate_schema_expression_safety(value)?;
            }
        }
        ExprKind::Object(fields) => {
            for field in fields {
                validate_schema_expression_safety(&field.value)?;
            }
        }
        ExprKind::Destructure { target, .. }
        | ExprKind::Cast { value: target, .. }
        | ExprKind::Unary {
            operand: target, ..
        }
        | ExprKind::Parenthesized(target) => validate_schema_expression_safety(target)?,
        ExprKind::Access { target, accessor } => {
            validate_schema_expression_safety(target)?;
            match accessor {
                turso_fastdb_parser::Accessor::Index(index) => {
                    validate_schema_expression_safety(index)?
                }
                turso_fastdb_parser::Accessor::Slice { start, end, .. } => {
                    if let Some(start) = start {
                        validate_schema_expression_safety(start)?;
                    }
                    if let Some(end) = end {
                        validate_schema_expression_safety(end)?;
                    }
                }
                turso_fastdb_parser::Accessor::Field(_)
                | turso_fastdb_parser::Accessor::Last(_) => {}
            }
        }
        ExprKind::Range(range) => {
            if let Some(start) = &range.start {
                validate_schema_expression_safety(start)?;
            }
            if let Some(end) = &range.end {
                validate_schema_expression_safety(end)?;
            }
        }
        ExprKind::Closure(closure) => validate_schema_expression_safety(&closure.body)?,
        ExprKind::Knn(_) | ExprKind::Traversal(_) => {
            return Err(FastDbError::Schema(
                "schema expressions cannot execute search or graph traversal".into(),
            ));
        }
        ExprKind::Binary { left, right, .. } => {
            validate_schema_expression_safety(left)?;
            validate_schema_expression_safety(right)?;
        }
        ExprKind::None
        | ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Parameter(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_)
        | ExprKind::NamespacedValue { .. } => {}
    }
    Ok(())
}

fn run_info_database(conn: &Connection, execution: &ExecutionState) -> Result<StatementResult> {
    let catalog = catalog_for_read(conn, execution)?;
    let snapshot = catalog.snapshot();
    let mut root = BTreeMap::new();
    for key in [
        "accesses",
        "analyzers",
        "apis",
        "buckets",
        "configs",
        "functions",
        "models",
        "modules",
        "params",
        "sequences",
        "tables",
        "users",
    ] {
        root.insert(key.to_string(), Value::Object(BTreeMap::new()));
    }
    if let Some(snapshot) = snapshot {
        let params = snapshot
            .parameters
            .iter()
            .map(|(name, parameter)| (name.clone(), Value::Str(parameter.definition.clone())))
            .collect();
        root.insert("params".into(), Value::Object(params));
        let functions = snapshot
            .functions
            .iter()
            .map(|(name, function)| (name.clone(), Value::Str(function.definition.clone())))
            .collect();
        root.insert("functions".into(), Value::Object(functions));
        let tables = snapshot
            .tables
            .iter()
            .map(|(name, table)| {
                (
                    name.clone(),
                    Value::Str(
                        table
                            .definition
                            .clone()
                            .unwrap_or_else(|| format!("DEFINE TABLE {name} SCHEMALESS")),
                    ),
                )
            })
            .collect();
        root.insert("tables".into(), Value::Object(tables));
        let analyzers = snapshot
            .analyzers
            .iter()
            .map(|(name, analyzer)| (name.clone(), Value::Str(analyzer.definition.clone())))
            .collect();
        root.insert("analyzers".into(), Value::Object(analyzers));
    }
    Ok(StatementResult::Value(Value::Object(root)))
}

fn render_schema_identifier(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && value
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        value.to_string()
    } else {
        format!("`{}`", value.replace('`', "``"))
    }
}

fn render_schema_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn canonical_table_definition(
    snapshot: &CatalogSnapshot,
    table: &TableDefinition,
) -> Result<String> {
    if let Some(view) = snapshot.views.get(&table.logical_name) {
        return Ok(canonical_view_definition(
            &table.logical_name,
            &view.select_source,
            table.permissions,
            table.comment.as_deref(),
        ));
    }
    let mut definition = format!(
        "DEFINE TABLE {}{} TYPE ",
        render_schema_identifier(&table.logical_name),
        if table.drop { " DROP" } else { "" },
    );
    match table.kind {
        TableKind::Normal => definition.push_str("NORMAL"),
        TableKind::Relation => {
            definition.push_str("RELATION");
            for (prefix, endpoint) in [
                (" IN ", table.relation_in_table_id),
                (" OUT ", table.relation_out_table_id),
            ] {
                if let Some(endpoint) = endpoint {
                    let endpoint = snapshot
                        .tables
                        .values()
                        .find(|candidate| candidate.id == endpoint)
                        .ok_or_else(|| FastDbError::format("relation endpoint table is missing"))?;
                    definition.push_str(prefix);
                    definition.push_str(&render_schema_identifier(&endpoint.logical_name));
                }
            }
            if table.relation_enforced {
                definition.push_str(" ENFORCED");
            }
        }
    }
    definition.push(' ');
    definition.push_str(match table.mode {
        TableMode::Schemaless => "SCHEMALESS",
        TableMode::Schemafull => "SCHEMAFULL",
    });
    definition.push_str(match table.permissions {
        turso_fastdb_parser::SchemaPermissions::Full => " PERMISSIONS FULL",
        turso_fastdb_parser::SchemaPermissions::None => " PERMISSIONS NONE",
    });
    if let Some(comment) = &table.comment {
        definition.push_str(" COMMENT ");
        definition.push_str(&render_schema_string(comment));
    }
    Ok(definition)
}

fn run_info_table(
    conn: &Connection,
    execution: &ExecutionState,
    statement: turso_fastdb_parser::InfoTableStatement,
) -> Result<StatementResult> {
    let catalog = catalog_for_read(conn, execution)?;
    let snapshot = catalog
        .snapshot()
        .ok_or_else(|| FastDbError::Schema("database catalog is empty".into()))?;
    let table = snapshot.tables.get(&statement.table.value).ok_or_else(|| {
        FastDbError::Schema(format!("table {:?} is not defined", statement.table.value))
    })?;
    let mut root = BTreeMap::from([
        ("events".into(), Value::Object(BTreeMap::new())),
        ("fields".into(), Value::Object(BTreeMap::new())),
        ("indexes".into(), Value::Object(BTreeMap::new())),
        ("lives".into(), Value::Object(BTreeMap::new())),
        ("tables".into(), Value::Object(BTreeMap::new())),
    ]);
    root.insert(
        "events".into(),
        Value::Object(
            table
                .events
                .iter()
                .map(|(name, event)| (name.clone(), Value::Str(event.definition.clone())))
                .collect(),
        ),
    );
    root.insert(
        "fields".into(),
        Value::Object(
            table
                .fields
                .values()
                .map(|field| {
                    (
                        render_schema_path(&field.path),
                        Value::Str(field.definition.clone()),
                    )
                })
                .collect(),
        ),
    );
    root.insert(
        "indexes".into(),
        Value::Object(
            table
                .indexes
                .iter()
                .filter(|(name, _)| !name.starts_with("__graph_"))
                .map(|(name, index)| (name.clone(), Value::Str(index.definition.clone())))
                .collect(),
        ),
    );
    Ok(StatementResult::Value(Value::Object(root)))
}

fn run_alter_table(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::AlterTableStatement,
) -> Result<StatementResult> {
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(existing) = snapshot.tables.get(&statement.name.value).cloned() else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "table {:?} is not defined",
                statement.name.value
            )));
        };
        let existing_view = snapshot.views.get(&statement.name.value).cloned();
        if existing_view.is_some() && statement.mode.is_some() {
            return Err(FastDbError::Schema(
                "materialized views remain SCHEMALESS; alter fields instead of table mode".into(),
            ));
        }
        let mut replacement = existing;
        if let Some(mode) = statement.mode {
            if mode.value == TableMode::Schemafull {
                for (_, mut document) in read_documents(conn, &replacement)? {
                    schema::validate_document(true, &replacement.fields, &mut document)?;
                }
            }
            replacement.mode = mode.value;
        }
        if let Some(permissions) = statement.permissions {
            replacement.permissions = permissions;
        }
        match statement.comment {
            turso_fastdb_parser::TableCommentChange::Unchanged => {}
            turso_fastdb_parser::TableCommentChange::Set(comment) => {
                replacement.comment = Some(comment)
            }
            turso_fastdb_parser::TableCommentChange::Drop => replacement.comment = None,
        }
        replacement.definition = Some(canonical_table_definition(snapshot, &replacement)?);
        catalog::replace_table(conn, &replacement)?;
        if let Some(mut view) = existing_view {
            catalog::remove_view(conn, &view)?;
            view.definition = replacement
                .definition
                .clone()
                .expect("altered view has a canonical definition");
            catalog::persist_view(conn, &view)?;
            conn.check_failpoint(Failpoint::AfterViewCatalog)?;
            snapshot.views.insert(view.logical_name.clone(), view);
        }
        snapshot
            .tables
            .insert(statement.name.value.clone(), replacement);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_remove_table(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::RemoveTableStatement,
) -> Result<StatementResult> {
    let cascaded = with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(table) = snapshot.tables.get(&statement.name.value).cloned() else {
            if statement.if_exists.is_some() {
                return Ok(false);
            }
            return Err(FastDbError::Schema(format!(
                "table {:?} is not defined",
                statement.name.value
            )));
        };
        if let Some(relation) = snapshot.tables.values().find(|candidate| {
            candidate.id != table.id
                && [
                    candidate.relation_in_table_id,
                    candidate.relation_out_table_id,
                ]
                .contains(&Some(table.id))
        }) {
            return Err(FastDbError::Constraint(format!(
                "table {:?} is required by relation table {:?}",
                table.logical_name, relation.logical_name
            )));
        }
        if let Some(view) = snapshot
            .views
            .values()
            .find(|view| view.id != table.id && view.dependencies.contains(&table.id))
        {
            return Err(FastDbError::Constraint(format!(
                "table {:?} is required by materialized view {:?}",
                table.logical_name, view.logical_name
            )));
        }
        let mut cascaded = false;
        if table.kind == TableKind::Normal {
            for relation in snapshot
                .tables
                .values()
                .filter(|candidate| candidate.kind == TableKind::Relation)
            {
                let hidden = catalog::graph_columns(snapshot, relation)?
                    .into_iter()
                    .map(|column| column.physical_name.clone())
                    .collect::<Vec<_>>();
                for forward in [true, false] {
                    let (delete, bindings) = lower::physical_graph_delete_edges_for_table_stmt(
                        &relation.physical_name,
                        &hidden,
                        &table.id.to_hex(),
                        forward,
                    )?;
                    conn.exec_bound(delete, bindings)?;
                    conn.check_failpoint(Failpoint::AfterDeleteMutation)?;
                }
                cascaded = true;
            }
        }
        for index in table.indexes.values() {
            conn.exec_bound(
                crate::provider::index_provider(index)?.drop_statement(index)?,
                vec![],
            )?;
        }
        conn.exec_bound(
            lower::physical_drop_table_ddl(&table.physical_name)?,
            vec![],
        )?;
        if let Some(view) = snapshot.views.remove(&statement.name.value) {
            catalog::remove_view(conn, &view)?;
        }
        catalog::remove_table_catalog(conn, &table)?;
        snapshot.tables.remove(&statement.name.value);
        snapshot
            .hidden_columns
            .retain(|_, column| column.table_id != table.id);
        for (provider, required) in [
            (
                BUILTIN_GRAPH_PROVIDER,
                snapshot
                    .tables
                    .values()
                    .any(|table| table.kind == TableKind::Relation),
            ),
            (
                BUILTIN_FTS_PROVIDER,
                snapshot.tables.values().any(|table| {
                    table
                        .indexes
                        .values()
                        .any(|index| index.kind == IndexKind::Fts)
                }),
            ),
            (
                BUILTIN_VECTOR_PROVIDER,
                snapshot.tables.values().any(|table| {
                    table
                        .fields
                        .values()
                        .any(|field| field.ty.vector_dimension().is_some())
                }),
            ),
        ] {
            if !required && snapshot.capabilities.remove(provider).is_some() {
                catalog::remove_capability(conn, provider)?;
            }
        }
        Ok(cascaded)
    })?;
    if cascaded {
        mark_relation_fts_dirty(execution);
    }
    Ok(StatementResult::None)
}

fn run_define_analyzer(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineAnalyzerStatement,
    source: &str,
) -> Result<StatementResult> {
    ensure_fts_available()?;
    let definition = source_slice(source, statement.span)?.to_string();
    let analyzer = catalog::allocate_analyzer(&statement.name.value, definition)?;
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        if snapshot.analyzers.contains_key(&statement.name.value) {
            return Err(FastDbError::Constraint(format!(
                "analyzer {:?} is already defined",
                statement.name.value
            )));
        }
        if !snapshot
            .capabilities
            .contains_key(catalog::BUILTIN_FTS_PROVIDER)
        {
            catalog::persist_fts_capability(conn)?;
            snapshot.capabilities.insert(
                catalog::BUILTIN_FTS_PROVIDER.to_string(),
                CapabilityRequirement {
                    provider: catalog::BUILTIN_FTS_PROVIDER.to_string(),
                    min_provider_version: catalog::BUILTIN_FTS_PROVIDER_VERSION,
                    min_encoding_version: catalog::BUILTIN_FTS_ENCODING_VERSION,
                },
            );
        }
        catalog::persist_analyzer(conn, &analyzer)?;
        conn.check_failpoint(Failpoint::AfterFtsAnalyzerCatalog)?;
        snapshot
            .analyzers
            .insert(statement.name.value.clone(), analyzer.clone());
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_create(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::CreateStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<StatementExecution> {
    let _timeout = StatementTimeoutGuard::install(conn, statement.timeout.as_ref(), params)?;
    let targets = resolve_create_ids(statement.target, params)?;
    if statement.only.is_some() && targets.len() != 1 {
        return Err(FastDbError::Schema(
            "CREATE ONLY requires exactly one target record".into(),
        ));
    }
    let functions = catalog_for_read(conn, execution)?
        .snapshot()
        .map(|snapshot| snapshot.functions.clone())
        .unwrap_or_default();
    let mut prepared = BTreeMap::new();
    for (table_name, id_value) in &targets {
        let id = RecordId::new(table_name, id_value.clone());
        let document = evaluate_create_document_with_custom_functions(
            conn,
            execution,
            statement.data.as_ref(),
            &id,
            params,
            &functions,
            script,
        )?;
        prepared.insert((table_name.clone(), encode_rid(id_value)?), document);
    }
    let mut values = Vec::with_capacity(targets.len());
    for (table_name, id_value) in &targets {
        let table_was_missing = !catalog_for_read(conn, execution)?
            .snapshot()
            .is_some_and(|snapshot| snapshot.tables.contains_key(table_name));
        let value = with_create_mutation(conn, execution, table_was_missing, |state| {
            let snapshot = ensure_snapshot(conn, state)?;
            if !snapshot.tables.contains_key(table_name) {
                let table = catalog::allocate_table(table_name, TableMode::Schemaless, None)?;
                catalog::persist_table(conn, &table)?;
                conn.check_failpoint(Failpoint::AfterCatalogRow)?;
                conn.exec_bound(lower::physical_table_ddl(&table.physical_name)?, vec![])?;
                conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
                snapshot.tables.insert(table_name.clone(), table);
            }
            let table = snapshot
                .tables
                .get(table_name)
                .cloned()
                .expect("table inserted or already present");
            reject_direct_view_write(snapshot, table_name)?;
            if table.kind == TableKind::Relation {
                return Err(FastDbError::Schema(
                    "relation records must be created with RELATE".into(),
                ));
            }
            if table.drop {
                return Err(FastDbError::Constraint(format!(
                    "table {table_name:?} is DROP and rejects CREATE"
                )));
            }
            let id = RecordId::new(table_name, id_value.clone());
            let mut document = prepared
                .remove(&(table_name.clone(), encode_rid(id_value)?))
                .ok_or_else(|| FastDbError::Engine("prepared CREATE document is missing".into()))?;
            let input = Value::Object(document.clone());
            reject_stored_id(&document)?;
            normalize_schema_document(
                &table,
                &mut document,
                None,
                &id,
                None,
                params,
                &functions,
                true,
            )?;
            validate_index_values(&table, &document)?;
            let derived_hidden = derived_hidden_values(snapshot, &table, &document)?;
            let encoded_rid = encode_rid(id_value)?;
            let (insert, bindings) = lower::physical_insert_document_with_hidden_stmt(
                &table.physical_name,
                &encoded_rid,
                &decode::encode_doc(&document)?,
                &derived_hidden,
            )?;
            let mut prepared = conn.prepare_bound(insert, bindings)?;
            conn.check_failpoint(Failpoint::AfterRecordPrepare)?;
            contextual_constraint(
                prepared.run_ignore_rows().map_err(FastDbError::from),
                "record ID already exists or violates a declared unique index",
            )?;
            conn.check_failpoint(Failpoint::AfterRecordInsert)?;
            Ok((id.clone(), full_record_value(&id, &document), input))
        })?;
        mark_fts_dirty(execution, table_name);
        run_table_events(
            conn,
            execution,
            EventInvocation {
                table_name,
                kind: "CREATE",
                id: &value.0,
                before: Value::Null,
                after: value.1.clone(),
                input: value.2.clone(),
            },
            script,
        )?;
        values.push(value);
    }
    let mutation_count = values.len();
    let returned = values
        .into_iter()
        .filter_map(|(id, value, _)| {
            mutation_return(
                statement.return_clause.as_ref(),
                &Value::Null,
                &value,
                &id,
                None,
                params,
            )
            .transpose()
        })
        .collect::<Result<Vec<_>>>()?;
    let result = if statement.only.is_some() {
        StatementResult::Value(returned.into_iter().next().unwrap_or(Value::Null))
    } else {
        StatementResult::Rows(returned)
    };
    StatementExecution::mutation(result, mutation_count)
}

fn evaluate_create_document_with_custom_functions(
    conn: &Connection,
    execution: &mut ExecutionState,
    data: Option<&CreateData>,
    id: &RecordId,
    params: &Params,
    functions: &BTreeMap<String, catalog::FunctionDefinition>,
    script: &mut ScriptRuntime,
) -> Result<BTreeMap<String, Value>> {
    let empty = BTreeMap::new();
    match data {
        None => Ok(BTreeMap::new()),
        Some(CreateData::Content(expression)) => {
            let value = evaluate_expression_with_custom_functions(
                conn, execution, &empty, id, None, expression, params, functions, script,
            )?;
            let Value::Object(document) = value else {
                return Err(FastDbError::Schema(
                    "CREATE CONTENT must evaluate to an object".into(),
                ));
            };
            Ok(document)
        }
        Some(CreateData::Set(assignments)) => {
            let mut evaluated = Vec::with_capacity(assignments.len());
            let context = EvalContext {
                document: &empty,
                id,
                endpoints: None,
                params,
                functions: Some(functions),
                function_calls: None,
                function_depth: 0,
            };
            for assignment in assignments {
                let value = if expression_invokes_custom_function(&assignment.value) {
                    EvalValue::Present(evaluate_expression_with_custom_functions(
                        conn,
                        execution,
                        &empty,
                        id,
                        None,
                        &assignment.value,
                        params,
                        functions,
                        script,
                    )?)
                } else {
                    eval::evaluate(&assignment.value, &context)?
                };
                evaluated.push((
                    assignment_path(&assignment.path)?,
                    assignment.operator.value,
                    value,
                ));
            }
            let mut document = BTreeMap::new();
            apply_assignments(&mut document, evaluated)?;
            Ok(document)
        }
    }
}

fn resolve_create_ids(target: Target, params: &Params) -> Result<Vec<(String, RecordIdValue)>> {
    match target {
        Target::Table(table) => Ok(vec![(
            table.name.value,
            RecordIdValue::Uuid(uuid::Uuid::now_v7()),
        )]),
        Target::Record(record) => Ok(vec![(record.table.value, record_id_value(record.id)?)]),
        Target::RecordRange(range) => resolve_integer_create_range(range),
        Target::Expression(expression) => resolve_target_expression(&expression, params)?
            .into_iter()
            .map(|(table, selector)| match selector {
                TargetSelector::All => Ok((table, RecordIdValue::Uuid(uuid::Uuid::now_v7()))),
                TargetSelector::Record(id) => Ok((table, id)),
                TargetSelector::Range(_) => Err(FastDbError::Schema(
                    "CREATE expression targets cannot contain record ranges".into(),
                )),
            })
            .collect(),
        Target::Batch { target, .. } => match *target {
            Target::Record(record) => {
                let RecordIdPartKind::Integer(count) = record.id.kind else {
                    return Err(FastDbError::Schema(
                        "batch CREATE count must be an integer".into(),
                    ));
                };
                let count = usize::try_from(count).map_err(|_| {
                    FastDbError::Schema("batch CREATE count must be nonnegative".into())
                })?;
                if count == 0 || count > 10_000 {
                    return Err(FastDbError::ResourceLimit(
                        "batch CREATE count must be between 1 and 10,000".into(),
                    ));
                }
                Ok((0..count)
                    .map(|_| {
                        (
                            record.table.value.clone(),
                            RecordIdValue::Uuid(uuid::Uuid::now_v7()),
                        )
                    })
                    .collect())
            }
            Target::RecordRange(range) => resolve_integer_create_range(range),
            _ => Err(FastDbError::Schema(
                "batch CREATE requires a count or integer record range".into(),
            )),
        },
    }
}

fn resolve_integer_create_range(
    range: turso_fastdb_parser::RecordRangeTarget,
) -> Result<Vec<(String, RecordIdValue)>> {
    let (Some(start), Some(end)) = (range.start, range.end) else {
        return Err(FastDbError::Schema(
            "CREATE record ranges require both integer bounds".into(),
        ));
    };
    let RecordIdPartKind::Integer(start) = start.kind else {
        return Err(FastDbError::Schema(
            "CREATE record ranges require integer bounds".into(),
        ));
    };
    let RecordIdPartKind::Integer(end) = end.kind else {
        return Err(FastDbError::Schema(
            "CREATE record ranges require integer bounds".into(),
        ));
    };
    let exclusive_end = if range.inclusive {
        end.checked_add(1)
            .ok_or_else(|| FastDbError::Schema("CREATE range end overflows i64".into()))?
    } else {
        end
    };
    if start > exclusive_end {
        return Err(FastDbError::Schema(
            "CREATE record range start exceeds end".into(),
        ));
    }
    let count = exclusive_end
        .checked_sub(start)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or_else(|| FastDbError::ResourceLimit("CREATE range is too large".into()))?;
    if count > 10_000 {
        return Err(FastDbError::ResourceLimit(
            "CREATE range exceeds 10,000 records".into(),
        ));
    }
    Ok((start..exclusive_end)
        .map(|id| (range.table.value.clone(), RecordIdValue::Integer(id)))
        .collect())
}

fn run_insert(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::InsertStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<StatementExecution> {
    let _timeout = StatementTimeoutGuard::install(conn, statement.timeout.as_ref(), params)?;
    if statement.relation.is_some() {
        return run_insert_relation(conn, execution, statement, params, script);
    }
    let table_name = statement.table.value.clone();
    let input_documents = evaluate_insert_documents(&statement.data, &table_name, params)?;
    let mut outcomes = Vec::new();
    for input in &input_documents {
        let table_was_missing = !catalog_for_read(conn, execution)?
            .snapshot()
            .is_some_and(|snapshot| snapshot.tables.contains_key(&table_name));
        let outcome = with_create_mutation(conn, execution, table_was_missing, |state| {
            let snapshot = ensure_snapshot(conn, state)?;
            let functions = snapshot.functions.clone();
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
                .cloned()
                .expect("insert table exists after registration");
            reject_direct_view_write(snapshot, &table_name)?;
            if table.kind == TableKind::Relation {
                return Err(FastDbError::Schema(
                    "relation tables require INSERT RELATION or RELATE".into(),
                ));
            }
            if table.drop {
                return Err(FastDbError::Constraint(format!(
                    "table {table_name:?} is DROP and rejects INSERT"
                )));
            }
            let event_input = Value::Object(input.clone());
            let (id, mut document) = normalize_insert_document(&table_name, input.clone())?;
            let existing = read_candidates(
                conn,
                snapshot,
                &table,
                CandidateReadOptions {
                    id: Some(&id.id),
                    range: None,
                    condition: None,
                    params,
                    allow_cache: false,
                    fts: None,
                    vector: None,
                },
            )?
            .into_iter()
            .next();
            if let Some(candidate) = existing {
                if statement.ignore.is_some() {
                    return Ok(None);
                }
                if statement.on_duplicate.is_empty() {
                    return Err(FastDbError::Constraint(
                        "INSERT record ID already exists".into(),
                    ));
                }
                let before = full_candidate_value(&candidate);
                let mut scoped_params = params.clone();
                let mut input_value = document.clone();
                input_value.insert("id".into(), Value::RecordId(id.clone()));
                scoped_params.insert("input".into(), Value::Object(input_value));
                let context = EvalContext {
                    document: &candidate.document,
                    id: &candidate.id,
                    endpoints: None,
                    params: &scoped_params,
                    functions: None,
                    function_calls: None,
                    function_depth: 0,
                };
                let assignments = evaluate_assignments(&statement.on_duplicate, &context)?;
                document = candidate.document.clone();
                apply_assignments(&mut document, assignments)?;
                reject_stored_id(&document)?;
                normalize_schema_document(
                    &table,
                    &mut document,
                    Some(&candidate.document),
                    &candidate.id,
                    None,
                    &scoped_params,
                    &functions,
                    false,
                )?;
                validate_index_values(&table, &document)?;
                let derived_hidden = derived_hidden_values(snapshot, &table, &document)?;
                let (update, bindings) = lower::physical_update_document_with_hidden_stmt(
                    &table.physical_name,
                    &candidate.encoded_rid,
                    &decode::encode_doc(&document)?,
                    &derived_hidden,
                )?;
                contextual_constraint(
                    conn.exec_bound(update, bindings),
                    "INSERT ON DUPLICATE KEY violates a declared unique index",
                )?;
                return Ok(Some((
                    "UPDATE",
                    candidate.id.clone(),
                    before,
                    full_candidate_with_document(&candidate, &document),
                    event_input,
                )));
            }

            reject_stored_id(&document)?;
            normalize_schema_document(
                &table,
                &mut document,
                None,
                &id,
                None,
                params,
                &functions,
                true,
            )?;
            validate_index_values(&table, &document)?;
            let derived_hidden = derived_hidden_values(snapshot, &table, &document)?;
            let encoded_rid = encode_rid(&id.id)?;
            let (insert, bindings) = lower::physical_insert_document_with_hidden_stmt(
                &table.physical_name,
                &encoded_rid,
                &decode::encode_doc(&document)?,
                &derived_hidden,
            )?;
            contextual_constraint(
                conn.exec_bound(insert, bindings),
                "INSERT violates a declared unique index",
            )?;
            Ok(Some((
                "CREATE",
                id.clone(),
                Value::Null,
                full_record_value(&id, &document),
                event_input,
            )))
        })?;
        let Some(outcome) = outcome else {
            continue;
        };
        mark_fts_dirty(execution, &table_name);
        run_table_events(
            conn,
            execution,
            EventInvocation {
                table_name: &table_name,
                kind: outcome.0,
                id: &outcome.1,
                before: outcome.2.clone(),
                after: outcome.3.clone(),
                input: outcome.4.clone(),
            },
            script,
        )?;
        outcomes.push(outcome);
    }
    let mutation_count = outcomes.len();
    let rows = outcomes
        .into_iter()
        .filter_map(|(_, id, before, after, _)| {
            mutation_return(
                statement.return_clause.as_ref(),
                &before,
                &after,
                &id,
                None,
                params,
            )
            .transpose()
        })
        .collect::<Result<Vec<_>>>()?;
    StatementExecution::mutation(StatementResult::Rows(rows), mutation_count)
}

fn evaluate_insert_documents(
    data: &InsertData,
    table_name: &str,
    params: &Params,
) -> Result<Vec<BTreeMap<String, Value>>> {
    let document = BTreeMap::new();
    let id = RecordId::new(table_name, uuid::Uuid::now_v7());
    let context = EvalContext {
        document: &document,
        id: &id,
        endpoints: None,
        params,
        functions: None,
        function_calls: None,
        function_depth: 0,
    };
    let values = match data {
        InsertData::Expression(expression) => {
            match eval::evaluate(expression, &context)?.into_projection() {
                Value::Object(value) => vec![Value::Object(value)],
                Value::Array(values) => values,
                _ => {
                    return Err(FastDbError::Schema(
                        "INSERT data must be an object or array of objects".into(),
                    ))
                }
            }
        }
        InsertData::Values { fields, rows } => rows
            .iter()
            .map(|row| {
                fields
                    .iter()
                    .zip(row)
                    .map(|(field, expression)| {
                        Ok((
                            field.value.clone(),
                            eval::evaluate(expression, &context)?.into_projection(),
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>>>()
                    .map(Value::Object)
            })
            .collect::<Result<Vec<_>>>()?,
    };
    values
        .into_iter()
        .map(|value| match value {
            Value::Object(document) => Ok(document),
            _ => Err(FastDbError::Schema(
                "INSERT array elements must be objects".into(),
            )),
        })
        .collect()
}

fn run_insert_relation(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::InsertStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<StatementExecution> {
    let relation_name = statement.table.value.clone();
    let inputs = evaluate_insert_documents(&statement.data, &relation_name, params)?
        .into_iter()
        .map(|mut document| {
            let from = match document.remove("in") {
                Some(Value::RecordId(record)) => record,
                _ => {
                    return Err(FastDbError::Schema(
                        "INSERT RELATION requires record field `in`".into(),
                    ))
                }
            };
            let to = match document.remove("out") {
                Some(Value::RecordId(record)) => record,
                _ => {
                    return Err(FastDbError::Schema(
                        "INSERT RELATION requires record field `out`".into(),
                    ))
                }
            };
            let (id, document) = normalize_insert_document(&relation_name, document)?;
            Ok((id, from, to, document))
        })
        .collect::<Result<Vec<_>>>()?;
    let catalogs_missing = !catalog_for_read(conn, execution)?
        .snapshot()
        .is_some_and(|snapshot| {
            snapshot.tables.contains_key(&relation_name)
                && inputs.iter().all(|(_, from, to, _)| {
                    snapshot.tables.contains_key(&from.table)
                        && snapshot.tables.contains_key(&to.table)
                })
        });
    with_create_mutation(conn, execution, catalogs_missing, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        for (_, from, to, _) in &inputs {
            for endpoint in [from, to] {
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
                    return Err(FastDbError::Schema(
                        "INSERT RELATION endpoints must be normal records".into(),
                    ));
                }
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
            return Err(FastDbError::Schema(
                "INSERT RELATION target is not a relation table".into(),
            ));
        }
        if relation.drop {
            return Err(FastDbError::Constraint(format!(
                "relation table {relation_name:?} is DROP and rejects INSERT"
            )));
        }
        Ok(())
    })?;
    let mut outcomes = Vec::with_capacity(inputs.len());
    for (id, from, to, input_document) in &inputs {
        let outcome = with_create_mutation(conn, execution, false, |state| {
            let snapshot = ensure_snapshot(conn, state)?;
            let functions = snapshot.functions.clone();
            let relation = snapshot.tables[&relation_name].clone();
            let hidden = catalog::graph_columns(snapshot, &relation)?
                .into_iter()
                .map(|column| column.physical_name.clone())
                .collect::<Vec<_>>();
            let mut event_input = input_document.clone();
            event_input.insert("id".into(), Value::RecordId(id.clone()));
            event_input.insert("in".into(), Value::RecordId(from.clone()));
            event_input.insert("out".into(), Value::RecordId(to.clone()));
            let event_input = Value::Object(event_input);
            let from_table = snapshot.tables[&from.table].clone();
            let to_table = snapshot.tables[&to.table].clone();
            if relation
                .relation_in_table_id
                .is_some_and(|expected| expected != from_table.id)
                || relation
                    .relation_out_table_id
                    .is_some_and(|expected| expected != to_table.id)
            {
                return Err(FastDbError::Schema(
                    "INSERT RELATION endpoint table violates the relation definition".into(),
                ));
            }
            if relation.relation_enforced
                && (!record_exists(conn, &from_table, &from.id)?
                    || !record_exists(conn, &to_table, &to.id)?)
            {
                return Err(FastDbError::Constraint(
                    "INSERT RELATION enforced endpoint does not exist".into(),
                ));
            }
            let existing = read_candidates(
                conn,
                snapshot,
                &relation,
                CandidateReadOptions {
                    id: Some(&id.id),
                    range: None,
                    condition: None,
                    params,
                    allow_cache: false,
                    fts: None,
                    vector: None,
                },
            )?
            .into_iter()
            .next();
            if let Some(candidate) = existing {
                if statement.ignore.is_some() {
                    return Ok(None);
                }
                if statement.on_duplicate.is_empty() {
                    return Err(FastDbError::Constraint(
                        "INSERT RELATION edge ID already exists".into(),
                    ));
                }
                if candidate.endpoints.as_ref() != Some(&(from.clone(), to.clone())) {
                    return Err(FastDbError::Schema(
                        "INSERT RELATION cannot change immutable endpoints".into(),
                    ));
                }
                let before = full_candidate_value(&candidate);
                let mut scoped_params = params.clone();
                let mut input = input_document.clone();
                input.insert("id".into(), Value::RecordId(id.clone()));
                input.insert("in".into(), Value::RecordId(from.clone()));
                input.insert("out".into(), Value::RecordId(to.clone()));
                scoped_params.insert("input".into(), Value::Object(input));
                let context = EvalContext {
                    document: &candidate.document,
                    id: &candidate.id,
                    endpoints: Some((from, to)),
                    params: &scoped_params,
                    functions: None,
                    function_calls: None,
                    function_depth: 0,
                };
                let assignments = evaluate_assignments(&statement.on_duplicate, &context)?;
                let mut document = candidate.document.clone();
                apply_assignments(&mut document, assignments)?;
                reject_stored_edge_fields(&document)?;
                normalize_schema_document(
                    &relation,
                    &mut document,
                    Some(&candidate.document),
                    id,
                    Some((from, to)),
                    &scoped_params,
                    &functions,
                    false,
                )?;
                validate_index_values(&relation, &document)?;
                let derived_hidden = derived_hidden_values(snapshot, &relation, &document)?;
                let (update, bindings) = lower::physical_update_document_with_hidden_stmt(
                    &relation.physical_name,
                    &candidate.encoded_rid,
                    &decode::encode_doc(&document)?,
                    &derived_hidden,
                )?;
                conn.exec_bound(update, bindings)?;
                return Ok(Some((
                    "UPDATE",
                    id.clone(),
                    from.clone(),
                    to.clone(),
                    before,
                    full_edge_value(id, from, to, &document),
                    event_input,
                )));
            }
            let mut document = input_document.clone();
            reject_stored_edge_fields(&document)?;
            normalize_schema_document(
                &relation,
                &mut document,
                None,
                id,
                Some((from, to)),
                params,
                &functions,
                true,
            )?;
            validate_index_values(&relation, &document)?;
            let derived_hidden = derived_hidden_values(snapshot, &relation, &document)?;
            let (insert, bindings) = lower::physical_relation_insert_with_hidden_stmt(
                &relation.physical_name,
                &hidden,
                &derived_hidden,
                &encode_rid(&id.id)?,
                &decode::encode_doc(&document)?,
                &from_table.id.to_hex(),
                &encode_rid(&from.id)?,
                &to_table.id.to_hex(),
                &encode_rid(&to.id)?,
            )?;
            conn.exec_bound(insert, bindings)?;
            conn.check_failpoint(Failpoint::AfterGraphEdgeInsert)?;
            Ok(Some((
                "CREATE",
                id.clone(),
                from.clone(),
                to.clone(),
                Value::Null,
                full_edge_value(id, from, to, &document),
                event_input,
            )))
        })?;
        let Some(outcome) = outcome else {
            continue;
        };
        mark_fts_dirty(execution, &relation_name);
        run_table_events(
            conn,
            execution,
            EventInvocation {
                table_name: &relation_name,
                kind: outcome.0,
                id: &outcome.1,
                before: outcome.4.clone(),
                after: outcome.5.clone(),
                input: outcome.6.clone(),
            },
            script,
        )?;
        outcomes.push(outcome);
    }
    let mutation_count = outcomes.len();
    let rows = outcomes
        .into_iter()
        .filter_map(|(_, id, from, to, before, after, _)| {
            mutation_return(
                statement.return_clause.as_ref(),
                &before,
                &after,
                &id,
                Some((&from, &to)),
                params,
            )
            .transpose()
        })
        .collect::<Result<Vec<_>>>()?;
    StatementExecution::mutation(StatementResult::Rows(rows), mutation_count)
}

fn normalize_insert_document(
    table_name: &str,
    mut document: BTreeMap<String, Value>,
) -> Result<(RecordId, BTreeMap<String, Value>)> {
    let id = match document.remove("id") {
        None | Some(Value::None) | Some(Value::Null) => {
            RecordId::new(table_name, uuid::Uuid::now_v7())
        }
        Some(Value::RecordId(record)) if record.table == table_name => record,
        Some(Value::RecordId(_)) => {
            return Err(FastDbError::Schema(
                "INSERT id belongs to a different table".into(),
            ))
        }
        Some(Value::Str(id)) => RecordId::new(table_name, id),
        Some(Value::Integer(id)) => RecordId::new(table_name, id),
        Some(Value::Uuid(id)) => RecordId::new(table_name, id),
        Some(Value::Array(id)) => RecordId::new(table_name, RecordIdValue::Array(id)),
        Some(Value::Object(id)) => RecordId::new(table_name, RecordIdValue::Object(id)),
        Some(_) => {
            return Err(FastDbError::Schema(
                "INSERT id must be a record, string, integer, UUID, array, or object".into(),
            ))
        }
    };
    Ok((id, document))
}

fn run_relate(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::RelateStatement,
    params: &Params,
    script: &mut ScriptRuntime,
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
        functions: None,
        function_calls: None,
        function_depth: 0,
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
    let mut event_input = document.clone();
    event_input.insert("in".into(), Value::RecordId(from.clone()));
    event_input.insert("out".into(), Value::RecordId(to.clone()));
    let event_input = Value::Object(event_input);

    let catalogs_missing = !catalog_for_read(conn, execution)?
        .snapshot()
        .is_some_and(|snapshot| {
            snapshot.tables.contains_key(&from.table)
                && snapshot.tables.contains_key(&to.table)
                && snapshot.tables.contains_key(&relation_name)
        });
    let value = with_create_mutation(conn, execution, catalogs_missing, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let functions = snapshot.functions.clone();
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
        if relation.drop {
            return Err(FastDbError::Constraint(format!(
                "relation table {relation_name:?} is DROP and rejects RELATE"
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
        normalize_schema_document(
            &relation,
            &mut document,
            None,
            &edge_id,
            Some((&from, &to)),
            params,
            &functions,
            true,
        )?;
        validate_index_values(&relation, &document)?;
        let hidden = catalog::graph_columns(snapshot, &relation)?
            .into_iter()
            .map(|column| column.physical_name.clone())
            .collect::<Vec<_>>();
        let derived_hidden = derived_hidden_values(snapshot, &relation, &document)?;
        let encoded_edge = encode_rid(&edge_id.id)?;
        let encoded_from = encode_rid(&from.id)?;
        let encoded_to = encode_rid(&to.id)?;
        let (insert, bindings) = lower::physical_relation_insert_with_hidden_stmt(
            &relation.physical_name,
            &hidden,
            &derived_hidden,
            &encoded_edge,
            &decode::encode_doc(&document)?,
            &from_table.id.to_hex(),
            &encoded_from,
            &to_table.id.to_hex(),
            &encoded_to,
        )?;
        conn.exec_bound(insert, bindings)?;
        conn.check_failpoint(Failpoint::AfterGraphEdgeInsert)?;
        Ok(full_edge_value(&edge_id, &from, &to, &document))
    })?;
    mark_fts_dirty(execution, &relation_name);
    run_table_events(
        conn,
        execution,
        EventInvocation {
            table_name: &relation_name,
            kind: "CREATE",
            id: &edge_id,
            before: Value::Null,
            after: value.clone(),
            input: event_input,
        },
        script,
    )?;

    let returned = mutation_return(
        statement.return_clause.as_ref(),
        &Value::Null,
        &value,
        &edge_id,
        Some((&from, &to)),
        params,
    )?;
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
    let encoded = encode_rid(id)?;
    let (statement, bindings) =
        lower::physical_select_stmt(&table.physical_name, Some(&encoded), &[])?;
    Ok(!conn.collect_rows(statement, bindings)?.is_empty())
}

fn run_select(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::SelectStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<StatementResult> {
    validate_projection_shapes(&statement.projections, statement.value.is_some())?;
    if !statement.additional_targets.is_empty()
        || !matches!(statement.target, SelectTarget::Target(_))
    {
        return run_multi_target_select(conn, execution, statement, params, script);
    }
    if let Some(only) = statement.only {
        let single_record = matches!(statement.target, SelectTarget::Target(Target::Record(_)));
        let bounded_table = statement
            .limit
            .as_ref()
            .is_some_and(|limit| limit.value == 1);
        if !single_record && !bounded_table {
            return unsupported(
                only,
                "SELECT ONLY requires a record target or an exact LIMIT 1",
            );
        }
    }
    let (table_name, selector) = select_target_parts(&statement.target)?;
    let catalog = catalog_for_read(conn, execution)?;
    let Some(snapshot) = catalog.snapshot().cloned() else {
        return Ok(if statement.only.is_some() {
            StatementResult::Value(Value::Null)
        } else {
            StatementResult::Rows(Vec::new())
        });
    };
    drop(catalog);
    let Some(table) = snapshot.tables.get(&table_name) else {
        return Ok(if statement.only.is_some() {
            StatementResult::Value(Value::Null)
        } else {
            StatementResult::Rows(Vec::new())
        });
    };
    let fts = resolve_fts_query(&statement, table, params)?;
    let vector = resolve_vector_query(&statement, &snapshot, table, params)?;
    if fts.is_some() && vector.is_some() {
        return Err(FastDbError::Schema(
            "FTS and KNN predicates cannot be combined in one Phase 9 SELECT".into(),
        ));
    }
    if fts.is_some()
        && matches!(
            &execution.transaction,
            TransactionState::Active(active) if active.dirty_fts_tables.contains(&table.id)
        )
    {
        return Err(FastDbError::Transaction(format!(
            "FTS index on table {table_name:?} is unavailable after an indexed write until commit"
        )));
    }
    let candidates = read_candidates(
        conn,
        &snapshot,
        table,
        CandidateReadOptions {
            id: selector.id(),
            range: selector.range(),
            condition: statement.condition.as_ref(),
            params,
            allow_cache: !matches!(execution.transaction, TransactionState::Active(_)),
            fts: fts.as_ref(),
            vector: vector.as_ref(),
        },
    )?;
    let mut matched = Vec::new();
    for candidate in candidates {
        if vector.is_some()
            || matches_condition_with_fts(
                conn,
                execution,
                statement.condition.as_ref(),
                &candidate,
                params,
                &snapshot,
                script,
            )?
        {
            matched.push(candidate);
        }
    }
    if statement.split.is_empty()
        && statement.group.is_none()
        && statement.omit.is_empty()
        && statement.fetch.is_empty()
        && statement.order_by.is_empty()
        && statement.order_random.is_none()
    {
        let start = resolve_pagination(
            statement.start.as_ref(),
            statement.start_expression.as_ref(),
            params,
            "START",
        )?
        .unwrap_or(0);
        let limit = resolve_pagination(
            statement.limit.as_ref(),
            statement.limit_expression.as_ref(),
            params,
            "LIMIT",
        )?
        .unwrap_or(usize::MAX);
        let rows = matched
            .into_iter()
            .skip(start)
            .take(limit)
            .map(|candidate| {
                project_select_candidate(
                    conn, execution, &snapshot, &candidate, &statement, params, script,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        return if statement.only.is_some() {
            Ok(StatementResult::Value(
                rows.into_iter().next().unwrap_or(Value::Null),
            ))
        } else {
            Ok(StatementResult::Rows(rows))
        };
    }
    let candidates = split_candidates(matched, &statement.split)?;
    let mut rows = if let Some(group) = &statement.group {
        project_grouped_candidates(
            conn,
            execution,
            &snapshot,
            &candidates,
            group,
            &statement,
            params,
            script,
        )?
    } else {
        candidates
            .into_iter()
            .map(|candidate| {
                let value = project_select_candidate(
                    conn, execution, &snapshot, &candidate, &statement, params, script,
                )?;
                Ok(QueryRow {
                    value,
                    source: Some(candidate),
                })
            })
            .collect::<Result<Vec<_>>>()?
    };
    for row in &mut rows {
        apply_omit(&mut row.value, &statement.omit)?;
        apply_fetch(conn, &snapshot, &mut row.value, &statement.fetch)?;
    }
    order_query_rows(&mut rows, &statement, params)?;
    let start = resolve_pagination(
        statement.start.as_ref(),
        statement.start_expression.as_ref(),
        params,
        "START",
    )?
    .unwrap_or(0);
    let limit = resolve_pagination(
        statement.limit.as_ref(),
        statement.limit_expression.as_ref(),
        params,
        "LIMIT",
    )?
    .unwrap_or(usize::MAX);
    let rows = rows
        .into_iter()
        .skip(start)
        .take(limit)
        .map(|row| row.value)
        .collect::<Vec<_>>();
    if statement.only.is_some() {
        Ok(StatementResult::Value(
            rows.into_iter().next().unwrap_or(Value::Null),
        ))
    } else {
        Ok(StatementResult::Rows(rows))
    }
}

fn run_multi_target_select(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::SelectStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<StatementResult> {
    if statement.value.is_some()
        || !matches!(statement.projections, ProjectionList::All(_))
        || statement.condition.is_some()
        || !statement.split.is_empty()
        || statement.group.is_some()
        || !statement.omit.is_empty()
        || !statement.fetch.is_empty()
        || statement.only.is_some()
    {
        return Err(FastDbError::Schema(
            "heterogeneous SELECT targets currently require SELECT * without WHERE/SPLIT/GROUP/OMIT/FETCH/ONLY"
                .into(),
        ));
    }
    let mut targets = vec![statement.target.clone()];
    targets.extend(statement.additional_targets.iter().cloned());
    let mut rows = Vec::new();
    for target in targets {
        match target {
            SelectTarget::Target(target) => {
                let mut nested = statement.clone();
                nested.target = SelectTarget::Target(target);
                nested.additional_targets.clear();
                nested.order_by.clear();
                nested.order_random = None;
                nested.limit = None;
                nested.limit_expression = None;
                nested.start = None;
                nested.start_expression = None;
                let StatementResult::Rows(values) =
                    run_select(conn, execution, nested, params, script)?
                else {
                    unreachable!("nested multi-target SELECT is not ONLY")
                };
                rows.extend(values.into_iter().map(|value| QueryRow {
                    value,
                    source: None,
                }));
            }
            SelectTarget::Expression(expression) => {
                let document = BTreeMap::new();
                let id = RecordId::new("__target", "value");
                let context = EvalContext {
                    document: &document,
                    id: &id,
                    endpoints: None,
                    params,
                    functions: None,
                    function_calls: None,
                    function_depth: 0,
                };
                let value = eval::evaluate(&expression, &context)?.into_projection();
                let values = match value {
                    Value::Array(values) => values,
                    value => vec![value],
                };
                rows.extend(values.into_iter().map(|value| QueryRow {
                    value,
                    source: None,
                }));
            }
            SelectTarget::Subquery(select) => {
                let result = run_select(conn, execution, *select, params, script)?;
                match result {
                    StatementResult::Rows(values) => {
                        rows.extend(values.into_iter().map(|value| QueryRow {
                            value,
                            source: None,
                        }));
                    }
                    StatementResult::Value(value) => rows.push(QueryRow {
                        value,
                        source: None,
                    }),
                    StatementResult::None => {}
                }
            }
        }
    }
    order_query_rows(&mut rows, &statement, params)?;
    let start = resolve_pagination(
        statement.start.as_ref(),
        statement.start_expression.as_ref(),
        params,
        "START",
    )?
    .unwrap_or(0);
    let limit = resolve_pagination(
        statement.limit.as_ref(),
        statement.limit_expression.as_ref(),
        params,
        "LIMIT",
    )?
    .unwrap_or(usize::MAX);
    Ok(StatementResult::Rows(
        rows.into_iter()
            .skip(start)
            .take(limit)
            .map(|row| row.value)
            .collect(),
    ))
}

fn split_candidates(
    mut candidates: Vec<Candidate>,
    paths: &[turso_fastdb_parser::FieldPath],
) -> Result<Vec<Candidate>> {
    for path in paths {
        let (path, _) = crate::path::parser_path(path)?;
        let mut split = Vec::new();
        for candidate in candidates {
            let Some(Value::Array(values)) = crate::path::get_path(&candidate.document, &path)
            else {
                split.push(candidate);
                continue;
            };
            if values.is_empty() {
                split.push(candidate);
                continue;
            }
            for value in values {
                if split.len() >= 100_000 {
                    return Err(FastDbError::ResourceLimit(
                        "SPLIT result exceeds 100,000 rows".into(),
                    ));
                }
                let mut candidate = candidate.clone();
                crate::path::set_path(&mut candidate.document, &path, value.clone())?;
                split.push(candidate);
            }
        }
        candidates = split;
    }
    Ok(candidates)
}

fn project_select_candidate(
    conn: &Connection,
    execution: &mut ExecutionState,
    snapshot: &CatalogSnapshot,
    candidate: &Candidate,
    statement: &turso_fastdb_parser::SelectStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<Value> {
    if statement.value.is_none() {
        return project_candidate(
            conn,
            execution,
            snapshot,
            candidate,
            &statement.projections,
            statement.include_all,
            params,
            script,
        );
    }
    let ProjectionList::Fields(projections) = &statement.projections else {
        return Err(FastDbError::Schema(
            "SELECT VALUE requires an expression".into(),
        ));
    };
    if projections.len() != 1 || projections[0].alias.is_some() {
        return Err(FastDbError::Schema(
            "SELECT VALUE requires exactly one unaliased expression".into(),
        ));
    }
    evaluate_projection_expression(
        conn,
        execution,
        snapshot,
        candidate,
        &projections[0].expression,
        params,
        script,
    )
}

#[allow(clippy::too_many_arguments)]
fn project_grouped_candidates(
    conn: &Connection,
    execution: &mut ExecutionState,
    snapshot: &CatalogSnapshot,
    candidates: &[Candidate],
    group: &GroupClause,
    statement: &turso_fastdb_parser::SelectStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<Vec<QueryRow>> {
    let mut groups: Vec<(Vec<Value>, Vec<Candidate>)> = Vec::new();
    if matches!(group, GroupClause::All(_)) && candidates.is_empty() {
        groups.push((Vec::new(), Vec::new()));
    }
    for candidate in candidates {
        let keys = match group {
            GroupClause::All(_) => Vec::new(),
            GroupClause::By(expressions) => expressions
                .iter()
                .map(|expression| {
                    evaluate_projection_expression(
                        conn, execution, snapshot, candidate, expression, params, script,
                    )
                })
                .collect::<Result<Vec<_>>>()?,
        };
        if let Some((_, values)) = groups.iter_mut().find(|(existing, _)| {
            existing.len() == keys.len()
                && existing.iter().zip(&keys).all(|(left, right)| {
                    decode::canonical_value_cmp(left, right) == Ordering::Equal
                })
        }) {
            values.push(candidate.clone());
        } else {
            groups.push((keys, vec![candidate.clone()]));
        }
    }
    groups
        .into_iter()
        .map(|(_, candidates)| {
            let value = project_group(
                conn,
                execution,
                snapshot,
                &candidates,
                group,
                statement,
                params,
                script,
            )?;
            Ok(QueryRow {
                value,
                source: None,
            })
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn project_group(
    conn: &Connection,
    execution: &mut ExecutionState,
    snapshot: &CatalogSnapshot,
    candidates: &[Candidate],
    group: &GroupClause,
    statement: &turso_fastdb_parser::SelectStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<Value> {
    let ProjectionList::Fields(projections) = &statement.projections else {
        return Err(FastDbError::Schema(
            "GROUP requires explicit projections".into(),
        ));
    };
    if statement.value.is_some() {
        if projections.len() != 1 || projections[0].alias.is_some() {
            return Err(FastDbError::Schema(
                "SELECT VALUE with GROUP requires one unaliased projection".into(),
            ));
        }
        return evaluate_group_expression(
            conn,
            execution,
            snapshot,
            candidates,
            group,
            &projections[0].expression,
            params,
            script,
        );
    }
    let mut object = BTreeMap::new();
    for projection in projections {
        let value = evaluate_group_expression(
            conn,
            execution,
            snapshot,
            candidates,
            group,
            &projection.expression,
            params,
            script,
        )?;
        if let Some(alias) = &projection.alias {
            object.insert(alias.value.clone(), value);
        } else if let ExprKind::FieldPath(path) = &projection.expression.kind {
            let path = path
                .segments
                .iter()
                .map(|segment| segment.value.clone())
                .collect::<Vec<_>>();
            crate::path::set_path(&mut object, &path, value)?;
        } else {
            return Err(FastDbError::Schema(
                "aggregate expressions require an alias".into(),
            ));
        }
    }
    Ok(Value::Object(object))
}

#[allow(clippy::too_many_arguments)]
fn evaluate_group_expression(
    conn: &Connection,
    execution: &mut ExecutionState,
    snapshot: &CatalogSnapshot,
    candidates: &[Candidate],
    group: &GroupClause,
    expression: &Expr,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<Value> {
    if let ExprKind::FunctionCall { name, arguments } = &expression.kind {
        if function_name_is(name, &["count"]) && arguments.is_empty() {
            return Ok(Value::Integer(i64::try_from(candidates.len()).map_err(
                |_| FastDbError::ResourceLimit("aggregate count exceeds i64".into()),
            )?));
        }
        if is_aggregate_function(name) && arguments.len() == 1 {
            let values = candidates
                .iter()
                .map(|candidate| {
                    evaluate_projection_expression(
                        conn,
                        execution,
                        snapshot,
                        candidate,
                        &arguments[0],
                        params,
                        script,
                    )
                })
                .collect::<Result<Vec<_>>>()?;
            if function_name_is(name, &["array", "group"]) {
                return Ok(Value::Array(values));
            }
            let mut aggregate_params = params.clone();
            aggregate_params.insert("__fastdb_group".into(), Value::Array(values));
            let aggregate = Expr::new(
                ExprKind::FunctionCall {
                    name: name.clone(),
                    arguments: vec![Expr::new(
                        ExprKind::Parameter("__fastdb_group".into()),
                        expression.span,
                    )],
                },
                expression.span,
            );
            let document = BTreeMap::new();
            let id = RecordId::new("__group", "all");
            return eval::evaluate(
                &aggregate,
                &EvalContext {
                    document: &document,
                    id: &id,
                    endpoints: None,
                    params: &aggregate_params,
                    functions: None,
                    function_calls: None,
                    function_depth: 0,
                },
            )
            .map(EvalValue::into_projection);
        }
    }
    if candidates.is_empty() {
        return Ok(Value::None);
    }
    let values = candidates
        .iter()
        .map(|candidate| {
            evaluate_projection_expression(
                conn, execution, snapshot, candidate, expression, params, script,
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let is_group_key = match group {
        GroupClause::All(_) => false,
        GroupClause::By(keys) => keys
            .iter()
            .any(|key| expressions_equivalent(key, expression)),
    };
    if is_group_key || values.len() == 1 {
        Ok(values.into_iter().next().unwrap_or(Value::None))
    } else {
        Ok(Value::Array(values))
    }
}

fn expressions_equivalent(left: &Expr, right: &Expr) -> bool {
    match (&left.kind, &right.kind) {
        (ExprKind::FieldPath(left), ExprKind::FieldPath(right)) => left
            .segments
            .iter()
            .map(|segment| &segment.value)
            .eq(right.segments.iter().map(|segment| &segment.value)),
        _ => left.kind == right.kind,
    }
}

fn is_aggregate_function(name: &[turso_fastdb_parser::Identifier]) -> bool {
    function_name_is(name, &["count"])
        || function_name_is(name, &["array", "group"])
        || [
            "max", "mean", "median", "min", "mode", "product", "spread", "stddev", "sum",
            "variance",
        ]
        .iter()
        .any(|function| function_name_is(name, &["math", function]))
}

fn evaluate_projection_expression(
    conn: &Connection,
    execution: &mut ExecutionState,
    snapshot: &CatalogSnapshot,
    candidate: &Candidate,
    expression: &Expr,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<Value> {
    if let ExprKind::Traversal(traversal) = &expression.kind {
        traverse_graph(conn, snapshot, &candidate.id, traversal)
    } else if matches!(&expression.kind, ExprKind::FunctionCall { name, .. }
        if custom_function_name(name).is_none())
    {
        evaluate_special_projection(expression, candidate, params)
    } else {
        evaluate_expression_with_custom_functions(
            conn,
            execution,
            &candidate.document,
            &candidate.id,
            candidate.endpoints.as_ref().map(|(from, to)| (from, to)),
            expression,
            params,
            &snapshot.functions,
            script,
        )
    }
}

fn apply_omit(value: &mut Value, paths: &[turso_fastdb_parser::FieldPath]) -> Result<()> {
    let Value::Object(document) = value else {
        return Ok(());
    };
    for path in paths {
        let (path, _) = crate::path::parser_path(path)?;
        crate::path::remove_path(document, &path)?;
    }
    Ok(())
}

fn apply_fetch(
    conn: &Connection,
    snapshot: &CatalogSnapshot,
    value: &mut Value,
    paths: &[turso_fastdb_parser::FieldPath],
) -> Result<()> {
    let Value::Object(document) = value else {
        return Ok(());
    };
    for path in paths {
        let (path, _) = crate::path::parser_path(path)?;
        let Some(value) = crate::path::get_path_mut(document, &path) else {
            continue;
        };
        fetch_value(conn, snapshot, value)?;
    }
    Ok(())
}

fn fetch_value(conn: &Connection, snapshot: &CatalogSnapshot, value: &mut Value) -> Result<()> {
    match value {
        Value::RecordId(record) => {
            *value = materialize_record(conn, snapshot, record)?.unwrap_or(Value::Null);
        }
        Value::Array(values) => {
            for value in values {
                if matches!(value, Value::RecordId(_)) {
                    fetch_value(conn, snapshot, value)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn order_query_rows(
    rows: &mut [QueryRow],
    statement: &turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<()> {
    if statement.order_random.is_some() {
        rows.shuffle(&mut rand::rng());
        return Ok(());
    }
    if statement.order_by.is_empty() {
        return Ok(());
    }
    let keyed = rows
        .iter()
        .map(|row| {
            statement
                .order_by
                .iter()
                .map(|term| query_row_order_value(row, term, params))
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?;
    let mut order = (0..rows.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        for ((left_value, right_value), term) in keyed[*left]
            .iter()
            .zip(&keyed[*right])
            .zip(&statement.order_by)
        {
            let ordering = compare_order_values(left_value, right_value, term);
            if ordering != Ordering::Equal {
                return match term.direction.value {
                    turso_fastdb_parser::OrderDirection::Ascending => ordering,
                    turso_fastdb_parser::OrderDirection::Descending => ordering.reverse(),
                };
            }
        }
        left.cmp(right)
    });
    let original = rows.to_vec();
    for (slot, index) in rows.iter_mut().zip(order) {
        *slot = original[index].clone();
    }
    Ok(())
}

fn query_row_order_value(
    row: &QueryRow,
    term: &turso_fastdb_parser::OrderBy,
    params: &Params,
) -> Result<Value> {
    let path = term
        .path
        .segments
        .iter()
        .map(|segment| segment.value.clone())
        .collect::<Vec<_>>();
    if let Value::Object(document) = &row.value {
        if let Some(value) = crate::path::get_path(document, &path) {
            return Ok(value.clone());
        }
    }
    if let Some(candidate) = &row.source {
        return eval::evaluate(
            &Expr::new(ExprKind::FieldPath(term.path.clone()), term.path.span),
            &candidate_context(candidate, params),
        )
        .map(EvalValue::into_projection);
    }
    Ok(Value::None)
}

fn compare_order_values(
    left: &Value,
    right: &Value,
    term: &turso_fastdb_parser::OrderBy,
) -> Ordering {
    if let (Value::Str(left), Value::Str(right)) = (left, right) {
        let (left, right) = if term.collate.is_some() {
            (left.to_lowercase(), right.to_lowercase())
        } else {
            (left.clone(), right.clone())
        };
        if term.numeric.is_some() {
            return natural_string_cmp(&left, &right);
        }
        return left.cmp(&right);
    }
    decode::canonical_value_cmp(left, right)
}

fn natural_string_cmp(left: &str, right: &str) -> Ordering {
    let mut left = left.chars().peekable();
    let mut right = right.chars().peekable();
    loop {
        match (left.peek(), right.peek()) {
            (Some(l), Some(r)) if l.is_ascii_digit() && r.is_ascii_digit() => {
                let left_digits = std::iter::from_fn(|| {
                    left.peek()
                        .copied()
                        .filter(char::is_ascii_digit)
                        .inspect(|_| {
                            left.next();
                        })
                })
                .collect::<String>();
                let right_digits = std::iter::from_fn(|| {
                    right
                        .peek()
                        .copied()
                        .filter(char::is_ascii_digit)
                        .inspect(|_| {
                            right.next();
                        })
                })
                .collect::<String>();
                let ordering = left_digits
                    .trim_start_matches('0')
                    .len()
                    .cmp(&right_digits.trim_start_matches('0').len())
                    .then_with(|| {
                        left_digits
                            .trim_start_matches('0')
                            .cmp(right_digits.trim_start_matches('0'))
                    });
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(_), Some(_)) => {
                let ordering = left.next().cmp(&right.next());
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
        }
    }
}

fn resolve_pagination(
    literal: Option<&turso_fastdb_parser::NonnegativeInteger>,
    expression: Option<&Expr>,
    params: &Params,
    label: &str,
) -> Result<Option<usize>> {
    if let Some(literal) = literal {
        return Ok(Some(usize::try_from(literal.value).unwrap_or(usize::MAX)));
    }
    let Some(expression) = expression else {
        return Ok(None);
    };
    let document = BTreeMap::new();
    let id = RecordId::new("__pagination", "value");
    let value = eval::evaluate(
        expression,
        &EvalContext {
            document: &document,
            id: &id,
            endpoints: None,
            params,
            functions: None,
            function_calls: None,
            function_depth: 0,
        },
    )?
    .into_projection();
    let Value::Integer(value) = value else {
        return Err(FastDbError::Schema(format!(
            "{label} must evaluate to a nonnegative integer"
        )));
    };
    usize::try_from(value)
        .map(Some)
        .map_err(|_| FastDbError::Schema(format!("{label} must be nonnegative")))
}

fn validate_projection_shapes(projections: &ProjectionList, select_value: bool) -> Result<()> {
    if let ProjectionList::Fields(projections) = projections {
        for projection in projections {
            if !select_value
                && projection.alias.is_none()
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
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::ExplainStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<StatementResult> {
    validate_projection_shapes(
        &statement.select.projections,
        statement.select.value.is_some(),
    )?;
    let analyzed = if statement.analyze.is_some() {
        let started = std::time::Instant::now();
        let result = run_select(conn, execution, statement.select.clone(), params, script)?;
        let row_count = match &result {
            StatementResult::Rows(rows) => rows.len(),
            StatementResult::Value(Value::Null) | StatementResult::None => 0,
            StatementResult::Value(_) => 1,
        };
        Some((row_count, started.elapsed()))
    } else {
        None
    };
    let physical_target = matches!(statement.select.target, SelectTarget::Target(_))
        && statement.select.additional_targets.is_empty();
    let graph_statements = if physical_target {
        lower_graph_scans_for_explain(conn, execution, &statement.select)?
    } else {
        Vec::new()
    };
    let vector_plan = if physical_target {
        lower_vector_scan_for_explain(conn, execution, &statement.select, params)?
    } else {
        None
    };
    let lowered = if let Some((lowered, _)) = &vector_plan {
        lowered.clone()
    } else if let Some(lowered) = if physical_target {
        lower_fts_scan_for_explain(conn, execution, &statement.select, params)?
    } else {
        None
    } {
        lowered
    } else if physical_target {
        lower_select_scan_for_explain(conn, execution, statement.select.clone(), params)?
    } else {
        let mut details = vec!["FASTDB BOUNDED MULTI-TARGET PIPELINE".to_string()];
        if statement.full.is_some() {
            details.push("FASTDB FULL DECODE/PROJECT/ORDER PIPELINE".into());
        }
        return explain_result(details, analyzed, statement.format_json.is_some());
    };
    let mut details = crate::connection::explain_statement(conn, lowered)?;
    if let Some((_, detail)) = vector_plan {
        details.push(detail);
    }
    if details
        .iter()
        .any(|detail| detail.contains("QUERY INDEX METHOD fts"))
    {
        if let Some(name) = fts_index_name_for_explain(conn, execution, &statement.select, params)?
        {
            details.push(format!("FTS INDEX {name}"));
        }
    }
    for graph_statement in graph_statements {
        details.extend(crate::connection::explain_statement(conn, graph_statement)?);
    }
    if statement.full.is_some() {
        details.push("FASTDB FULL DECODE/FILTER/PROJECT/ORDER PIPELINE".into());
    }
    explain_result(details, analyzed, statement.format_json.is_some())
}

fn explain_result(
    details: Vec<String>,
    analyzed: Option<(usize, std::time::Duration)>,
    format_json: bool,
) -> Result<StatementResult> {
    if format_json {
        let mut value = BTreeMap::from([
            (
                "operation".into(),
                Value::Str("FastDB query pipeline".into()),
            ),
            (
                "details".into(),
                Value::Array(details.into_iter().map(Value::Str).collect()),
            ),
        ]);
        if let Some((rows, elapsed)) = analyzed {
            value.insert(
                "actual_rows".into(),
                Value::Integer(i64::try_from(rows).map_err(|_| {
                    FastDbError::ResourceLimit("EXPLAIN row count exceeds i64".into())
                })?),
            );
            value.insert(
                "elapsed_ns".into(),
                Value::Integer(i64::try_from(elapsed.as_nanos()).unwrap_or(i64::MAX)),
            );
        }
        return Ok(StatementResult::Value(Value::Object(value)));
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
    let mut rows = rows;
    if let Some((actual_rows, elapsed)) = analyzed {
        rows.push(Value::Object(BTreeMap::from([
            (
                "actual_rows".into(),
                Value::Integer(i64::try_from(actual_rows).map_err(|_| {
                    FastDbError::ResourceLimit("EXPLAIN row count exceeds i64".into())
                })?),
            ),
            (
                "elapsed_ns".into(),
                Value::Integer(i64::try_from(elapsed.as_nanos()).unwrap_or(i64::MAX)),
            ),
        ])));
    }
    Ok(StatementResult::Rows(rows))
}

fn lower_vector_scan_for_explain(
    conn: &Connection,
    execution: &ExecutionState,
    select: &turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<Option<(turso_parser::ast::Stmt, String)>> {
    let (table_name, selector) = select_target_parts(&select.target)?;
    let catalog = catalog_for_read(conn, execution)?;
    let Some(snapshot) = catalog.snapshot() else {
        return Ok(None);
    };
    let Some(table) = snapshot.tables.get(&table_name) else {
        return Ok(None);
    };
    let Some(vector) = resolve_vector_query(select, snapshot, table, params)? else {
        return Ok(None);
    };
    let predicates = select
        .condition
        .as_ref()
        .map(|condition| safe_pushdowns(condition, params, table))
        .unwrap_or_default();
    let graph = if table.kind == TableKind::Relation {
        catalog::graph_columns(snapshot, table)?
            .into_iter()
            .map(|column| column.physical_name.clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if selector.range().is_some() {
        return Err(FastDbError::Schema(
            "record ranges cannot be combined with KNN search".into(),
        ));
    }
    let encoded_id = selector.id().map(encode_rid).transpose()?;
    let (lowered, _) = lower::physical_vector_select_stmt(
        &table.physical_name,
        &vector.column.physical_name,
        &graph,
        encoded_id.as_deref(),
        &predicates,
        vector.query.clone(),
        vector.metric,
        vector.k,
        vector.needs_document,
    )?;
    let metric = match vector.metric {
        turso_fastdb_parser::KnnMetric::Cosine => "COSINE",
        turso_fastdb_parser::KnnMetric::Euclidean => "EUCLIDEAN",
    };
    Ok(Some((
        lowered,
        format!("VECTOR EXACT SCAN {metric} K={}", vector.k),
    )))
}

fn fts_index_name_for_explain(
    conn: &Connection,
    execution: &ExecutionState,
    select: &turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<Option<String>> {
    let (table_name, _) = select_target_parts(&select.target)?;
    let catalog = catalog_for_read(conn, execution)?;
    let Some(table) = catalog
        .snapshot()
        .and_then(|snapshot| snapshot.tables.get(&table_name))
    else {
        return Ok(None);
    };
    Ok(resolve_fts_query(select, table, params)?.map(|query| query.index.physical_name))
}

fn lower_fts_scan_for_explain(
    conn: &Connection,
    execution: &ExecutionState,
    select: &turso_fastdb_parser::SelectStatement,
    params: &Params,
) -> Result<Option<turso_parser::ast::Stmt>> {
    let (table_name, selector) = select_target_parts(&select.target)?;
    let catalog = catalog_for_read(conn, execution)?;
    let Some(snapshot) = catalog.snapshot() else {
        return Ok(None);
    };
    let Some(table) = snapshot.tables.get(&table_name) else {
        return Ok(None);
    };
    let Some(fts) = resolve_fts_query(select, table, params)? else {
        return Ok(None);
    };
    if matches!(
        &execution.transaction,
        TransactionState::Active(active) if active.dirty_fts_tables.contains(&table.id)
    ) {
        return Err(FastDbError::Transaction(format!(
            "FTS index on table {table_name:?} is unavailable after an indexed write until commit"
        )));
    }
    let graph = if table.kind == TableKind::Relation {
        catalog::graph_columns(snapshot, table)?
            .into_iter()
            .map(|column| column.physical_name.clone())
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    if selector.range().is_some() {
        return Err(FastDbError::Schema(
            "record ranges cannot be combined with full-text search".into(),
        ));
    }
    let encoded_id = selector.id().map(encode_rid).transpose()?;
    let (statement, _) = lower::physical_fts_select_stmt(
        &table.physical_name,
        &fts.index.physical_columns,
        &graph,
        encoded_id.as_deref(),
        &fts.query,
    )?;
    Ok(Some(statement))
}

fn lower_graph_scans_for_explain(
    conn: &Connection,
    execution: &ExecutionState,
    select: &turso_fastdb_parser::SelectStatement,
) -> Result<Vec<turso_parser::ast::Stmt>> {
    let ProjectionList::Fields(projections) = &select.projections else {
        return Ok(Vec::new());
    };
    let (source_table_name, _) = select_target_parts(&select.target)?;
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
                    &encode_rid("explain")?,
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
    upsert: bool,
    script: &mut ScriptRuntime,
) -> Result<StatementExecution> {
    let _timeout = StatementTimeoutGuard::install(conn, statement.timeout.as_ref(), params)?;
    let targets = resolve_mutation_targets(statement.target.clone(), params)?;
    let table_was_missing = {
        let catalog = catalog_for_read(conn, execution)?;
        let snapshot = catalog.snapshot();
        targets
            .iter()
            .any(|(table, _)| !snapshot.is_some_and(|snapshot| snapshot.tables.contains_key(table)))
    };
    let work = with_create_mutation(conn, execution, upsert && table_was_missing, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        for (table_name, _) in &targets {
            if snapshot.tables.contains_key(table_name) || !upsert {
                continue;
            }
            let table = catalog::allocate_table(table_name, TableMode::Schemaless, None)?;
            catalog::persist_table(conn, &table)?;
            conn.check_failpoint(Failpoint::AfterCatalogRow)?;
            conn.exec_bound(lower::physical_table_ddl(&table.physical_name)?, vec![])?;
            conn.check_failpoint(Failpoint::AfterPhysicalDdl)?;
            snapshot.tables.insert(table_name.clone(), table);
        }

        let mut work = Vec::new();
        let mut seen = BTreeSet::new();
        for (table_name, selector) in &targets {
            let Some(table) = snapshot.tables.get(table_name).cloned() else {
                continue;
            };
            reject_direct_view_write(snapshot, table_name)?;
            if table.drop {
                return Err(FastDbError::Constraint(format!(
                    "table {table_name:?} is DROP and rejects {}",
                    if upsert { "UPSERT" } else { "UPDATE" }
                )));
            }
            let candidates = read_candidates(
                conn,
                snapshot,
                &table,
                CandidateReadOptions {
                    id: selector.id(),
                    range: selector.range(),
                    condition: statement.condition.as_ref(),
                    params,
                    allow_cache: false,
                    fts: None,
                    vector: None,
                },
            )?;
            let mut matched = Vec::new();
            for candidate in candidates {
                if matches_condition(statement.condition.as_ref(), &candidate, params, snapshot)? {
                    matched.push(candidate);
                }
            }
            if matched.is_empty() && upsert {
                if table.kind == TableKind::Relation {
                    return Err(FastDbError::Schema(
                        "UPSERT cannot create a relation without immutable in/out endpoints".into(),
                    ));
                }
                let id = match selector {
                    TargetSelector::All => RecordIdValue::Uuid(uuid::Uuid::now_v7()),
                    TargetSelector::Record(id) => id.clone(),
                    TargetSelector::Range(_) => {
                        return Err(FastDbError::Schema(
                            "UPSERT cannot create a record from a record-range target".into(),
                        ))
                    }
                };
                let encoded = encode_rid(&id)?;
                if seen.insert((table_name.clone(), encoded)) {
                    work.push(UpdateWork::Missing(table_name.clone(), id));
                }
                continue;
            }
            for candidate in matched {
                if seen.insert((table_name.clone(), candidate.encoded_rid.clone())) {
                    work.push(UpdateWork::Existing(
                        table_name.clone(),
                        Box::new(candidate),
                    ));
                }
            }
        }
        if statement.only.is_some() && work.len() > 1 {
            return Err(FastDbError::Schema(
                "UPDATE/UPSERT ONLY matched more than one record".into(),
            ));
        }

        Ok(work)
    })?;
    conn.check_failpoint(Failpoint::BeforeUpdateMutations)?;
    let mut outcomes = Vec::with_capacity(work.len());
    for item in work {
        let outcome = with_create_mutation(conn, execution, false, |state| {
            let snapshot = ensure_snapshot(conn, state)?;
            let functions = snapshot.functions.clone();
            let (
                table_name,
                id,
                endpoints,
                before,
                previous_document,
                mut document,
                encoded_rid,
                existing,
            ) = match item {
                UpdateWork::Existing(table_name, candidate) => {
                    let document = apply_update_data(
                        &statement.data,
                        candidate.document.clone(),
                        &candidate.id,
                        candidate.endpoints.as_ref().map(|(from, to)| (from, to)),
                        params,
                    )?;
                    (
                        table_name,
                        candidate.id.clone(),
                        candidate.endpoints.clone(),
                        full_candidate_value(&candidate),
                        Some(candidate.document.clone()),
                        document,
                        candidate.encoded_rid,
                        true,
                    )
                }
                UpdateWork::Missing(table_name, id_value) => {
                    let id = RecordId::new(&table_name, id_value.clone());
                    let document =
                        apply_update_data(&statement.data, BTreeMap::new(), &id, None, params)?;
                    (
                        table_name,
                        id,
                        None,
                        Value::Null,
                        None,
                        document,
                        encode_rid(&id_value)?,
                        false,
                    )
                }
            };
            let table = snapshot.tables.get(&table_name).cloned().ok_or_else(|| {
                FastDbError::Schema(format!(
                    "table {table_name:?} was removed during UPDATE/UPSERT"
                ))
            })?;
            let event_input = Value::Object(document.clone());
            if table.kind == TableKind::Relation {
                reject_stored_edge_fields(&document)?;
            } else {
                reject_stored_id(&document)?;
            }
            normalize_schema_document(
                &table,
                &mut document,
                previous_document.as_ref(),
                &id,
                endpoints.as_ref().map(|(from, to)| (from, to)),
                params,
                &functions,
                !existing,
            )?;
            validate_index_values(&table, &document)?;
            let derived_hidden = derived_hidden_values(snapshot, &table, &document)?;
            let (update, bindings) = if existing {
                lower::physical_update_document_with_hidden_stmt(
                    &table.physical_name,
                    &encoded_rid,
                    &decode::encode_doc(&document)?,
                    &derived_hidden,
                )?
            } else {
                lower::physical_insert_document_with_hidden_stmt(
                    &table.physical_name,
                    &encoded_rid,
                    &decode::encode_doc(&document)?,
                    &derived_hidden,
                )?
            };
            contextual_constraint(
                conn.exec_bound(update, bindings),
                "UPDATE/UPSERT violates a declared unique index",
            )?;
            conn.check_failpoint(Failpoint::AfterUpdateMutation)?;
            let after = match &endpoints {
                Some((from, to)) => full_edge_value(&id, from, to, &document),
                None => full_record_value(&id, &document),
            };
            Ok((
                if existing { "UPDATE" } else { "CREATE" },
                table_name,
                id.clone(),
                endpoints,
                before,
                after,
                event_input,
            ))
        })?;
        mark_fts_dirty(execution, &outcome.1);
        run_table_events(
            conn,
            execution,
            EventInvocation {
                table_name: &outcome.1,
                kind: outcome.0,
                id: &outcome.2,
                before: outcome.4.clone(),
                after: outcome.5.clone(),
                input: outcome.6.clone(),
            },
            script,
        )?;
        outcomes.push(outcome);
    }
    let mutation_count = outcomes.len();
    let rows = outcomes
        .into_iter()
        .filter_map(|(_, _, id, endpoints, before, after, _)| {
            mutation_return(
                statement.return_clause.as_ref(),
                &before,
                &after,
                &id,
                endpoints.as_ref().map(|(from, to)| (from, to)),
                params,
            )
            .transpose()
        })
        .collect::<Result<Vec<_>>>()?;
    let result = if statement.only.is_some() {
        StatementResult::Value(rows.into_iter().next().unwrap_or(Value::Null))
    } else {
        StatementResult::Rows(rows)
    };
    StatementExecution::mutation(result, mutation_count)
}

fn run_delete(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DeleteStatement,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<StatementExecution> {
    let _timeout = StatementTimeoutGuard::install(conn, statement.timeout.as_ref(), params)?;
    let targets = resolve_mutation_targets(statement.target.clone(), params)?;
    let selected = data_mutation(conn, execution, || {
        let catalog = catalog_for_read(conn, execution)?;
        let Some(snapshot) = catalog.snapshot() else {
            return Ok(Vec::new());
        };
        let mut selected = Vec::new();
        let mut seen = BTreeSet::new();
        for (table_name, selector) in &targets {
            let Some(table) = snapshot.tables.get(table_name).cloned() else {
                continue;
            };
            reject_direct_view_write(snapshot, table_name)?;
            let candidates = read_candidates(
                conn,
                snapshot,
                &table,
                CandidateReadOptions {
                    id: selector.id(),
                    range: selector.range(),
                    condition: statement.condition.as_ref(),
                    params,
                    allow_cache: false,
                    fts: None,
                    vector: None,
                },
            )?;
            for candidate in candidates {
                if matches_condition(statement.condition.as_ref(), &candidate, params, snapshot)?
                    && seen.insert((table_name.clone(), candidate.encoded_rid.clone()))
                {
                    selected.push((table.clone(), candidate));
                }
            }
        }
        if statement.only.is_some() && selected.len() > 1 {
            return Err(FastDbError::Schema(
                "DELETE ONLY matched more than one record".into(),
            ));
        }
        Ok(selected)
    })?;
    conn.check_failpoint(Failpoint::BeforeDeleteMutations)?;
    let mut deleted = Vec::with_capacity(selected.len());
    let mut cascaded_count = 0_usize;
    for (selected_table, selected_candidate) in selected {
        let deleted_one = data_mutation(conn, execution, || {
            let catalog = catalog_for_read(conn, execution)?;
            let Some(snapshot) = catalog.snapshot() else {
                return Ok(None);
            };
            let Some(table) = snapshot.tables.get(&selected_table.logical_name).cloned() else {
                return Ok(None);
            };
            let Some(candidate) = read_candidates(
                conn,
                snapshot,
                &table,
                CandidateReadOptions {
                    id: Some(&selected_candidate.id.id),
                    range: None,
                    condition: None,
                    params,
                    allow_cache: false,
                    fts: None,
                    vector: None,
                },
            )?
            .into_iter()
            .next() else {
                return Ok(None);
            };
            let mut connected_edges = BTreeMap::new();
            if table.kind == TableKind::Normal {
                for relation in snapshot
                    .tables
                    .values()
                    .filter(|table| table.kind == TableKind::Relation)
                {
                    for encoded_edge in
                        connected_edge_ids(conn, snapshot, relation, &table, &candidate.id.id)?
                    {
                        let edge_id = decode_rid(&encoded_edge)?;
                        let Some(edge) = read_candidates(
                            conn,
                            snapshot,
                            relation,
                            CandidateReadOptions {
                                id: Some(&edge_id),
                                range: None,
                                condition: None,
                                params,
                                allow_cache: false,
                                fts: None,
                                vector: None,
                            },
                        )?
                        .into_iter()
                        .next() else {
                            return Err(FastDbError::format(
                                "graph adjacency points to a missing relation record",
                            ));
                        };
                        connected_edges
                            .entry((relation.physical_name.clone(), encoded_edge))
                            .or_insert_with(|| (relation.clone(), edge));
                    }
                }
            }
            for (physical_table, encoded_edge) in connected_edges.keys() {
                let (delete, bindings) =
                    lower::physical_delete_by_rid_stmt(physical_table, encoded_edge)?;
                conn.exec_bound(delete, bindings)?;
                conn.check_failpoint(Failpoint::AfterDeleteMutation)?;
            }
            let (delete, bindings) =
                lower::physical_delete_by_rid_stmt(&table.physical_name, &candidate.encoded_rid)?;
            conn.exec_bound(delete, bindings)?;
            conn.check_failpoint(Failpoint::AfterDeleteMutation)?;
            Ok(Some((
                table,
                candidate,
                connected_edges.into_values().collect::<Vec<_>>(),
            )))
        })?;
        let Some((table, candidate, cascaded_edges)) = deleted_one else {
            continue;
        };
        for (relation, edge) in &cascaded_edges {
            mark_fts_dirty(execution, &relation.logical_name);
            run_table_events(
                conn,
                execution,
                EventInvocation {
                    table_name: &relation.logical_name,
                    kind: "DELETE",
                    id: &edge.id,
                    before: full_candidate_value(edge),
                    after: Value::Null,
                    input: Value::Null,
                },
                script,
            )?;
        }
        cascaded_count = cascaded_count
            .checked_add(cascaded_edges.len())
            .ok_or_else(|| FastDbError::Engine("cascade mutation count overflowed usize".into()))?;
        mark_fts_dirty(execution, &table.logical_name);
        run_table_events(
            conn,
            execution,
            EventInvocation {
                table_name: &table.logical_name,
                kind: "DELETE",
                id: &candidate.id,
                before: full_candidate_value(&candidate),
                after: Value::Null,
                input: Value::Null,
            },
            script,
        )?;
        deleted.push((table, candidate));
    }
    let mutation_count = deleted
        .len()
        .checked_add(cascaded_count)
        .ok_or_else(|| FastDbError::Engine("cascade mutation count overflowed usize".into()))?;
    if statement.return_clause.is_some() {
        let rows = deleted
            .into_iter()
            .filter_map(|(_, candidate)| {
                let before = full_candidate_value(&candidate);
                mutation_return(
                    statement.return_clause.as_ref(),
                    &before,
                    &Value::Null,
                    &candidate.id,
                    candidate.endpoints.as_ref().map(|(from, to)| (from, to)),
                    params,
                )
                .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        let result = if statement.only.is_some() {
            StatementResult::Value(rows.into_iter().next().unwrap_or(Value::Null))
        } else {
            StatementResult::Rows(rows)
        };
        StatementExecution::mutation(result, mutation_count)
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
            &encode_rid(endpoint_id)?,
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
) -> Result<Vec<(Vec<String>, AssignmentOperator, EvalValue)>> {
    assignments
        .iter()
        .map(|assignment| {
            Ok((
                assignment_path(&assignment.path)?,
                assignment.operator.value,
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

fn apply_update_data(
    data: &UpdateData,
    mut document: BTreeMap<String, Value>,
    id: &RecordId,
    endpoints: Option<(&RecordId, &RecordId)>,
    params: &Params,
) -> Result<BTreeMap<String, Value>> {
    let source_document = document.clone();
    let context = EvalContext {
        document: &source_document,
        id,
        endpoints,
        params,
        functions: None,
        function_calls: None,
        function_depth: 0,
    };
    match data {
        UpdateData::Content(expression) | UpdateData::Replace(expression) => {
            let value = eval::evaluate(expression, &context)?.into_projection();
            let Value::Object(replacement) = value else {
                return Err(FastDbError::Schema(
                    "CONTENT and REPLACE require an object".into(),
                ));
            };
            document = replacement;
        }
        UpdateData::Merge(expression) => {
            let value = eval::evaluate(expression, &context)?.into_projection();
            let Value::Object(values) = value else {
                return Err(FastDbError::Schema("MERGE requires an object".into()));
            };
            merge_objects(&mut document, values, 0)?;
        }
        UpdateData::Patch(expression) => {
            let patch = eval::evaluate(expression, &context)?.into_projection();
            let patched = crate::value_functions::patch(&Value::Object(document), &patch)?;
            let Value::Object(patched) = patched else {
                return Err(FastDbError::Schema(
                    "PATCH must leave the record as an object".into(),
                ));
            };
            document = patched;
        }
        UpdateData::Set(assignments) => {
            let evaluated = evaluate_assignments(assignments, &context)?;
            apply_assignments(&mut document, evaluated)?;
        }
        UpdateData::Unset(paths) => {
            for path in paths {
                let path = assignment_path(path)?;
                crate::path::remove_path(&mut document, &path)?;
            }
        }
    }
    Ok(document)
}

fn merge_objects(
    destination: &mut BTreeMap<String, Value>,
    source: BTreeMap<String, Value>,
    depth: usize,
) -> Result<()> {
    if depth > 64 {
        return Err(FastDbError::ResourceLimit(
            "MERGE nesting exceeds 64 levels".into(),
        ));
    }
    for (key, value) in source {
        match (destination.get_mut(&key), value) {
            (Some(Value::Object(destination)), Value::Object(source)) => {
                merge_objects(destination, source, depth + 1)?;
            }
            (_, value) => {
                destination.insert(key, value);
            }
        }
    }
    Ok(())
}

fn mutation_return(
    clause: Option<&turso_fastdb_parser::ReturnClause>,
    before: &Value,
    after: &Value,
    id: &RecordId,
    endpoints: Option<(&RecordId, &RecordId)>,
    params: &Params,
) -> Result<Option<Value>> {
    match clause.map(|clause| &clause.kind.value) {
        Some(ReturnKind::None) => Ok(None),
        Some(ReturnKind::Before) => Ok(Some(before.clone())),
        Some(ReturnKind::Diff) => crate::value_functions::diff(before, after).map(Some),
        Some(ReturnKind::Value(expression)) => {
            let source = if matches!(after, Value::Object(_)) {
                after
            } else {
                before
            };
            let mut document = match source {
                Value::Object(document) => document.clone(),
                _ => BTreeMap::new(),
            };
            document.remove("id");
            document.remove("in");
            document.remove("out");
            let context = EvalContext {
                document: &document,
                id,
                endpoints,
                params,
                functions: None,
                function_calls: None,
                function_depth: 0,
            };
            Ok(Some(
                eval::evaluate(expression, &context)?.into_projection(),
            ))
        }
        Some(ReturnKind::After) | None => Ok(Some(after.clone())),
    }
}

fn apply_assignments(
    document: &mut BTreeMap<String, Value>,
    assignments: Vec<(Vec<String>, AssignmentOperator, EvalValue)>,
) -> Result<()> {
    for (path, operator, value) in assignments {
        let value = match operator {
            AssignmentOperator::Set => value,
            AssignmentOperator::Add | AssignmentOperator::Subtract => {
                apply_compound_assignment(document, &path, operator, value)?
            }
        };
        match value {
            EvalValue::Missing => crate::path::remove_path(document, &path)?,
            EvalValue::Present(value) => crate::path::set_path(document, &path, value)?,
        }
    }
    Ok(())
}

fn apply_compound_assignment(
    document: &BTreeMap<String, Value>,
    path: &[String],
    operator: AssignmentOperator,
    right: EvalValue,
) -> Result<EvalValue> {
    let left = crate::path::get_path(document, path).cloned();
    let EvalValue::Present(right_value) = &right else {
        return Ok(EvalValue::Missing);
    };
    if let Some(Value::Array(left)) = left.as_ref() {
        return match operator {
            AssignmentOperator::Add => {
                let mut output = left.clone();
                match right_value {
                    Value::Array(values) => output.extend(values.iter().cloned()),
                    value => output.push(value.clone()),
                }
                if output.len() > 65_536 {
                    return Err(FastDbError::ResourceLimit(
                        "compound array assignment exceeds 65,536 elements".into(),
                    ));
                }
                Ok(EvalValue::Present(Value::Array(output)))
            }
            AssignmentOperator::Subtract => Ok(EvalValue::Present(Value::Array(
                left.iter()
                    .filter(|value| *value != right_value)
                    .cloned()
                    .collect(),
            ))),
            AssignmentOperator::Set => unreachable!(),
        };
    }
    if left.is_none() && operator == AssignmentOperator::Add {
        return Ok(right);
    }
    eval::evaluate_binary(
        match operator {
            AssignmentOperator::Add => BinaryOperator::Add,
            AssignmentOperator::Subtract => BinaryOperator::Subtract,
            AssignmentOperator::Set => unreachable!(),
        },
        left.map_or(EvalValue::Missing, EvalValue::Present),
        right,
    )
}

#[allow(clippy::too_many_arguments)]
fn project_candidate(
    conn: &Connection,
    execution: &mut ExecutionState,
    snapshot: &CatalogSnapshot,
    candidate: &Candidate,
    projections: &ProjectionList,
    include_all: bool,
    params: &Params,
    script: &mut ScriptRuntime,
) -> Result<Value> {
    if matches!(projections, ProjectionList::All(_)) {
        return Ok(full_candidate_value(candidate));
    }
    let ProjectionList::Fields(projections) = projections else {
        unreachable!()
    };
    let mut object = if include_all {
        match full_candidate_value(candidate) {
            Value::Object(object) => object,
            _ => unreachable!("full candidate is an object"),
        }
    } else {
        BTreeMap::new()
    };
    for projection in projections {
        let value = evaluate_projection_expression(
            conn,
            execution,
            snapshot,
            candidate,
            &projection.expression,
            params,
            script,
        )?;
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

fn evaluate_special_projection(
    expression: &Expr,
    candidate: &Candidate,
    params: &Params,
) -> Result<Value> {
    let ExprKind::FunctionCall { name, arguments } = &expression.kind else {
        unreachable!("caller filters function calls");
    };
    if function_name_is(name, &["vector", "distance", "knn"]) {
        if !arguments.is_empty() {
            return Err(FastDbError::Schema(
                "vector::distance::knn requires no arguments".into(),
            ));
        }
        return candidate.vector_distance.map(Value::Float).ok_or_else(|| {
            FastDbError::Schema("vector::distance::knn requires a KNN predicate".into())
        });
    }
    if function_name_is(name, &["vector", "distance", "euclidean"])
        || function_name_is(name, &["vector", "similarity", "cosine"])
    {
        if arguments.len() != 2 {
            return Err(FastDbError::Schema(
                "vector distance/similarity functions require exactly two vectors".into(),
            ));
        }
        let left = projection_vector(&arguments[0], candidate, params)?;
        let right = projection_vector(&arguments[1], candidate, params)?;
        if left.len() != right.len() || left.is_empty() {
            return Err(FastDbError::Schema(
                "vector function arguments must have equal nonzero dimensions".into(),
            ));
        }
        let value = if function_name_is(name, &["vector", "distance", "euclidean"]) {
            left.iter()
                .zip(&right)
                .map(|(left, right)| (left - right).powi(2))
                .sum::<f64>()
                .sqrt()
        } else {
            let dot = left.iter().zip(&right).map(|(l, r)| l * r).sum::<f64>();
            let left_norm = left.iter().map(|value| value * value).sum::<f64>().sqrt();
            let right_norm = right.iter().map(|value| value * value).sum::<f64>().sqrt();
            if left_norm == 0.0 || right_norm == 0.0 {
                0.0
            } else {
                dot / (left_norm * right_norm)
            }
        };
        return Ok(Value::Float(value));
    }
    if function_name_is(name, &["search", "score"])
        || function_name_is(name, &["search", "highlight"])
        || function_name_is(name, &["fts_match"])
        || function_name_is(name, &["fts_score"])
        || function_name_is(name, &["fts_highlight"])
    {
        evaluate_fts_projection(expression, candidate, params)
    } else {
        Ok(eval::evaluate(expression, &candidate_context(candidate, params))?.into_projection())
    }
}

fn projection_vector(
    expression: &Expr,
    candidate: &Candidate,
    params: &Params,
) -> Result<Vec<f64>> {
    let value =
        eval::evaluate(expression, &candidate_context(candidate, params))?.into_projection();
    vector_values_from_value(&value, "vector function argument")
}

fn evaluate_fts_projection(
    expression: &Expr,
    candidate: &Candidate,
    params: &Params,
) -> Result<Value> {
    let ExprKind::FunctionCall { name, arguments } = &expression.kind else {
        unreachable!("caller filters function calls");
    };
    let fts = candidate.fts.as_ref().ok_or_else(|| {
        FastDbError::Schema("FTS projection function requires an indexed FTS predicate".into())
    })?;
    if function_name_is(name, &["search", "score"]) {
        if arguments.len() != 1 {
            return Err(FastDbError::Schema(
                "search::score requires exactly one match reference".into(),
            ));
        }
        return Ok(Value::Float(fts.score));
    }
    if function_name_is(name, &["search", "highlight"]) {
        if arguments.len() != 3 {
            return Err(FastDbError::Schema(
                "search::highlight requires before tag, after tag, and reference".into(),
            ));
        }
        let before = fts_projection_string(&arguments[0], candidate, params, "before tag")?;
        let after = fts_projection_string(&arguments[1], candidate, params, "after tag")?;
        let text =
            derive_fts_text(&candidate.document, &fts.query.index.paths[0])?.unwrap_or_default();
        if !fts.query.options.highlights {
            return Ok(Value::Str(text));
        }
        return Ok(Value::Str(highlight_blank(
            &text,
            &fts.query.query,
            &before,
            &after,
        )));
    }
    if function_name_is(name, &["fts_match"]) {
        validate_native_fts_call(arguments, fts, params)?;
        return Ok(Value::Bool(true));
    }
    if function_name_is(name, &["fts_score"]) {
        validate_native_fts_call(arguments, fts, params)?;
        return Ok(Value::Float(fts.score));
    }
    if function_name_is(name, &["fts_highlight"]) {
        if arguments.len() != 4 {
            return Err(FastDbError::Schema(
                "fts_highlight requires field, before tag, after tag, and query".into(),
            ));
        }
        let ExprKind::FieldPath(path) = &arguments[0].kind else {
            return Err(FastDbError::Schema(
                "fts_highlight first argument must be an indexed field".into(),
            ));
        };
        let path = crate::path::parser_path(path)?.0;
        if fts.query.index.paths.len() != 1 || fts.query.index.paths[0] != path {
            return Err(FastDbError::Schema(
                "fts_highlight field does not match the selected FTS index".into(),
            ));
        }
        let before = fts_projection_string(&arguments[1], candidate, params, "before tag")?;
        let after = fts_projection_string(&arguments[2], candidate, params, "after tag")?;
        let query = fts_query_string(&arguments[3], params)?;
        if query != fts.query.query {
            return Err(FastDbError::Schema(
                "fts_highlight query does not match the selected FTS predicate".into(),
            ));
        }
        let Some(text) = derive_fts_text(&candidate.document, &path)? else {
            return Ok(Value::Null);
        };
        #[cfg(not(target_family = "wasm"))]
        return Ok(Value::Str(turso_core::index_method::fts::fts_highlight(
            &text, &query, &before, &after,
        )));
        #[cfg(target_family = "wasm")]
        return Err(fts_unavailable());
    }
    Err(FastDbError::Schema(format!(
        "function {} is not an executable Phase 8 FTS projection",
        name.iter()
            .map(|segment| segment.value.as_str())
            .collect::<Vec<_>>()
            .join("::")
    )))
}

fn fts_projection_string(
    expression: &Expr,
    candidate: &Candidate,
    params: &Params,
    label: &str,
) -> Result<String> {
    match eval::evaluate(expression, &candidate_context(candidate, params))?.into_projection() {
        Value::Str(value) => Ok(value),
        _ => Err(FastDbError::Schema(format!("FTS {label} must be a string"))),
    }
}

fn validate_native_fts_call(
    arguments: &[Expr],
    fts: &FtsCandidateContext,
    params: &Params,
) -> Result<()> {
    if arguments.len() < 2 {
        return Err(FastDbError::Schema(
            "native FTS call requires fields followed by a query".into(),
        ));
    }
    let paths = arguments[..arguments.len() - 1]
        .iter()
        .map(|argument| {
            let ExprKind::FieldPath(path) = &argument.kind else {
                return Err(FastDbError::Schema(
                    "native FTS field arguments must be paths".into(),
                ));
            };
            Ok(crate::path::parser_path(path)?.0)
        })
        .collect::<Result<Vec<_>>>()?;
    let query = fts_query_string(arguments.last().expect("length checked"), params)?;
    if paths != fts.query.index.paths || query != fts.query.query {
        return Err(FastDbError::Schema(
            "native FTS projection does not match the selected index/query".into(),
        ));
    }
    Ok(())
}

fn highlight_blank(text: &str, query: &str, before: &str, after: &str) -> String {
    let terms = query.split_whitespace().collect::<BTreeSet<_>>();
    let mut output = String::with_capacity(text.len());
    for piece in text.split_inclusive(char::is_whitespace) {
        let token_end = piece.find(char::is_whitespace).unwrap_or(piece.len());
        let (token, whitespace) = piece.split_at(token_end);
        if !token.is_empty() && terms.contains(token) {
            output.push_str(before);
            output.push_str(token);
            output.push_str(after);
        } else {
            output.push_str(token);
        }
        output.push_str(whitespace);
    }
    output
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
        &encode_rid(&endpoint.id)?,
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
    let encoded = encode_rid(&endpoint.id)?;
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
    snapshot: &CatalogSnapshot,
) -> Result<bool> {
    let Some(condition) = condition else {
        return Ok(true);
    };
    let calls = std::cell::Cell::new(0);
    Ok(eval::evaluate(
        condition,
        &EvalContext {
            document: &candidate.document,
            id: &candidate.id,
            endpoints: candidate.endpoints.as_ref().map(|(from, to)| (from, to)),
            params,
            functions: Some(&snapshot.functions),
            function_calls: Some(&calls),
            function_depth: 0,
        },
    )?
    .truthy())
}

fn resolve_vector_query(
    select: &turso_fastdb_parser::SelectStatement,
    snapshot: &CatalogSnapshot,
    table: &TableDefinition,
    params: &Params,
) -> Result<Option<ResolvedVectorQuery>> {
    let Some(condition) = select.condition.as_ref() else {
        return Ok(None);
    };
    let mut predicates = Vec::new();
    collect_knn_predicates(condition, &mut predicates)?;
    if predicates.is_empty() {
        return Ok(None);
    }
    if predicates.len() != 1 {
        return Err(FastDbError::Schema(
            "Phase 9 permits exactly one KNN predicate per SELECT".into(),
        ));
    }
    validate_knn_prefilters(condition, params, table)?;
    let ExprKind::Knn(knn) = &predicates[0].kind else {
        unreachable!("collector returns KNN predicates");
    };
    let ExprKind::FieldPath(path) = &knn.field.kind else {
        return Err(FastDbError::Schema(
            "the left side of a KNN predicate must be a vector field path".into(),
        ));
    };
    let (_, path_key) = crate::path::parser_path(path)?;
    let field = table
        .fields
        .get(&path_key)
        .ok_or_else(|| FastDbError::Schema(format!("KNN field {path_key} is not declared")))?;
    let dimension = field.ty.vector_dimension().ok_or_else(|| {
        FastDbError::Schema(format!("KNN field {path_key} is not array<float,N>"))
    })?;
    let query_values = vector_query_values(&knn.query, params)?;
    if query_values.len() != dimension as usize {
        return Err(FastDbError::Schema(format!(
            "KNN query requires exactly {dimension} elements"
        )));
    }
    if knn.metric.value == turso_fastdb_parser::KnnMetric::Cosine
        && query_values.iter().all(|value| *value == 0.0)
    {
        return Err(FastDbError::Schema(
            "COSINE KNN query vector must have nonzero magnitude".into(),
        ));
    }
    let mut matches = snapshot
        .hidden_columns
        .values()
        .filter(|column| {
            column.table_id == table.id
                && matches!(column.role, catalog::HiddenColumnRole::Vector64(_))
                && column.field_path_key.as_deref() == Some(path_key.as_str())
                && column.dimension == Some(i64::from(dimension))
        })
        .cloned()
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(FastDbError::format(
            "KNN field does not own exactly one native vector column",
        ));
    }
    let document = BTreeMap::from([(
        "query".to_string(),
        Value::Array(query_values.iter().copied().map(Value::Float).collect()),
    )]);
    let query =
        derive_vector64(&document, &["query".to_string()], dimension)?.ok_or_else(|| {
            FastDbError::Engine("KNN query encoding unexpectedly produced null".into())
        })?;
    Ok(Some(ResolvedVectorQuery {
        column: matches.pop().expect("one match"),
        query,
        k: knn.k.value,
        metric: knn.metric.value,
        needs_document: vector_select_needs_document(select),
    }))
}

fn vector_select_needs_document(select: &turso_fastdb_parser::SelectStatement) -> bool {
    match &select.projections {
        ProjectionList::All(_) => true,
        ProjectionList::Fields(projections) => projections
            .iter()
            .any(|projection| expression_needs_document(&projection.expression)),
    }
}

fn expression_needs_document(expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::FieldPath(path) => path.segments.len() != 1 || path.segments[0].value != "id",
        ExprKind::FunctionCall { name, arguments }
            if function_name_is(name, &["vector", "distance", "knn"]) =>
        {
            !arguments.is_empty()
        }
        ExprKind::FunctionCall { arguments, .. } | ExprKind::Array(arguments) => {
            arguments.iter().any(expression_needs_document)
        }
        ExprKind::Object(fields) => fields
            .iter()
            .any(|field| expression_needs_document(&field.value)),
        ExprKind::Access { target, accessor } => {
            expression_needs_document(target)
                || match accessor {
                    turso_fastdb_parser::Accessor::Field(_)
                    | turso_fastdb_parser::Accessor::Last(_) => false,
                    turso_fastdb_parser::Accessor::Index(index) => expression_needs_document(index),
                    turso_fastdb_parser::Accessor::Slice { start, end, .. } => start
                        .iter()
                        .chain(end.iter())
                        .any(|bound| expression_needs_document(bound)),
                }
        }
        ExprKind::Cast { value, .. } => expression_needs_document(value),
        ExprKind::Range(range) => range
            .start
            .iter()
            .chain(range.end.iter())
            .any(|bound| expression_needs_document(bound)),
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => {
            expression_needs_document(operand)
        }
        ExprKind::Binary { left, right, .. } => {
            expression_needs_document(left) || expression_needs_document(right)
        }
        ExprKind::Knn(_) | ExprKind::Traversal(_) => false,
        _ => false,
    }
}

fn validate_knn_prefilters(
    expression: &Expr,
    params: &Params,
    table: &TableDefinition,
) -> Result<()> {
    match &expression.kind {
        ExprKind::Knn(_) => Ok(()),
        ExprKind::Parenthesized(inner) => validate_knn_prefilters(inner, params, table),
        ExprKind::Binary {
            left,
            operator,
            right,
        } if operator.value == BinaryOperator::And => {
            validate_knn_prefilters(left, params, table)?;
            validate_knn_prefilters(right, params, table)
        }
        _ if safe_pushdowns(expression, params, table).len() == 1 => Ok(()),
        _ => Err(FastDbError::Schema(
            "KNN ordinary predicates must be scalar comparisons that can run before top-k".into(),
        )),
    }
}

fn collect_knn_predicates<'a>(expression: &'a Expr, found: &mut Vec<&'a Expr>) -> Result<()> {
    match &expression.kind {
        ExprKind::Parenthesized(inner) => collect_knn_predicates(inner, found),
        ExprKind::Binary {
            left,
            operator,
            right,
        } if operator.value == BinaryOperator::And => {
            collect_knn_predicates(left, found)?;
            collect_knn_predicates(right, found)
        }
        ExprKind::Knn(_) => {
            found.push(expression);
            Ok(())
        }
        _ if contains_knn_predicate(expression) => Err(FastDbError::Schema(
            "KNN predicates must be top-level AND conjuncts".into(),
        )),
        _ => Ok(()),
    }
}

fn contains_knn_predicate(expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::Knn(_) => true,
        ExprKind::Binary { left, right, .. } => {
            contains_knn_predicate(left) || contains_knn_predicate(right)
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => {
            contains_knn_predicate(operand)
        }
        ExprKind::FunctionCall { arguments, .. } | ExprKind::Array(arguments) => {
            arguments.iter().any(contains_knn_predicate)
        }
        ExprKind::Object(fields) => fields
            .iter()
            .any(|field| contains_knn_predicate(&field.value)),
        _ => false,
    }
}

fn vector_query_values(expression: &Expr, params: &Params) -> Result<Vec<f64>> {
    let value = match &expression.kind {
        ExprKind::Parameter(name) => params
            .get(name)
            .cloned()
            .ok_or_else(|| FastDbError::Schema(format!("missing value for parameter ${name}")))?,
        ExprKind::Array(elements) => Value::Array(
            elements
                .iter()
                .map(vector_literal_value)
                .collect::<Result<Vec<_>>>()?,
        ),
        _ => {
            return Err(FastDbError::Schema(
                "KNN query must be a vector literal or bound vector parameter".into(),
            ))
        }
    };
    vector_values_from_value(&value, "KNN query")
}

fn vector_literal_value(expression: &Expr) -> Result<Value> {
    match &expression.kind {
        ExprKind::Integer(value) => Ok(Value::Integer(*value)),
        ExprKind::Float(value) => Ok(Value::Float(*value)),
        ExprKind::Parenthesized(inner) => vector_literal_value(inner),
        ExprKind::Unary { operator, operand }
            if matches!(
                operator.value,
                turso_fastdb_parser::UnaryOperator::Plus
                    | turso_fastdb_parser::UnaryOperator::Minus
            ) =>
        {
            let value = vector_literal_value(operand)?;
            if operator.value == turso_fastdb_parser::UnaryOperator::Plus {
                return Ok(value);
            }
            match value {
                Value::Integer(value) => value
                    .checked_neg()
                    .map(Value::Integer)
                    .ok_or_else(|| FastDbError::Schema("vector integer literal overflow".into())),
                Value::Float(value) => Ok(Value::Float(-value)),
                _ => unreachable!("recursive vector literal returns numeric values"),
            }
        }
        _ => Err(FastDbError::Schema(
            "KNN vector literals may contain only numbers".into(),
        )),
    }
}

fn vector_values_from_value(value: &Value, label: &str) -> Result<Vec<f64>> {
    let Value::Array(elements) = value else {
        return Err(FastDbError::Schema(format!("{label} must be an array")));
    };
    if elements.is_empty() || elements.len() > 65_536 {
        return Err(FastDbError::Schema(format!(
            "{label} dimension must be between 1 and 65536"
        )));
    }
    elements
        .iter()
        .map(|element| {
            let value = match element {
                Value::Integer(value) => *value as f64,
                Value::Float(value) => *value,
                _ => {
                    return Err(FastDbError::Schema(format!(
                        "{label} elements must be finite numbers"
                    )))
                }
            };
            if !value.is_finite() {
                return Err(FastDbError::Schema(format!(
                    "{label} elements must be finite numbers"
                )));
            }
            Ok(value)
        })
        .collect()
}

fn resolve_fts_query(
    select: &turso_fastdb_parser::SelectStatement,
    table: &TableDefinition,
    params: &Params,
) -> Result<Option<ResolvedFtsQuery>> {
    let Some(condition) = select.condition.as_ref() else {
        return Ok(None);
    };
    let mut predicates = Vec::new();
    collect_fts_predicates(condition, &mut predicates)?;
    if predicates.is_empty() {
        return Ok(None);
    }
    ensure_fts_available()?;
    if predicates.len() != 1 {
        return Err(FastDbError::Schema(
            "Phase 8 permits exactly one FTS predicate per SELECT".into(),
        ));
    }
    let predicate = predicates[0];
    let (paths, query_expression, reference, surface) = match &predicate.kind {
        ExprKind::Binary {
            left,
            operator,
            right,
        } if matches!(operator.value, BinaryOperator::FtsMatch(_)) => {
            let ExprKind::FieldPath(path) = &left.kind else {
                return Err(FastDbError::Schema(
                    "the left side of an FTS match must be a field path".into(),
                ));
            };
            let (path, _) = crate::path::parser_path(path)?;
            let BinaryOperator::FtsMatch(reference) = operator.value else {
                unreachable!("guarded FTS operator");
            };
            (
                vec![path],
                right.as_ref(),
                reference.unwrap_or(0),
                "surreal",
            )
        }
        ExprKind::FunctionCall { name, arguments } if function_name_is(name, &["fts_match"]) => {
            if arguments.len() < 2 {
                return Err(FastDbError::Schema(
                    "fts_match requires indexed fields followed by a query".into(),
                ));
            }
            let mut paths = Vec::new();
            for argument in &arguments[..arguments.len() - 1] {
                let ExprKind::FieldPath(path) = &argument.kind else {
                    return Err(FastDbError::Schema(
                        "fts_match indexed arguments must be field paths".into(),
                    ));
                };
                paths.push(crate::path::parser_path(path)?.0);
            }
            (
                paths,
                arguments.last().expect("length checked"),
                0,
                "fastdb",
            )
        }
        _ => unreachable!("collector returns supported predicate shapes"),
    };
    let query = fts_query_string(query_expression, params)?;
    if query.len() > 65_536 {
        return Err(FastDbError::Schema(
            "FTS query exceeds the Phase 8 65536-byte limit".into(),
        ));
    }
    let path_keys = paths
        .iter()
        .map(crate::path::canonical_path)
        .collect::<Result<Vec<_>>>()?;
    let mut matches = table
        .indexes
        .values()
        .filter(|index| index.kind == IndexKind::Fts && index.path_keys == path_keys)
        .filter_map(|index| {
            catalog::FtsIndexOptions::parse_canonical(&index.options_json)
                .ok()
                .filter(|options| options.surface == surface)
                .map(|options| (index, options))
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(FastDbError::Schema(format!(
            "FTS predicate resolves to {} matching {surface} indexes",
            matches.len()
        )));
    }
    let (index, options) = matches.pop().expect("one match");
    validate_fts_projection_calls(&select.projections, reference, surface)?;
    Ok(Some(ResolvedFtsQuery {
        index: index.clone(),
        options,
        query,
    }))
}

fn collect_fts_predicates<'a>(expression: &'a Expr, found: &mut Vec<&'a Expr>) -> Result<()> {
    match &expression.kind {
        ExprKind::Parenthesized(inner) => collect_fts_predicates(inner, found),
        ExprKind::Binary {
            operator,
            left,
            right,
        } if operator.value == BinaryOperator::And => {
            collect_fts_predicates(left, found)?;
            collect_fts_predicates(right, found)
        }
        ExprKind::Binary { operator, .. }
            if matches!(operator.value, BinaryOperator::FtsMatch(_)) =>
        {
            found.push(expression);
            Ok(())
        }
        ExprKind::FunctionCall { name, .. } if function_name_is(name, &["fts_match"]) => {
            found.push(expression);
            Ok(())
        }
        ExprKind::Binary { operator, .. } if operator.value == BinaryOperator::Or => {
            if contains_fts_predicate(expression) {
                Err(FastDbError::Schema(
                    "FTS predicates under OR are outside the Phase 8 subset".into(),
                ))
            } else {
                Ok(())
            }
        }
        ExprKind::Unary { .. } => {
            if contains_fts_predicate(expression) {
                Err(FastDbError::Schema(
                    "FTS predicates under NOT are outside the Phase 8 subset".into(),
                ))
            } else {
                Ok(())
            }
        }
        _ => {
            if contains_fts_predicate(expression) {
                Err(FastDbError::Schema(
                    "FTS predicate nesting is outside the Phase 8 subset".into(),
                ))
            } else {
                Ok(())
            }
        }
    }
}

fn contains_fts_predicate(expression: &Expr) -> bool {
    match &expression.kind {
        ExprKind::Binary {
            left,
            operator,
            right,
        } => {
            matches!(operator.value, BinaryOperator::FtsMatch(_))
                || contains_fts_predicate(left)
                || contains_fts_predicate(right)
        }
        ExprKind::FunctionCall { name, arguments } => {
            function_name_is(name, &["fts_match"]) || arguments.iter().any(contains_fts_predicate)
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => {
            contains_fts_predicate(operand)
        }
        ExprKind::Array(values) => values.iter().any(contains_fts_predicate),
        ExprKind::Object(fields) => fields
            .iter()
            .any(|field| contains_fts_predicate(&field.value)),
        _ => false,
    }
}

fn fts_query_string(expression: &Expr, params: &Params) -> Result<String> {
    match &expression.kind {
        ExprKind::String(value) => Ok(value.clone()),
        ExprKind::Parameter(name) => match params.get(name) {
            Some(Value::Str(value)) => Ok(value.clone()),
            Some(_) => Err(FastDbError::Schema(format!(
                "FTS query parameter ${name} must be a string"
            ))),
            None => Err(FastDbError::Schema(format!(
                "missing value for parameter ${name}"
            ))),
        },
        _ => Err(FastDbError::Schema(
            "FTS query must be a string literal or bound string parameter".into(),
        )),
    }
}

fn function_name_is(name: &[turso_fastdb_parser::Identifier], expected: &[&str]) -> bool {
    name.len() == expected.len()
        && name
            .iter()
            .zip(expected)
            .all(|(actual, expected)| actual.value.eq_ignore_ascii_case(expected))
}

fn validate_fts_projection_calls(
    projections: &ProjectionList,
    reference: u32,
    surface: &str,
) -> Result<()> {
    let ProjectionList::Fields(projections) = projections else {
        return Ok(());
    };
    for projection in projections {
        validate_fts_projection_expression(&projection.expression, reference, surface)?;
    }
    Ok(())
}

fn validate_fts_projection_expression(
    expression: &Expr,
    reference: u32,
    surface: &str,
) -> Result<()> {
    match &expression.kind {
        ExprKind::FunctionCall { name, arguments }
            if function_name_is(name, &["search", "score"])
                || function_name_is(name, &["search", "highlight"]) =>
        {
            if surface != "surreal" {
                return Err(FastDbError::Schema(
                    "search::* functions require a Surreal FTS predicate".into(),
                ));
            }
            let reference_argument = arguments.last().ok_or_else(|| {
                FastDbError::Schema("search function requires a match reference".into())
            })?;
            if !matches!(reference_argument.kind, ExprKind::Integer(value) if value >= 0 && value as u32 == reference)
            {
                return Err(FastDbError::Schema(
                    "search function reference does not match the FTS predicate".into(),
                ));
            }
            for argument in arguments {
                validate_fts_projection_expression(argument, reference, surface)?;
            }
            Ok(())
        }
        ExprKind::FunctionCall { arguments, .. } | ExprKind::Array(arguments) => {
            for argument in arguments {
                validate_fts_projection_expression(argument, reference, surface)?;
            }
            Ok(())
        }
        ExprKind::Object(fields) => {
            for field in fields {
                validate_fts_projection_expression(&field.value, reference, surface)?;
            }
            Ok(())
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => {
            validate_fts_projection_expression(operand, reference, surface)
        }
        ExprKind::Binary { left, right, .. } => {
            validate_fts_projection_expression(left, reference, surface)?;
            validate_fts_projection_expression(right, reference, surface)
        }
        _ => Ok(()),
    }
}

fn matches_condition_with_fts(
    conn: &Connection,
    execution: &mut ExecutionState,
    condition: Option<&Expr>,
    candidate: &Candidate,
    params: &Params,
    snapshot: &CatalogSnapshot,
    script: &mut ScriptRuntime,
) -> Result<bool> {
    let Some(condition) = condition else {
        return Ok(true);
    };
    match &condition.kind {
        ExprKind::Parenthesized(inner) => matches_condition_with_fts(
            conn,
            execution,
            Some(inner),
            candidate,
            params,
            snapshot,
            script,
        ),
        ExprKind::Binary {
            left,
            operator,
            right,
        } if operator.value == BinaryOperator::And => Ok(matches_condition_with_fts(
            conn,
            execution,
            Some(left),
            candidate,
            params,
            snapshot,
            script,
        )? && matches_condition_with_fts(
            conn,
            execution,
            Some(right),
            candidate,
            params,
            snapshot,
            script,
        )?),
        ExprKind::Binary { operator, .. }
            if matches!(operator.value, BinaryOperator::FtsMatch(_)) =>
        {
            let fts = candidate.fts.as_ref().ok_or_else(|| {
                FastDbError::Engine("FTS candidate is missing provider context".into())
            })?;
            Ok(fts.query.options.surface != "surreal"
                || fts
                    .query
                    .index
                    .paths
                    .first()
                    .and_then(|path| crate::path::get_path(&candidate.document, path))
                    .is_some_and(|value| {
                        matches!(value, Value::Str(text) if blank_matches(text, &fts.query.query))
                    }))
        }
        ExprKind::FunctionCall { name, .. } if function_name_is(name, &["fts_match"]) => Ok(true),
        ExprKind::Knn(_) => Ok(candidate.vector_distance.is_some()),
        _ => evaluate_projection_expression(
            conn, execution, snapshot, candidate, condition, params, script,
        )
        .map(|value| script_value_truthy(&value)),
    }
}

fn blank_matches(text: &str, query: &str) -> bool {
    let tokens = text.split_whitespace().collect::<BTreeSet<_>>();
    let query_tokens = query.split_whitespace().collect::<Vec<_>>();
    !query_tokens.is_empty() && query_tokens.into_iter().all(|term| tokens.contains(term))
}

fn candidate_context<'a>(candidate: &'a Candidate, params: &'a Params) -> EvalContext<'a> {
    EvalContext {
        document: &candidate.document,
        id: &candidate.id,
        endpoints: candidate.endpoints.as_ref().map(|(from, to)| (from, to)),
        params,
        functions: None,
        function_calls: None,
        function_depth: 0,
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

fn target_parts(target: Target) -> Result<(String, TargetSelector)> {
    match target {
        Target::Table(table) => Ok((table.name.value, TargetSelector::All)),
        Target::Record(record) => Ok((
            record.table.value,
            TargetSelector::Record(record_id_value(record.id)?),
        )),
        Target::RecordRange(range) => Ok((
            range.table.value,
            TargetSelector::Range(ResolvedRecordRange {
                start: range.start.map(record_id_value).transpose()?,
                end: range.end.map(record_id_value).transpose()?,
                inclusive: range.inclusive,
            }),
        )),
        Target::Batch { .. } => Err(FastDbError::Schema(
            "batch targets are valid only for CREATE".into(),
        )),
        Target::Expression(_) => Err(FastDbError::Schema(
            "expression targets must be resolved before physical access".into(),
        )),
    }
}

fn resolve_mutation_targets(
    target: Target,
    params: &Params,
) -> Result<Vec<(String, TargetSelector)>> {
    match target {
        Target::Expression(expression) => resolve_target_expression(&expression, params),
        target => target_parts(target).map(|target| vec![target]),
    }
}

fn resolve_target_expression(
    expression: &Expr,
    params: &Params,
) -> Result<Vec<(String, TargetSelector)>> {
    let document = BTreeMap::new();
    let id = RecordId::new("__target", "expression");
    let context = EvalContext {
        document: &document,
        id: &id,
        endpoints: None,
        params,
        functions: None,
        function_calls: None,
        function_depth: 0,
    };
    let value = eval::evaluate(expression, &context)?.into_projection();
    let mut targets = Vec::new();
    append_target_values(value, &mut targets)?;
    if targets.is_empty() {
        return Err(FastDbError::Schema(
            "mutation target expression must resolve to at least one table or record".into(),
        ));
    }
    if targets.len() > 10_000 {
        return Err(FastDbError::ResourceLimit(
            "mutation target expression exceeds 10,000 targets".into(),
        ));
    }
    Ok(targets)
}

fn append_target_values(value: Value, targets: &mut Vec<(String, TargetSelector)>) -> Result<()> {
    match value {
        Value::RecordId(record) => {
            targets.push((record.table, TargetSelector::Record(record.id)));
            Ok(())
        }
        Value::Table(table) => {
            targets.push((table.as_str().to_string(), TargetSelector::All));
            Ok(())
        }
        Value::Array(values) => {
            for value in values {
                if targets.len() >= 10_000 {
                    return Err(FastDbError::ResourceLimit(
                        "mutation target expression exceeds 10,000 targets".into(),
                    ));
                }
                append_target_values(value, targets)?;
            }
            Ok(())
        }
        _ => Err(FastDbError::Schema(
            "mutation target expressions may contain only tables, records, or arrays of them"
                .into(),
        )),
    }
}

fn select_target_parts(target: &SelectTarget) -> Result<(String, TargetSelector)> {
    match target {
        SelectTarget::Target(target) => target_parts(target.clone()),
        SelectTarget::Expression(_) | SelectTarget::Subquery(_) => Err(FastDbError::Schema(
            "this SELECT operation requires a physical table or record target".into(),
        )),
    }
}

fn record_id_value(value: RecordIdPart) -> Result<RecordIdValue> {
    Ok(match value.kind {
        RecordIdPartKind::Bare(value) | RecordIdPartKind::Quoted(value) => {
            RecordIdValue::String(value)
        }
        RecordIdPartKind::Integer(value) => RecordIdValue::Integer(value),
        RecordIdPartKind::Uuid(value) => RecordIdValue::Uuid(value),
        RecordIdPartKind::Complex(expression) => {
            let document = BTreeMap::new();
            let params = Params::new();
            let id = RecordId::new("__literal", RecordIdValue::String("literal".into()));
            let context = EvalContext {
                document: &document,
                id: &id,
                endpoints: None,
                params: &params,
                functions: None,
                function_calls: None,
                function_depth: 0,
            };
            match eval::evaluate(&expression, &context)?.into_projection() {
                Value::Array(values) => RecordIdValue::Array(values),
                Value::Object(values) => RecordIdValue::Object(values),
                _ => {
                    return Err(FastDbError::Schema(
                        "complex record ID must evaluate to an array or object".into(),
                    ))
                }
            }
        }
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
    options: CandidateReadOptions<'_>,
) -> Result<Vec<Candidate>> {
    let CandidateReadOptions {
        id,
        range,
        condition,
        params,
        allow_cache,
        fts,
        vector,
    } = options;
    if range.is_some() && (fts.is_some() || vector.is_some()) {
        return Err(FastDbError::Schema(
            "record ranges cannot be combined with FTS or KNN search".into(),
        ));
    }
    let predicates = condition
        .map(|condition| safe_pushdowns(condition, params, table))
        .unwrap_or_default();
    let encoded_rid = id.map(encode_rid).transpose()?;
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
    if let Some(vector) =
        vector.filter(|query| query.metric == turso_fastdb_parser::KnnMetric::Cosine)
    {
        let dimension = vector
            .column
            .dimension
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| FastDbError::format("vector hidden dimension is invalid"))?;
        let mut zero = vec![
            0_u8;
            dimension.checked_mul(8).ok_or_else(|| {
                FastDbError::Schema("vector dimension overflows native encoding".into())
            })?
        ];
        zero.push(2);
        let (zero_scan, zero_bindings) = lower::physical_vector_select_stmt(
            &table.physical_name,
            &vector.column.physical_name,
            &[],
            encoded_rid.as_deref(),
            &predicates,
            turso_core::Value::from_blob(zero),
            turso_fastdb_parser::KnnMetric::Euclidean,
            1,
            false,
        )?;
        let nearest = conn.collect_rows(zero_scan, zero_bindings)?;
        let zero_distance = nearest.first().and_then(|row| match row.get(2) {
            Some(turso_core::Value::Numeric(turso_core::Numeric::Integer(value))) => {
                Some(*value as f64)
            }
            Some(turso_core::Value::Numeric(turso_core::Numeric::Float(value))) => {
                Some(f64::from(*value))
            }
            _ => None,
        });
        if zero_distance == Some(0.0) {
            return Err(FastDbError::Schema(
                "COSINE KNN cannot evaluate a zero-magnitude stored vector".into(),
            ));
        }
    }
    let (statement, bindings) = if let Some(fts) = fts {
        lower::physical_fts_select_stmt(
            &table.physical_name,
            &fts.index.physical_columns,
            hidden.as_deref().unwrap_or(&[]),
            encoded_rid.as_deref(),
            &fts.query,
        )?
    } else if let Some(vector) = vector {
        lower::physical_vector_select_stmt(
            &table.physical_name,
            &vector.column.physical_name,
            hidden.as_deref().unwrap_or(&[]),
            encoded_rid.as_deref(),
            &predicates,
            vector.query.clone(),
            vector.metric,
            vector.k,
            vector.needs_document,
        )?
    } else if let Some(hidden) = &hidden {
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
    let rows = if fts.is_some() {
        conn.collect_rows(statement, bindings)?
    } else if let Some(vector) = vector {
        conn.collect_vector_candidates(
            statement,
            bindings,
            &table.physical_name,
            encoded_rid.is_some(),
            &predicates,
            allow_cache,
            format!(
                "vector:{}:{:?}:{}:{}",
                vector.column.physical_name,
                vector.metric,
                vector.k,
                table.kind == TableKind::Relation
            ),
        )?
    } else {
        conn.collect_select_candidates(
            statement,
            bindings,
            &table.physical_name,
            encoded_rid.is_some(),
            &predicates,
            allow_cache && table.kind == TableKind::Normal,
        )
        .map_err(stored_value_error)?
    };
    if fts.is_some() && rows.len() > 10_000 {
        return Err(FastDbError::Constraint(
            "FTS result candidate count exceeds 10,000 rows".into(),
        ));
    }
    let mut candidates = rows
        .into_iter()
        .map(|row| {
            let encoded_rid = value_to_string(row.first().unwrap_or(&turso_core::Value::Null))
                .map_err(stored_value_error)?;
            let id = RecordId::new(&table.logical_name, decode_rid(&encoded_rid)?);
            let document = if vector.is_some_and(|vector| !vector.needs_document) {
                BTreeMap::new()
            } else {
                let json = value_to_string(row.get(1).unwrap_or(&turso_core::Value::Null))
                    .map_err(stored_value_error)?;
                decode::parse_doc(&json)?.into_iter().collect()
            };
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
            let fts = fts
                .map(|query| {
                    let score_index = 2 + if table.kind == TableKind::Relation {
                        4
                    } else {
                        0
                    };
                    let score = match row.get(score_index) {
                        Some(turso_core::Value::Numeric(turso_core::Numeric::Integer(value))) => {
                            *value as f64
                        }
                        Some(turso_core::Value::Numeric(turso_core::Numeric::Float(value))) => {
                            f64::from(*value)
                        }
                        _ => {
                            return Err(stored_value_error(FastDbError::Engine(
                                "FTS provider returned a non-numeric score".into(),
                            )))
                        }
                    };
                    Ok(FtsCandidateContext {
                        query: query.clone(),
                        score,
                    })
                })
                .transpose()?;
            let vector_distance = if vector.is_some() {
                let distance_index = 2 + if table.kind == TableKind::Relation {
                    4
                } else {
                    0
                };
                Some(match row.get(distance_index) {
                    Some(turso_core::Value::Numeric(turso_core::Numeric::Integer(value))) => {
                        *value as f64
                    }
                    Some(turso_core::Value::Numeric(turso_core::Numeric::Float(value))) => {
                        f64::from(*value)
                    }
                    _ => {
                        return Err(stored_value_error(FastDbError::Engine(
                            "vector provider returned a non-numeric distance".into(),
                        )))
                    }
                })
            } else {
                None
            };
            Ok(Candidate {
                encoded_rid,
                id,
                document,
                endpoints,
                fts,
                vector_distance,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if let Some(range) = range {
        candidates.retain(|candidate| record_id_in_range(&candidate.id.id, range));
    }
    if let Some(fts) = fts.filter(|fts| fts.options.surface == "surreal") {
        let scores = surreal_blank_scores(conn, table, fts)?;
        for candidate in &mut candidates {
            if let Some(context) = &mut candidate.fts {
                context.score = scores.get(&candidate.encoded_rid).copied().unwrap_or(0.0);
            }
        }
    }
    Ok(candidates)
}

fn record_id_in_range(id: &RecordIdValue, range: &ResolvedRecordRange) -> bool {
    let after_start = range
        .start
        .as_ref()
        .is_none_or(|start| decode::record_component_cmp(id, start) != Ordering::Less);
    let before_end = range.end.as_ref().is_none_or(|end| {
        let ordering = decode::record_component_cmp(id, end);
        ordering == Ordering::Less || (range.inclusive && ordering == Ordering::Equal)
    });
    after_start && before_end
}

fn surreal_blank_scores(
    conn: &Connection,
    table: &TableDefinition,
    query: &ResolvedFtsQuery,
) -> Result<BTreeMap<String, f64>> {
    let path = query
        .index
        .paths
        .first()
        .ok_or_else(|| FastDbError::format("Surreal FTS index has no field"))?;
    let documents = read_documents(conn, table)?
        .into_iter()
        .filter_map(
            |(rid, document)| match crate::path::get_path(&document, path) {
                Some(Value::Str(text)) => Some((rid, text.clone())),
                None | Some(Value::Null) => None,
                Some(_) => None,
            },
        )
        .collect::<Vec<_>>();
    if documents.is_empty() {
        return Ok(BTreeMap::new());
    }
    let query_terms = query.query.split_whitespace().collect::<BTreeSet<_>>();
    if query_terms.is_empty() {
        return Ok(BTreeMap::new());
    }
    let document_tokens = documents
        .iter()
        .map(|(_, text)| text.split_whitespace().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let document_count = document_tokens.len() as f32;
    let average_length =
        document_tokens.iter().map(Vec::len).sum::<usize>() as f32 / document_count;
    let mut document_frequencies = BTreeMap::new();
    for term in &query_terms {
        let frequency = document_tokens
            .iter()
            .filter(|tokens| tokens.iter().any(|token| token == term))
            .count() as f32;
        document_frequencies.insert(*term, frequency);
    }
    let mut scores = BTreeMap::new();
    for ((rid, _), tokens) in documents.iter().zip(&document_tokens) {
        let mut score = 0.0_f32;
        for term in &query_terms {
            let frequency = tokens.iter().filter(|token| *token == term).count();
            if frequency == 0 {
                continue;
            }
            let document_frequency = document_frequencies[term];
            let idf = ((document_count - document_frequency + 0.5) / (document_frequency + 0.5))
                .ln()
                .max(0.0);
            let logarithmic_tf = 1.0 + (frequency as f32).ln();
            let length_normalization = 1.0 - 0.75 + 0.75 * (tokens.len() as f32 / average_length);
            score +=
                idf * logarithmic_tf * (1.2 + 1.0) / (logarithmic_tf + 1.2 * length_normalization);
        }
        scores.insert(rid.clone(), f64::from(score));
    }
    Ok(scores)
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

struct StatementTimeoutGuard<'a> {
    conn: &'a Connection,
    previous: Duration,
}

impl<'a> StatementTimeoutGuard<'a> {
    fn install(conn: &'a Connection, expression: Option<&Expr>, params: &Params) -> Result<Self> {
        let previous = conn.native().get_query_timeout();
        let Some(expression) = expression else {
            return Ok(Self { conn, previous });
        };
        let document = BTreeMap::new();
        let id = RecordId::new("__timeout", "statement");
        let context = EvalContext {
            document: &document,
            id: &id,
            endpoints: None,
            params,
            functions: None,
            function_calls: None,
            function_depth: 0,
        };
        let value = eval::evaluate(expression, &context)?.into_projection();
        let Value::Duration(timeout) = value else {
            return Err(FastDbError::Schema(
                "TIMEOUT must evaluate to a duration".into(),
            ));
        };
        let timeout = Duration::new(timeout.seconds(), timeout.nanoseconds());
        if timeout.is_zero() {
            return Err(FastDbError::Schema(
                "TIMEOUT must be greater than zero".into(),
            ));
        }
        let effective = if previous.is_zero() {
            timeout
        } else {
            previous.min(timeout)
        };
        conn.native().set_query_timeout(effective);
        Ok(Self { conn, previous })
    }
}

impl Drop for StatementTimeoutGuard<'_> {
    fn drop(&mut self) {
        self.conn.native().set_query_timeout(self.previous);
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

fn mark_fts_dirty(execution: &mut ExecutionState, table_name: &str) {
    let TransactionState::Active(active) = &mut execution.transaction else {
        return;
    };
    let Some(table) = active
        .catalog
        .snapshot()
        .and_then(|snapshot| snapshot.tables.get(table_name))
    else {
        return;
    };
    if table
        .indexes
        .values()
        .any(|index| index.kind == IndexKind::Fts)
    {
        active.dirty_fts_tables.insert(table.id);
    }
}

fn mark_relation_fts_dirty(execution: &mut ExecutionState) {
    let TransactionState::Active(active) = &mut execution.transaction else {
        return;
    };
    let dirty = active
        .catalog
        .snapshot()
        .into_iter()
        .flat_map(|snapshot| snapshot.tables.values())
        .filter(|table| {
            table.kind == TableKind::Relation
                && table
                    .indexes
                    .values()
                    .any(|index| index.kind == IndexKind::Fts)
        })
        .map(|table| table.id)
        .collect::<Vec<_>>();
    active.dirty_fts_tables.extend(dirty);
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
    _params: &Params,
    script: &mut ScriptRuntime,
) -> Result<StatementResult> {
    if statement.view.is_some() {
        return run_define_view(conn, execution, statement, source, script);
    }
    let definition = source_slice(source, statement.span)?.to_string();
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        if let Some(existing) = snapshot.tables.get(&statement.name.value).cloned() {
            if statement.if_not_exists.is_some() {
                return Ok(());
            }
            if statement.overwrite.is_none() {
                return Err(FastDbError::Constraint(format!(
                    "table {:?} is already defined",
                    statement.name.value
                )));
            }
            let requested_kind = match statement.kind {
                TableKindSyntax::Normal { .. } => TableKind::Normal,
                TableKindSyntax::Relation(_) => TableKind::Relation,
            };
            if requested_kind != existing.kind {
                return Err(FastDbError::Schema(
                    "DEFINE TABLE OVERWRITE cannot change NORMAL/RELATION physical kind".into(),
                ));
            }
            if statement.mode.value == TableMode::Schemafull {
                for (_, mut document) in read_documents(conn, &existing)? {
                    schema::validate_document(true, &existing.fields, &mut document)?;
                }
            }
            let mut replacement = existing;
            replacement.mode = statement.mode.value;
            replacement.definition = Some(definition.clone());
            replacement.drop = statement.drop.is_some();
            replacement.permissions = statement.permissions;
            replacement.comment = statement.comment.as_ref().map(|value| value.value.clone());
            if let TableKindSyntax::Relation(relation) = &statement.kind {
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
                        return Err(FastDbError::Schema(
                            "relation endpoint constraint must name a normal table".into(),
                        ));
                    }
                }
                replacement.relation_in_table_id = relation
                    .input
                    .as_ref()
                    .map(|endpoint| snapshot.tables[&endpoint.value].id);
                replacement.relation_out_table_id = relation
                    .output
                    .as_ref()
                    .map(|endpoint| snapshot.tables[&endpoint.value].id);
                replacement.relation_enforced = relation.enforced.is_some();
                validate_existing_relation_edges(conn, snapshot, &replacement)?;
            }
            catalog::replace_table(conn, &replacement)?;
            snapshot
                .tables
                .insert(statement.name.value.clone(), replacement);
            return Ok(());
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
        let table = snapshot
            .tables
            .get_mut(&statement.name.value)
            .expect("defined table was published");
        table.drop = statement.drop.is_some();
        table.permissions = statement.permissions;
        table.comment = statement.comment.as_ref().map(|value| value.value.clone());
        catalog::replace_table(conn, table)?;
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn run_define_view(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineTableStatement,
    source: &str,
    script: &mut ScriptRuntime,
) -> Result<StatementResult> {
    let select = *statement
        .view
        .clone()
        .expect("view dispatcher checks SELECT presence");
    validate_view_select(&select)?;
    let select_source = source_slice(source, select.span)?.trim().to_string();
    let definition = canonical_view_definition(
        &statement.name.value,
        &select_source,
        statement.permissions,
        statement
            .comment
            .as_ref()
            .map(|comment| comment.value.as_str()),
    );
    let changed = with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let source_name = view_source_name(&select)?;
        let source_table = snapshot.tables.get(source_name).cloned().ok_or_else(|| {
            FastDbError::Schema(format!("view source table {source_name:?} is not defined"))
        })?;
        if let Some(existing) = snapshot.tables.get(&statement.name.value).cloned() {
            if statement.if_not_exists.is_some() {
                return Ok(false);
            }
            if statement.overwrite.is_none() {
                return Err(FastDbError::Constraint(format!(
                    "table {:?} is already defined",
                    statement.name.value
                )));
            }
            let old_view = snapshot
                .views
                .get(&statement.name.value)
                .cloned()
                .ok_or_else(|| {
                    FastDbError::Schema(
                        "DEFINE TABLE OVERWRITE cannot convert a stored table into a view".into(),
                    )
                })?;
            if source_table.id == existing.id
                || view_dependency_reaches(snapshot, source_table.id, existing.id)
            {
                return Err(FastDbError::Schema(
                    "materialized view dependency graph would contain a cycle".into(),
                ));
            }
            let mut replacement = existing;
            replacement.definition = Some(definition.clone());
            replacement.mode = TableMode::Schemaless;
            replacement.drop = false;
            replacement.permissions = statement.permissions;
            replacement.comment = statement.comment.as_ref().map(|value| value.value.clone());
            catalog::replace_table(conn, &replacement)?;
            catalog::remove_view(conn, &old_view)?;
            let view = catalog::allocate_view(
                replacement.id,
                &replacement.logical_name,
                select.clone(),
                select_source.clone(),
                vec![source_table.id],
                definition.clone(),
            )?;
            catalog::persist_view(conn, &view)?;
            snapshot
                .tables
                .insert(replacement.logical_name.clone(), replacement);
            snapshot.views.insert(view.logical_name.clone(), view);
            return Ok(true);
        }
        if source_name == statement.name.value {
            return Err(FastDbError::Schema(
                "a materialized view cannot select from itself".into(),
            ));
        }
        register_normal_table(
            conn,
            snapshot,
            &statement.name.value,
            TableMode::Schemaless,
            Some(definition.clone()),
        )?;
        let table = snapshot
            .tables
            .get_mut(&statement.name.value)
            .expect("view table was registered");
        table.permissions = statement.permissions;
        table.comment = statement.comment.as_ref().map(|value| value.value.clone());
        catalog::replace_table(conn, table)?;
        let view = catalog::allocate_view(
            table.id,
            &statement.name.value,
            select.clone(),
            select_source.clone(),
            vec![source_table.id],
            definition.clone(),
        )?;
        catalog::persist_view(conn, &view)?;
        conn.check_failpoint(Failpoint::AfterViewCatalog)?;
        snapshot.views.insert(view.logical_name.clone(), view);
        Ok(true)
    })?;
    if changed {
        refresh_view(conn, execution, &statement.name.value, script)?;
        refresh_dependent_views(conn, execution, &statement.name.value, script)?;
    }
    Ok(StatementResult::None)
}

fn validate_view_select(select: &turso_fastdb_parser::SelectStatement) -> Result<()> {
    view_source_name(select)?;
    if select.value.is_some()
        || select.only.is_some()
        || !select.fetch.is_empty()
        || !select.split.is_empty()
        || select.order_random.is_some()
    {
        return Err(FastDbError::Schema(
            "materialized views require object rows and reject ONLY, VALUE, FETCH, SPLIT, and random ordering"
                .into(),
        ));
    }
    if eval::validate_parameter_references(&Statement::Select(select.clone()), &Params::new())
        .is_err()
    {
        return Err(FastDbError::Schema(
            "materialized view definitions cannot contain parameters".into(),
        ));
    }
    validate_projection_shapes(&select.projections, false)?;
    let mut expressions = Vec::new();
    if let ProjectionList::Fields(projections) = &select.projections {
        for projection in projections {
            if projection
                .alias
                .as_ref()
                .is_some_and(|alias| alias.value.starts_with(VIEW_INTERNAL_FIELD_PREFIX))
            {
                return Err(FastDbError::Schema(
                    "view projection alias uses a reserved materialization field".into(),
                ));
            }
            expressions.push(&projection.expression);
        }
    }
    expressions.extend(select.condition.iter());
    if let Some(GroupClause::By(keys)) = &select.group {
        expressions.extend(keys);
    }
    expressions.extend(select.limit_expression.iter());
    expressions.extend(select.start_expression.iter());
    for expression in expressions {
        validate_schema_expression_safety(expression)?;
    }
    Ok(())
}

fn view_dependency_reaches(
    snapshot: &CatalogSnapshot,
    current: crate::names::CatalogId,
    target: crate::names::CatalogId,
) -> bool {
    if current == target {
        return true;
    }
    snapshot
        .views
        .values()
        .find(|view| view.id == current)
        .is_some_and(|view| {
            view.dependencies
                .iter()
                .any(|dependency| view_dependency_reaches(snapshot, *dependency, target))
        })
}

fn canonical_view_definition(
    logical_name: &str,
    select_source: &str,
    permissions: turso_fastdb_parser::SchemaPermissions,
    comment: Option<&str>,
) -> String {
    let mut definition = format!(
        "DEFINE TABLE {} TYPE NORMAL SCHEMALESS AS {} PERMISSIONS {}",
        render_schema_identifier(logical_name),
        select_source.trim(),
        match permissions {
            turso_fastdb_parser::SchemaPermissions::Full => "FULL",
            turso_fastdb_parser::SchemaPermissions::None => "NONE",
        }
    );
    if let Some(comment) = comment {
        definition.push_str(" COMMENT ");
        definition.push_str(&render_schema_string(comment));
    }
    definition
}

fn validate_existing_relation_edges(
    conn: &Connection,
    snapshot: &CatalogSnapshot,
    relation: &TableDefinition,
) -> Result<()> {
    debug_assert_eq!(relation.kind, TableKind::Relation);
    let params = Params::new();
    for candidate in read_candidates(
        conn,
        snapshot,
        relation,
        CandidateReadOptions {
            id: None,
            range: None,
            condition: None,
            params: &params,
            allow_cache: false,
            fts: None,
            vector: None,
        },
    )? {
        let (input, output) = candidate
            .endpoints
            .ok_or_else(|| FastDbError::format("relation record is missing stored endpoints"))?;
        let input_table = snapshot.tables.get(&input.table).ok_or_else(|| {
            FastDbError::format("relation input endpoint table is missing from the catalog")
        })?;
        let output_table = snapshot.tables.get(&output.table).ok_or_else(|| {
            FastDbError::format("relation output endpoint table is missing from the catalog")
        })?;
        if input_table.kind != TableKind::Normal || output_table.kind != TableKind::Normal {
            return Err(FastDbError::format(
                "relation endpoint references a non-normal table",
            ));
        }
        if relation
            .relation_in_table_id
            .is_some_and(|expected| expected != input_table.id)
            || relation
                .relation_out_table_id
                .is_some_and(|expected| expected != output_table.id)
        {
            return Err(FastDbError::Schema(
                "DEFINE TABLE OVERWRITE endpoint constraints reject an existing edge".into(),
            ));
        }
        if relation.relation_enforced
            && (!record_exists(conn, input_table, &input.id)?
                || !record_exists(conn, output_table, &output.id)?)
        {
            return Err(FastDbError::Constraint(
                "DEFINE TABLE OVERWRITE ENFORCED requires every existing endpoint record".into(),
            ));
        }
    }
    Ok(())
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
    if statement.reference.is_some() && !ty.supports_reference() {
        return Err(FastDbError::Schema(
            "REFERENCE requires a record or record collection field type".into(),
        ));
    }
    let schema_expression = |expression: &Expr| -> Result<SchemaExpression> {
        validate_schema_expression_safety(expression)?;
        Ok(SchemaExpression {
            expression: expression.clone(),
            source: source_slice(source, expression.span)?.to_string(),
        })
    };
    let default_always = statement
        .default
        .as_ref()
        .is_some_and(|default| default.always.is_some());
    let default = statement
        .default
        .as_ref()
        .map(|default| schema_expression(&default.value))
        .transpose()?;
    let value = statement
        .value
        .as_ref()
        .map(schema_expression)
        .transpose()?;
    let assert = statement
        .assert
        .as_ref()
        .map(schema_expression)
        .transpose()?;
    let rule = FieldRule {
        path,
        path_key: path_key.clone(),
        required: ty.required(),
        ty,
        definition,
        default,
        default_always,
        value,
        assert,
        readonly: statement.readonly.is_some(),
        reference: statement.reference.is_some(),
        permissions: statement.permissions,
        comment: statement
            .comment
            .as_ref()
            .map(|comment| comment.value.clone()),
    };
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ready_snapshot_mut(state)?;
        let table = snapshot
            .tables
            .get(&statement.table.value)
            .cloned()
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
            if statement.if_not_exists.is_some() {
                return Ok(());
            }
            if statement.overwrite.is_none() {
                return Err(FastDbError::Constraint(format!(
                    "field {path_key} is already defined on table {:?}",
                    statement.table.value
                )));
            }
            return replace_field_rule(conn, snapshot, &table, rule.clone());
        }
        schema::validate_field_relationships(table.fields.values(), &rule)?;
        let mut candidate_fields = table.fields.clone();
        candidate_fields.insert(path_key.clone(), rule.clone());
        let mut rows = read_documents(conn, &table)?;
        for (_, document) in &mut rows {
            validate_candidate_field_document(&table, &candidate_fields, &rule, document)?;
        }
        conn.check_failpoint(Failpoint::AfterFieldValidation)?;
        let vector_ordinal = snapshot
            .hidden_columns
            .values()
            .filter(|column| {
                column.table_id == table.id
                    && matches!(column.role, catalog::HiddenColumnRole::Vector64(_))
            })
            .count();
        let vector_column = rule.ty.vector_dimension().map(|dimension| {
            catalog::allocate_vector_hidden_column(
                table.id,
                path_key.clone(),
                dimension,
                vector_ordinal,
            )
        });
        if let Some(column) = &vector_column {
            catalog::persist_hidden_column(conn, column)?;
            if !snapshot.capabilities.contains_key(BUILTIN_VECTOR_PROVIDER) {
                catalog::persist_vector_capability(conn)?;
            }
            conn.check_failpoint(Failpoint::AfterVectorHiddenCatalog)?;
            conn.exec_bound(
                lower::physical_add_vector_column_ddl(&table.physical_name, &column.physical_name)?,
                vec![],
            )?;
            conn.check_failpoint(Failpoint::AfterVectorPhysicalColumn)?;
        }
        for (rid, document) in &rows {
            let hidden = if let Some(column) = &vector_column {
                let dimension = u32::try_from(column.dimension.expect("allocated dimension"))
                    .expect("allocated vector dimension fits");
                vec![(
                    column.physical_name.clone(),
                    derive_vector64(document, &rule.path, dimension)?,
                )]
            } else {
                Vec::new()
            };
            let (update, bindings) = lower::physical_update_document_with_hidden_stmt(
                &table.physical_name,
                rid,
                &decode::encode_doc(document)?,
                &hidden,
            )?;
            conn.exec_bound(update, bindings)?;
        }
        if vector_column.is_some() {
            conn.check_failpoint(Failpoint::AfterVectorBackfill)?;
        }
        catalog::persist_field(conn, &table, &rule)?;
        conn.check_failpoint(Failpoint::AfterFieldCatalogRow)?;
        if let Some(column) = vector_column {
            snapshot.hidden_columns.insert(column.id, column);
            snapshot.capabilities.insert(
                BUILTIN_VECTOR_PROVIDER.to_string(),
                CapabilityRequirement {
                    provider: BUILTIN_VECTOR_PROVIDER.to_string(),
                    min_provider_version: BUILTIN_VECTOR_PROVIDER_VERSION,
                    min_encoding_version: BUILTIN_VECTOR_ENCODING_VERSION,
                },
            );
        }
        snapshot
            .tables
            .get_mut(&statement.table.value)
            .expect("cloned table remains present")
            .fields
            .insert(path_key.clone(), rule.clone());
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn validate_candidate_field_document(
    table: &TableDefinition,
    candidate_fields: &BTreeMap<String, FieldRule>,
    candidate: &FieldRule,
    document: &mut BTreeMap<String, Value>,
) -> Result<()> {
    if candidate.default.is_some() && crate::path::get_path(document, &candidate.path).is_none() {
        let mut fields = candidate_fields.clone();
        fields
            .get_mut(&candidate.path_key)
            .expect("candidate field is present")
            .required = false;
        schema::validate_document(table.mode == TableMode::Schemafull, &fields, document)?;
    } else {
        schema::validate_document(
            table.mode == TableMode::Schemafull,
            candidate_fields,
            document,
        )?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn normalize_schema_document(
    table: &TableDefinition,
    document: &mut BTreeMap<String, Value>,
    before: Option<&BTreeMap<String, Value>>,
    id: &RecordId,
    endpoints: Option<(&RecordId, &RecordId)>,
    params: &Params,
    functions: &BTreeMap<String, catalog::FunctionDefinition>,
    create: bool,
) -> Result<()> {
    for field in table.fields.values() {
        let prior = before.and_then(|before| crate::path::get_path(before, &field.path));
        let supplied = crate::path::get_path(document, &field.path).cloned();
        if field.readonly && !create && supplied.as_ref() != prior {
            return Err(FastDbError::Constraint(format!(
                "readonly field {} cannot be changed",
                field.path_key
            )));
        }
        let apply_default = field.default.is_some()
            && ((create && supplied.is_none())
                || (field.default_always && matches!(supplied.as_ref(), None | Some(Value::None))));
        if apply_default {
            let default = field.default.as_ref().expect("checked default presence");
            let value = evaluate_schema_expression(
                default,
                document,
                before,
                id,
                endpoints,
                params,
                functions,
                supplied.clone().unwrap_or(Value::None),
            )?;
            if matches!(value, Value::None) {
                crate::path::remove_path(document, &field.path)?;
            } else {
                crate::path::set_path(document, &field.path, value)?;
            }
        }
        if let Some(expression) = &field.value {
            let current = crate::path::get_path(document, &field.path)
                .cloned()
                .unwrap_or(Value::None);
            let value = evaluate_schema_expression(
                expression, document, before, id, endpoints, params, functions, current,
            )?;
            if matches!(value, Value::None) {
                crate::path::remove_path(document, &field.path)?;
            } else {
                crate::path::set_path(document, &field.path, value)?;
            }
        }
    }
    schema::validate_document(table.mode == TableMode::Schemafull, &table.fields, document)?;
    for field in table.fields.values() {
        let Some(assertion) = &field.assert else {
            continue;
        };
        let value = crate::path::get_path(document, &field.path)
            .cloned()
            .unwrap_or(Value::None);
        let outcome = evaluate_schema_expression(
            assertion, document, before, id, endpoints, params, functions, value,
        )?;
        if !EvalValue::Present(outcome).truthy() {
            return Err(FastDbError::Constraint(format!(
                "field {} does not satisfy its ASSERT expression",
                field.path_key
            )));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn evaluate_schema_expression(
    expression: &SchemaExpression,
    document: &BTreeMap<String, Value>,
    before: Option<&BTreeMap<String, Value>>,
    id: &RecordId,
    endpoints: Option<(&RecordId, &RecordId)>,
    params: &Params,
    functions: &BTreeMap<String, catalog::FunctionDefinition>,
    value: Value,
) -> Result<Value> {
    let mut params = params.clone();
    params.insert("value".into(), value);
    params.insert(
        "before".into(),
        before.cloned().map(Value::Object).unwrap_or(Value::None),
    );
    params.insert("after".into(), Value::Object(document.clone()));
    let calls = std::cell::Cell::new(0);
    eval::evaluate(
        &expression.expression,
        &EvalContext {
            document,
            id,
            endpoints,
            params: &params,
            functions: Some(functions),
            function_calls: Some(&calls),
            function_depth: 0,
        },
    )
    .map(EvalValue::into_projection)
}

fn replace_field_rule(
    conn: &Connection,
    snapshot: &mut CatalogSnapshot,
    table: &TableDefinition,
    replacement: FieldRule,
) -> Result<()> {
    let existing = table
        .fields
        .get(&replacement.path_key)
        .expect("replacement field exists");
    if existing.ty != replacement.ty {
        return Err(FastDbError::Schema(
            "DEFINE FIELD OVERWRITE cannot change the physical field type; use ALTER FIELD TYPE"
                .into(),
        ));
    }
    let mut candidate_fields = table.fields.clone();
    candidate_fields.insert(replacement.path_key.clone(), replacement.clone());
    for (_, mut document) in read_documents(conn, table)? {
        validate_candidate_field_document(table, &candidate_fields, &replacement, &mut document)?;
    }
    catalog::remove_field(conn, table, &replacement.path_key)?;
    catalog::persist_field(conn, table, &replacement)?;
    snapshot
        .tables
        .get_mut(&table.logical_name)
        .expect("replacement table exists")
        .fields
        .insert(replacement.path_key.clone(), replacement);
    Ok(())
}

fn render_schema_path(path: &[String]) -> String {
    path.iter()
        .map(|segment| render_schema_identifier(segment))
        .collect::<Vec<_>>()
        .join(".")
}

fn canonical_field_definition(table: &TableDefinition, field: &FieldRule) -> String {
    let mut definition = format!(
        "DEFINE FIELD {} ON {} TYPE {}",
        render_schema_path(&field.path),
        render_schema_identifier(&table.logical_name),
        field.ty.canonical(),
    );
    if field.reference {
        definition.push_str(" REFERENCE");
    }
    if let Some(default) = &field.default {
        definition.push_str(" DEFAULT");
        if field.default_always {
            definition.push_str(" ALWAYS");
        }
        definition.push(' ');
        definition.push_str(&default.source);
    }
    if field.readonly {
        definition.push_str(" READONLY");
    }
    if let Some(value) = &field.value {
        definition.push_str(" VALUE ");
        definition.push_str(&value.source);
    }
    if let Some(assert) = &field.assert {
        definition.push_str(" ASSERT ");
        definition.push_str(&assert.source);
    }
    definition.push_str(match field.permissions {
        turso_fastdb_parser::SchemaPermissions::Full => " PERMISSIONS FULL",
        turso_fastdb_parser::SchemaPermissions::None => " PERMISSIONS NONE",
    });
    if let Some(comment) = &field.comment {
        definition.push_str(" COMMENT ");
        definition.push_str(&render_schema_string(comment));
    }
    definition
}

fn run_alter_field(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::AlterFieldStatement,
    source: &str,
) -> Result<StatementResult> {
    let (_, path_key) = crate::path::parser_path(&statement.path)?;
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(table) = snapshot.tables.get(&statement.table.value).cloned() else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "table {:?} is not defined",
                statement.table.value
            )));
        };
        let Some(mut replacement) = table.fields.get(&path_key).cloned() else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "field {path_key} is not defined on table {:?}",
                table.logical_name
            )));
        };
        match statement.change {
            turso_fastdb_parser::AlterFieldChange::Type(ty) => {
                let ty = FieldType::from_parser(&ty);
                if ty.vector_dimension() != replacement.ty.vector_dimension()
                    && (ty.vector_dimension().is_some()
                        || replacement.ty.vector_dimension().is_some())
                {
                    return Err(FastDbError::Schema(
                        "ALTER FIELD cannot change the native vector representation".into(),
                    ));
                }
                replacement.ty = ty;
                replacement.required = replacement.ty.required();
            }
            turso_fastdb_parser::AlterFieldChange::Default(default) => {
                validate_schema_expression_safety(&default.value)?;
                replacement.default_always = default.always.is_some();
                replacement.default = Some(SchemaExpression {
                    source: source_slice(source, default.value.span)?.to_string(),
                    expression: default.value,
                });
            }
            turso_fastdb_parser::AlterFieldChange::Value(value) => {
                validate_schema_expression_safety(&value)?;
                replacement.value = Some(SchemaExpression {
                    source: source_slice(source, value.span)?.to_string(),
                    expression: value,
                });
            }
            turso_fastdb_parser::AlterFieldChange::Assert(assert) => {
                validate_schema_expression_safety(&assert)?;
                replacement.assert = Some(SchemaExpression {
                    source: source_slice(source, assert.span)?.to_string(),
                    expression: assert,
                });
            }
            turso_fastdb_parser::AlterFieldChange::Readonly => replacement.readonly = true,
            turso_fastdb_parser::AlterFieldChange::Reference => replacement.reference = true,
            turso_fastdb_parser::AlterFieldChange::Permissions(permissions) => {
                replacement.permissions = permissions
            }
            turso_fastdb_parser::AlterFieldChange::Comment(comment) => {
                replacement.comment = Some(comment)
            }
            turso_fastdb_parser::AlterFieldChange::DropType => {
                replacement.ty = FieldType::Any;
                replacement.required = false;
            }
            turso_fastdb_parser::AlterFieldChange::DropDefault => {
                replacement.default = None;
                replacement.default_always = false;
            }
            turso_fastdb_parser::AlterFieldChange::DropValue => replacement.value = None,
            turso_fastdb_parser::AlterFieldChange::DropAssert => replacement.assert = None,
            turso_fastdb_parser::AlterFieldChange::DropReadonly => replacement.readonly = false,
            turso_fastdb_parser::AlterFieldChange::DropReference => replacement.reference = false,
            turso_fastdb_parser::AlterFieldChange::DropComment => replacement.comment = None,
        }
        if replacement.reference && !replacement.ty.supports_reference() {
            return Err(FastDbError::Schema(
                "REFERENCE requires a record or record collection field type".into(),
            ));
        }
        let mut candidate_fields = table.fields.clone();
        candidate_fields.insert(path_key.clone(), replacement.clone());
        schema::validate_field_relationships(
            candidate_fields
                .values()
                .filter(|field| field.path_key != path_key),
            &replacement,
        )?;
        let functions = snapshot.functions.clone();
        for (rid, mut document) in read_documents(conn, &table)? {
            validate_candidate_field_document(
                &table,
                &candidate_fields,
                &replacement,
                &mut document,
            )?;
            validate_field_assertion(
                &replacement,
                &document,
                &RecordId::new(&table.logical_name, decode_rid(&rid)?),
                &functions,
            )?;
            let hidden = derived_hidden_values(snapshot, &table, &document)?;
            let (update, bindings) = lower::physical_update_document_with_hidden_stmt(
                &table.physical_name,
                &rid,
                &decode::encode_doc(&document)?,
                &hidden,
            )?;
            conn.exec_bound(update, bindings)?;
        }
        replacement.definition = canonical_field_definition(&table, &replacement);
        catalog::remove_field(conn, &table, &path_key)?;
        catalog::persist_field(conn, &table, &replacement)?;
        snapshot
            .tables
            .get_mut(&table.logical_name)
            .expect("altered table exists")
            .fields
            .insert(path_key.clone(), replacement);
        Ok(())
    })?;
    Ok(StatementResult::None)
}

fn validate_field_assertion(
    field: &FieldRule,
    document: &BTreeMap<String, Value>,
    id: &RecordId,
    functions: &BTreeMap<String, catalog::FunctionDefinition>,
) -> Result<()> {
    let Some(assertion) = &field.assert else {
        return Ok(());
    };
    let value = crate::path::get_path(document, &field.path)
        .cloned()
        .unwrap_or(Value::None);
    if !EvalValue::Present(evaluate_schema_expression(
        assertion,
        document,
        None,
        id,
        None,
        &Params::new(),
        functions,
        value,
    )?)
    .truthy()
    {
        return Err(FastDbError::Constraint(format!(
            "field {} does not satisfy its ASSERT expression",
            field.path_key
        )));
    }
    Ok(())
}

fn run_remove_field(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::RemoveFieldStatement,
) -> Result<StatementResult> {
    let (_, path_key) = crate::path::parser_path(&statement.path)?;
    with_schema_mutation(conn, execution, |state| {
        let snapshot = ensure_snapshot(conn, state)?;
        let Some(table) = snapshot.tables.get(&statement.table.value).cloned() else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "table {:?} is not defined",
                statement.table.value
            )));
        };
        let Some(field) = table.fields.get(&path_key).cloned() else {
            if statement.if_exists.is_some() {
                return Ok(());
            }
            return Err(FastDbError::Schema(format!(
                "field {path_key} is not defined on table {:?}",
                table.logical_name
            )));
        };
        if let Some(index) = table
            .indexes
            .values()
            .find(|index| index.path_keys.contains(&path_key))
        {
            return Err(FastDbError::Constraint(format!(
                "field {path_key} is required by index {:?}",
                index.logical_name
            )));
        }
        let vector_column = snapshot
            .hidden_columns
            .values()
            .find(|column| {
                column.table_id == table.id
                    && column.field_path_key.as_deref() == Some(path_key.as_str())
                    && matches!(column.role, catalog::HiddenColumnRole::Vector64(_))
            })
            .cloned();
        if let Some(column) = &vector_column {
            conn.exec_bound(
                lower::physical_drop_hidden_column_ddl(
                    &table.physical_name,
                    &column.physical_name,
                )?,
                vec![],
            )?;
            let (delete, bindings) = lower::hidden_column_delete(&column.id.to_hex());
            conn.exec_bound(delete, bindings)?;
            snapshot.hidden_columns.remove(&column.id);
        }
        catalog::remove_field(conn, &table, &path_key)?;
        snapshot
            .tables
            .get_mut(&table.logical_name)
            .expect("field table exists")
            .fields
            .remove(&path_key);
        if field.ty.vector_dimension().is_some()
            && !snapshot.tables.values().any(|table| {
                table
                    .fields
                    .values()
                    .any(|field| field.ty.vector_dimension().is_some())
            })
            && snapshot
                .capabilities
                .remove(BUILTIN_VECTOR_PROVIDER)
                .is_some()
        {
            catalog::remove_capability(conn, BUILTIN_VECTOR_PROVIDER)?;
        }
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
    if !matches!(statement.kind, IndexKindSyntax::Btree) {
        return run_define_fts_index(conn, execution, statement, source);
    }
    match &statement.kind {
        IndexKindSyntax::Btree => {}
        IndexKindSyntax::Fulltext { .. } | IndexKindSyntax::Provider { .. } => unreachable!(),
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

fn run_define_fts_index(
    conn: &Connection,
    execution: &mut ExecutionState,
    statement: turso_fastdb_parser::DefineIndexStatement,
    source: &str,
) -> Result<StatementResult> {
    ensure_fts_available()?;
    if statement.unique.is_some() {
        return Err(FastDbError::Schema("FTS indexes cannot be UNIQUE".into()));
    }
    let definition = source_slice(source, statement.span)?.to_string();
    let paths = statement
        .fields
        .iter()
        .map(|path| crate::path::parser_path(path).map(|(segments, _)| segments))
        .collect::<Result<Vec<_>>>()?;
    let path_keys = paths
        .iter()
        .map(crate::path::canonical_path)
        .collect::<Result<Vec<_>>>()?;
    if path_keys.iter().collect::<BTreeSet<_>>().len() != path_keys.len() {
        return Err(FastDbError::Schema(
            "FTS index fields must be distinct".into(),
        ));
    }
    if paths.iter().any(|path| {
        path.first()
            .is_some_and(|segment| matches!(segment.as_str(), "id" | "in" | "out"))
    }) {
        return Err(FastDbError::Schema(
            "synthesized record fields cannot be full-text indexed".into(),
        ));
    }

    let options = match &statement.kind {
        IndexKindSyntax::Fulltext {
            analyzer,
            highlights,
            ..
        } => {
            if statement.surface != IndexDefinitionSurface::SurrealDefine {
                return Err(FastDbError::Schema(
                    "FULLTEXT ANALYZER is available only through DEFINE INDEX".into(),
                ));
            }
            if paths.len() != 1 {
                return Err(FastDbError::Schema(
                    "Phase 8 Surreal FULLTEXT indexes require exactly one field".into(),
                ));
            }
            catalog::FtsIndexOptions {
                surface: "surreal".to_string(),
                tokenizer: "whitespace".to_string(),
                weights: vec![1.0],
                analyzer: Some(analyzer.value.clone()),
                highlights: highlights.is_some(),
            }
        }
        IndexKindSyntax::Provider {
            span,
            name,
            options,
        } => {
            if statement.surface != IndexDefinitionSurface::FastDbCreate {
                return unsupported(
                    *span,
                    "provider indexes use the labeled CREATE INDEX extension surface",
                );
            }
            if !name.value.eq_ignore_ascii_case("fts") {
                return unsupported(*span, "only the sealed fts provider is available");
            }
            parse_native_fts_options(options, paths.len())?
        }
        IndexKindSyntax::Btree => unreachable!("dispatched ordinary B-tree earlier"),
    };
    let options_json = options.canonical_json()?;

    with_schema_mutation(conn, execution, |state| {
        let snapshot = ready_snapshot_mut(state)?;
        if let Some(analyzer_name) = &options.analyzer {
            if !snapshot.analyzers.contains_key(analyzer_name) {
                return Err(FastDbError::Schema(format!(
                    "analyzer {analyzer_name:?} is not defined"
                )));
            }
        }
        let table = snapshot.tables.get(&statement.table.value).ok_or_else(|| {
            FastDbError::Schema(format!("table {:?} is not defined", statement.table.value))
        })?;
        if table.indexes.contains_key(&statement.name.value) {
            return Err(FastDbError::Constraint(format!(
                "index {:?} is already defined on table {:?}",
                statement.name.value, statement.table.value
            )));
        }
        let first_ordinal = snapshot
            .hidden_columns
            .values()
            .filter(|column| {
                column.table_id == table.id
                    && matches!(column.role, catalog::HiddenColumnRole::FtsText(_))
            })
            .count();
        let (index, hidden) = catalog::allocate_fts_index(
            table.id,
            first_ordinal,
            &statement.name.value,
            paths.clone(),
            definition.clone(),
            options_json.clone(),
        )?;
        let rows = read_documents(conn, table)?;
        let derived = rows
            .iter()
            .map(|(rid, document)| {
                let values = index
                    .paths
                    .iter()
                    .map(|path| derive_fts_text(document, path))
                    .collect::<Result<Vec<_>>>()?;
                Ok((rid.clone(), values))
            })
            .collect::<Result<Vec<_>>>()?;

        if !snapshot
            .capabilities
            .contains_key(catalog::BUILTIN_FTS_PROVIDER)
        {
            catalog::persist_fts_capability(conn)?;
        }
        for column in &hidden {
            catalog::persist_hidden_column(conn, column)?;
        }
        conn.check_failpoint(Failpoint::AfterFtsHiddenCatalog)?;
        for column in &hidden {
            conn.exec_bound(
                lower::physical_add_fts_column_ddl(&table.physical_name, &column.physical_name)?,
                vec![],
            )?;
            conn.check_failpoint(Failpoint::AfterFtsPhysicalColumn)?;
        }
        for (rid, values) in &derived {
            for (column, value) in hidden.iter().zip(values) {
                let (update, bindings) = lower::physical_update_fts_column_stmt(
                    &table.physical_name,
                    &column.physical_name,
                    rid,
                    value.as_deref(),
                )?;
                conn.exec_bound(update, bindings)?;
            }
        }
        conn.check_failpoint(Failpoint::AfterFtsBackfill)?;
        conn.exec_bound(
            crate::provider::index_provider(&index)?
                .create_statement(&index, &table.physical_name)?,
            vec![],
        )
        .map_err(|error| logical_index_constraint(error, &index.logical_name))?;
        conn.check_failpoint(Failpoint::AfterFtsProviderIndex)?;
        catalog::persist_index(conn, table, &index)?;

        let table = snapshot
            .tables
            .get_mut(&statement.table.value)
            .expect("table remained present under schema mutex");
        table
            .indexes
            .insert(statement.name.value.clone(), index.clone());
        for column in hidden {
            snapshot.hidden_columns.insert(column.id, column);
        }
        snapshot.capabilities.insert(
            catalog::BUILTIN_FTS_PROVIDER.to_string(),
            CapabilityRequirement {
                provider: catalog::BUILTIN_FTS_PROVIDER.to_string(),
                min_provider_version: catalog::BUILTIN_FTS_PROVIDER_VERSION,
                min_encoding_version: catalog::BUILTIN_FTS_ENCODING_VERSION,
            },
        );
        Ok(())
    })?;
    mark_fts_dirty(execution, &statement.table.value);
    Ok(StatementResult::None)
}

#[cfg(not(target_family = "wasm"))]
fn ensure_fts_available() -> Result<()> {
    Ok(())
}

#[cfg(target_family = "wasm")]
fn ensure_fts_available() -> Result<()> {
    Err(fts_unavailable())
}

#[cfg(target_family = "wasm")]
fn fts_unavailable() -> FastDbError {
    FastDbError::Schema("full-text search is unavailable on this WASM target".into())
}

fn parse_native_fts_options(
    options: &[turso_fastdb_parser::IndexOption],
    field_count: usize,
) -> Result<catalog::FtsIndexOptions> {
    let mut tokenizer = "default".to_string();
    let mut weights = vec![1.0; field_count];
    let mut seen = BTreeSet::new();
    for option in options {
        let key = option.key.value.to_ascii_lowercase();
        if !seen.insert(key.clone()) {
            return Err(FastDbError::Schema(format!(
                "duplicate FTS index option {:?}",
                option.key.value
            )));
        }
        match key.as_str() {
            "tokenizer" => {
                let ExprKind::String(value) = &option.value.kind else {
                    return Err(FastDbError::Schema(
                        "FTS tokenizer option must be a string".into(),
                    ));
                };
                tokenizer = value.to_ascii_lowercase();
                if !matches!(
                    tokenizer.as_str(),
                    "default" | "raw" | "simple" | "whitespace" | "ngram"
                ) {
                    return Err(FastDbError::Schema(format!(
                        "unsupported FTS tokenizer {value:?}"
                    )));
                }
            }
            "weights" => {
                let ExprKind::Array(values) = &option.value.kind else {
                    return Err(FastDbError::Schema(
                        "FTS weights option must be a numeric array".into(),
                    ));
                };
                weights = values
                    .iter()
                    .map(|value| match value.kind {
                        ExprKind::Integer(value) => Ok(value as f64),
                        ExprKind::Float(value) => Ok(value),
                        _ => Err(FastDbError::Schema(
                            "FTS weights must contain only numbers".into(),
                        )),
                    })
                    .collect::<Result<Vec<_>>>()?;
                if weights.len() != field_count
                    || weights
                        .iter()
                        .any(|weight| !weight.is_finite() || *weight <= 0.0)
                {
                    return Err(FastDbError::Schema(
                        "FTS weights must be finite positive values matching the field count"
                            .into(),
                    ));
                }
            }
            _ => {
                return Err(FastDbError::Schema(format!(
                    "unknown FTS index option {:?}",
                    option.key.value
                )));
            }
        }
    }
    Ok(catalog::FtsIndexOptions {
        surface: "fastdb".to_string(),
        tokenizer,
        weights,
        analyzer: None,
        highlights: false,
    })
}

fn derive_fts_text(document: &BTreeMap<String, Value>, path: &[String]) -> Result<Option<String>> {
    match crate::path::get_path(document, path) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Str(value)) => Ok(Some(value.clone())),
        Some(_) => Err(FastDbError::Constraint(format!(
            "full-text indexed field {} must be a string, null, or missing",
            crate::path::canonical_path(path)?
        ))),
    }
}

fn derived_hidden_values(
    snapshot: &CatalogSnapshot,
    table: &TableDefinition,
    document: &BTreeMap<String, Value>,
) -> Result<Vec<(String, Option<turso_core::Value>)>> {
    let mut columns = snapshot
        .hidden_columns
        .values()
        .filter(|column| {
            column.table_id == table.id
                && matches!(column.role, catalog::HiddenColumnRole::FtsText(_))
        })
        .collect::<Vec<_>>();
    columns.sort_by_key(|column| match column.role {
        catalog::HiddenColumnRole::FtsText(ordinal) => ordinal,
        catalog::HiddenColumnRole::Graph(_) => usize::MAX,
        catalog::HiddenColumnRole::Vector64(_) => usize::MAX,
    });
    let mut values = columns
        .into_iter()
        .map(|column| {
            let path_key = column.field_path_key.as_deref().ok_or_else(|| {
                FastDbError::format("FTS hidden column has no logical field ownership")
            })?;
            let path = crate::path::decode_canonical_path(path_key)?;
            Ok((
                column.physical_name.clone(),
                derive_fts_text(document, &path)?.map(turso_core::Value::build_text),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut vector_columns = snapshot
        .hidden_columns
        .values()
        .filter(|column| {
            column.table_id == table.id
                && matches!(column.role, catalog::HiddenColumnRole::Vector64(_))
        })
        .collect::<Vec<_>>();
    vector_columns.sort_by(|left, right| left.physical_name.cmp(&right.physical_name));
    for column in vector_columns {
        let path_key = column.field_path_key.as_deref().ok_or_else(|| {
            FastDbError::format("vector hidden column has no logical field ownership")
        })?;
        let dimension = column
            .dimension
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| FastDbError::format("vector hidden dimension is invalid"))?;
        let path = crate::path::decode_canonical_path(path_key)?;
        values.push((
            column.physical_name.clone(),
            derive_vector64(document, &path, dimension)?,
        ));
    }
    Ok(values)
}

fn derive_vector64(
    document: &BTreeMap<String, Value>,
    path: &[String],
    dimension: u32,
) -> Result<Option<turso_core::Value>> {
    let Some(value) = crate::path::get_path(document, path) else {
        return Ok(None);
    };
    if matches!(value, Value::Null) {
        return Ok(None);
    }
    let Value::Array(elements) = value else {
        return Err(FastDbError::Constraint(format!(
            "vector field {} must be an array",
            crate::path::canonical_path(path)?
        )));
    };
    if elements.len() != dimension as usize {
        return Err(FastDbError::Constraint(format!(
            "vector field {} requires exactly {dimension} elements",
            crate::path::canonical_path(path)?
        )));
    }
    let capacity = elements
        .len()
        .checked_mul(std::mem::size_of::<f64>())
        .and_then(|bytes| bytes.checked_add(1))
        .ok_or_else(|| FastDbError::Constraint("vector encoding is too large".into()))?;
    let mut encoded = Vec::with_capacity(capacity);
    for element in elements {
        let number = match element {
            Value::Integer(value) => *value as f64,
            Value::Float(value) => *value,
            _ => {
                return Err(FastDbError::Constraint(
                    "vector elements must be finite numbers".into(),
                ))
            }
        };
        if !number.is_finite() {
            return Err(FastDbError::Constraint(
                "vector elements must be finite numbers".into(),
            ));
        }
        encoded.extend_from_slice(&number.to_le_bytes());
    }
    encoded.push(2);
    Ok(Some(turso_core::Value::from_blob(encoded)))
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
    if resolved.kind == IndexKind::Fts {
        return Err(FastDbError::Schema(
            "removing FTS indexes is deferred until provider-owned hidden columns can be removed atomically"
                .into(),
        ));
    }
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
        *state = CatalogState::Ready(Box::new(snapshot));
    }
    ready_snapshot_mut(state)
}

fn ready_snapshot_mut(state: &mut CatalogState) -> Result<&mut catalog::CatalogSnapshot> {
    match state {
        CatalogState::Ready(snapshot) => Ok(snapshot.as_mut()),
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
        if index.kind != IndexKind::Btree {
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
            Value::None
            | Value::Null
            | Value::Decimal(_)
            | Value::Bytes(_)
            | Value::Duration(_)
            | Value::Datetime(_)
            | Value::Uuid(_)
            | Value::Array(_)
            | Value::Object(_)
            | Value::Set(_)
            | Value::Range(_)
            | Value::Regex(_)
            | Value::RecordId(_)
            | Value::Table(_)
            | Value::File(_) => {
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
    let (table_name, selector) = select_target_parts(&statement.target)?;
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
    let encoded_id = selector.id().map(encode_rid).transpose()?;
    lower::physical_select_predicates_stmt(&table.physical_name, encoded_id.as_deref(), &predicates)
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
