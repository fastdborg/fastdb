#![no_main]

use libfuzzer_sys::fuzz_target;
use turso_fastdb_parser::{
    Accessor, AlterFieldChange, CreateData, Expr, ExprKind, FieldPath, GroupClause, InsertData,
    ProjectionList, ReturnKind, SchemaType, SchemaTypeKind, Script, ScriptBlock, SelectTarget,
    Span, Statement, Target, UpdateData,
};

fuzz_target!(|bytes: &[u8]| {
    let Ok(source) = std::str::from_utf8(bytes) else {
        return;
    };
    let Ok(script) = turso_fastdb_parser::parse(source) else {
        return;
    };
    validate_script(&script, source.len());
    let debug = format!("{script:?}");
    assert!(!debug.is_empty());
});

fn validate_script(script: &Script, source_len: usize) {
    valid(script.span, source_len);
    for statement in &script.statements {
        let span = statement.span();
        child(script.span, span, source_len);
        validate_statement(statement, source_len);
    }
}

fn validate_statement(statement: &Statement, source_len: usize) {
    let parent = statement.span();
    match statement {
        Statement::Create(statement) => {
            if let Some(span) = statement.only {
                child(parent, span, source_len);
            }
            validate_target(parent, &statement.target, source_len);
            if let Some(data) = &statement.data {
                validate_create_data(parent, data, source_len);
            }
            validate_return_clause(parent, statement.return_clause.as_ref(), source_len);
            if let Some(timeout) = &statement.timeout {
                validate_expr(parent, timeout, source_len);
            }
        }
        Statement::Insert(statement) => {
            child(parent, statement.table.span, source_len);
            match &statement.data {
                InsertData::Expression(expression) => validate_expr(parent, expression, source_len),
                InsertData::Values { fields, rows } => {
                    for field in fields {
                        child(parent, field.span, source_len);
                    }
                    for expression in rows.iter().flatten() {
                        validate_expr(parent, expression, source_len);
                    }
                }
            }
            validate_assignments(parent, &statement.on_duplicate, source_len);
            validate_return_clause(parent, statement.return_clause.as_ref(), source_len);
            if let Some(timeout) = &statement.timeout {
                validate_expr(parent, timeout, source_len);
            }
        }
        Statement::Relate(statement) => {
            if let Some(span) = statement.only {
                child(parent, span, source_len);
            }
            validate_expr(parent, &statement.from, source_len);
            child(parent, statement.relation.span, source_len);
            validate_expr(parent, &statement.to, source_len);
            if let Some(data) = &statement.data {
                validate_create_data(parent, data, source_len);
            }
            validate_return_clause(parent, statement.return_clause.as_ref(), source_len);
        }
        Statement::Select(statement) => validate_select(parent, statement, source_len),
        Statement::Explain(statement) => {
            child(parent, statement.select.span, source_len);
            validate_statement(&Statement::Select(statement.select.clone()), source_len);
        }
        Statement::RemoveIndex(statement) | Statement::RebuildIndex(statement) => {
            child(parent, statement.name.span, source_len);
            if let Some(table_keyword) = statement.table_keyword {
                child(parent, table_keyword, source_len);
            }
            child(parent, statement.table.span, source_len);
        }
        Statement::Update(statement) | Statement::Upsert(statement) => {
            validate_target(parent, &statement.target, source_len);
            match &statement.data {
                UpdateData::Content(expression)
                | UpdateData::Merge(expression)
                | UpdateData::Patch(expression)
                | UpdateData::Replace(expression) => validate_expr(parent, expression, source_len),
                UpdateData::Set(assignments) => {
                    validate_assignments(parent, assignments, source_len)
                }
                UpdateData::Unset(paths) => {
                    for path in paths {
                        validate_path(parent, path, source_len);
                    }
                }
            }
            if let Some(condition) = &statement.condition {
                validate_expr(parent, condition, source_len);
            }
            validate_return_clause(parent, statement.return_clause.as_ref(), source_len);
            if let Some(timeout) = &statement.timeout {
                validate_expr(parent, timeout, source_len);
            }
        }
        Statement::Delete(statement) => {
            validate_target(parent, &statement.target, source_len);
            if let Some(condition) = &statement.condition {
                validate_expr(parent, condition, source_len);
            }
            validate_return_clause(parent, statement.return_clause.as_ref(), source_len);
            if let Some(timeout) = &statement.timeout {
                validate_expr(parent, timeout, source_len);
            }
        }
        Statement::DefineTable(statement) => {
            child(parent, statement.name.span, source_len);
            child(parent, statement.mode.span, source_len);
            match &statement.kind {
                turso_fastdb_parser::TableKindSyntax::Normal {
                    type_span: Some(span),
                } => child(parent, *span, source_len),
                turso_fastdb_parser::TableKindSyntax::Normal { type_span: None } => {}
                turso_fastdb_parser::TableKindSyntax::Relation(relation) => {
                    child(parent, relation.span, source_len);
                    if let Some(input) = &relation.input {
                        child(relation.span, input.span, source_len);
                    }
                    if let Some(output) = &relation.output {
                        child(relation.span, output.span, source_len);
                    }
                    if let Some(enforced) = relation.enforced {
                        child(relation.span, enforced, source_len);
                    }
                }
            }
            validate_permissions(parent, &statement.permissions, source_len);
            if let Some(view) = &statement.view {
                validate_select(parent, view, source_len);
            }
        }
        Statement::DefineField(statement) => {
            validate_path(parent, &statement.path, source_len);
            if let Some(span) = statement.table_keyword {
                child(parent, span, source_len);
            }
            child(parent, statement.table.span, source_len);
            validate_schema_type(parent, &statement.ty, source_len);
            if let Some(default) = &statement.default {
                child(parent, default.span, source_len);
                validate_expr(default.span, &default.value, source_len);
            }
            if let Some(value) = &statement.value {
                validate_expr(parent, value, source_len);
            }
            if let Some(assertion) = &statement.assert {
                validate_expr(parent, assertion, source_len);
            }
            validate_permissions(parent, &statement.permissions, source_len);
        }
        Statement::DefineAnalyzer(statement) => {
            child(parent, statement.name.span, source_len);
            child(parent, statement.tokenizer.span, source_len);
        }
        Statement::DefineIndex(statement) => {
            child(parent, statement.name.span, source_len);
            if let Some(span) = statement.table_keyword {
                child(parent, span, source_len);
            }
            child(parent, statement.table.span, source_len);
            for field in &statement.fields {
                validate_path(parent, field, source_len);
            }
            if let Some(span) = statement.unique {
                child(parent, span, source_len);
            }
            match &statement.kind {
                turso_fastdb_parser::IndexKindSyntax::Btree => {}
                turso_fastdb_parser::IndexKindSyntax::Fulltext {
                    span,
                    analyzer,
                    highlights,
                } => {
                    child(parent, *span, source_len);
                    child(*span, analyzer.span, source_len);
                    if let Some(highlights) = highlights {
                        child(*span, *highlights, source_len);
                    }
                }
                turso_fastdb_parser::IndexKindSyntax::Provider {
                    span,
                    name,
                    options,
                } => {
                    child(parent, *span, source_len);
                    child(*span, name.span, source_len);
                    for option in options {
                        child(*span, option.span, source_len);
                        child(option.span, option.key.span, source_len);
                        validate_expr(option.span, &option.value, source_len);
                    }
                }
            }
        }
        Statement::Let(statement) => {
            child(parent, statement.name.span, source_len);
            validate_expr(parent, &statement.value, source_len);
        }
        Statement::ScriptReturn(statement)
        | Statement::Throw(statement)
        | Statement::Sleep(statement) => validate_expr(parent, &statement.value, source_len),
        Statement::If(statement) => {
            for (condition, block) in &statement.branches {
                validate_expr(parent, condition, source_len);
                validate_script_block(parent, block, source_len);
            }
            if let Some(block) = &statement.otherwise {
                validate_script_block(parent, block, source_len);
            }
        }
        Statement::For(statement) => {
            child(parent, statement.binding.span, source_len);
            validate_expr(parent, &statement.iterable, source_len);
            validate_script_block(parent, &statement.body, source_len);
        }
        Statement::Break(_) | Statement::Continue(_) => {}
        Statement::DefineParam(statement) => {
            child(parent, statement.name.span, source_len);
            validate_expr(parent, &statement.value, source_len);
            validate_permissions(parent, &statement.permissions, source_len);
        }
        Statement::AlterParam(statement) => {
            child(parent, statement.name.span, source_len);
            if let Some(value) = &statement.value {
                validate_expr(parent, value, source_len);
            }
            if let Some(permissions) = &statement.permissions {
                validate_permissions(parent, permissions, source_len);
            }
        }
        Statement::RemoveParam(statement) => child(parent, statement.name.span, source_len),
        Statement::DefineFunction(statement) => {
            for segment in &statement.name {
                child(parent, segment.span, source_len);
            }
            for argument in &statement.arguments {
                child(parent, argument.span, source_len);
                child(argument.span, argument.name.span, source_len);
                validate_schema_type(argument.span, &argument.ty, source_len);
            }
            validate_script_block(parent, &statement.body, source_len);
            validate_permissions(parent, &statement.permissions, source_len);
        }
        Statement::AlterFunction(statement) => {
            for segment in &statement.name {
                child(parent, segment.span, source_len);
            }
            validate_permissions(parent, &statement.permissions, source_len);
        }
        Statement::RemoveFunction(statement) => {
            for segment in &statement.name {
                child(parent, segment.span, source_len);
            }
        }
        Statement::DefineEvent(statement) => {
            child(parent, statement.name.span, source_len);
            child(parent, statement.table.span, source_len);
            if let Some(condition) = &statement.condition {
                validate_expr(parent, condition, source_len);
            }
            validate_script_block(parent, &statement.action.block, source_len);
        }
        Statement::AlterEvent(statement) => {
            child(parent, statement.name.span, source_len);
            child(parent, statement.table.span, source_len);
            if let Some(Some(condition)) = &statement.changes.condition {
                validate_expr(parent, condition, source_len);
            }
            if let Some(Some(action)) = &statement.changes.action {
                validate_script_block(parent, &action.block, source_len);
            }
        }
        Statement::RemoveEvent(statement) => {
            child(parent, statement.name.span, source_len);
            child(parent, statement.table.span, source_len);
        }
        Statement::InfoDatabase(_) => {}
        Statement::AlterTable(statement) => {
            child(parent, statement.name.span, source_len);
            if let Some(permissions) = &statement.permissions {
                validate_permissions(parent, permissions, source_len);
            }
        }
        Statement::RemoveTable(statement) => child(parent, statement.name.span, source_len),
        Statement::InfoTable(statement) => child(parent, statement.table.span, source_len),
        Statement::AlterField(statement) => {
            validate_path(parent, &statement.path, source_len);
            child(parent, statement.table.span, source_len);
            match &statement.change {
                AlterFieldChange::Type(ty) => validate_schema_type(parent, ty, source_len),
                AlterFieldChange::Default(default) => {
                    child(parent, default.span, source_len);
                    validate_expr(default.span, &default.value, source_len);
                }
                AlterFieldChange::Value(value) | AlterFieldChange::Assert(value) => {
                    validate_expr(parent, value, source_len)
                }
                AlterFieldChange::Permissions(permissions) => {
                    validate_permissions(parent, permissions, source_len)
                }
                AlterFieldChange::Flexible
                | AlterFieldChange::Readonly
                | AlterFieldChange::Reference(_)
                | AlterFieldChange::Comment(_)
                | AlterFieldChange::DropType
                | AlterFieldChange::DropFlexible
                | AlterFieldChange::DropDefault
                | AlterFieldChange::DropValue
                | AlterFieldChange::DropAssert
                | AlterFieldChange::DropReadonly
                | AlterFieldChange::DropReference
                | AlterFieldChange::DropComment => {}
            }
        }
        Statement::RemoveField(statement) => {
            validate_path(parent, &statement.path, source_len);
            child(parent, statement.table.span, source_len);
        }
        Statement::Begin(_) | Statement::Commit(_) | Statement::Cancel(_) => {}
    }
}

fn validate_script_block(parent: Span, block: &ScriptBlock, source_len: usize) {
    child(parent, block.span, source_len);
    for statement in &block.statements {
        child(block.span, statement.span(), source_len);
        validate_statement(statement, source_len);
    }
}

fn validate_create_data(parent: Span, data: &CreateData, source_len: usize) {
    match data {
        CreateData::Content(expression) => validate_expr(parent, expression, source_len),
        CreateData::Set(assignments) => validate_assignments(parent, assignments, source_len),
    }
}

fn validate_assignments(
    parent: Span,
    assignments: &[turso_fastdb_parser::Assignment],
    source_len: usize,
) {
    for assignment in assignments {
        child(parent, assignment.span, source_len);
        validate_path(assignment.span, &assignment.path, source_len);
        child(assignment.span, assignment.operator.span, source_len);
        validate_expr(assignment.span, &assignment.value, source_len);
    }
}

fn validate_return_clause(
    parent: Span,
    clause: Option<&turso_fastdb_parser::ReturnClause>,
    source_len: usize,
) {
    let Some(clause) = clause else {
        return;
    };
    child(parent, clause.span, source_len);
    child(clause.span, clause.kind.span, source_len);
    if let ReturnKind::Value(expression) = &clause.kind.value {
        validate_expr(clause.span, expression, source_len);
    }
}

fn validate_select(
    parent: Span,
    statement: &turso_fastdb_parser::SelectStatement,
    source_len: usize,
) {
    child(parent, statement.span, source_len);
    match &statement.projections {
        ProjectionList::All(span) => child(statement.span, *span, source_len),
        ProjectionList::Fields(projections) => {
            for projection in projections {
                child(statement.span, projection.span, source_len);
                validate_expr(projection.span, &projection.expression, source_len);
                if let Some(alias) = &projection.alias {
                    child(projection.span, alias.span, source_len);
                }
            }
        }
    }
    validate_select_target(statement.span, &statement.target, source_len);
    for target in &statement.additional_targets {
        validate_select_target(statement.span, target, source_len);
    }
    if let Some(condition) = &statement.condition {
        validate_expr(statement.span, condition, source_len);
    }
    for path in statement.split.iter().chain(&statement.omit) {
        validate_path(statement.span, path, source_len);
    }
    if let Some(group) = &statement.group {
        match group {
            GroupClause::All(span) => child(statement.span, *span, source_len),
            GroupClause::By(expressions) => {
                for expression in expressions {
                    validate_expr(statement.span, expression, source_len);
                }
            }
        }
    }
    for order in &statement.order_by {
        child(statement.span, order.span, source_len);
        validate_path(order.span, &order.path, source_len);
        child(order.span, order.direction.span, source_len);
    }
    if let Some(limit) = &statement.limit {
        child(statement.span, limit.span, source_len);
    }
    if let Some(limit) = &statement.limit_expression {
        validate_expr(statement.span, limit, source_len);
    }
    if let Some(start) = &statement.start {
        child(statement.span, start.span, source_len);
    }
    if let Some(start) = &statement.start_expression {
        validate_expr(statement.span, start, source_len);
    }
    for path in &statement.fetch {
        validate_path(statement.span, path, source_len);
    }
}

fn validate_select_target(parent: Span, target: &SelectTarget, source_len: usize) {
    match target {
        SelectTarget::Target(target) => validate_target(parent, target, source_len),
        SelectTarget::Expression(expression) => validate_expr(parent, expression, source_len),
        SelectTarget::Subquery(select) => validate_select(parent, select, source_len),
    }
}

fn validate_permissions(
    parent: Span,
    permissions: &turso_fastdb_parser::SchemaPermissions,
    source_len: usize,
) {
    if let turso_fastdb_parser::SchemaPermissions::Specific(clauses) = permissions {
        for clause in clauses {
            child(parent, clause.span, source_len);
            if let turso_fastdb_parser::SchemaPermissionValue::Where { expression, .. } =
                &clause.value
            {
                validate_expr(clause.span, expression, source_len);
            }
        }
    }
}

fn validate_target(parent: Span, target: &Target, source_len: usize) {
    child(parent, target.span(), source_len);
    match target {
        Target::Table(table) => child(table.span, table.name.span, source_len),
        Target::Record(record) => {
            child(record.span, record.table.span, source_len);
            child(record.span, record.id.span, source_len);
            if let turso_fastdb_parser::RecordIdPartKind::Complex(expression) = &record.id.kind {
                validate_expr(record.id.span, expression, source_len);
            }
        }
        Target::RecordRange(range) => {
            child(range.span, range.table.span, source_len);
            if let Some(start) = &range.start {
                child(range.span, start.span, source_len);
                if let turso_fastdb_parser::RecordIdPartKind::Complex(expression) = &start.kind {
                    validate_expr(start.span, expression, source_len);
                }
            }
            if let Some(end) = &range.end {
                child(range.span, end.span, source_len);
                if let turso_fastdb_parser::RecordIdPartKind::Complex(expression) = &end.kind {
                    validate_expr(end.span, expression, source_len);
                }
            }
        }
        Target::Expression(expression) => validate_expr(parent, expression, source_len),
        Target::Batch { span, target } => validate_target(*span, target, source_len),
    }
}

fn validate_path(parent: Span, path: &FieldPath, source_len: usize) {
    child(parent, path.span, source_len);
    for segment in &path.segments {
        child(path.span, segment.span, source_len);
    }
}

fn validate_schema_type(parent: Span, ty: &SchemaType, source_len: usize) {
    child(parent, ty.span, source_len);
    match &ty.kind {
        SchemaTypeKind::Union(variants) => {
            for variant in variants {
                validate_schema_type(ty.span, variant, source_len);
            }
        }
        SchemaTypeKind::TypedArray { element, .. } | SchemaTypeKind::Option(element) => {
            validate_schema_type(ty.span, element, source_len)
        }
        SchemaTypeKind::Set {
            element: Some(element),
            ..
        } => validate_schema_type(ty.span, element, source_len),
        SchemaTypeKind::Record { tables } => {
            for table in tables {
                child(ty.span, table.span, source_len);
            }
        }
        _ => {}
    }
}

fn validate_expr(parent: Span, expression: &Expr, source_len: usize) {
    child(parent, expression.span, source_len);
    match &expression.kind {
        ExprKind::Array(elements) | ExprKind::DestructureList(elements) => {
            for element in elements {
                validate_expr(expression.span, element, source_len);
            }
        }
        ExprKind::Object(fields) => {
            for field in fields {
                child(expression.span, field.span, source_len);
                child(field.span, field.key.span, source_len);
                validate_expr(field.span, &field.value, source_len);
            }
        }
        ExprKind::RecordId(record) => {
            child(expression.span, record.span, source_len);
            child(record.span, record.table.span, source_len);
            child(record.span, record.id.span, source_len);
            if let turso_fastdb_parser::RecordIdPartKind::Complex(value) = &record.id.kind {
                validate_expr(record.id.span, value, source_len);
            }
        }
        ExprKind::FieldPath(path) => validate_path(expression.span, path, source_len),
        ExprKind::Destructure { target, fields } => {
            validate_expr(expression.span, target, source_len);
            for field in fields {
                child(expression.span, field.span, source_len);
            }
        }
        ExprKind::Access { target, accessor } => {
            validate_expr(expression.span, target, source_len);
            match accessor {
                Accessor::Field(field) => child(expression.span, field.span, source_len),
                Accessor::Index(index) => validate_expr(expression.span, index, source_len),
                Accessor::Last(span) => child(expression.span, *span, source_len),
                Accessor::Slice {
                    start, end, span, ..
                } => {
                    child(expression.span, *span, source_len);
                    if let Some(start) = start {
                        validate_expr(*span, start, source_len);
                    }
                    if let Some(end) = end {
                        validate_expr(*span, end, source_len);
                    }
                }
            }
        }
        ExprKind::Cast { ty, value } => {
            validate_schema_type(expression.span, ty, source_len);
            validate_expr(expression.span, value, source_len);
        }
        ExprKind::Range(range) => {
            child(expression.span, range.operator_span, source_len);
            if let Some(start) = &range.start {
                validate_expr(expression.span, start, source_len);
            }
            if let Some(end) = &range.end {
                validate_expr(expression.span, end, source_len);
            }
        }
        ExprKind::FunctionCall { name, arguments } => {
            for segment in name {
                child(expression.span, segment.span, source_len);
            }
            for argument in arguments {
                validate_expr(expression.span, argument, source_len);
            }
        }
        ExprKind::NamespacedValue { name } => {
            for segment in name {
                child(expression.span, segment.span, source_len);
            }
        }
        ExprKind::Closure(closure) => {
            for parameter in &closure.parameters {
                child(expression.span, parameter.span, source_len);
            }
            validate_expr(expression.span, &closure.body, source_len);
        }
        ExprKind::Knn(knn) => {
            child(expression.span, knn.operator_span, source_len);
            child(expression.span, knn.k.span, source_len);
            child(expression.span, knn.metric.span, source_len);
            validate_expr(expression.span, &knn.field, source_len);
            validate_expr(expression.span, &knn.query, source_len);
        }
        ExprKind::Traversal(traversal) => {
            for hop in &traversal.hops {
                child(expression.span, hop.span, source_len);
                child(hop.span, hop.direction.span, source_len);
                child(hop.span, hop.relation.span, source_len);
                child(hop.span, hop.endpoint_table.span, source_len);
            }
        }
        ExprKind::Unary { operator, operand } => {
            child(expression.span, operator.span, source_len);
            validate_expr(expression.span, operand, source_len);
        }
        ExprKind::Binary {
            left,
            operator,
            right,
        } => {
            validate_expr(expression.span, left, source_len);
            child(expression.span, operator.span, source_len);
            validate_expr(expression.span, right, source_len);
        }
        ExprKind::Parenthesized(inner) => validate_expr(expression.span, inner, source_len),
        ExprKind::None
        | ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Parameter(_) => {}
    }
}

fn child(parent: Span, span: Span, source_len: usize) {
    valid(span, source_len);
    assert!(span.offset >= parent.offset);
    assert!(span.end() <= parent.end());
}

fn valid(span: Span, source_len: usize) {
    assert!(span.is_within(source_len));
}
