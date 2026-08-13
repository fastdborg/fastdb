#![no_main]

use libfuzzer_sys::fuzz_target;
use turso_fastdb_parser::{
    CreateData, Expr, ExprKind, FieldPath, ProjectionList, SchemaType, SchemaTypeKind, Script,
    Span, Statement, Target,
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
            match &statement.data {
                CreateData::Content(expression) => validate_expr(parent, expression, source_len),
                CreateData::Set(assignments) => {
                    for assignment in assignments {
                        child(parent, assignment.span, source_len);
                        validate_path(assignment.span, &assignment.path, source_len);
                        validate_expr(assignment.span, &assignment.value, source_len);
                    }
                }
            }
            if let Some(clause) = &statement.return_clause {
                child(parent, clause.span, source_len);
                child(clause.span, clause.kind.span, source_len);
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
                match data {
                    CreateData::Content(expression) => {
                        validate_expr(parent, expression, source_len);
                    }
                    CreateData::Set(assignments) => {
                        for assignment in assignments {
                            child(parent, assignment.span, source_len);
                            validate_path(assignment.span, &assignment.path, source_len);
                            validate_expr(assignment.span, &assignment.value, source_len);
                        }
                    }
                }
            }
            if let Some(clause) = &statement.return_clause {
                child(parent, clause.span, source_len);
                child(clause.span, clause.kind.span, source_len);
            }
        }
        Statement::Select(statement) => {
            match &statement.projections {
                ProjectionList::All(span) => child(parent, *span, source_len),
                ProjectionList::Fields(projections) => {
                    for projection in projections {
                        child(parent, projection.span, source_len);
                        validate_expr(projection.span, &projection.expression, source_len);
                        if let Some(alias) = &projection.alias {
                            child(projection.span, alias.span, source_len);
                        }
                    }
                }
            }
            if let Some(span) = statement.only {
                child(parent, span, source_len);
            }
            validate_target(parent, &statement.target, source_len);
            if let Some(condition) = &statement.condition {
                validate_expr(parent, condition, source_len);
            }
            for order in &statement.order_by {
                child(parent, order.span, source_len);
                validate_path(order.span, &order.path, source_len);
                child(order.span, order.direction.span, source_len);
            }
            if let Some(limit) = &statement.limit {
                child(parent, limit.span, source_len);
            }
            if let Some(start) = &statement.start {
                child(parent, start.span, source_len);
            }
        }
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
        Statement::Update(statement) => {
            validate_target(parent, &statement.target, source_len);
            for assignment in &statement.assignments {
                child(parent, assignment.span, source_len);
                validate_path(assignment.span, &assignment.path, source_len);
                validate_expr(assignment.span, &assignment.value, source_len);
            }
            if let Some(condition) = &statement.condition {
                validate_expr(parent, condition, source_len);
            }
            if let Some(clause) = &statement.return_clause {
                child(parent, clause.span, source_len);
                child(clause.span, clause.kind.span, source_len);
            }
        }
        Statement::Delete(statement) => {
            validate_target(parent, &statement.target, source_len);
            if let Some(condition) = &statement.condition {
                validate_expr(parent, condition, source_len);
            }
            if let Some(clause) = &statement.return_clause {
                child(parent, clause.span, source_len);
                child(clause.span, clause.kind.span, source_len);
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
        }
        Statement::DefineField(statement) => {
            validate_path(parent, &statement.path, source_len);
            if let Some(span) = statement.table_keyword {
                child(parent, span, source_len);
            }
            child(parent, statement.table.span, source_len);
            validate_schema_type(parent, &statement.ty, source_len);
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
        Statement::Begin(_) | Statement::Commit(_) | Statement::Cancel(_) => {}
    }
}

fn validate_target(parent: Span, target: &Target, source_len: usize) {
    child(parent, target.span(), source_len);
    match target {
        Target::Table(table) => child(table.span, table.name.span, source_len),
        Target::Record(record) => {
            child(record.span, record.table.span, source_len);
            child(record.span, record.id.span, source_len);
        }
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
    if let SchemaTypeKind::Option(inner) = &ty.kind {
        validate_schema_type(ty.span, inner, source_len);
    }
}

fn validate_expr(parent: Span, expression: &Expr, source_len: usize) {
    child(parent, expression.span, source_len);
    match &expression.kind {
        ExprKind::Array(elements) => {
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
        }
        ExprKind::FieldPath(path) => validate_path(expression.span, path, source_len),
        ExprKind::FunctionCall { name, arguments } => {
            for segment in name {
                child(expression.span, segment.span, source_len);
            }
            for argument in arguments {
                validate_expr(expression.span, argument, source_len);
            }
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
        ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
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
