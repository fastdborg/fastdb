//! Canonical schema type representation and document validation.

use crate::decode::Value;
use crate::error::{FastDbError, Result};
use crate::path::get_path_mut;
use std::collections::BTreeMap;
use turso_fastdb_parser::{SchemaType, SchemaTypeKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    Union(Vec<FieldType>),
    Literal(FieldTypeLiteral),
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
    Record {
        tables: Vec<String>,
    },
    Option(Box<FieldType>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldTypeLiteral {
    None,
    Null,
    Bool(bool),
    Integer(i64),
    Float(u64),
    String(String),
}

impl FieldType {
    pub fn from_parser(value: &SchemaType) -> Self {
        match &value.kind {
            SchemaTypeKind::Union(variants) => {
                Self::Union(variants.iter().map(Self::from_parser).collect())
            }
            SchemaTypeKind::Literal(literal) => Self::Literal(match literal {
                turso_fastdb_parser::SchemaTypeLiteral::None => FieldTypeLiteral::None,
                turso_fastdb_parser::SchemaTypeLiteral::Null => FieldTypeLiteral::Null,
                turso_fastdb_parser::SchemaTypeLiteral::Bool(value) => {
                    FieldTypeLiteral::Bool(*value)
                }
                turso_fastdb_parser::SchemaTypeLiteral::Integer(value) => {
                    FieldTypeLiteral::Integer(*value)
                }
                turso_fastdb_parser::SchemaTypeLiteral::Float(value) => {
                    FieldTypeLiteral::Float(value.to_bits())
                }
                turso_fastdb_parser::SchemaTypeLiteral::String(value) => {
                    FieldTypeLiteral::String(value.clone())
                }
            }),
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
            SchemaTypeKind::Record { tables } => Self::Record {
                tables: tables.iter().map(|table| table.value.clone()).collect(),
            },
            SchemaTypeKind::Option(inner) => Self::Option(Box::new(Self::from_parser(inner))),
        }
    }

    pub fn parse_canonical(value: &str) -> Result<Self> {
        let source = format!("DEFINE FIELD value ON value TYPE {value}");
        let statement = turso_fastdb_parser::parse_one(&source)
            .map_err(|_| FastDbError::format("stored field type AST is invalid"))?;
        let turso_fastdb_parser::Statement::DefineField(field) = statement else {
            return Err(FastDbError::format("stored field type AST is invalid"));
        };
        let parsed = Self::from_parser(&field.ty);
        if parsed.canonical() != value {
            return Err(FastDbError::format(
                "stored field type AST is not canonical",
            ));
        }
        Ok(parsed)
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::Union(variants) => variants
                .iter()
                .map(Self::canonical)
                .collect::<Vec<_>>()
                .join("|"),
            Self::Literal(literal) => literal.canonical(),
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
            Self::Record { tables } if tables.is_empty() => "record".into(),
            Self::Record { tables } => format!(
                "record<{}>",
                tables
                    .iter()
                    .map(|table| render_type_identifier(table))
                    .collect::<Vec<_>>()
                    .join("|")
            ),
            Self::Option(inner) => format!("option<{}>", inner.canonical()),
        }
    }

    pub fn required(&self) -> bool {
        match self {
            Self::Any | Self::Option(_) | Self::Literal(FieldTypeLiteral::None) => false,
            Self::Union(variants) => {
                let mut index = 0;
                while index < variants.len() {
                    if !variants[index].required() {
                        return false;
                    }
                    index += 1;
                }
                true
            }
            _ => true,
        }
    }

    pub fn base_is_object(&self) -> bool {
        match self {
            Self::Any | Self::Object => true,
            Self::Union(variants) => {
                let mut index = 0;
                while index < variants.len() {
                    if variants[index].base_is_object() {
                        return true;
                    }
                    index += 1;
                }
                false
            }
            Self::Option(inner) => inner.base_is_object(),
            _ => false,
        }
    }

    pub fn vector_dimension(&self) -> Option<u32> {
        match self {
            Self::Vector { dimension } => Some(*dimension),
            Self::Option(inner) => inner.vector_dimension(),
            Self::Union(variants) => {
                let mut dimension = None;
                let mut index = 0;
                while index < variants.len() {
                    if let Some(candidate) = variants[index].vector_dimension() {
                        if dimension.is_some() && dimension != Some(candidate) {
                            return None;
                        }
                        dimension = Some(candidate);
                    } else if variants[index].required() {
                        return None;
                    }
                    index += 1;
                }
                dimension
            }
            _ => None,
        }
    }

    pub fn supports_reference(&self) -> bool {
        match self {
            Self::Record { .. } => true,
            Self::Option(inner) => inner.supports_reference(),
            Self::Union(variants) => {
                let mut found_reference = false;
                for variant in variants {
                    if variant.supports_reference() {
                        found_reference = true;
                    } else if variant.required() {
                        return false;
                    }
                }
                found_reference
            }
            Self::TypedArray { element, .. } => element.supports_reference(),
            Self::Set {
                element: Some(element),
                ..
            } => element.supports_reference(),
            _ => false,
        }
    }

    pub fn references_table(&self, table: &str) -> bool {
        match self {
            Self::Record { tables } => tables.iter().any(|candidate| candidate == table),
            Self::Union(variants) => variants
                .iter()
                .any(|variant| variant.references_table(table)),
            Self::TypedArray { element, .. } | Self::Option(element) => {
                element.references_table(table)
            }
            Self::Set {
                element: Some(element),
                ..
            } => element.references_table(table),
            _ => false,
        }
    }

    pub fn base_is_any(&self) -> bool {
        match self {
            Self::Any => true,
            Self::Option(inner) => inner.base_is_any(),
            Self::Union(variants) => {
                let mut index = 0;
                while index < variants.len() {
                    if variants[index].base_is_any() {
                        return true;
                    }
                    index += 1;
                }
                false
            }
            _ => false,
        }
    }
}

impl FieldTypeLiteral {
    fn canonical(&self) -> String {
        match self {
            Self::None => "none".into(),
            Self::Null => "null".into(),
            Self::Bool(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
            Self::Float(bits) => format!("{:?}", f64::from_bits(*bits)),
            Self::String(value) => {
                format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
            }
        }
    }
}

fn render_type_identifier(value: &str) -> String {
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
    pub flexible: bool,
    pub required: bool,
    pub definition: String,
    pub default: Option<SchemaExpression>,
    pub default_always: bool,
    pub value: Option<SchemaExpression>,
    pub assert: Option<SchemaExpression>,
    pub readonly: bool,
    pub reference: bool,
    pub reference_action: Option<turso_fastdb_parser::ReferenceDeleteAction>,
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
    if let FieldType::Union(variants) = ty {
        for variant in variants {
            let mut candidate = value.clone();
            if let Ok(changed) = validate_type(variant, &mut candidate, path) {
                *value = candidate;
                return Ok(changed);
            }
        }
        return type_mismatch(ty, path);
    }
    if let FieldType::Literal(literal) = ty {
        let matches = match (literal, &*value) {
            (FieldTypeLiteral::None, Value::None) | (FieldTypeLiteral::Null, Value::Null) => true,
            (FieldTypeLiteral::Bool(expected), Value::Bool(actual)) => expected == actual,
            (FieldTypeLiteral::Integer(expected), Value::Integer(actual)) => expected == actual,
            (FieldTypeLiteral::Float(expected), Value::Float(actual)) => {
                *expected == actual.to_bits()
            }
            (FieldTypeLiteral::String(expected), Value::Str(actual)) => expected == actual,
            _ => false,
        };
        return if matches {
            Ok(false)
        } else {
            type_mismatch(ty, path)
        };
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
        | (FieldType::Float, Value::Float(_)) => Ok(false),
        (FieldType::Record { tables }, Value::RecordId(record))
            if tables.is_empty() || tables.iter().any(|table| table == &record.table) =>
        {
            Ok(false)
        }
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
                || ((field.ty.base_is_any() || field.flexible) && is_prefix(&field.path, prefix))
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
            flexible: false,
            definition: "test".into(),
            default: None,
            default_always: false,
            value: None,
            assert: None,
            readonly: false,
            reference: false,
            reference_action: None,
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
