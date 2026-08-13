//! Authoritative FastDB expression evaluation over decoded documents.

use crate::decode::{RecordId, RecordIdValue, Value};
use crate::error::{FastDbError, Result};
use crate::Params;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use turso_fastdb_parser::{BinaryOperator, Expr, ExprKind, FieldPath, Statement, UnaryOperator};

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EvalValue {
    Missing,
    Present(Value),
}

impl EvalValue {
    pub(crate) fn into_projection(self) -> Value {
        match self {
            Self::Missing => Value::Null,
            Self::Present(value) => value,
        }
    }

    pub(crate) fn truthy(&self) -> bool {
        match self {
            Self::Missing | Self::Present(Value::Null) | Self::Present(Value::Bool(false)) => false,
            Self::Present(Value::Integer(0)) => false,
            Self::Present(Value::Float(value)) if *value == 0.0 => false,
            Self::Present(Value::Str(value)) => !value.is_empty(),
            Self::Present(Value::Array(value)) => !value.is_empty(),
            Self::Present(Value::Object(value)) => !value.is_empty(),
            Self::Present(
                Value::Bool(_) | Value::Integer(_) | Value::Float(_) | Value::RecordId(_),
            ) => true,
        }
    }
}

pub(crate) struct EvalContext<'a> {
    pub(crate) document: &'a BTreeMap<String, Value>,
    pub(crate) id: &'a RecordId,
    pub(crate) endpoints: Option<(&'a RecordId, &'a RecordId)>,
    pub(crate) params: &'a Params,
}

pub(crate) fn validate_parameter_references(statement: &Statement, params: &Params) -> Result<()> {
    let mut names = Vec::new();
    match statement {
        Statement::Create(statement) => match &statement.data {
            turso_fastdb_parser::CreateData::Content(expression) => {
                reject_all_functions(expression)?;
                reject_unavailable_functions(expression)?;
                collect_parameters(expression, &mut names)
            }
            turso_fastdb_parser::CreateData::Set(assignments) => {
                for assignment in assignments {
                    reject_all_functions(&assignment.value)?;
                    reject_unavailable_functions(&assignment.value)?;
                    collect_parameters(&assignment.value, &mut names);
                }
            }
        },
        Statement::Relate(statement) => {
            collect_parameters(&statement.from, &mut names);
            collect_parameters(&statement.to, &mut names);
            if let Some(data) = &statement.data {
                match data {
                    turso_fastdb_parser::CreateData::Content(expression) => {
                        reject_all_functions(expression)?;
                        reject_unavailable_functions(expression)?;
                        collect_parameters(expression, &mut names);
                    }
                    turso_fastdb_parser::CreateData::Set(assignments) => {
                        for assignment in assignments {
                            reject_all_functions(&assignment.value)?;
                            reject_unavailable_functions(&assignment.value)?;
                            collect_parameters(&assignment.value, &mut names);
                        }
                    }
                }
            }
        }
        Statement::Select(statement) => {
            if let turso_fastdb_parser::ProjectionList::Fields(projections) = &statement.projections
            {
                for projection in projections {
                    reject_unavailable_functions(&projection.expression)?;
                    collect_parameters(&projection.expression, &mut names);
                }
            }
            if let Some(condition) = &statement.condition {
                reject_unavailable_functions(condition)?;
                collect_parameters(condition, &mut names);
            }
        }
        Statement::Update(statement) => {
            for assignment in &statement.assignments {
                reject_all_functions(&assignment.value)?;
                reject_unavailable_functions(&assignment.value)?;
                collect_parameters(&assignment.value, &mut names);
            }
            if let Some(condition) = &statement.condition {
                reject_unavailable_functions(condition)?;
                collect_parameters(condition, &mut names);
            }
        }
        Statement::Delete(statement) => {
            if let Some(condition) = &statement.condition {
                reject_unavailable_functions(condition)?;
                collect_parameters(condition, &mut names);
            }
        }
        Statement::Explain(statement) => {
            if let turso_fastdb_parser::ProjectionList::Fields(projections) =
                &statement.select.projections
            {
                for projection in projections {
                    reject_unavailable_functions(&projection.expression)?;
                    collect_parameters(&projection.expression, &mut names);
                }
            }
            if let Some(condition) = &statement.select.condition {
                reject_unavailable_functions(condition)?;
                collect_parameters(condition, &mut names);
            }
        }
        Statement::DefineTable(_)
        | Statement::DefineField(_)
        | Statement::DefineAnalyzer(_)
        | Statement::DefineIndex(_)
        | Statement::RemoveIndex(_)
        | Statement::RebuildIndex(_)
        | Statement::Begin(_)
        | Statement::Commit(_)
        | Statement::Cancel(_) => {}
    }
    if let Some(name) = names.into_iter().find(|name| !params.contains_key(*name)) {
        return Err(FastDbError::Schema(format!(
            "missing value for parameter ${name}"
        )));
    }
    Ok(())
}

fn collect_parameters<'a>(expression: &'a Expr, names: &mut Vec<&'a str>) {
    match &expression.kind {
        ExprKind::Parameter(name) => names.push(name),
        ExprKind::Array(values) => {
            for value in values {
                collect_parameters(value, names);
            }
        }
        ExprKind::Object(fields) => {
            for field in fields {
                collect_parameters(&field.value, names);
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => {
            collect_parameters(operand, names)
        }
        ExprKind::Binary { left, right, .. } => {
            collect_parameters(left, names);
            collect_parameters(right, names);
        }
        ExprKind::FunctionCall { arguments, .. } => {
            for argument in arguments {
                collect_parameters(argument, names);
            }
        }
        ExprKind::Knn(knn) => {
            collect_parameters(&knn.field, names);
            collect_parameters(&knn.query, names);
        }
        ExprKind::Traversal(_) => {}
        ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::String(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_) => {}
    }
}

fn reject_unavailable_functions(expression: &Expr) -> Result<()> {
    match &expression.kind {
        ExprKind::FunctionCall { name, arguments } => {
            for argument in arguments {
                reject_unavailable_functions(argument)?;
            }
            let normalized = name
                .iter()
                .map(|segment| segment.value.to_ascii_lowercase())
                .collect::<Vec<_>>();
            if matches!(
                normalized.as_slice(),
                [name] if matches!(name.as_str(), "fts_match" | "fts_score" | "fts_highlight")
            ) || matches!(
                normalized.as_slice(),
                [namespace, name]
                    if namespace == "search" && matches!(name.as_str(), "score" | "highlight")
            ) || matches!(
                normalized.as_slice(),
                [vector, distance, name]
                    if vector == "vector"
                        && distance == "distance"
                        && matches!(name.as_str(), "euclidean" | "knn")
            ) || matches!(
                normalized.as_slice(),
                [vector, similarity, name]
                    if vector == "vector" && similarity == "similarity" && name == "cosine"
            ) {
                Ok(())
            } else {
                Err(FastDbError::UnsupportedSyntax(
                    turso_fastdb_parser::ParseError::unsupported(
                        "function is outside the Phase 8 FTS subset",
                        expression.span,
                    ),
                ))
            }
        }
        ExprKind::Traversal(_) => Ok(()),
        ExprKind::Knn(knn) => {
            reject_unavailable_functions(&knn.field)?;
            reject_unavailable_functions(&knn.query)
        }
        ExprKind::Array(values) => {
            for value in values {
                reject_unavailable_functions(value)?;
            }
            Ok(())
        }
        ExprKind::Object(fields) => {
            for field in fields {
                reject_unavailable_functions(&field.value)?;
            }
            Ok(())
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => {
            reject_unavailable_functions(operand)
        }
        ExprKind::Binary { left, right, .. } => {
            reject_unavailable_functions(left)?;
            reject_unavailable_functions(right)
        }
        ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::String(_)
        | ExprKind::Parameter(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_) => Ok(()),
    }
}

fn reject_all_functions(expression: &Expr) -> Result<()> {
    match &expression.kind {
        ExprKind::FunctionCall { .. } => Err(FastDbError::UnsupportedSyntax(
            turso_fastdb_parser::ParseError::unsupported(
                "function calls are unavailable in mutation values",
                expression.span,
            ),
        )),
        ExprKind::Knn(_) => Err(FastDbError::UnsupportedSyntax(
            turso_fastdb_parser::ParseError::unsupported(
                "KNN predicates are available only in SELECT WHERE clauses",
                expression.span,
            ),
        )),
        ExprKind::Array(values) => {
            for value in values {
                reject_all_functions(value)?;
            }
            Ok(())
        }
        ExprKind::Object(fields) => {
            for field in fields {
                reject_all_functions(&field.value)?;
            }
            Ok(())
        }
        ExprKind::Unary { operand, .. } | ExprKind::Parenthesized(operand) => {
            reject_all_functions(operand)
        }
        ExprKind::Binary { left, right, .. } => {
            reject_all_functions(left)?;
            reject_all_functions(right)
        }
        _ => Ok(()),
    }
}

pub(crate) fn evaluate(expression: &Expr, context: &EvalContext<'_>) -> Result<EvalValue> {
    match &expression.kind {
        ExprKind::Null => Ok(EvalValue::Present(Value::Null)),
        ExprKind::Bool(value) => Ok(EvalValue::Present(Value::Bool(*value))),
        ExprKind::Integer(value) => Ok(EvalValue::Present(Value::Integer(*value))),
        ExprKind::Float(value) if value.is_finite() => Ok(EvalValue::Present(Value::Float(*value))),
        ExprKind::Float(_) => Err(FastDbError::Schema("non-finite float expression".into())),
        ExprKind::String(value) => Ok(EvalValue::Present(Value::Str(value.clone()))),
        ExprKind::Array(values) => values
            .iter()
            .map(|value| Ok(evaluate(value, context)?.into_projection()))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array)
            .map(EvalValue::Present),
        ExprKind::Object(fields) => {
            let mut object = BTreeMap::new();
            for field in fields {
                let key = match &field.key.kind {
                    turso_fastdb_parser::ObjectKeyKind::Identifier(key)
                    | turso_fastdb_parser::ObjectKeyKind::String(key) => key.clone(),
                };
                object.insert(key, evaluate(&field.value, context)?.into_projection());
            }
            Ok(EvalValue::Present(Value::Object(object)))
        }
        ExprKind::Parameter(name) => Ok(EvalValue::Present(
            context
                .params
                .get(name)
                .expect("parameter references are prevalidated")
                .clone(),
        )),
        ExprKind::RecordId(record) => Ok(EvalValue::Present(Value::RecordId(RecordId::new(
            record.table.value.clone(),
            match &record.id.kind {
                turso_fastdb_parser::RecordIdPartKind::Bare(value)
                | turso_fastdb_parser::RecordIdPartKind::Quoted(value) => {
                    RecordIdValue::String(value.clone())
                }
                turso_fastdb_parser::RecordIdPartKind::Integer(value) => {
                    RecordIdValue::Integer(*value)
                }
                turso_fastdb_parser::RecordIdPartKind::Uuid(value) => RecordIdValue::Uuid(*value),
            },
        )))),
        ExprKind::FieldPath(path) => Ok(read_path(path, context)),
        ExprKind::FunctionCall { name, .. } => Err(FastDbError::Schema(format!(
            "function {} is not available in Phase 6",
            name.iter()
                .map(|segment| segment.value.as_str())
                .collect::<Vec<_>>()
                .join("::")
        ))),
        ExprKind::Knn(_) => Err(FastDbError::Schema(
            "KNN predicate requires exact vector SELECT context".into(),
        )),
        ExprKind::Traversal(_) => Err(FastDbError::Schema(
            "graph traversal requires SELECT projection context".into(),
        )),
        ExprKind::Parenthesized(inner) => evaluate(inner, context),
        ExprKind::Unary { operator, operand } => {
            let value = evaluate(operand, context)?;
            evaluate_unary(operator.value, value)
        }
        ExprKind::Binary {
            left,
            operator,
            right,
        } => {
            let left = evaluate(left, context)?;
            match operator.value {
                BinaryOperator::And if !left.truthy() => Ok(left),
                BinaryOperator::Or if left.truthy() => Ok(left),
                BinaryOperator::And | BinaryOperator::Or => evaluate(right, context),
                operator => {
                    let right = evaluate(right, context)?;
                    evaluate_binary(operator, left, right)
                }
            }
        }
    }
}

fn read_path(path: &FieldPath, context: &EvalContext<'_>) -> EvalValue {
    let mut segments = path.segments.iter();
    let Some(first) = segments.next() else {
        return EvalValue::Missing;
    };
    let mut value = if first.value == "id" {
        Value::RecordId(context.id.clone())
    } else if first.value == "in" {
        let Some((from, _)) = context.endpoints else {
            return EvalValue::Missing;
        };
        Value::RecordId(from.clone())
    } else if first.value == "out" {
        let Some((_, to)) = context.endpoints else {
            return EvalValue::Missing;
        };
        Value::RecordId(to.clone())
    } else {
        let Some(value) = context.document.get(&first.value) else {
            return EvalValue::Missing;
        };
        value.clone()
    };
    for segment in segments {
        let Value::Object(object) = value else {
            return EvalValue::Missing;
        };
        let Some(next) = object.get(&segment.value) else {
            return EvalValue::Missing;
        };
        value = next.clone();
    }
    EvalValue::Present(value)
}

fn evaluate_unary(operator: UnaryOperator, value: EvalValue) -> Result<EvalValue> {
    match operator {
        UnaryOperator::Not => Ok(EvalValue::Present(Value::Bool(!value.truthy()))),
        UnaryOperator::Plus => match value {
            EvalValue::Missing => Ok(EvalValue::Missing),
            EvalValue::Present(Value::Null) => Ok(EvalValue::Present(Value::Null)),
            EvalValue::Present(value @ (Value::Integer(_) | Value::Float(_))) => {
                Ok(EvalValue::Present(value))
            }
            EvalValue::Present(_) => Err(FastDbError::Schema(
                "unary plus requires a numeric operand".into(),
            )),
        },
        UnaryOperator::Minus => match value {
            EvalValue::Missing => Ok(EvalValue::Missing),
            EvalValue::Present(Value::Null) => Ok(EvalValue::Present(Value::Null)),
            EvalValue::Present(Value::Integer(value)) => value
                .checked_neg()
                .map(Value::Integer)
                .map(EvalValue::Present)
                .ok_or_else(|| FastDbError::Schema("integer unary negation overflow".into())),
            EvalValue::Present(Value::Float(value)) if (-value).is_finite() => {
                Ok(EvalValue::Present(Value::Float(-value)))
            }
            EvalValue::Present(Value::Float(_)) => {
                Err(FastDbError::Schema("non-finite arithmetic result".into()))
            }
            EvalValue::Present(_) => Err(FastDbError::Schema(
                "unary minus requires a numeric operand".into(),
            )),
        },
    }
}

fn evaluate_binary(
    operator: BinaryOperator,
    left: EvalValue,
    right: EvalValue,
) -> Result<EvalValue> {
    match operator {
        BinaryOperator::Equal | BinaryOperator::NotEqual => {
            let equal = equal_values(&left, &right);
            Ok(EvalValue::Present(Value::Bool(
                if operator == BinaryOperator::Equal {
                    equal
                } else {
                    !equal
                },
            )))
        }
        BinaryOperator::Less
        | BinaryOperator::LessEqual
        | BinaryOperator::Greater
        | BinaryOperator::GreaterEqual => {
            let ordering = compare_values(&left, &right);
            let result = match operator {
                BinaryOperator::Less => ordering == Ordering::Less,
                BinaryOperator::LessEqual => ordering != Ordering::Greater,
                BinaryOperator::Greater => ordering == Ordering::Greater,
                BinaryOperator::GreaterEqual => ordering != Ordering::Less,
                _ => unreachable!(),
            };
            Ok(EvalValue::Present(Value::Bool(result)))
        }
        BinaryOperator::Add
        | BinaryOperator::Subtract
        | BinaryOperator::Multiply
        | BinaryOperator::Divide => arithmetic(operator, left, right),
        BinaryOperator::And | BinaryOperator::Or => {
            unreachable!("short-circuit operators are handled before RHS evaluation")
        }
        BinaryOperator::FtsMatch(_) => Err(FastDbError::Schema(
            "FTS match predicates require an indexed SELECT context".into(),
        )),
    }
}

fn arithmetic(operator: BinaryOperator, left: EvalValue, right: EvalValue) -> Result<EvalValue> {
    let (left, right) = match (left, right) {
        (EvalValue::Missing, _) | (_, EvalValue::Missing) => return Ok(EvalValue::Missing),
        (EvalValue::Present(Value::Null), _) | (_, EvalValue::Present(Value::Null)) => {
            return Ok(EvalValue::Present(Value::Null));
        }
        (EvalValue::Present(left), EvalValue::Present(right)) => (left, right),
    };
    match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => {
            if operator == BinaryOperator::Divide && right == 0 {
                return Ok(EvalValue::Present(Value::Null));
            }
            let result = match operator {
                BinaryOperator::Add => left.checked_add(right),
                BinaryOperator::Subtract => left.checked_sub(right),
                BinaryOperator::Multiply => left.checked_mul(right),
                BinaryOperator::Divide => left.checked_div(right),
                _ => unreachable!(),
            };
            result
                .map(Value::Integer)
                .map(EvalValue::Present)
                .ok_or_else(|| FastDbError::Schema("integer arithmetic overflow".into()))
        }
        (Value::Integer(left), Value::Float(right)) => {
            float_arithmetic(operator, left as f64, right)
        }
        (Value::Float(left), Value::Integer(right)) => {
            float_arithmetic(operator, left, right as f64)
        }
        (Value::Float(left), Value::Float(right)) => float_arithmetic(operator, left, right),
        _ => Err(FastDbError::Schema(
            "arithmetic operators require numeric operands".into(),
        )),
    }
}

fn float_arithmetic(operator: BinaryOperator, left: f64, right: f64) -> Result<EvalValue> {
    if operator == BinaryOperator::Divide && right == 0.0 {
        return Ok(EvalValue::Present(Value::Null));
    }
    let result = match operator {
        BinaryOperator::Add => left + right,
        BinaryOperator::Subtract => left - right,
        BinaryOperator::Multiply => left * right,
        BinaryOperator::Divide => left / right,
        _ => unreachable!(),
    };
    if !result.is_finite() {
        return Err(FastDbError::Schema("non-finite arithmetic result".into()));
    }
    Ok(EvalValue::Present(Value::Float(result)))
}

fn equal_values(left: &EvalValue, right: &EvalValue) -> bool {
    compare_values(left, right) == Ordering::Equal
}

pub(crate) fn compare_values(left: &EvalValue, right: &EvalValue) -> Ordering {
    let left_rank = value_rank(left);
    let right_rank = value_rank(right);
    match left_rank.cmp(&right_rank) {
        Ordering::Equal => {}
        ordering => return ordering,
    }
    match (left, right) {
        (EvalValue::Missing, EvalValue::Missing)
        | (EvalValue::Present(Value::Null), EvalValue::Present(Value::Null)) => Ordering::Equal,
        (EvalValue::Present(Value::Bool(left)), EvalValue::Present(Value::Bool(right))) => {
            left.cmp(right)
        }
        (EvalValue::Present(Value::Integer(left)), EvalValue::Present(Value::Integer(right))) => {
            left.cmp(right)
        }
        (EvalValue::Present(Value::Float(left)), EvalValue::Present(Value::Float(right))) => {
            left.partial_cmp(right).expect("finite floats")
        }
        (EvalValue::Present(Value::Integer(left)), EvalValue::Present(Value::Float(right))) => {
            compare_integer_float(*left, *right)
        }
        (EvalValue::Present(Value::Float(left)), EvalValue::Present(Value::Integer(right))) => {
            compare_integer_float(*right, *left).reverse()
        }
        (EvalValue::Present(Value::Str(left)), EvalValue::Present(Value::Str(right))) => {
            left.cmp(right)
        }
        (EvalValue::Present(Value::Array(left)), EvalValue::Present(Value::Array(right))) => {
            compare_sequences(left.iter(), right.iter())
        }
        (EvalValue::Present(Value::Object(left)), EvalValue::Present(Value::Object(right))) => {
            compare_objects(left, right)
        }
        (EvalValue::Present(Value::RecordId(left)), EvalValue::Present(Value::RecordId(right))) => {
            compare_record_ids(left, right)
        }
        _ => unreachable!("equal ranks imply comparable variants"),
    }
}

fn value_rank(value: &EvalValue) -> u8 {
    match value {
        EvalValue::Missing => 0,
        EvalValue::Present(Value::Null) => 1,
        EvalValue::Present(Value::Bool(_)) => 2,
        EvalValue::Present(Value::Integer(_) | Value::Float(_)) => 3,
        EvalValue::Present(Value::Str(_)) => 4,
        EvalValue::Present(Value::Array(_)) => 5,
        EvalValue::Present(Value::Object(_)) => 6,
        EvalValue::Present(Value::RecordId(_)) => 7,
    }
}

fn compare_integer_float(integer: i64, float: f64) -> Ordering {
    const I64_UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
    const I64_LOWER: f64 = -9_223_372_036_854_775_808.0;
    if float >= I64_UPPER_EXCLUSIVE {
        return Ordering::Less;
    }
    if float < I64_LOWER {
        return Ordering::Greater;
    }
    let truncated = float.trunc() as i64;
    match integer.cmp(&truncated) {
        Ordering::Equal if float.fract() > 0.0 => Ordering::Less,
        Ordering::Equal if float.fract() < 0.0 => Ordering::Greater,
        ordering => ordering,
    }
}

fn compare_sequences<'a>(
    mut left: impl Iterator<Item = &'a Value>,
    mut right: impl Iterator<Item = &'a Value>,
) -> Ordering {
    loop {
        match (left.next(), right.next()) {
            (Some(left), Some(right)) => {
                let ordering = compare_values(
                    &EvalValue::Present(left.clone()),
                    &EvalValue::Present(right.clone()),
                );
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn compare_objects(left: &BTreeMap<String, Value>, right: &BTreeMap<String, Value>) -> Ordering {
    let mut left = left.iter();
    let mut right = right.iter();
    loop {
        match (left.next(), right.next()) {
            (Some((left_key, left_value)), Some((right_key, right_value))) => {
                match left_key.cmp(right_key) {
                    Ordering::Equal => {}
                    ordering => return ordering,
                }
                let ordering = compare_values(
                    &EvalValue::Present(left_value.clone()),
                    &EvalValue::Present(right_value.clone()),
                );
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn compare_record_ids(left: &RecordId, right: &RecordId) -> Ordering {
    match left.table.cmp(&right.table) {
        Ordering::Equal => {}
        ordering => return ordering,
    }
    let rank = |value: &RecordIdValue| match value {
        RecordIdValue::Integer(_) => 0,
        RecordIdValue::String(_) => 1,
        RecordIdValue::Uuid(_) => 2,
    };
    match rank(&left.id).cmp(&rank(&right.id)) {
        Ordering::Equal => {}
        ordering => return ordering,
    }
    match (&left.id, &right.id) {
        (RecordIdValue::Integer(left), RecordIdValue::Integer(right)) => left.cmp(right),
        (RecordIdValue::String(left), RecordIdValue::String(right)) => left.cmp(right),
        (RecordIdValue::Uuid(left), RecordIdValue::Uuid(right)) => {
            left.as_bytes().cmp(right.as_bytes())
        }
        _ => unreachable!("equal component ranks imply equal variants"),
    }
}
