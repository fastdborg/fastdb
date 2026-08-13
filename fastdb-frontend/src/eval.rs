//! Authoritative FastDB expression evaluation over decoded documents.

use crate::decode::{
    canonical_value_cmp, DatetimeValue, DecimalValue, DurationValue, FileValue, RangeBound,
    RangeValue, RecordId, RecordIdValue, RegexValue, SetValue, TableValue, Value,
};
use crate::error::{FastDbError, Result};
use crate::Params;
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
        | ExprKind::String(_)
        | ExprKind::Parameter(_)
        | ExprKind::RecordId(_)
        | ExprKind::FieldPath(_) => Ok(()),
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
        ExprKind::Access { target, accessor } => {
            reject_all_functions(target)?;
            match accessor {
                Accessor::Field(_) | Accessor::Last(_) => Ok(()),
                Accessor::Index(index) => reject_all_functions(index),
                Accessor::Slice { start, end, .. } => {
                    if let Some(start) = start {
                        reject_all_functions(start)?;
                    }
                    if let Some(end) = end {
                        reject_all_functions(end)?;
                    }
                    Ok(())
                }
            }
        }
        ExprKind::Cast { value, .. } => reject_all_functions(value),
        ExprKind::Range(range) => {
            if let Some(start) = &range.start {
                reject_all_functions(start)?;
            }
            if let Some(end) = &range.end {
                reject_all_functions(end)?;
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
        ExprKind::None => Ok(EvalValue::Present(Value::None)),
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
        SchemaTypeKind::Bool => Ok(Value::Bool(EvalValue::Present(value).truthy())),
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
                && value.trunc() >= i64::MIN as f64
                && value.trunc() < 9_223_372_036_854_775_808.0 =>
        {
            Ok(value.trunc() as i64)
        }
        Value::Decimal(value) => value
            .as_decimal()
            .trunc()
            .to_i64()
            .ok_or_else(|| FastDbError::Schema("decimal is outside the integer range".into())),
        Value::Bool(value) => Ok(i64::from(value)),
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
        Value::Bool(value) => f64::from(u8::from(value)),
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
        Value::Bool(value) => DecimalValue::parse(if value { "1" } else { "0" }),
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
    if length < 0 || length > 65_536 {
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
        Value::RecordId(value) => value.to_string(),
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
