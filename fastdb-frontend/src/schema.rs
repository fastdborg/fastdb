//! Canonical schema type representation and document validation.

use crate::decode::Value;
use crate::error::{FastDbError, Result};
use crate::path::get_path_mut;
use std::collections::BTreeMap;
use turso_fastdb_parser::{SchemaType, SchemaTypeKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    Any,
    Bool,
    Int,
    Float,
    Number,
    Decimal,
    String,
    Bytes,
    Datetime,
    Duration,
    Uuid,
    Regex,
    File,
    Table,
    Object,
    Array,
    TypedArray {
        element: Box<FieldType>,
        length: Option<u32>,
    },
    Vector {
        dimension: u32,
    },
    Set {
        element: Option<Box<FieldType>>,
        length: Option<u32>,
    },
    Range,
    Record,
    Option(Box<FieldType>),
}

impl FieldType {
    pub fn from_parser(value: &SchemaType) -> Self {
        match &value.kind {
            SchemaTypeKind::Any => Self::Any,
            SchemaTypeKind::Bool => Self::Bool,
            SchemaTypeKind::Int => Self::Int,
            SchemaTypeKind::Float => Self::Float,
            SchemaTypeKind::Number => Self::Number,
            SchemaTypeKind::Decimal => Self::Decimal,
            SchemaTypeKind::String => Self::String,
            SchemaTypeKind::Bytes => Self::Bytes,
            SchemaTypeKind::Datetime => Self::Datetime,
            SchemaTypeKind::Duration => Self::Duration,
            SchemaTypeKind::Uuid => Self::Uuid,
            SchemaTypeKind::Regex => Self::Regex,
            SchemaTypeKind::File => Self::File,
            SchemaTypeKind::Table => Self::Table,
            SchemaTypeKind::Object => Self::Object,
            SchemaTypeKind::Array => Self::Array,
            SchemaTypeKind::TypedArray { element, length } => Self::TypedArray {
                element: Box::new(Self::from_parser(element)),
                length: length.as_ref().map(|length| length.value as u32),
            },
            SchemaTypeKind::FixedFloatArray(dimension) => Self::Vector {
                dimension: dimension.value as u32,
            },
            SchemaTypeKind::Set { element, length } => Self::Set {
                element: element
                    .as_ref()
                    .map(|element| Box::new(Self::from_parser(element))),
                length: length.as_ref().map(|length| length.value as u32),
            },
            SchemaTypeKind::Range => Self::Range,
            SchemaTypeKind::Record => Self::Record,
            SchemaTypeKind::Option(inner) => Self::Option(Box::new(Self::from_parser(inner))),
        }
    }

    pub fn parse_canonical(value: &str) -> Result<Self> {
        let (parsed, consumed) = parse_type(value)?;
        if consumed != value.len() || parsed.canonical() != value {
            return Err(FastDbError::format(
                "stored field type AST is not canonical",
            ));
        }
        Ok(parsed)
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::Any => "any".into(),
            Self::Bool => "bool".into(),
            Self::Int => "int".into(),
            Self::Float => "float".into(),
            Self::Number => "number".into(),
            Self::Decimal => "decimal".into(),
            Self::String => "string".into(),
            Self::Bytes => "bytes".into(),
            Self::Datetime => "datetime".into(),
            Self::Duration => "duration".into(),
            Self::Uuid => "uuid".into(),
            Self::Regex => "regex".into(),
            Self::File => "file".into(),
            Self::Table => "table".into(),
            Self::Object => "object".into(),
            Self::Array => "array".into(),
            Self::TypedArray { element, length } => match length {
                Some(length) => format!("array<{},{}>", element.canonical(), length),
                None => format!("array<{}>", element.canonical()),
            },
            Self::Vector { dimension } => format!("array<float,{dimension}>"),
            Self::Set { element, length } => match (element, length) {
                (None, None) => "set".into(),
                (Some(element), None) => format!("set<{}>", element.canonical()),
                (Some(element), Some(length)) => {
                    format!("set<{},{}>", element.canonical(), length)
                }
                (None, Some(_)) => unreachable!("set length requires an element type"),
            },
            Self::Range => "range".into(),
            Self::Record => "record".into(),
            Self::Option(inner) => format!("option<{}>", inner.canonical()),
        }
    }

    pub const fn required(&self) -> bool {
        !matches!(self, Self::Any | Self::Option(_))
    }

    pub const fn base_is_object(&self) -> bool {
        match self {
            Self::Any | Self::Object => true,
            Self::Option(inner) => inner.base_is_object(),
            _ => false,
        }
    }

    pub const fn vector_dimension(&self) -> Option<u32> {
        match self {
            Self::Vector { dimension } => Some(*dimension),
            Self::Option(inner) => inner.vector_dimension(),
            _ => None,
        }
    }

    pub const fn supports_reference(&self) -> bool {
        match self {
            Self::Record => true,
            Self::Option(inner) => inner.supports_reference(),
            Self::TypedArray { element, .. } => element.supports_reference(),
            Self::Set {
                element: Some(element),
                ..
            } => element.supports_reference(),
            _ => false,
        }
    }

    pub const fn base_is_any(&self) -> bool {
        match self {
            Self::Any => true,
            Self::Option(inner) => inner.base_is_any(),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SchemaExpression {
    pub expression: turso_fastdb_parser::Expr,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FieldRule {
    pub path: Vec<String>,
    pub path_key: String,
    pub ty: FieldType,
    pub required: bool,
    pub definition: String,
    pub default: Option<SchemaExpression>,
    pub default_always: bool,
    pub value: Option<SchemaExpression>,
    pub assert: Option<SchemaExpression>,
    pub readonly: bool,
    pub reference: bool,
    pub permissions: turso_fastdb_parser::SchemaPermissions,
    pub comment: Option<String>,
}

pub fn validate_field_relationships<'a>(
    existing: impl IntoIterator<Item = &'a FieldRule>,
    candidate: &FieldRule,
) -> Result<()> {
    for field in existing {
        if is_prefix(&field.path, &candidate.path)
            && field.path != candidate.path
            && !field.ty.base_is_object()
        {
            return Err(FastDbError::Schema(format!(
                "field {} cannot contain declared descendants because it is not object",
                field.path_key
            )));
        }
        if is_prefix(&candidate.path, &field.path)
            && field.path != candidate.path
            && !candidate.ty.base_is_object()
        {
            return Err(FastDbError::Schema(format!(
                "field {} must be object because it has declared descendants",
                candidate.path_key
            )));
        }
    }
    Ok(())
}

/// Validate and normalize a document. Returns whether integer-to-float
/// normalization changed it.
pub fn validate_document(
    schemafull: bool,
    fields: &BTreeMap<String, FieldRule>,
    document: &mut BTreeMap<String, Value>,
) -> Result<bool> {
    let mut changed = false;
    for field in fields.values() {
        match get_path_mut(document, &field.path) {
            Some(value) => changed |= validate_type(&field.ty, value, &field.path_key)?,
            None if field.required => {
                return Err(FastDbError::Schema(format!(
                    "required field {} is missing",
                    field.path_key
                )))
            }
            None => {}
        }
    }
    if schemafull {
        validate_declared_paths(document, fields, &mut Vec::new())?;
    }
    Ok(changed)
}

fn validate_type(ty: &FieldType, value: &mut Value, path: &str) -> Result<bool> {
    if matches!(ty, FieldType::Any) {
        return Ok(false);
    }
    if matches!(value, Value::Null) {
        return Err(FastDbError::Schema(format!(
            "declared field {path} cannot be null"
        )));
    }
    if let FieldType::Option(inner) = ty {
        if matches!(value, Value::None) {
            return Ok(false);
        }
        return validate_type(inner, value, path);
    }
    if let FieldType::TypedArray { element, length } = ty {
        let Value::Array(values) = value else {
            return type_mismatch(ty, path);
        };
        if length.is_some_and(|length| values.len() != length as usize) {
            return Err(FastDbError::Schema(format!(
                "field {path} requires exactly {} array elements",
                length.expect("checked length presence")
            )));
        }
        let mut changed = false;
        for value in values {
            changed |= validate_type(element, value, path)?;
        }
        return Ok(changed);
    }
    if let FieldType::Set { element, length } = ty {
        let Value::Set(set) = value else {
            return type_mismatch(ty, path);
        };
        if length.is_some_and(|length| set.as_slice().len() != length as usize) {
            return Err(FastDbError::Schema(format!(
                "field {path} requires exactly {} set elements",
                length.expect("checked length presence")
            )));
        }
        let Some(element) = element else {
            return Ok(false);
        };
        let mut values = set.clone().into_vec();
        let mut changed = false;
        for value in &mut values {
            changed |= validate_type(element, value, path)?;
        }
        if changed {
            *value = Value::Set(crate::decode::SetValue::new(values)?);
        }
        return Ok(changed);
    }
    if let FieldType::Vector { dimension } = ty {
        let Value::Array(values) = value else {
            return Err(FastDbError::Schema(format!(
                "field {path} does not match type {}",
                ty.canonical()
            )));
        };
        if values.len() != *dimension as usize {
            return Err(FastDbError::Schema(format!(
                "field {path} requires exactly {dimension} vector dimensions"
            )));
        }
        let mut changed = false;
        for element in values {
            match element {
                Value::Integer(integer) => {
                    *element = Value::Float(*integer as f64);
                    changed = true;
                }
                Value::Float(value) if value.is_finite() => {}
                Value::Float(_) => {
                    return Err(FastDbError::Schema(format!(
                        "field {path} vector elements must be finite"
                    )))
                }
                _ => {
                    return Err(FastDbError::Schema(format!(
                        "field {path} vector elements must be numeric"
                    )))
                }
            }
        }
        return Ok(changed);
    }
    match (ty, value) {
        (FieldType::Bool, Value::Bool(_))
        | (FieldType::Int, Value::Integer(_))
        | (FieldType::Number, Value::Integer(_) | Value::Float(_) | Value::Decimal(_))
        | (FieldType::Decimal, Value::Decimal(_))
        | (FieldType::String, Value::Str(_))
        | (FieldType::Bytes, Value::Bytes(_))
        | (FieldType::Datetime, Value::Datetime(_))
        | (FieldType::Duration, Value::Duration(_))
        | (FieldType::Uuid, Value::Uuid(_))
        | (FieldType::Regex, Value::Regex(_))
        | (FieldType::File, Value::File(_))
        | (FieldType::Table, Value::Table(_))
        | (FieldType::Object, Value::Object(_))
        | (FieldType::Array, Value::Array(_))
        | (FieldType::Range, Value::Range(_))
        | (FieldType::Record, Value::RecordId(_))
        | (FieldType::Float, Value::Float(_)) => Ok(false),
        (FieldType::Float, value @ Value::Integer(_)) => {
            let Value::Integer(integer) = value else {
                unreachable!();
            };
            *value = Value::Float(*integer as f64);
            Ok(true)
        }
        _ => type_mismatch(ty, path),
    }
}

pub(crate) fn validate_standalone_value(
    ty: &FieldType,
    value: &mut Value,
    label: &str,
) -> Result<()> {
    validate_type(ty, value, label).map(|_| ())
}

fn type_mismatch(ty: &FieldType, path: &str) -> Result<bool> {
    Err(FastDbError::Schema(format!(
        "field {path} does not match type {}",
        ty.canonical()
    )))
}

fn validate_declared_paths(
    object: &BTreeMap<String, Value>,
    fields: &BTreeMap<String, FieldRule>,
    prefix: &mut Vec<String>,
) -> Result<()> {
    for (key, value) in object {
        prefix.push(key.clone());
        let allowed = fields.values().any(|field| {
            prefix == &field.path
                || is_prefix(prefix, &field.path)
                || (field.ty.base_is_any() && is_prefix(&field.path, prefix))
        });
        if !allowed {
            let path = crate::path::canonical_path(prefix.iter().map(String::as_str))?;
            return Err(FastDbError::Schema(format!(
                "undeclared field {path} is not allowed in a schemafull table"
            )));
        }
        if let Value::Object(child) = value {
            validate_declared_paths(child, fields, prefix)?;
        }
        prefix.pop();
    }
    Ok(())
}

fn is_prefix(left: &[String], right: &[String]) -> bool {
    left.len() <= right.len() && left.iter().zip(right).all(|(left, right)| left == right)
}

fn parse_type(value: &str) -> Result<(FieldType, usize)> {
    if let Some(rest) = value.strip_prefix("array<float,") {
        let Some(close) = rest.find('>') else {
            return Err(FastDbError::format(
                "stored fixed vector type is missing its closing delimiter",
            ));
        };
        let dimension = rest[..close]
            .parse::<u32>()
            .map_err(|_| FastDbError::format("stored fixed vector dimension is invalid"))?;
        if dimension == 0 || dimension > 65_536 {
            return Err(FastDbError::format(
                "stored fixed vector dimension is outside the supported range",
            ));
        }
        return Ok((
            FieldType::Vector { dimension },
            "array<float,".len() + close + 1,
        ));
    }
    if let Some(rest) = value.strip_prefix("array<") {
        let (element, consumed) = parse_type(rest)?;
        let suffix = &rest[consumed..];
        let (length, suffix_consumed) = parse_collection_suffix(suffix)?;
        return Ok((
            FieldType::TypedArray {
                element: Box::new(element),
                length,
            },
            "array<".len() + consumed + suffix_consumed,
        ));
    }
    if let Some(rest) = value.strip_prefix("set<") {
        let (element, consumed) = parse_type(rest)?;
        let suffix = &rest[consumed..];
        let (length, suffix_consumed) = parse_collection_suffix(suffix)?;
        return Ok((
            FieldType::Set {
                element: Some(Box::new(element)),
                length,
            },
            "set<".len() + consumed + suffix_consumed,
        ));
    }
    for (name, ty) in [
        ("any", FieldType::Any),
        ("bool", FieldType::Bool),
        ("int", FieldType::Int),
        ("float", FieldType::Float),
        ("number", FieldType::Number),
        ("decimal", FieldType::Decimal),
        ("string", FieldType::String),
        ("bytes", FieldType::Bytes),
        ("datetime", FieldType::Datetime),
        ("duration", FieldType::Duration),
        ("uuid", FieldType::Uuid),
        ("regex", FieldType::Regex),
        ("file", FieldType::File),
        ("table", FieldType::Table),
        ("object", FieldType::Object),
        ("array", FieldType::Array),
        (
            "set",
            FieldType::Set {
                element: None,
                length: None,
            },
        ),
        ("range", FieldType::Range),
        ("record", FieldType::Record),
    ] {
        if value.starts_with(name) {
            return Ok((ty, name.len()));
        }
    }
    let Some(rest) = value.strip_prefix("option<") else {
        return Err(FastDbError::format("stored field type AST is unknown"));
    };
    let (inner, consumed) = parse_type(rest)?;
    if rest.as_bytes().get(consumed) != Some(&b'>') {
        return Err(FastDbError::format(
            "stored option type AST is missing its closing delimiter",
        ));
    }
    Ok((
        FieldType::Option(Box::new(inner)),
        "option<".len() + consumed + 1,
    ))
}

fn parse_collection_suffix(value: &str) -> Result<(Option<u32>, usize)> {
    if value.starts_with('>') {
        return Ok((None, 1));
    }
    let Some(rest) = value.strip_prefix(',') else {
        return Err(FastDbError::format(
            "stored typed collection is missing its closing delimiter",
        ));
    };
    let Some(close) = rest.find('>') else {
        return Err(FastDbError::format(
            "stored typed collection is missing its closing delimiter",
        ));
    };
    let length = rest[..close]
        .parse::<u32>()
        .map_err(|_| FastDbError::format("stored typed collection length is invalid"))?;
    if length == 0 || length > 65_536 {
        return Err(FastDbError::format(
            "stored typed collection length is outside the supported range",
        ));
    }
    Ok((Some(length), 1 + close + 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::set_path;

    fn rule(path: &[&str], ty: FieldType) -> FieldRule {
        let path = path
            .iter()
            .map(|value| (*value).to_string())
            .collect::<Vec<_>>();
        FieldRule {
            path_key: crate::path::canonical_path(&path).unwrap(),
            path,
            required: ty.required(),
            ty,
            definition: "test".into(),
            default: None,
            default_always: false,
            value: None,
            assert: None,
            readonly: false,
            reference: false,
            permissions: turso_fastdb_parser::SchemaPermissions::Full,
            comment: None,
        }
    }

    #[test]
    fn p2_schema_001_required_optional_null_and_numeric_normalization() {
        let rules = [
            rule(&["age"], FieldType::Float),
            rule(
                &["nickname"],
                FieldType::Option(Box::new(FieldType::String)),
            ),
        ]
        .into_iter()
        .map(|rule| (rule.path_key.clone(), rule))
        .collect();
        let mut document = BTreeMap::new();
        document.insert("age".into(), Value::Integer(42));
        assert!(validate_document(false, &rules, &mut document).unwrap());
        assert_eq!(document.get("age"), Some(&Value::Float(42.0)));
        document.insert("nickname".into(), Value::Null);
        assert!(validate_document(false, &rules, &mut document).is_err());
    }

    #[test]
    fn p2_schema_002_schemafull_descendants_authorize_ancestors_not_siblings() {
        let child = rule(&["profile", "age"], FieldType::Int);
        let rules = [(child.path_key.clone(), child)].into_iter().collect();
        let mut document = BTreeMap::new();
        set_path(
            &mut document,
            &["profile".into(), "age".into()],
            Value::Integer(42),
        )
        .unwrap();
        validate_document(true, &rules, &mut document).unwrap();
        set_path(
            &mut document,
            &["profile".into(), "name".into()],
            Value::Str("Tracy".into()),
        )
        .unwrap();
        assert!(validate_document(true, &rules, &mut document).is_err());
    }
}
