//! Authoritative FastDB expression evaluation over decoded documents.

use crate::builtins::{
    self, Builtin, BuiltinClass, BuiltinSyntax, CryptoDigest, DurationUnit, MathConstant,
    MathUnary, TimePart, TimeTruncate, TypeCast, TypeKind,
};
use crate::decode::{
    canonical_value_cmp, decode_value, encode_value, DatetimeValue, DecimalValue, DurationValue,
    FileValue, RangeBound, RangeValue, RecordId, RecordIdValue, RegexValue, SetValue, TableValue,
    Value,
};
use crate::error::{FastDbError, Result};
use crate::Params;
use base64::Engine as _;
use chrono::{Datelike as _, Local, Timelike as _, Utc};
use rust_decimal::prelude::ToPrimitive as _;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use turso_fastdb_parser::{
    Accessor, BinaryOperator, Expr, ExprKind, FieldPath, SchemaType, SchemaTypeKind, Statement,
    UnaryOperator,
};

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
            Self::Missing | Self::Present(Value::None | Value::Null | Value::Bool(false)) => false,
            Self::Present(Value::Integer(0)) => false,
            Self::Present(Value::Float(value)) if *value == 0.0 => false,
            Self::Present(Value::Decimal(value)) if value.is_zero() => false,
            Self::Present(Value::Str(value)) => !value.is_empty(),
            Self::Present(Value::Bytes(value)) => !value.is_empty(),
            Self::Present(Value::Array(value)) => !value.is_empty(),
            Self::Present(Value::Object(value)) => !value.is_empty(),
            Self::Present(Value::Set(value)) => !value.as_slice().is_empty(),
            Self::Present(
                Value::Bool(_)
                | Value::Integer(_)
                | Value::Float(_)
                | Value::Decimal(_)
                | Value::Duration(_)
                | Value::Datetime(_)
                | Value::Uuid(_)
                | Value::Range(_)
                | Value::Regex(_)
                | Value::RecordId(_)
                | Value::Table(_)
                | Value::File(_),
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
                reject_unavailable_functions(expression)?;
                collect_parameters(expression, &mut names)
            }
            turso_fastdb_parser::CreateData::Set(assignments) => {
                for assignment in assignments {
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
                        reject_unavailable_functions(expression)?;
                        collect_parameters(expression, &mut names);
                    }
                    turso_fastdb_parser::CreateData::Set(assignments) => {
                        for assignment in assignments {
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
        ExprKind::Access { target, accessor } => {
            collect_parameters(target, names);
            collect_accessor_parameters(accessor, names);
        }
        ExprKind::Cast { value, .. } => collect_parameters(value, names),
        ExprKind::Range(range) => {
            if let Some(start) = &range.start {
                collect_parameters(start, names);
            }
            if let Some(end) = &range.end {
                collect_parameters(end, names);
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
        ExprKind::NamespacedValue { .. } => {}
        ExprKind::Knn(knn) => {
            collect_parameters(&knn.field, names);
            collect_parameters(&knn.query, names);
        }
        ExprKind::Traversal(_) => {}
        ExprKind::None
        | ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_) => {}
    }
}

fn collect_accessor_parameters<'a>(accessor: &'a Accessor, names: &mut Vec<&'a str>) {
    match accessor {
        Accessor::Field(_) | Accessor::Last(_) => {}
        Accessor::Index(index) => collect_parameters(index, names),
        Accessor::Slice { start, end, .. } => {
            if let Some(start) = start {
                collect_parameters(start, names);
            }
            if let Some(end) = end {
                collect_parameters(end, names);
            }
        }
    }
}

fn reject_unavailable_functions(expression: &Expr) -> Result<()> {
    match &expression.kind {
        ExprKind::FunctionCall { name, arguments } => {
            for argument in arguments {
                reject_unavailable_functions(argument)?;
            }
            let normalized = normalized_function_segments(name);
            let canonical = normalized.join("::");
            if let Some(spec) = builtins::lookup(&canonical) {
                if !matches!(spec.class, BuiltinClass::Pure | BuiltinClass::Context)
                    || spec.implementation_version != 1
                {
                    return Err(FastDbError::Engine(
                        "built-in registry contains an unsupported implementation class".into(),
                    ));
                }
                if !(spec.min_arity..=spec.max_arity).contains(&arguments.len()) {
                    return Err(FastDbError::Schema(format!(
                        "{} requires {} argument(s)",
                        spec.name,
                        arity_description(spec.min_arity, spec.max_arity)
                    )));
                }
                if spec.syntax != BuiltinSyntax::Function {
                    return Err(FastDbError::Schema(format!(
                        "{} is a constant and must not use parentheses",
                        spec.name
                    )));
                }
                return Ok(());
            }
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
                        "unknown or unavailable built-in function",
                        expression.span,
                    ),
                ))
            }
        }
        ExprKind::Traversal(_) => Ok(()),
        ExprKind::NamespacedValue { name } => {
            let canonical = normalized_function_segments(name).join("::");
            match builtins::lookup(&canonical) {
                Some(spec) if spec.syntax == BuiltinSyntax::Constant => Ok(()),
                Some(spec) => Err(FastDbError::Schema(format!(
                    "{} is a function and requires parentheses",
                    spec.name
                ))),
                None => Err(FastDbError::UnsupportedSyntax(
                    turso_fastdb_parser::ParseError::unsupported(
                        "unknown or unavailable namespaced value",
                        expression.span,
                    ),
                )),
            }
        }
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
        ExprKind::Access { target, accessor } => {
            reject_unavailable_functions(target)?;
            reject_accessor_functions(accessor)
        }
        ExprKind::Cast { value, .. } => reject_unavailable_functions(value),
        ExprKind::Range(range) => {
            if let Some(start) = &range.start {
                reject_unavailable_functions(start)?;
            }
            if let Some(end) = &range.end {
                reject_unavailable_functions(end)?;
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
        ExprKind::None
        | ExprKind::Null
        | ExprKind::Bool(_)
        | ExprKind::Integer(_)
        | ExprKind::Float(_)
        | ExprKind::Duration(_)
        | ExprKind::String(_)
        | ExprKind::Parameter(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_) => Ok(()),
    }
}

fn normalized_function_segments(name: &[turso_fastdb_parser::Identifier]) -> Vec<String> {
    name.iter()
        .map(|segment| segment.value.to_ascii_lowercase())
        .collect()
}

fn arity_description(minimum: usize, maximum: usize) -> String {
    if minimum == maximum {
        minimum.to_string()
    } else {
        format!("between {minimum} and {maximum}")
    }
}

fn reject_accessor_functions(accessor: &Accessor) -> Result<()> {
    match accessor {
        Accessor::Field(_) | Accessor::Last(_) => Ok(()),
        Accessor::Index(index) => reject_unavailable_functions(index),
        Accessor::Slice { start, end, .. } => {
            if let Some(start) = start {
                reject_unavailable_functions(start)?;
            }
            if let Some(end) = end {
                reject_unavailable_functions(end)?;
            }
            Ok(())
        }
    }
}

pub(crate) fn evaluate(expression: &Expr, context: &EvalContext<'_>) -> Result<EvalValue> {
    match &expression.kind {
        ExprKind::None => Ok(EvalValue::Present(Value::None)),
        ExprKind::Null => Ok(EvalValue::Present(Value::Null)),
        ExprKind::Bool(value) => Ok(EvalValue::Present(Value::Bool(*value))),
        ExprKind::Integer(value) => Ok(EvalValue::Present(Value::Integer(*value))),
        ExprKind::Float(value) if value.is_finite() => Ok(EvalValue::Present(Value::Float(*value))),
        ExprKind::Float(_) => Err(FastDbError::Schema("non-finite float expression".into())),
        ExprKind::Duration(value) => DurationValue::parse(value)
            .map(Value::Duration)
            .map(EvalValue::Present),
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
        ExprKind::Access { target, accessor } => {
            let target = evaluate(target, context)?;
            evaluate_access(target, accessor, context)
        }
        ExprKind::Cast { ty, value } => {
            let value = evaluate(value, context)?;
            cast_value(value, ty)
        }
        ExprKind::Range(range) => {
            let start = match &range.start {
                Some(start) => {
                    RangeBound::Included(Box::new(evaluate(start, context)?.into_projection()))
                }
                None => RangeBound::Unbounded,
            };
            let end = match &range.end {
                Some(end) if range.inclusive => {
                    RangeBound::Included(Box::new(evaluate(end, context)?.into_projection()))
                }
                Some(end) => {
                    RangeBound::Excluded(Box::new(evaluate(end, context)?.into_projection()))
                }
                None => RangeBound::Unbounded,
            };
            Ok(EvalValue::Present(Value::Range(RangeValue::new(
                start, end,
            ))))
        }
        ExprKind::FunctionCall { name, arguments } => {
            let canonical = normalized_function_segments(name).join("::");
            let Some(spec) = builtins::lookup(&canonical) else {
                return Err(FastDbError::Schema(format!(
                    "function {canonical} requires a specialized query context"
                )));
            };
            let arguments = arguments
                .iter()
                .map(|argument| evaluate(argument, context).map(EvalValue::into_function_value))
                .collect::<Result<Vec<_>>>()?;
            evaluate_builtin(spec.function, arguments).map(EvalValue::Present)
        }
        ExprKind::NamespacedValue { name } => {
            let canonical = normalized_function_segments(name).join("::");
            let spec = builtins::lookup(&canonical).ok_or_else(|| {
                FastDbError::Schema(format!("unknown namespaced value {canonical}"))
            })?;
            evaluate_builtin(spec.function, Vec::new()).map(EvalValue::Present)
        }
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
                BinaryOperator::NullCoalesce if !left.is_nullish() => Ok(left),
                BinaryOperator::TruthyCoalesce if left.truthy() => Ok(left),
                BinaryOperator::And | BinaryOperator::Or => evaluate(right, context),
                BinaryOperator::NullCoalesce | BinaryOperator::TruthyCoalesce => {
                    evaluate(right, context)
                }
                operator => {
                    let right = evaluate(right, context)?;
                    evaluate_binary(operator, left, right)
                }
            }
        }
    }
}

impl EvalValue {
    fn is_nullish(&self) -> bool {
        matches!(
            self,
            Self::Missing | Self::Present(Value::None | Value::Null)
        )
    }

    fn into_function_value(self) -> Value {
        match self {
            Self::Missing => Value::None,
            Self::Present(value) => value,
        }
    }
}

fn evaluate_access(
    target: EvalValue,
    accessor: &Accessor,
    context: &EvalContext<'_>,
) -> Result<EvalValue> {
    let EvalValue::Present(target) = target else {
        return Ok(EvalValue::Missing);
    };
    let none = || EvalValue::Present(Value::None);
    match accessor {
        Accessor::Field(field) => match target {
            Value::Object(values) => Ok(values
                .get(&field.value)
                .cloned()
                .map_or_else(none, EvalValue::Present)),
            _ => Ok(none()),
        },
        Accessor::Last(_) => match target {
            Value::Array(values) => {
                Ok(values.last().cloned().map_or_else(none, EvalValue::Present))
            }
            Value::Set(values) => Ok(values
                .as_slice()
                .last()
                .cloned()
                .map_or_else(none, EvalValue::Present)),
            _ => Ok(none()),
        },
        Accessor::Index(index) => {
            let index = evaluate(index, context)?;
            match (target, index) {
                (Value::Array(values), EvalValue::Present(Value::Integer(index))) => {
                    Ok(index_value(&values, index))
                }
                (Value::Set(values), EvalValue::Present(Value::Integer(index))) => {
                    Ok(index_value(values.as_slice(), index))
                }
                (Value::Object(values), EvalValue::Present(Value::Str(key))) => Ok(values
                    .get(&key)
                    .cloned()
                    .map_or_else(none, EvalValue::Present)),
                (_, EvalValue::Missing | EvalValue::Present(Value::None | Value::Null)) => {
                    Ok(none())
                }
                _ => Ok(none()),
            }
        }
        Accessor::Slice {
            start,
            end,
            inclusive,
            ..
        } => {
            let Some(start) = evaluate_slice_bound(start.as_deref(), context, 0)? else {
                return Ok(none());
            };
            match target {
                Value::Array(values) => {
                    Ok(
                        slice_values(&values, start, end.as_deref(), *inclusive, context)?
                            .map(Value::Array)
                            .map_or_else(none, EvalValue::Present),
                    )
                }
                Value::Set(values) => match slice_values(
                    values.as_slice(),
                    start,
                    end.as_deref(),
                    *inclusive,
                    context,
                )? {
                    Some(values) => SetValue::new(values)
                        .map(Value::Set)
                        .map(EvalValue::Present),
                    None => Ok(none()),
                },
                _ => Ok(none()),
            }
        }
    }
}

fn index_value(values: &[Value], index: i64) -> EvalValue {
    usize::try_from(index)
        .ok()
        .and_then(|index| values.get(index))
        .cloned()
        .map_or(EvalValue::Present(Value::None), EvalValue::Present)
}

fn evaluate_slice_bound(
    expression: Option<&Expr>,
    context: &EvalContext<'_>,
    default: usize,
) -> Result<Option<usize>> {
    let Some(expression) = expression else {
        return Ok(Some(default));
    };
    match evaluate(expression, context)? {
        EvalValue::Present(Value::Integer(value)) => Ok(usize::try_from(value).ok()),
        _ => Ok(None),
    }
}

fn slice_values(
    values: &[Value],
    start: usize,
    end: Option<&Expr>,
    inclusive: bool,
    context: &EvalContext<'_>,
) -> Result<Option<Vec<Value>>> {
    let Some(mut end) = evaluate_slice_bound(end, context, values.len())? else {
        return Ok(None);
    };
    if inclusive && end != usize::MAX {
        end = end.saturating_add(1);
    }
    if start > end || end > values.len() {
        return Ok(None);
    }
    Ok(Some(values[start..end].to_vec()))
}

fn cast_value(value: EvalValue, ty: &SchemaType) -> Result<EvalValue> {
    let EvalValue::Present(value) = value else {
        return Ok(EvalValue::Missing);
    };
    cast_present(value, &ty.kind).map(EvalValue::Present)
}

fn cast_present(value: Value, ty: &SchemaTypeKind) -> Result<Value> {
    match ty {
        SchemaTypeKind::Option(_) if matches!(value, Value::None | Value::Null) => Ok(value),
        SchemaTypeKind::Option(inner) => cast_present(value, &inner.kind),
        SchemaTypeKind::Bool => match value {
            Value::Bool(value) => Ok(Value::Bool(value)),
            Value::Str(value) if value.eq_ignore_ascii_case("true") => Ok(Value::Bool(true)),
            Value::Str(value) if value.eq_ignore_ascii_case("false") => Ok(Value::Bool(false)),
            _ => Err(FastDbError::Schema(
                "value cannot be cast to boolean".into(),
            )),
        },
        SchemaTypeKind::Int => cast_int(value).map(Value::Integer),
        SchemaTypeKind::Float => cast_float(value).map(Value::Float),
        SchemaTypeKind::Number => match value {
            value @ (Value::Integer(_) | Value::Float(_) | Value::Decimal(_)) => Ok(value),
            Value::Str(value) => value
                .parse::<i64>()
                .map(Value::Integer)
                .or_else(|_| {
                    value.parse::<f64>().map_err(|_| ()).and_then(|value| {
                        value.is_finite().then_some(Value::Float(value)).ok_or(())
                    })
                })
                .map_err(|_| FastDbError::Schema("value cannot be cast to number".into())),
            other => cast_float(other).map(Value::Float),
        },
        SchemaTypeKind::Decimal => cast_decimal(value).map(Value::Decimal),
        SchemaTypeKind::String => render_string(value).map(Value::Str),
        SchemaTypeKind::Bytes => match value {
            Value::Bytes(value) => Ok(Value::Bytes(value)),
            Value::Str(value) => Ok(Value::Bytes(value.into_bytes())),
            _ => Err(FastDbError::Schema(
                "only strings and bytes can be cast to bytes".into(),
            )),
        },
        SchemaTypeKind::Datetime => match value {
            Value::Datetime(value) => Ok(Value::Datetime(value)),
            Value::Str(value) => DatetimeValue::parse(&value).map(Value::Datetime),
            Value::Integer(value) => DatetimeValue::from_timestamp(value, 0).map(Value::Datetime),
            _ => Err(FastDbError::Schema(
                "value cannot be cast to datetime".into(),
            )),
        },
        SchemaTypeKind::Duration => match value {
            Value::Duration(value) => Ok(Value::Duration(value)),
            Value::Str(value) => DurationValue::parse(&value).map(Value::Duration),
            Value::Integer(value) if value >= 0 => {
                DurationValue::new(value as u64, 0).map(Value::Duration)
            }
            _ => Err(FastDbError::Schema(
                "value cannot be cast to duration".into(),
            )),
        },
        SchemaTypeKind::Uuid => match value {
            Value::Uuid(value) => Ok(Value::Uuid(value)),
            Value::Str(value) => uuid::Uuid::parse_str(&value)
                .map(Value::Uuid)
                .map_err(|_| FastDbError::Schema("value cannot be cast to UUID".into())),
            _ => Err(FastDbError::Schema("value cannot be cast to UUID".into())),
        },
        SchemaTypeKind::Regex => match value {
            Value::Regex(value) => Ok(Value::Regex(value)),
            Value::Str(value) => RegexValue::new(value).map(Value::Regex),
            _ => Err(FastDbError::Schema("value cannot be cast to regex".into())),
        },
        SchemaTypeKind::File => match value {
            Value::File(value) => Ok(Value::File(value)),
            Value::Str(value) => FileValue::new(value).map(Value::File),
            _ => Err(FastDbError::Schema("value cannot be cast to file".into())),
        },
        SchemaTypeKind::Table => match value {
            Value::Table(value) => Ok(Value::Table(value)),
            Value::Str(value) => TableValue::new(value).map(Value::Table),
            Value::RecordId(value) => TableValue::new(value.table).map(Value::Table),
            _ => Err(FastDbError::Schema("value cannot be cast to table".into())),
        },
        SchemaTypeKind::Object => match value {
            Value::Object(_) => Ok(value),
            _ => Err(FastDbError::Schema("value cannot be cast to object".into())),
        },
        SchemaTypeKind::Array => cast_array(value).map(Value::Array),
        SchemaTypeKind::TypedArray { element, length } => {
            let values = cast_array(value)?;
            if length
                .as_ref()
                .is_some_and(|length| usize::try_from(length.value).ok() != Some(values.len()))
            {
                return Err(FastDbError::Schema(
                    "cast array has the wrong fixed length".into(),
                ));
            }
            values
                .into_iter()
                .map(|value| cast_present(value, &element.kind))
                .collect::<Result<Vec<_>>>()
                .map(Value::Array)
        }
        SchemaTypeKind::FixedFloatArray(dimension) => {
            let values = cast_array(value)?;
            if usize::try_from(dimension.value).ok() != Some(values.len()) {
                return Err(FastDbError::Schema(
                    "cast vector has the wrong fixed dimension".into(),
                ));
            }
            values
                .into_iter()
                .map(|value| cast_float(value).map(Value::Float))
                .collect::<Result<Vec<_>>>()
                .map(Value::Array)
        }
        SchemaTypeKind::Set { element, length } => {
            let values = match value {
                Value::Set(values) => values.into_vec(),
                other => cast_array(other)?,
            };
            let values = match element {
                Some(element) => values
                    .into_iter()
                    .map(|value| cast_present(value, &element.kind))
                    .collect::<Result<Vec<_>>>()?,
                None => values,
            };
            let values = SetValue::new(values)?;
            if length.as_ref().is_some_and(|length| {
                usize::try_from(length.value).ok() != Some(values.as_slice().len())
            }) {
                return Err(FastDbError::Schema(
                    "cast set has the wrong fixed length".into(),
                ));
            }
            Ok(Value::Set(values))
        }
        SchemaTypeKind::Range => match value {
            Value::Range(_) => Ok(value),
            Value::Array(values) if values.len() == 2 => Ok(Value::Range(RangeValue::new(
                RangeBound::Included(Box::new(values[0].clone())),
                RangeBound::Excluded(Box::new(values[1].clone())),
            ))),
            _ => Err(FastDbError::Schema("value cannot be cast to range".into())),
        },
        SchemaTypeKind::Record => match value {
            Value::RecordId(_) => Ok(value),
            Value::Str(value) => parse_record_string(&value).map(Value::RecordId),
            _ => Err(FastDbError::Schema("value cannot be cast to record".into())),
        },
    }
}

fn cast_int(value: Value) -> Result<i64> {
    match value {
        Value::Integer(value) => Ok(value),
        Value::Float(value)
            if value.is_finite()
                && value.fract() == 0.0
                && value.trunc() >= i64::MIN as f64
                && value.trunc() < 9_223_372_036_854_775_808.0 =>
        {
            Ok(value.trunc() as i64)
        }
        Value::Decimal(value) if value.as_decimal().fract().is_zero() => value
            .as_decimal()
            .to_i64()
            .ok_or_else(|| FastDbError::Schema("decimal is outside the integer range".into())),
        Value::Str(value) => value
            .parse::<i64>()
            .map_err(|_| FastDbError::Schema("value cannot be cast to integer".into())),
        _ => Err(FastDbError::Schema(
            "value cannot be cast to integer".into(),
        )),
    }
}

fn cast_float(value: Value) -> Result<f64> {
    let value = match value {
        Value::Float(value) => value,
        Value::Integer(value) => value as f64,
        Value::Decimal(value) => value
            .as_decimal()
            .to_f64()
            .ok_or_else(|| FastDbError::Schema("decimal cannot be represented as float".into()))?,
        Value::Str(value) => value
            .parse::<f64>()
            .map_err(|_| FastDbError::Schema("value cannot be cast to float".into()))?,
        _ => return Err(FastDbError::Schema("value cannot be cast to float".into())),
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(FastDbError::Schema(
            "cast produced a non-finite float".into(),
        ))
    }
}

fn cast_decimal(value: Value) -> Result<DecimalValue> {
    match value {
        Value::Decimal(value) => Ok(value),
        Value::Integer(value) => DecimalValue::parse(&value.to_string()),
        Value::Float(value) if value.is_finite() => DecimalValue::parse(&value.to_string()),
        Value::Str(value) => DecimalValue::parse(&value),
        _ => Err(FastDbError::Schema(
            "value cannot be cast to decimal".into(),
        )),
    }
}

fn cast_array(value: Value) -> Result<Vec<Value>> {
    match value {
        Value::Array(values) => Ok(values),
        Value::Set(values) => Ok(values.into_vec()),
        Value::Range(range) => expand_integer_range(&range),
        _ => Err(FastDbError::Schema("value cannot be cast to array".into())),
    }
}

fn expand_integer_range(range: &RangeValue) -> Result<Vec<Value>> {
    let start = match range.start() {
        RangeBound::Included(value) => match value.as_ref() {
            Value::Integer(value) => *value,
            _ => return Err(FastDbError::Schema("range start must be an integer".into())),
        },
        RangeBound::Excluded(value) => match value.as_ref() {
            Value::Integer(value) => value
                .checked_add(1)
                .ok_or_else(|| FastDbError::Schema("range start overflow".into()))?,
            _ => return Err(FastDbError::Schema("range start must be an integer".into())),
        },
        RangeBound::Unbounded => {
            return Err(FastDbError::Schema(
                "unbounded range cannot be cast to array".into(),
            ))
        }
    };
    let end = match range.end() {
        RangeBound::Excluded(value) => match value.as_ref() {
            Value::Integer(value) => *value,
            _ => return Err(FastDbError::Schema("range end must be an integer".into())),
        },
        RangeBound::Included(value) => match value.as_ref() {
            Value::Integer(value) => value
                .checked_add(1)
                .ok_or_else(|| FastDbError::Schema("range end overflow".into()))?,
            _ => return Err(FastDbError::Schema("range end must be an integer".into())),
        },
        RangeBound::Unbounded => {
            return Err(FastDbError::Schema(
                "unbounded range cannot be cast to array".into(),
            ))
        }
    };
    let length = end.saturating_sub(start);
    if !(0..=65_536).contains(&length) {
        return Err(FastDbError::Schema(
            "range expansion exceeds the collection limit".into(),
        ));
    }
    Ok((start..end).map(Value::Integer).collect())
}

fn parse_record_string(value: &str) -> Result<RecordId> {
    let (table, component) = value
        .split_once(':')
        .ok_or_else(|| FastDbError::Schema("record string must contain ':'".into()))?;
    TableValue::new(table)?;
    let component = component
        .parse::<i64>()
        .map(RecordIdValue::Integer)
        .unwrap_or_else(|_| RecordIdValue::String(component.to_string()));
    Ok(RecordId::new(table, component))
}

fn render_string(value: Value) -> Result<String> {
    Ok(match value {
        Value::None => "NONE".into(),
        Value::Null => "NULL".into(),
        Value::Bool(value) => value.to_string(),
        Value::Integer(value) => value.to_string(),
        Value::Float(value) if value.is_finite() => value.to_string(),
        Value::Decimal(value) => value.to_canonical(),
        Value::Str(value) => value,
        Value::Bytes(value) => String::from_utf8(value)
            .map_err(|_| FastDbError::Schema("bytes are not valid UTF-8".into()))?,
        Value::Duration(value) => value.to_canonical(),
        Value::Datetime(value) => value.to_canonical(),
        Value::Uuid(value) => value.hyphenated().to_string(),
        Value::Regex(value) => value.as_str().to_string(),
        Value::RecordId(value) => render_record_id(&value),
        Value::Table(value) => value.as_str().to_string(),
        Value::File(value) => value.as_str().to_string(),
        Value::Array(_) | Value::Object(_) | Value::Set(_) | Value::Range(_) => {
            return Err(FastDbError::Schema(
                "collection cannot be cast to string by this expression surface".into(),
            ))
        }
        Value::Float(_) => return Err(FastDbError::Schema("non-finite float value".into())),
    })
}

fn render_record_id(value: &RecordId) -> String {
    let id = match &value.id {
        RecordIdValue::String(value) if is_bare_record_component(value) => value.clone(),
        other => other.to_source(),
    };
    format!("{}:{id}", value.table)
}

fn is_bare_record_component(value: &str) -> bool {
    let mut chars = value.chars();
    chars
        .next()
        .is_some_and(|first| first == '_' || first.is_alphabetic())
        && chars.all(|character| character == '_' || character.is_alphanumeric())
}

fn evaluate_builtin(function: Builtin, arguments: Vec<Value>) -> Result<Value> {
    use Builtin::*;
    match function {
        ArrayAdd => {
            let mut values = take_array(&arguments[0], "array::add")?;
            push_unique(&mut values, arguments[1].clone())?;
            Ok(Value::Array(values))
        }
        ArrayAppend => {
            let mut values = take_array(&arguments[0], "array append")?;
            push_bounded(&mut values, arguments[1].clone())?;
            Ok(Value::Array(values))
        }
        ArrayPrepend => {
            let mut values = take_array(&arguments[0], "array::prepend")?;
            ensure_collection_growth(values.len(), 1)?;
            values.insert(0, arguments[1].clone());
            Ok(Value::Array(values))
        }
        ArrayAt => Ok(array_at(
            collection_slice(&arguments[0], "array::at")?,
            expect_integer(&arguments[1], "array::at index")?,
        )),
        ArrayPop => Ok(take_array(&arguments[0], "array::pop")?
            .pop()
            .unwrap_or(Value::None)),
        ArrayFirst | ArrayLast | ArrayMax | ArrayMin => {
            let values = collection_slice(&arguments[0], "array function")?;
            let selected = match function {
                ArrayFirst => values.first(),
                ArrayLast => values.last(),
                ArrayMax => values
                    .iter()
                    .max_by(|left, right| canonical_value_cmp(left, right)),
                ArrayMin => values
                    .iter()
                    .min_by(|left, right| canonical_value_cmp(left, right)),
                _ => unreachable!(),
            };
            Ok(selected.cloned().unwrap_or(Value::None))
        }
        ArrayLen => Ok(Value::Integer(collection_len(&arguments[0], "array::len")?)),
        ArrayIsEmpty => Ok(Value::Bool(
            collection_slice(&arguments[0], "array::is_empty")?.is_empty(),
        )),
        ArrayConcat => {
            let mut output = Vec::new();
            for value in &arguments {
                let values = collection_slice(value, "array::concat")?;
                ensure_collection_growth(output.len(), values.len())?;
                output.extend_from_slice(values);
            }
            Ok(Value::Array(output))
        }
        ArrayDistinct => {
            let mut output = Vec::new();
            for value in collection_slice(&arguments[0], "array::distinct")? {
                push_unique(&mut output, value.clone())?;
            }
            Ok(Value::Array(output))
        }
        ArrayReverse => {
            let mut values = take_array(&arguments[0], "array::reverse")?;
            values.reverse();
            Ok(Value::Array(values))
        }
        ArraySlice => Ok(Value::Array(slice_collection(
            collection_slice(&arguments[0], "array::slice")?,
            expect_integer(&arguments[1], "array::slice start")?,
            arguments
                .get(2)
                .map(|value| expect_integer(value, "array::slice end"))
                .transpose()?,
        ))),
        ArrayIncludes => Ok(Value::Bool(value_in_collection(
            &arguments[1],
            collection_slice(&arguments[0], "array::includes")?,
        ))),
        ArrayIndexOf => Ok(collection_slice(&arguments[0], "array::index_of")?
            .iter()
            .position(|value| values_equal(value, &arguments[1]))
            .map(|index| Value::Integer(index as i64))
            .unwrap_or(Value::None)),
        ArrayUnion | ArrayIntersect | ArrayDifference | ArrayComplement => {
            evaluate_array_set_operation(function, &arguments[0], &arguments[1])
        }
        ArrayRemove => {
            let mut values = take_array(&arguments[0], "array::remove")?;
            if let Some(index) = relative_index(
                expect_integer(&arguments[1], "array::remove index")?,
                values.len(),
            ) {
                values.remove(index);
            }
            Ok(Value::Array(values))
        }
        ArrayRepeat => {
            let count = expect_nonnegative_usize(&arguments[1], "array::repeat count")?;
            ensure_collection_growth(0, count)?;
            Ok(Value::Array(vec![arguments[0].clone(); count]))
        }
        ArrayRange => evaluate_array_range(&arguments),
        ArraySort | ArraySortAsc | ArraySortDesc => {
            let mut values = take_array(&arguments[0], "array::sort")?;
            let descending = match function {
                ArraySortDesc => true,
                ArraySortAsc => false,
                ArraySort => arguments.get(1).is_some_and(|value| {
                    matches!(value, Value::Bool(false))
                        || matches!(value, Value::Str(value) if value.eq_ignore_ascii_case("desc"))
                }),
                _ => unreachable!(),
            };
            values.sort_by(canonical_value_cmp);
            if descending {
                values.reverse();
            }
            Ok(Value::Array(values))
        }
        ArrayJoin => join_collection(&arguments[0], &arguments[1]),
        BytesLen => match &arguments[0] {
            Value::Bytes(value) => Ok(Value::Integer(value.len() as i64)),
            _ => Err(argument_type("bytes::len", "bytes")),
        },
        ObjectEntries => {
            let object = expect_object(&arguments[0], "object::entries")?;
            Ok(Value::Array(
                object
                    .iter()
                    .map(|(key, value)| Value::Array(vec![Value::Str(key.clone()), value.clone()]))
                    .collect(),
            ))
        }
        ObjectKeys => Ok(Value::Array(
            expect_object(&arguments[0], "object::keys")?
                .keys()
                .cloned()
                .map(Value::Str)
                .collect(),
        )),
        ObjectValues => Ok(Value::Array(
            expect_object(&arguments[0], "object::values")?
                .values()
                .cloned()
                .collect(),
        )),
        ObjectLen => Ok(Value::Integer(
            expect_object(&arguments[0], "object::len")?.len() as i64,
        )),
        ObjectIsEmpty => Ok(Value::Bool(
            expect_object(&arguments[0], "object::is_empty")?.is_empty(),
        )),
        ObjectExtend => {
            let mut output = expect_object(&arguments[0], "object::extend")?.clone();
            output.extend(expect_object(&arguments[1], "object::extend")?.clone());
            Ok(Value::Object(output))
        }
        ObjectFromEntries => object_from_entries(&arguments[0]),
        ObjectRemove => {
            let mut output = expect_object(&arguments[0], "object::remove")?.clone();
            for value in &arguments[1..] {
                output.remove(expect_string(value, "object::remove key")?);
            }
            Ok(Value::Object(output))
        }
        SetAdd => {
            let mut values = take_set(&arguments[0], "set::add")?;
            values.push(arguments[1].clone());
            SetValue::new(values).map(Value::Set)
        }
        SetAt => Ok(array_at(
            set_slice(&arguments[0], "set::at")?,
            expect_integer(&arguments[1], "set::at index")?,
        )),
        SetContains => Ok(Value::Bool(value_in_collection(
            &arguments[1],
            set_slice(&arguments[0], "set::contains")?,
        ))),
        SetFirst | SetLast | SetMax | SetMin => {
            let values = set_slice(&arguments[0], "set function")?;
            let selected = match function {
                SetFirst | SetMin => values.first(),
                SetLast | SetMax => values.last(),
                _ => unreachable!(),
            };
            Ok(selected.cloned().unwrap_or(Value::None))
        }
        SetLen => Ok(Value::Integer(
            set_slice(&arguments[0], "set::len")?.len() as i64
        )),
        SetIsEmpty => Ok(Value::Bool(
            set_slice(&arguments[0], "set::is_empty")?.is_empty(),
        )),
        SetJoin => join_collection(&arguments[0], &arguments[1]),
        SetRemove => {
            let mut values = take_set(&arguments[0], "set::remove")?;
            values.retain(|value| !values_equal(value, &arguments[1]));
            SetValue::new(values).map(Value::Set)
        }
        SetSlice => SetValue::new(slice_collection(
            set_slice(&arguments[0], "set::slice")?,
            expect_integer(&arguments[1], "set::slice start")?,
            arguments
                .get(2)
                .map(|value| expect_integer(value, "set::slice end"))
                .transpose()?,
        ))
        .map(Value::Set),
        SetUnion | SetIntersect | SetDifference | SetComplement => {
            evaluate_set_operation(function, &arguments[0], &arguments[1])
        }
        MathConstant(constant) => Ok(Value::Float(math_constant(constant))),
        MathUnary(operation) => evaluate_math_unary(operation, &arguments[0]),
        MathClamp => {
            let value = numeric_f64(&arguments[0], "math::clamp")?;
            let minimum = numeric_f64(&arguments[1], "math::clamp")?;
            let maximum = numeric_f64(&arguments[2], "math::clamp")?;
            if minimum > maximum {
                return Err(FastDbError::Schema(
                    "math::clamp minimum exceeds maximum".into(),
                ));
            }
            Ok(if value < minimum {
                arguments[1].clone()
            } else if value > maximum {
                arguments[2].clone()
            } else {
                arguments[0].clone()
            })
        }
        MathLerp => finite_float(
            numeric_f64(&arguments[0], "math::lerp")?
                + (numeric_f64(&arguments[1], "math::lerp")?
                    - numeric_f64(&arguments[0], "math::lerp")?)
                    * numeric_f64(&arguments[2], "math::lerp")?,
            "math::lerp",
        ),
        MathLog => finite_float(
            numeric_f64(&arguments[0], "math::log")?.log(numeric_f64(&arguments[1], "math::log")?),
            "math::log",
        ),
        MathPow => arithmetic(
            BinaryOperator::Power,
            EvalValue::Present(arguments[0].clone()),
            EvalValue::Present(arguments[1].clone()),
        )
        .map(EvalValue::into_function_value),
        MathMax | MathMin => {
            let values = collection_slice(&arguments[0], "math min/max")?;
            let value = if function == MathMax {
                values
                    .iter()
                    .max_by(|left, right| canonical_value_cmp(left, right))
            } else {
                values
                    .iter()
                    .min_by(|left, right| canonical_value_cmp(left, right))
            };
            Ok(value.cloned().unwrap_or(Value::None))
        }
        MathSum | MathProduct => {
            let values = collection_slice(&arguments[0], "math aggregate")?;
            let identity = if function == MathSum { 0 } else { 1 };
            let operator = if function == MathSum {
                BinaryOperator::Add
            } else {
                BinaryOperator::Multiply
            };
            let mut result = Value::Integer(identity);
            for value in values {
                result = arithmetic(
                    operator,
                    EvalValue::Present(result),
                    EvalValue::Present(value.clone()),
                )?
                .into_function_value();
            }
            Ok(result)
        }
        MathMean => {
            let values = collection_slice(&arguments[0], "math::mean")?;
            if values.is_empty() {
                return Ok(Value::None);
            }
            let sum = values.iter().try_fold(0.0, |sum, value| {
                Ok::<_, FastDbError>(sum + numeric_f64(value, "math::mean")?)
            })?;
            finite_float(sum / values.len() as f64, "math::mean")
        }
        MathSpread => {
            let values = collection_slice(&arguments[0], "math::spread")?;
            let Some(minimum) = values
                .iter()
                .min_by(|left, right| canonical_value_cmp(left, right))
            else {
                return Ok(Value::None);
            };
            let maximum = values
                .iter()
                .max_by(|left, right| canonical_value_cmp(left, right))
                .expect("nonempty values have maximum");
            arithmetic(
                BinaryOperator::Subtract,
                EvalValue::Present(maximum.clone()),
                EvalValue::Present(minimum.clone()),
            )
            .map(EvalValue::into_function_value)
        }
        TypeCast(cast) => evaluate_type_cast(cast, &arguments),
        TypeIs(kind) => Ok(Value::Bool(value_is_type(&arguments[0], kind))),
        TypeOf => Ok(Value::Str(type_name(&arguments[0]).into())),
        RecordId => match &arguments[0] {
            Value::RecordId(record) => Ok(match &record.id {
                RecordIdValue::String(value) => Value::Str(value.clone()),
                RecordIdValue::Integer(value) => Value::Integer(*value),
                RecordIdValue::Uuid(value) => Value::Uuid(*value),
            }),
            _ => Err(argument_type("record::id", "record")),
        },
        RecordTable => match &arguments[0] {
            Value::RecordId(record) => Ok(Value::Str(record.table.clone())),
            _ => Err(argument_type("record table", "record")),
        },
        DurationMax => DurationValue::new(u64::MAX, 999_999_999).map(Value::Duration),
        DurationExtract(unit) => {
            let Value::Duration(duration) = arguments[0] else {
                return Err(argument_type("duration extraction", "duration"));
            };
            let value = duration_nanos(duration) / duration_unit_nanos(unit);
            i64::try_from(value)
                .map(Value::Integer)
                .map_err(|_| FastDbError::Schema("duration extraction exceeds int range".into()))
        }
        DurationFrom(unit) => {
            let value = u128::try_from(expect_integer(&arguments[0], "duration constructor")?)
                .map_err(|_| FastDbError::Schema("duration input must be nonnegative".into()))?;
            let nanos = value
                .checked_mul(duration_unit_nanos(unit))
                .ok_or_else(|| FastDbError::Schema("duration constructor overflow".into()))?;
            duration_from_nanos(nanos).map(Value::Duration)
        }
        TimeEpoch => DatetimeValue::from_timestamp(0, 0).map(Value::Datetime),
        TimeNow => DatetimeValue::from_utc(Utc::now()).map(Value::Datetime),
        TimeTimezone => Ok(Value::Str(Local::now().offset().to_string())),
        TimePart(part) => evaluate_time_part(part, &arguments[0]),
        TimeFrom(unit) => evaluate_time_from(unit, &arguments[0]),
        TimeFromUuid => evaluate_time_from_uuid(&arguments[0]),
        TimeIsLeapYear => {
            let datetime = expect_datetime(&arguments[0], "time::is_leap_year")?.as_utc();
            let year = datetime.year();
            Ok(Value::Bool(
                year % 4 == 0 && (year % 100 != 0 || year % 400 == 0),
            ))
        }
        TimeMin | TimeMax => {
            let values = collection_slice(&arguments[0], "time min/max")?;
            let mut datetimes = values
                .iter()
                .map(|value| expect_datetime(value, "time min/max"));
            let Some(first) = datetimes.next() else {
                return Ok(Value::None);
            };
            let mut selected = *first?;
            for datetime in datetimes {
                let datetime = datetime?;
                if (function == TimeMin && datetime < &selected)
                    || (function == TimeMax && datetime > &selected)
                {
                    selected = *datetime;
                }
            }
            Ok(Value::Datetime(selected))
        }
        TimeFormat => {
            let datetime = expect_datetime(&arguments[0], "time::format")?.as_utc();
            let format = expect_string(&arguments[1], "time::format pattern")?;
            if format.len() > 4_096 {
                return Err(FastDbError::ResourceLimit(
                    "time format pattern exceeds the limit".into(),
                ));
            }
            let output = datetime.format(format).to_string();
            if output.len() > 1_048_576 {
                return Err(FastDbError::ResourceLimit(
                    "formatted time exceeds the output limit".into(),
                ));
            }
            Ok(Value::Str(output))
        }
        TimeSet(part) => evaluate_time_set(part, &arguments[0], &arguments[1]),
        TimeTruncate(mode) => evaluate_time_truncate(mode, &arguments[0], &arguments[1]),
        EncodingBase64Encode => match &arguments[0] {
            Value::Bytes(value) => Ok(Value::Str(
                base64::engine::general_purpose::STANDARD_NO_PAD.encode(value),
            )),
            _ => Err(argument_type("encoding::base64::encode", "bytes")),
        },
        EncodingBase64Decode => {
            let value = expect_string(&arguments[0], "encoding::base64::decode")?;
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(value)
                .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(value))
                .map_err(|_| FastDbError::Schema("invalid base64 input".into()))?;
            if decoded.len() > 16 * 1024 * 1024 {
                return Err(FastDbError::ResourceLimit(
                    "decoded base64 exceeds the byte limit".into(),
                ));
            }
            Ok(Value::Bytes(decoded))
        }
        EncodingJsonEncode => {
            let encoded = serde_json::to_string(&encode_value(&arguments[0])?)
                .map_err(|error| FastDbError::Schema(error.to_string()))?;
            if encoded.len() > 16 * 1024 * 1024 {
                return Err(FastDbError::ResourceLimit(
                    "encoded JSON exceeds the output limit".into(),
                ));
            }
            Ok(Value::Str(encoded))
        }
        EncodingJsonDecode => {
            let value = expect_string(&arguments[0], "encoding::json::decode")?;
            if value.len() > 16 * 1024 * 1024 {
                return Err(FastDbError::ResourceLimit(
                    "JSON input exceeds the byte limit".into(),
                ));
            }
            let decoded = serde_json::from_str(value)
                .map_err(|error| FastDbError::Schema(format!("invalid JSON: {error}")))?;
            decode_value(decoded)
        }
        EncodingCborEncode => {
            let encoded = encode_value(&arguments[0])?;
            let mut output = Vec::new();
            ciborium::into_writer(&encoded, &mut output)
                .map_err(|error| FastDbError::Schema(format!("CBOR encoding failed: {error}")))?;
            if output.len() > 16 * 1024 * 1024 {
                return Err(FastDbError::ResourceLimit(
                    "encoded CBOR exceeds the output limit".into(),
                ));
            }
            Ok(Value::Bytes(output))
        }
        EncodingCborDecode => {
            let Value::Bytes(value) = &arguments[0] else {
                return Err(argument_type("encoding::cbor::decode", "bytes"));
            };
            if value.len() > 16 * 1024 * 1024 {
                return Err(FastDbError::ResourceLimit(
                    "CBOR input exceeds the byte limit".into(),
                ));
            }
            let decoded: serde_json::Value = ciborium::from_reader(value.as_slice())
                .map_err(|error| FastDbError::Schema(format!("invalid CBOR: {error}")))?;
            decode_value(decoded)
        }
        CryptoDigest(digest) => evaluate_crypto_digest(digest, &arguments[0]),
        CryptoJoaat => {
            let bytes = crypto_input(&arguments[0])?;
            let mut hash = 0_u32;
            for byte in bytes {
                hash = hash.wrapping_add(u32::from(*byte));
                hash = hash.wrapping_add(hash << 10);
                hash ^= hash >> 6;
            }
            hash = hash.wrapping_add(hash << 3);
            hash ^= hash >> 11;
            hash = hash.wrapping_add(hash << 15);
            Ok(Value::Integer(i64::from(hash)))
        }
    }
}

const MAX_FUNCTION_COLLECTION: usize = 65_536;

fn take_array(value: &Value, function: &str) -> Result<Vec<Value>> {
    match value {
        Value::Array(values) => Ok(values.clone()),
        _ => Err(argument_type(function, "array")),
    }
}

fn collection_slice<'a>(value: &'a Value, function: &str) -> Result<&'a [Value]> {
    match value {
        Value::Array(values) => Ok(values),
        _ => Err(argument_type(function, "array")),
    }
}

fn take_set(value: &Value, function: &str) -> Result<Vec<Value>> {
    match value {
        Value::Set(values) => Ok(values.as_slice().to_vec()),
        _ => Err(argument_type(function, "set")),
    }
}

fn set_slice<'a>(value: &'a Value, function: &str) -> Result<&'a [Value]> {
    match value {
        Value::Set(values) => Ok(values.as_slice()),
        _ => Err(argument_type(function, "set")),
    }
}

fn collection_len(value: &Value, function: &str) -> Result<i64> {
    Ok(i64::try_from(collection_slice(value, function)?.len())
        .expect("bounded collection length fits i64"))
}

fn expect_object<'a>(value: &'a Value, function: &str) -> Result<&'a BTreeMap<String, Value>> {
    match value {
        Value::Object(value) => Ok(value),
        _ => Err(argument_type(function, "object")),
    }
}

fn expect_integer(value: &Value, name: &str) -> Result<i64> {
    match value {
        Value::Integer(value) => Ok(*value),
        _ => Err(argument_type(name, "integer")),
    }
}

fn expect_nonnegative_usize(value: &Value, name: &str) -> Result<usize> {
    usize::try_from(expect_integer(value, name)?)
        .map_err(|_| FastDbError::Schema(format!("{name} must be nonnegative")))
}

fn expect_string<'a>(value: &'a Value, name: &str) -> Result<&'a str> {
    match value {
        Value::Str(value) => Ok(value),
        _ => Err(argument_type(name, "string")),
    }
}

fn argument_type(function: &str, expected: &str) -> FastDbError {
    FastDbError::Schema(format!("{function} requires {expected} arguments"))
}

fn values_equal(left: &Value, right: &Value) -> bool {
    canonical_value_cmp(left, right) == Ordering::Equal
}

fn value_in_collection(value: &Value, values: &[Value]) -> bool {
    values
        .iter()
        .any(|candidate| values_equal(candidate, value))
}

fn ensure_collection_growth(length: usize, additional: usize) -> Result<()> {
    if length.saturating_add(additional) > MAX_FUNCTION_COLLECTION {
        Err(FastDbError::ResourceLimit(
            "function result exceeds the collection limit".into(),
        ))
    } else {
        Ok(())
    }
}

fn push_bounded(values: &mut Vec<Value>, value: Value) -> Result<()> {
    ensure_collection_growth(values.len(), 1)?;
    values.push(value);
    Ok(())
}

fn push_unique(values: &mut Vec<Value>, value: Value) -> Result<()> {
    if !value_in_collection(&value, values) {
        push_bounded(values, value)?;
    }
    Ok(())
}

fn relative_index(index: i64, length: usize) -> Option<usize> {
    let length = i64::try_from(length).ok()?;
    let index = if index < 0 {
        length.checked_add(index)?
    } else {
        index
    };
    usize::try_from(index)
        .ok()
        .filter(|index| *index < length as usize)
}

fn array_at(values: &[Value], index: i64) -> Value {
    relative_index(index, values.len())
        .and_then(|index| values.get(index))
        .cloned()
        .unwrap_or(Value::None)
}

fn slice_collection(values: &[Value], start: i64, end: Option<i64>) -> Vec<Value> {
    let length = values.len() as i64;
    let normalize = |index: i64| {
        if index < 0 {
            length.saturating_add(index)
        } else {
            index
        }
        .clamp(0, length) as usize
    };
    let start = normalize(start);
    let end = normalize(end.unwrap_or(length));
    if start > end {
        Vec::new()
    } else {
        values[start..end].to_vec()
    }
}

fn evaluate_array_set_operation(function: Builtin, left: &Value, right: &Value) -> Result<Value> {
    let left = collection_slice(left, "array set operation")?;
    let right = collection_slice(right, "array set operation")?;
    let mut output = Vec::new();
    match function {
        Builtin::ArrayUnion => {
            for value in left.iter().chain(right) {
                push_unique(&mut output, value.clone())?;
            }
        }
        Builtin::ArrayIntersect => {
            for value in left {
                if value_in_collection(value, right) {
                    push_unique(&mut output, value.clone())?;
                }
            }
        }
        Builtin::ArrayComplement => {
            for value in left {
                if !value_in_collection(value, right) {
                    push_unique(&mut output, value.clone())?;
                }
            }
        }
        Builtin::ArrayDifference => {
            for value in left {
                if !value_in_collection(value, right) {
                    push_unique(&mut output, value.clone())?;
                }
            }
            for value in right {
                if !value_in_collection(value, left) {
                    push_unique(&mut output, value.clone())?;
                }
            }
        }
        _ => unreachable!(),
    }
    Ok(Value::Array(output))
}

fn evaluate_set_operation(function: Builtin, left: &Value, right: &Value) -> Result<Value> {
    let left = set_slice(left, "set operation")?;
    let right = set_slice(right, "set operation")?;
    let values = match function {
        Builtin::SetUnion => left.iter().chain(right).cloned().collect::<Vec<_>>(),
        Builtin::SetIntersect => left
            .iter()
            .filter(|value| value_in_collection(value, right))
            .cloned()
            .collect(),
        Builtin::SetComplement => left
            .iter()
            .filter(|value| !value_in_collection(value, right))
            .cloned()
            .collect(),
        Builtin::SetDifference => left
            .iter()
            .filter(|value| !value_in_collection(value, right))
            .chain(
                right
                    .iter()
                    .filter(|value| !value_in_collection(value, left)),
            )
            .cloned()
            .collect(),
        _ => unreachable!(),
    };
    SetValue::new(values).map(Value::Set)
}

fn evaluate_array_range(arguments: &[Value]) -> Result<Value> {
    let start = expect_integer(&arguments[0], "array::range start")?;
    let end = expect_integer(&arguments[1], "array::range end")?;
    let step = arguments
        .get(2)
        .map(|value| expect_integer(value, "array::range step"))
        .transpose()?
        .unwrap_or(if start <= end { 1 } else { -1 });
    if step == 0 {
        return Err(FastDbError::Schema(
            "array::range step cannot be zero".into(),
        ));
    }
    let mut output = Vec::new();
    let mut value = start;
    while (step > 0 && value < end) || (step < 0 && value > end) {
        push_bounded(&mut output, Value::Integer(value))?;
        value = value
            .checked_add(step)
            .ok_or_else(|| FastDbError::Schema("array::range overflow".into()))?;
    }
    Ok(Value::Array(output))
}

fn join_collection(collection: &Value, separator: &Value) -> Result<Value> {
    let separator = expect_string(separator, "join separator")?;
    let values = match collection {
        Value::Array(values) => values.as_slice(),
        Value::Set(values) => values.as_slice(),
        _ => return Err(argument_type("join", "array or set")),
    };
    let mut output = String::new();
    for (index, value) in values.iter().cloned().enumerate() {
        if index > 0 {
            output.push_str(separator);
        }
        output.push_str(&render_string(value)?);
        if output.len() > 16 * 1024 * 1024 {
            return Err(FastDbError::ResourceLimit(
                "joined string exceeds the output limit".into(),
            ));
        }
    }
    Ok(Value::Str(output))
}

fn object_from_entries(value: &Value) -> Result<Value> {
    let entries = collection_slice(value, "object::from_entries")?;
    let mut output = BTreeMap::new();
    for entry in entries {
        let pair = collection_slice(entry, "object::from_entries entry")?;
        if pair.len() != 2 {
            return Err(FastDbError::Schema(
                "object::from_entries entries require exactly two values".into(),
            ));
        }
        output.insert(
            expect_string(&pair[0], "object::from_entries key")?.to_string(),
            pair[1].clone(),
        );
    }
    Ok(Value::Object(output))
}

fn math_constant(constant: MathConstant) -> f64 {
    match constant {
        MathConstant::E => std::f64::consts::E,
        MathConstant::Frac1Pi => std::f64::consts::FRAC_1_PI,
        MathConstant::Frac1Sqrt2 => std::f64::consts::FRAC_1_SQRT_2,
        MathConstant::Frac2Pi => std::f64::consts::FRAC_2_PI,
        MathConstant::Frac2SqrtPi => std::f64::consts::FRAC_2_SQRT_PI,
        MathConstant::FracPi2 => std::f64::consts::FRAC_PI_2,
        MathConstant::FracPi3 => std::f64::consts::FRAC_PI_3,
        MathConstant::FracPi4 => std::f64::consts::FRAC_PI_4,
        MathConstant::FracPi6 => std::f64::consts::FRAC_PI_6,
        MathConstant::FracPi8 => std::f64::consts::FRAC_PI_8,
        MathConstant::Ln2 => std::f64::consts::LN_2,
        MathConstant::Ln10 => std::f64::consts::LN_10,
        MathConstant::Log2E => std::f64::consts::LOG2_E,
        MathConstant::Log2Ten => std::f64::consts::LOG2_10,
        MathConstant::Log10E => std::f64::consts::LOG10_E,
        MathConstant::Log10Two => std::f64::consts::LOG10_2,
        MathConstant::Pi => std::f64::consts::PI,
        MathConstant::Sqrt2 => std::f64::consts::SQRT_2,
        MathConstant::Tau => std::f64::consts::TAU,
    }
}

fn duration_unit_nanos(unit: DurationUnit) -> u128 {
    const SECOND: u128 = 1_000_000_000;
    match unit {
        DurationUnit::Years => 365 * 24 * 60 * 60 * SECOND,
        DurationUnit::Weeks => 7 * 24 * 60 * 60 * SECOND,
        DurationUnit::Days => 24 * 60 * 60 * SECOND,
        DurationUnit::Hours => 60 * 60 * SECOND,
        DurationUnit::Minutes => 60 * SECOND,
        DurationUnit::Seconds => SECOND,
        DurationUnit::Milliseconds => 1_000_000,
        DurationUnit::Microseconds => 1_000,
        DurationUnit::Nanoseconds => 1,
    }
}

fn expect_datetime<'a>(value: &'a Value, function: &str) -> Result<&'a DatetimeValue> {
    match value {
        Value::Datetime(value) => Ok(value),
        _ => Err(argument_type(function, "datetime")),
    }
}

fn evaluate_time_part(part: TimePart, value: &Value) -> Result<Value> {
    let datetime = expect_datetime(value, "time extraction")?.as_utc();
    let value = match part {
        TimePart::Year => i64::from(datetime.year()),
        TimePart::Month => i64::from(datetime.month()),
        TimePart::Day => i64::from(datetime.day()),
        TimePart::Hour => i64::from(datetime.hour()),
        TimePart::Minute => i64::from(datetime.minute()),
        TimePart::Second => i64::from(datetime.second()),
        TimePart::Nano => datetime
            .timestamp_nanos_opt()
            .ok_or_else(|| FastDbError::Schema("datetime nanoseconds exceed int range".into()))?,
        TimePart::Unix => datetime.timestamp(),
        TimePart::Millis => datetime.timestamp_millis(),
        TimePart::Micros => datetime.timestamp_micros(),
        TimePart::Weekday => i64::from(datetime.weekday().num_days_from_sunday()),
        TimePart::Week => i64::from(datetime.iso_week().week()),
        TimePart::YearDay => i64::from(datetime.ordinal()),
    };
    Ok(Value::Integer(value))
}

fn evaluate_time_from(unit: DurationUnit, value: &Value) -> Result<Value> {
    let value = expect_integer(value, "time constructor")?;
    let nanos_per_unit = duration_unit_nanos(unit);
    let total = i128::from(value)
        .checked_mul(i128::try_from(nanos_per_unit).expect("time unit fits i128"))
        .ok_or_else(|| FastDbError::Schema("time constructor overflow".into()))?;
    let seconds = total.div_euclid(1_000_000_000);
    let nanos = total.rem_euclid(1_000_000_000);
    DatetimeValue::from_timestamp(
        i64::try_from(seconds)
            .map_err(|_| FastDbError::Schema("datetime is outside the supported range".into()))?,
        u32::try_from(nanos).expect("nanosecond remainder fits u32"),
    )
    .map(Value::Datetime)
}

fn evaluate_time_from_uuid(value: &Value) -> Result<Value> {
    let Value::Uuid(value) = value else {
        return Err(argument_type("time::from_uuid", "UUIDv7"));
    };
    let timestamp = value
        .get_timestamp()
        .ok_or_else(|| FastDbError::Schema("time::from_uuid requires UUIDv7".into()))?;
    let (seconds, nanoseconds) = timestamp.to_unix();
    DatetimeValue::from_timestamp(
        i64::try_from(seconds)
            .map_err(|_| FastDbError::Schema("UUID time is outside the supported range".into()))?,
        nanoseconds,
    )
    .map(Value::Datetime)
}

fn evaluate_time_set(part: TimePart, datetime: &Value, replacement: &Value) -> Result<Value> {
    let datetime = expect_datetime(datetime, "time setter")?.as_utc();
    let replacement = expect_integer(replacement, "time setter value")?;
    let changed = match part {
        TimePart::Year => datetime.with_year(
            i32::try_from(replacement)
                .map_err(|_| FastDbError::Schema("time year is outside range".into()))?,
        ),
        TimePart::Month => datetime.with_month(
            u32::try_from(replacement)
                .map_err(|_| FastDbError::Schema("time month is outside range".into()))?,
        ),
        TimePart::Day => datetime.with_day(
            u32::try_from(replacement)
                .map_err(|_| FastDbError::Schema("time day is outside range".into()))?,
        ),
        TimePart::Hour => datetime.with_hour(
            u32::try_from(replacement)
                .map_err(|_| FastDbError::Schema("time hour is outside range".into()))?,
        ),
        TimePart::Minute => datetime.with_minute(
            u32::try_from(replacement)
                .map_err(|_| FastDbError::Schema("time minute is outside range".into()))?,
        ),
        TimePart::Second => datetime.with_second(
            u32::try_from(replacement)
                .map_err(|_| FastDbError::Schema("time second is outside range".into()))?,
        ),
        TimePart::Nano => datetime.with_nanosecond(
            u32::try_from(replacement)
                .map_err(|_| FastDbError::Schema("time nanosecond is outside range".into()))?,
        ),
        _ => {
            return Err(FastDbError::Schema(
                "time setter does not support this component".into(),
            ))
        }
    }
    .ok_or_else(|| FastDbError::Schema("time setter produced an invalid datetime".into()))?;
    DatetimeValue::from_utc(changed).map(Value::Datetime)
}

fn evaluate_time_truncate(mode: TimeTruncate, datetime: &Value, duration: &Value) -> Result<Value> {
    let datetime = expect_datetime(datetime, "time rounding")?;
    let Value::Duration(duration) = duration else {
        return Err(argument_type("time rounding", "duration"));
    };
    let quantum = i128::try_from(duration_nanos(*duration))
        .map_err(|_| FastDbError::Schema("time rounding duration is too large".into()))?;
    if quantum == 0 {
        return Err(FastDbError::Schema(
            "time rounding duration must be nonzero".into(),
        ));
    }
    let value =
        i128::from(datetime.timestamp()) * 1_000_000_000 + i128::from(datetime.nanosecond());
    let floor = value.div_euclid(quantum) * quantum;
    let rounded = match mode {
        TimeTruncate::Floor => floor,
        TimeTruncate::Ceil if floor == value => floor,
        TimeTruncate::Ceil => floor + quantum,
        TimeTruncate::Round if value - floor < quantum - (value - floor) => floor,
        TimeTruncate::Round => floor + quantum,
    };
    DatetimeValue::from_timestamp(
        i64::try_from(rounded.div_euclid(1_000_000_000))
            .map_err(|_| FastDbError::Schema("rounded datetime is outside range".into()))?,
        u32::try_from(rounded.rem_euclid(1_000_000_000)).expect("nanosecond remainder fits u32"),
    )
    .map(Value::Datetime)
}

fn evaluate_math_unary(operation: MathUnary, value: &Value) -> Result<Value> {
    if operation == MathUnary::Abs {
        return match value {
            Value::Integer(value) => value
                .checked_abs()
                .map(Value::Integer)
                .ok_or_else(|| FastDbError::Schema("math::abs overflow".into())),
            Value::Decimal(value) => {
                DecimalValue::parse(&value.as_decimal().abs().to_string()).map(Value::Decimal)
            }
            _ => finite_float(numeric_f64(value, "math::abs")?.abs(), "math::abs"),
        };
    }
    if operation == MathUnary::Sign {
        let value = numeric_f64(value, "math::sign")?;
        return Ok(Value::Integer(if value < 0.0 {
            -1
        } else if value > 0.0 {
            1
        } else {
            0
        }));
    }
    let value = numeric_f64(value, "math function")?;
    let output = match operation {
        MathUnary::Acos => value.acos(),
        MathUnary::Acot => std::f64::consts::FRAC_PI_2 - value.atan(),
        MathUnary::Asin => value.asin(),
        MathUnary::Atan => value.atan(),
        MathUnary::Ceil => value.ceil(),
        MathUnary::Cos => value.cos(),
        MathUnary::Cot => 1.0 / value.tan(),
        MathUnary::DegToRad => value.to_radians(),
        MathUnary::Floor => value.floor(),
        MathUnary::Ln => value.ln(),
        MathUnary::Log10 => value.log10(),
        MathUnary::Log2 => value.log2(),
        MathUnary::RadToDeg => value.to_degrees(),
        MathUnary::Round => value.round(),
        MathUnary::Sin => value.sin(),
        MathUnary::Sqrt => value.sqrt(),
        MathUnary::Tan => value.tan(),
        MathUnary::Abs | MathUnary::Sign => unreachable!(),
    };
    finite_float(output, "math function")
}

fn numeric_f64(value: &Value, function: &str) -> Result<f64> {
    match value {
        Value::Integer(value) => Ok(*value as f64),
        Value::Float(value) if value.is_finite() => Ok(*value),
        Value::Decimal(value) => value
            .as_decimal()
            .to_f64()
            .filter(|value| value.is_finite())
            .ok_or_else(|| argument_type(function, "finite numeric")),
        _ => Err(argument_type(function, "numeric")),
    }
}

fn finite_float(value: f64, function: &str) -> Result<Value> {
    if value.is_finite() {
        Ok(Value::Float(value))
    } else {
        Err(FastDbError::Schema(format!(
            "{function} produced a non-finite result"
        )))
    }
}

fn crypto_input(value: &Value) -> Result<&[u8]> {
    match value {
        Value::Str(value) => Ok(value.as_bytes()),
        Value::Bytes(value) => Ok(value),
        _ => Err(argument_type("crypto digest", "string or bytes")),
    }
}

fn evaluate_crypto_digest(digest: CryptoDigest, value: &Value) -> Result<Value> {
    let input = crypto_input(value)?;
    let output = match digest {
        CryptoDigest::Blake3 => blake3::hash(input).to_hex().to_string(),
        CryptoDigest::Md5 => {
            use md5::Digest as _;
            hex::encode(md5::Md5::digest(input))
        }
        CryptoDigest::Sha1 => {
            use sha1::Digest as _;
            hex::encode(sha1::Sha1::digest(input))
        }
        CryptoDigest::Sha256 => {
            use sha2::Digest as _;
            hex::encode(sha2::Sha256::digest(input))
        }
        CryptoDigest::Sha512 => {
            use sha2::Digest as _;
            hex::encode(sha2::Sha512::digest(input))
        }
    };
    Ok(Value::Str(output))
}

fn evaluate_type_cast(cast: TypeCast, arguments: &[Value]) -> Result<Value> {
    match cast {
        TypeCast::Array => cast_array(arguments[0].clone()).map(Value::Array),
        TypeCast::Bool => match &arguments[0] {
            Value::Bool(value) => Ok(Value::Bool(*value)),
            Value::Str(value) if value.eq_ignore_ascii_case("true") => Ok(Value::Bool(true)),
            Value::Str(value) if value.eq_ignore_ascii_case("false") => Ok(Value::Bool(false)),
            _ => Err(argument_type("type::bool", "bool or boolean string")),
        },
        TypeCast::Bytes => match &arguments[0] {
            Value::Bytes(value) => Ok(Value::Bytes(value.clone())),
            Value::Str(value) => Ok(Value::Bytes(value.as_bytes().to_vec())),
            _ => Err(argument_type("type::bytes", "string or bytes")),
        },
        TypeCast::Datetime => match &arguments[0] {
            Value::Datetime(value) => Ok(Value::Datetime(*value)),
            Value::Str(value) => DatetimeValue::parse(value).map(Value::Datetime),
            _ => Err(argument_type(
                "type::datetime",
                "datetime or RFC 3339 string",
            )),
        },
        TypeCast::Decimal => cast_decimal(arguments[0].clone()).map(Value::Decimal),
        TypeCast::Duration => match &arguments[0] {
            Value::Duration(value) => Ok(Value::Duration(*value)),
            Value::Str(value) => DurationValue::parse(value).map(Value::Duration),
            _ => Err(argument_type(
                "type::duration",
                "duration or duration string",
            )),
        },
        TypeCast::File => match &arguments[0] {
            Value::File(value) => Ok(Value::File(value.clone())),
            Value::Str(value) => FileValue::new(value.clone()).map(Value::File),
            _ => Err(argument_type("type::file", "file or string")),
        },
        TypeCast::Float => cast_float(arguments[0].clone()).map(Value::Float),
        TypeCast::Int => cast_int(arguments[0].clone()).map(Value::Integer),
        TypeCast::Number => match &arguments[0] {
            value @ (Value::Integer(_) | Value::Float(_) | Value::Decimal(_)) => Ok(value.clone()),
            Value::Str(value) => value
                .parse::<i64>()
                .map(Value::Integer)
                .or_else(|_| {
                    value.parse::<f64>().map_err(|_| ()).and_then(|value| {
                        value.is_finite().then_some(Value::Float(value)).ok_or(())
                    })
                })
                .map_err(|_| argument_type("type::number", "numeric or numeric string")),
            _ => Err(argument_type("type::number", "numeric or numeric string")),
        },
        TypeCast::Range => cast_present(arguments[0].clone(), &SchemaTypeKind::Range),
        TypeCast::Record | TypeCast::Thing => {
            if arguments.len() == 2 {
                let table = expect_string(&arguments[0], "record table")?;
                let id = match &arguments[1] {
                    Value::Str(value) => RecordIdValue::String(value.clone()),
                    Value::Integer(value) => RecordIdValue::Integer(*value),
                    Value::Uuid(value) => RecordIdValue::Uuid(*value),
                    _ => return Err(argument_type("type::record", "record ID component")),
                };
                Ok(Value::RecordId(RecordId::new(table, id)))
            } else {
                match &arguments[0] {
                    Value::RecordId(value) => Ok(Value::RecordId(value.clone())),
                    Value::Str(value) => parse_record_string(value).map(Value::RecordId),
                    _ => Err(argument_type("type::record", "record or record string")),
                }
            }
        }
        TypeCast::String => render_string(arguments[0].clone()).map(Value::Str),
        TypeCast::StringLossy => match &arguments[0] {
            Value::Bytes(value) => Ok(Value::Str(String::from_utf8_lossy(value).into_owned())),
            value => render_string(value.clone()).map(Value::Str),
        },
        TypeCast::Table => match &arguments[0] {
            Value::Table(value) => Ok(Value::Table(value.clone())),
            Value::RecordId(value) => TableValue::new(value.table.clone()).map(Value::Table),
            Value::Str(value) => TableValue::new(value.clone()).map(Value::Table),
            _ => Err(argument_type("type::table", "table, record, or string")),
        },
        TypeCast::Uuid => match &arguments[0] {
            Value::Uuid(value) => Ok(Value::Uuid(*value)),
            Value::Str(value) => uuid::Uuid::parse_str(value)
                .map(Value::Uuid)
                .map_err(|_| argument_type("type::uuid", "UUID or UUID string")),
            _ => Err(argument_type("type::uuid", "UUID or UUID string")),
        },
    }
}

fn value_is_type(value: &Value, kind: TypeKind) -> bool {
    match kind {
        TypeKind::Array => matches!(value, Value::Array(_)),
        TypeKind::Bool => matches!(value, Value::Bool(_)),
        TypeKind::Bytes => matches!(value, Value::Bytes(_)),
        TypeKind::Collection => matches!(value, Value::Array(_) | Value::Set(_)),
        TypeKind::Datetime => matches!(value, Value::Datetime(_)),
        TypeKind::Decimal => matches!(value, Value::Decimal(_)),
        TypeKind::Duration => matches!(value, Value::Duration(_)),
        TypeKind::Float => matches!(value, Value::Float(_)),
        TypeKind::None => matches!(value, Value::None),
        TypeKind::Null => matches!(value, Value::Null),
        TypeKind::Number => matches!(
            value,
            Value::Integer(_) | Value::Float(_) | Value::Decimal(_)
        ),
        TypeKind::Object => matches!(value, Value::Object(_)),
        TypeKind::Range => matches!(value, Value::Range(_)),
        TypeKind::Record => matches!(value, Value::RecordId(_)),
        TypeKind::String => matches!(value, Value::Str(_)),
        TypeKind::Uuid => matches!(value, Value::Uuid(_)),
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::None => "none",
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Integer(_) => "int",
        Value::Float(_) => "float",
        Value::Decimal(_) => "decimal",
        Value::Str(_) => "string",
        Value::Bytes(_) => "bytes",
        Value::Duration(_) => "duration",
        Value::Datetime(_) => "datetime",
        Value::Uuid(_) => "uuid",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::Set(_) => "set",
        Value::Range(_) => "range",
        Value::Regex(_) => "regex",
        Value::RecordId(_) => "record",
        Value::Table(_) => "table",
        Value::File(_) => "file",
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
        BinaryOperator::Equal | BinaryOperator::ExactEqual | BinaryOperator::NotEqual => {
            let equal = equal_values(&left, &right);
            Ok(EvalValue::Present(Value::Bool(
                if operator != BinaryOperator::NotEqual {
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
        | BinaryOperator::Divide
        | BinaryOperator::Modulo
        | BinaryOperator::Power => arithmetic(operator, left, right),
        BinaryOperator::Contains
        | BinaryOperator::ContainsNot
        | BinaryOperator::ContainsAll
        | BinaryOperator::ContainsAny
        | BinaryOperator::ContainsNone
        | BinaryOperator::Inside
        | BinaryOperator::NotInside
        | BinaryOperator::AllInside
        | BinaryOperator::AnyInside
        | BinaryOperator::NoneInside
        | BinaryOperator::AnyEqual
        | BinaryOperator::AllEqual => containment(operator, left, right),
        BinaryOperator::And
        | BinaryOperator::Or
        | BinaryOperator::NullCoalesce
        | BinaryOperator::TruthyCoalesce => {
            unreachable!("short-circuit operators are handled before RHS evaluation")
        }
        BinaryOperator::FtsMatch(_) => Err(FastDbError::Schema(
            "FTS match predicates require an indexed SELECT context".into(),
        )),
    }
}

fn containment(operator: BinaryOperator, left: EvalValue, right: EvalValue) -> Result<EvalValue> {
    let result = match operator {
        BinaryOperator::Contains => contains(&left, &right)?,
        BinaryOperator::ContainsNot => !contains(&left, &right)?,
        BinaryOperator::ContainsAll => collection_values(&right)?
            .iter()
            .all(|value| contains(&left, &EvalValue::Present(value.clone())).unwrap_or(false)),
        BinaryOperator::ContainsAny => collection_values(&right)?
            .iter()
            .any(|value| contains(&left, &EvalValue::Present(value.clone())).unwrap_or(false)),
        BinaryOperator::ContainsNone => !collection_values(&right)?
            .iter()
            .any(|value| contains(&left, &EvalValue::Present(value.clone())).unwrap_or(false)),
        BinaryOperator::Inside => contains(&right, &left)?,
        BinaryOperator::NotInside => !contains(&right, &left)?,
        BinaryOperator::AllInside => collection_values(&left)?
            .iter()
            .all(|value| contains(&right, &EvalValue::Present(value.clone())).unwrap_or(false)),
        BinaryOperator::AnyInside => collection_values(&left)?
            .iter()
            .any(|value| contains(&right, &EvalValue::Present(value.clone())).unwrap_or(false)),
        BinaryOperator::NoneInside => !collection_values(&left)?
            .iter()
            .any(|value| contains(&right, &EvalValue::Present(value.clone())).unwrap_or(false)),
        BinaryOperator::AnyEqual => collection_values(&left)?
            .iter()
            .any(|value| equal_values(&EvalValue::Present(value.clone()), &right)),
        BinaryOperator::AllEqual => collection_values(&left)?
            .iter()
            .all(|value| equal_values(&EvalValue::Present(value.clone()), &right)),
        _ => unreachable!(),
    };
    Ok(EvalValue::Present(Value::Bool(result)))
}

fn collection_values(value: &EvalValue) -> Result<&[Value]> {
    match value {
        EvalValue::Present(Value::Array(values)) => Ok(values),
        EvalValue::Present(Value::Set(values)) => Ok(values.as_slice()),
        _ => Err(FastDbError::Schema(
            "all/any/none operator requires a collection operand".into(),
        )),
    }
}

fn contains(container: &EvalValue, needle: &EvalValue) -> Result<bool> {
    let (EvalValue::Present(container), EvalValue::Present(needle)) = (container, needle) else {
        return Ok(false);
    };
    Ok(match container {
        Value::Array(values) => values
            .iter()
            .any(|value| canonical_value_cmp(value, needle) == Ordering::Equal),
        Value::Set(values) => values
            .as_slice()
            .iter()
            .any(|value| canonical_value_cmp(value, needle) == Ordering::Equal),
        Value::Str(value) => match needle {
            Value::Str(needle) => value.contains(needle),
            _ => false,
        },
        Value::Bytes(value) => match needle {
            Value::Integer(needle) => u8::try_from(*needle)
                .ok()
                .is_some_and(|needle| value.contains(&needle)),
            Value::Bytes(needle) => {
                needle.is_empty() || value.windows(needle.len()).any(|window| window == needle)
            }
            _ => false,
        },
        Value::Object(value) => match needle {
            Value::Str(key) => value.contains_key(key),
            _ => false,
        },
        Value::Range(range) => range_contains(range, needle),
        _ => false,
    })
}

fn range_contains(range: &RangeValue, value: &Value) -> bool {
    let after_start = match range.start() {
        RangeBound::Unbounded => true,
        RangeBound::Included(start) => canonical_value_cmp(value, start) != Ordering::Less,
        RangeBound::Excluded(start) => canonical_value_cmp(value, start) == Ordering::Greater,
    };
    let before_end = match range.end() {
        RangeBound::Unbounded => true,
        RangeBound::Included(end) => canonical_value_cmp(value, end) != Ordering::Greater,
        RangeBound::Excluded(end) => canonical_value_cmp(value, end) == Ordering::Less,
    };
    after_start && before_end
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
        (Value::Str(mut left), Value::Str(right)) if operator == BinaryOperator::Add => {
            left.try_reserve(right.len()).map_err(|_| {
                FastDbError::ResourceLimit("string concatenation is too large".into())
            })?;
            left.push_str(&right);
            Ok(EvalValue::Present(Value::Str(left)))
        }
        (Value::Bytes(mut left), Value::Bytes(right)) if operator == BinaryOperator::Add => {
            left.try_reserve(right.len()).map_err(|_| {
                FastDbError::ResourceLimit("byte concatenation is too large".into())
            })?;
            left.extend(right);
            Ok(EvalValue::Present(Value::Bytes(left)))
        }
        (Value::Array(mut left), Value::Array(right)) if operator == BinaryOperator::Add => {
            if left.len().saturating_add(right.len()) > 65_536 {
                return Err(FastDbError::ResourceLimit(
                    "array concatenation exceeds the collection limit".into(),
                ));
            }
            left.extend(right);
            Ok(EvalValue::Present(Value::Array(left)))
        }
        (Value::Duration(left), Value::Duration(right)) => {
            duration_arithmetic(operator, left, right)
        }
        (Value::Duration(left), Value::Integer(right))
            if matches!(operator, BinaryOperator::Multiply | BinaryOperator::Divide) =>
        {
            duration_scale(operator, left, right)
        }
        (Value::Datetime(left), Value::Duration(right))
            if matches!(operator, BinaryOperator::Add | BinaryOperator::Subtract) =>
        {
            datetime_duration(operator, left, right)
        }
        (
            left @ (Value::Integer(_) | Value::Decimal(_)),
            right @ (Value::Integer(_) | Value::Decimal(_)),
        ) if matches!(left, Value::Decimal(_)) || matches!(right, Value::Decimal(_)) => {
            decimal_arithmetic(operator, left, right)
        }
        (Value::Integer(left), Value::Integer(right)) => {
            if operator == BinaryOperator::Divide && right == 0 {
                return Ok(EvalValue::Present(Value::Null));
            }
            if operator == BinaryOperator::Modulo && right == 0 {
                return Err(FastDbError::Schema("integer remainder by zero".into()));
            }
            let result = match operator {
                BinaryOperator::Add => left.checked_add(right),
                BinaryOperator::Subtract => left.checked_sub(right),
                BinaryOperator::Multiply => left.checked_mul(right),
                BinaryOperator::Divide => left.checked_div(right),
                BinaryOperator::Modulo => left.checked_rem(right),
                BinaryOperator::Power => u32::try_from(right)
                    .ok()
                    .and_then(|exponent| left.checked_pow(exponent)),
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
    if operator == BinaryOperator::Modulo && right == 0.0 {
        return Err(FastDbError::Schema("float remainder by zero".into()));
    }
    let result = match operator {
        BinaryOperator::Add => left + right,
        BinaryOperator::Subtract => left - right,
        BinaryOperator::Multiply => left * right,
        BinaryOperator::Divide => left / right,
        BinaryOperator::Modulo => left % right,
        BinaryOperator::Power => {
            if right < 0.0 || right.fract() != 0.0 {
                return Err(FastDbError::Schema(
                    "power exponent must be a nonnegative integer".into(),
                ));
            }
            left.powf(right)
        }
        _ => unreachable!(),
    };
    if !result.is_finite() {
        return Err(FastDbError::Schema("non-finite arithmetic result".into()));
    }
    Ok(EvalValue::Present(Value::Float(result)))
}

fn decimal_arithmetic(operator: BinaryOperator, left: Value, right: Value) -> Result<EvalValue> {
    let decimal = |value| match value {
        Value::Integer(value) => Ok(rust_decimal::Decimal::from(value)),
        Value::Decimal(value) => Ok(value.as_decimal()),
        _ => Err(FastDbError::Schema(
            "expected decimal-compatible number".into(),
        )),
    };
    let left = decimal(left)?;
    let right = decimal(right)?;
    if operator == BinaryOperator::Divide && right.is_zero() {
        return Ok(EvalValue::Present(Value::Null));
    }
    if operator == BinaryOperator::Modulo && right.is_zero() {
        return Err(FastDbError::Schema("decimal remainder by zero".into()));
    }
    let result = match operator {
        BinaryOperator::Add => left.checked_add(right),
        BinaryOperator::Subtract => left.checked_sub(right),
        BinaryOperator::Multiply => left.checked_mul(right),
        BinaryOperator::Divide => left.checked_div(right),
        BinaryOperator::Modulo => left.checked_rem(right),
        BinaryOperator::Power => {
            let exponent = right
                .to_u32()
                .filter(|_| right.fract().is_zero())
                .ok_or_else(|| {
                    FastDbError::Schema("power exponent must be a nonnegative integer".into())
                })?;
            let mut result = rust_decimal::Decimal::ONE;
            for _ in 0..exponent {
                result = result
                    .checked_mul(left)
                    .ok_or_else(|| FastDbError::Schema("decimal power overflow".into()))?;
            }
            Some(result)
        }
        _ => unreachable!(),
    }
    .ok_or_else(|| FastDbError::Schema("decimal arithmetic overflow".into()))?;
    DecimalValue::parse(&result.normalize().to_string())
        .map(Value::Decimal)
        .map(EvalValue::Present)
}

fn duration_arithmetic(
    operator: BinaryOperator,
    left: DurationValue,
    right: DurationValue,
) -> Result<EvalValue> {
    let left = duration_nanos(left);
    let right = duration_nanos(right);
    let value = match operator {
        BinaryOperator::Add => left.checked_add(right),
        BinaryOperator::Subtract => left.checked_sub(right),
        _ => None,
    }
    .ok_or_else(|| FastDbError::Schema("duration arithmetic overflow".into()))?;
    duration_from_nanos(value)
        .map(Value::Duration)
        .map(EvalValue::Present)
}

fn duration_scale(
    operator: BinaryOperator,
    duration: DurationValue,
    factor: i64,
) -> Result<EvalValue> {
    let factor = u128::try_from(factor)
        .map_err(|_| FastDbError::Schema("duration factor must be nonnegative".into()))?;
    if operator == BinaryOperator::Divide && factor == 0 {
        return Ok(EvalValue::Present(Value::Null));
    }
    let value = match operator {
        BinaryOperator::Multiply => duration_nanos(duration).checked_mul(factor),
        BinaryOperator::Divide => Some(duration_nanos(duration) / factor),
        _ => None,
    }
    .ok_or_else(|| FastDbError::Schema("duration arithmetic overflow".into()))?;
    duration_from_nanos(value)
        .map(Value::Duration)
        .map(EvalValue::Present)
}

fn datetime_duration(
    operator: BinaryOperator,
    datetime: DatetimeValue,
    duration: DurationValue,
) -> Result<EvalValue> {
    let base = i128::from(datetime.timestamp())
        .checked_mul(1_000_000_000)
        .and_then(|value| value.checked_add(i128::from(datetime.nanosecond())))
        .ok_or_else(|| FastDbError::Schema("datetime arithmetic overflow".into()))?;
    let duration = i128::try_from(duration_nanos(duration))
        .map_err(|_| FastDbError::Schema("duration is too large for datetime".into()))?;
    let value = match operator {
        BinaryOperator::Add => base.checked_add(duration),
        BinaryOperator::Subtract => base.checked_sub(duration),
        _ => None,
    }
    .ok_or_else(|| FastDbError::Schema("datetime arithmetic overflow".into()))?;
    let seconds = value.div_euclid(1_000_000_000);
    let nanoseconds = value.rem_euclid(1_000_000_000);
    DatetimeValue::from_timestamp(
        i64::try_from(seconds)
            .map_err(|_| FastDbError::Schema("datetime is outside the supported range".into()))?,
        u32::try_from(nanoseconds).expect("nanosecond remainder fits u32"),
    )
    .map(Value::Datetime)
    .map(EvalValue::Present)
}

fn duration_nanos(value: DurationValue) -> u128 {
    u128::from(value.seconds()) * 1_000_000_000 + u128::from(value.nanoseconds())
}

fn duration_from_nanos(value: u128) -> Result<DurationValue> {
    DurationValue::new(
        u64::try_from(value / 1_000_000_000)
            .map_err(|_| FastDbError::Schema("duration arithmetic overflow".into()))?,
        u32::try_from(value % 1_000_000_000).expect("nanosecond remainder fits u32"),
    )
}

fn equal_values(left: &EvalValue, right: &EvalValue) -> bool {
    compare_values(left, right) == Ordering::Equal
}

pub(crate) fn compare_values(left: &EvalValue, right: &EvalValue) -> Ordering {
    match (left, right) {
        (EvalValue::Missing, EvalValue::Missing) => Ordering::Equal,
        (EvalValue::Missing, EvalValue::Present(_)) => Ordering::Less,
        (EvalValue::Present(_), EvalValue::Missing) => Ordering::Greater,
        (EvalValue::Present(left), EvalValue::Present(right)) => canonical_value_cmp(left, right),
    }
}
