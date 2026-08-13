//! Collision-safe strict JSON encoding used by the public API and CLI.

use crate::{Error, QueryResponse, RecordId, RecordIdValue, StatementResult, Value};
use serde_json::{Map, Number};
use std::collections::BTreeMap;

const KEY: &str = "$fastdb";
const VERSION: i64 = 1;

pub fn value_to_json(value: &Value) -> Result<serde_json::Value, Error> {
    match value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(value) => Ok((*value).into()),
        Value::Integer(value) => Ok((*value).into()),
        Value::Float(value) => Number::from_f64(*value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| Error::new(crate::ErrorCategory::Schema, "non-finite JSON number")),
        Value::Str(value) => Ok(value.clone().into()),
        Value::Array(values) => values
            .iter()
            .map(value_to_json)
            .collect::<Result<Vec<_>, _>>()
            .map(serde_json::Value::Array),
        Value::Object(values) => object_to_json(values),
        Value::RecordId(record) => Ok(record_envelope(record)),
    }
}

pub fn value_from_json(value: serde_json::Value) -> Result<Value, Error> {
    match value {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(value) => Ok(Value::Bool(value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Value::Integer(value))
            } else if let Some(value) = value.as_f64().filter(|value| value.is_finite()) {
                Ok(Value::Float(value))
            } else {
                Err(Error::new(
                    crate::ErrorCategory::Schema,
                    "JSON number is outside FastDB's finite i64/f64 range",
                ))
            }
        }
        serde_json::Value::String(value) => Ok(Value::Str(value)),
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(value_from_json)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        serde_json::Value::Object(values) => json_object_to_value(values),
    }
}

pub fn response_to_json(response: &QueryResponse) -> Result<serde_json::Value, Error> {
    let statements = response
        .statements
        .iter()
        .map(|statement| {
            let mut object = Map::new();
            match statement {
                StatementResult::None => {
                    object.insert("kind".into(), "none".into());
                }
                StatementResult::Rows(rows) => {
                    object.insert("kind".into(), "rows".into());
                    object.insert(
                        "value".into(),
                        serde_json::Value::Array(rows.iter().map(value_to_json).collect::<Result<
                            Vec<_>,
                            _,
                        >>(
                        )?),
                    );
                }
                StatementResult::Value(value) => {
                    object.insert("kind".into(), "value".into());
                    object.insert("value".into(), value_to_json(value)?);
                }
            }
            Ok(serde_json::Value::Object(object))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(envelope(
        "response",
        [
            ("statements", serde_json::Value::Array(statements)),
            (
                "mutation_count",
                serde_json::Value::Number(response.mutation_count.into()),
            ),
        ],
    ))
}

pub fn error_to_json(error: &Error) -> serde_json::Value {
    let mut fields = vec![
        ("category", error.category().as_str().into()),
        ("message", error.to_string().into()),
    ];
    if let Some(span) = error.span() {
        fields.push((
            "span",
            serde_json::json!({"offset": span.offset, "len": span.len}),
        ));
    }
    envelope("error", fields)
}

fn object_to_json(values: &BTreeMap<String, Value>) -> Result<serde_json::Value, Error> {
    let encoded = values
        .iter()
        .map(|(key, value)| Ok((key.clone(), value_to_json(value)?)))
        .collect::<Result<Map<_, _>, Error>>()?;
    if !values.contains_key(KEY) {
        return Ok(serde_json::Value::Object(encoded));
    }
    Ok(envelope(
        "object",
        [("value", serde_json::Value::Object(encoded))],
    ))
}

fn json_object_to_value(values: Map<String, serde_json::Value>) -> Result<Value, Error> {
    if values.len() == 1 && values.contains_key(KEY) {
        if let Some(tag) = values.get(KEY).and_then(|value| value.as_object().cloned()) {
            return decode_envelope(tag);
        }
    }
    values
        .into_iter()
        .map(|(key, value)| Ok((key, value_from_json(value)?)))
        .collect::<Result<BTreeMap<_, _>, Error>>()
        .map(Value::Object)
}

fn decode_envelope(mut tag: Map<String, serde_json::Value>) -> Result<Value, Error> {
    let version = tag.remove("v").and_then(|value| value.as_i64());
    let kind = tag
        .remove("t")
        .and_then(|value| value.as_str().map(str::to_owned));
    if version != Some(VERSION) {
        return Err(Error::new(
            crate::ErrorCategory::Schema,
            "unknown $fastdb JSON envelope version",
        ));
    }
    match kind.as_deref() {
        Some("object") => match tag.remove("value") {
            Some(serde_json::Value::Object(value)) if tag.is_empty() => value
                .into_iter()
                .map(|(key, value)| Ok((key, value_from_json(value)?)))
                .collect::<Result<BTreeMap<_, _>, Error>>()
                .map(Value::Object),
            _ => Err(Error::new(
                crate::ErrorCategory::Schema,
                "invalid escaped object envelope",
            )),
        },
        Some("rid") => decode_record(tag).map(Value::RecordId),
        _ => Err(Error::new(
            crate::ErrorCategory::Schema,
            "JSON envelope is not a FastDB value",
        )),
    }
}

fn record_envelope(record: &RecordId) -> serde_json::Value {
    let (kind, value) = match &record.id {
        RecordIdValue::String(value) => ("string", value.clone().into()),
        RecordIdValue::Integer(value) => ("integer", (*value).into()),
        RecordIdValue::Uuid(value) => ("uuid", value.hyphenated().to_string().into()),
    };
    envelope(
        "rid",
        [
            ("table", record.table.clone().into()),
            ("id_type", kind.into()),
            ("id", value),
        ],
    )
}

fn decode_record(mut tag: Map<String, serde_json::Value>) -> Result<RecordId, Error> {
    let table = tag
        .remove("table")
        .and_then(|value| value.as_str().map(str::to_owned));
    let kind = tag
        .remove("id_type")
        .and_then(|value| value.as_str().map(str::to_owned));
    let id = tag.remove("id");
    if !tag.is_empty() || table.as_deref().is_none_or(str::is_empty) {
        return Err(Error::new(
            crate::ErrorCategory::Schema,
            "invalid record ID JSON envelope",
        ));
    }
    let id = match (kind.as_deref(), id) {
        (Some("string"), Some(serde_json::Value::String(value))) => RecordIdValue::String(value),
        (Some("integer"), Some(serde_json::Value::Number(value))) => value
            .as_i64()
            .map(RecordIdValue::Integer)
            .ok_or_else(|| Error::new(crate::ErrorCategory::Schema, "invalid integer record ID"))?,
        (Some("uuid"), Some(serde_json::Value::String(value))) => {
            let value = uuid::Uuid::parse_str(&value)
                .map_err(|_| Error::new(crate::ErrorCategory::Schema, "invalid UUID record ID"))?;
            if !matches!(value.get_version_num(), 4 | 7) {
                return Err(Error::new(
                    crate::ErrorCategory::Schema,
                    "record-ID UUID must be UUIDv4 or UUIDv7",
                ));
            }
            RecordIdValue::Uuid(value)
        }
        _ => {
            return Err(Error::new(
                crate::ErrorCategory::Schema,
                "record ID JSON envelope has mismatched component type",
            ))
        }
    };
    Ok(RecordId::new(table.expect("table validated above"), id))
}

fn envelope(
    kind: &str,
    fields: impl IntoIterator<Item = (&'static str, serde_json::Value)>,
) -> serde_json::Value {
    let mut tag = Map::new();
    tag.insert("v".into(), VERSION.into());
    tag.insert("t".into(), kind.into());
    for (key, value) in fields {
        tag.insert(key.into(), value);
    }
    let mut outer = Map::new();
    outer.insert(KEY.into(), serde_json::Value::Object(tag));
    serde_json::Value::Object(outer)
}
