//! Stable FastDB value model and format-1 JSON envelope codec.

use crate::error::{FastDbError, Result};
use crate::names::{decode_rid, encode_rid};
use serde_json::{Map, Number};
use std::collections::BTreeMap;

const TAG_KEY: &str = "$fastdb";
const TAG_VERSION: i64 = 1;

/// The typed component of a FastDB record ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RecordIdValue {
    String(String),
    Integer(i64),
    Uuid(uuid::Uuid),
}

impl RecordIdValue {
    /// Render a source-addressable component.
    pub fn to_source(&self) -> String {
        match self {
            Self::String(value) => format!("`{}`", value.replace('`', "``")),
            Self::Integer(value) => value.to_string(),
            Self::Uuid(value) => format!("u'{}'", value.hyphenated()),
        }
    }
}

impl From<String> for RecordIdValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for RecordIdValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<&String> for RecordIdValue {
    fn from(value: &String) -> Self {
        Self::String(value.clone())
    }
}

impl From<i64> for RecordIdValue {
    fn from(value: i64) -> Self {
        Self::Integer(value)
    }
}

impl From<uuid::Uuid> for RecordIdValue {
    fn from(value: uuid::Uuid) -> Self {
        Self::Uuid(value)
    }
}

impl From<&RecordIdValue> for RecordIdValue {
    fn from(value: &RecordIdValue) -> Self {
        value.clone()
    }
}

impl PartialEq<str> for RecordIdValue {
    fn eq(&self, other: &str) -> bool {
        matches!(self, Self::String(value) if value == other)
    }
}

impl PartialEq<&str> for RecordIdValue {
    fn eq(&self, other: &&str) -> bool {
        self == *other
    }
}

/// A logical record ID. The table name is catalog-resolved; the component is
/// encoded independently in the physical `rid` column.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RecordId {
    pub table: String,
    pub id: RecordIdValue,
}

impl RecordId {
    pub fn new(table: impl Into<String>, id: impl Into<RecordIdValue>) -> Self {
        Self {
            table: table.into(),
            id: id.into(),
        }
    }
}

impl std::fmt::Display for RecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.table, self.id.to_source())
    }
}

/// A decoded FastDB value. Object keys are always kept in lexicographic order.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    Str(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
    RecordId(RecordId),
}

impl From<bool> for Value {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

macro_rules! signed_value_from {
    ($($ty:ty),+ $(,)?) => {
        $(impl From<$ty> for Value {
            fn from(value: $ty) -> Self {
                Self::Integer(i64::from(value))
            }
        })+
    };
}

signed_value_from!(i8, i16, i32, i64, u8, u16, u32);

impl From<f32> for Value {
    fn from(value: f32) -> Self {
        Self::Float(f64::from(value))
    }
}

impl From<f64> for Value {
    fn from(value: f64) -> Self {
        Self::Float(value)
    }
}

impl From<String> for Value {
    fn from(value: String) -> Self {
        Self::Str(value)
    }
}

impl From<&str> for Value {
    fn from(value: &str) -> Self {
        Self::Str(value.to_owned())
    }
}

impl From<RecordId> for Value {
    fn from(value: RecordId) -> Self {
        Self::RecordId(value)
    }
}

impl From<Vec<Value>> for Value {
    fn from(value: Vec<Value>) -> Self {
        Self::Array(value)
    }
}

impl From<BTreeMap<String, Value>> for Value {
    fn from(value: BTreeMap<String, Value>) -> Self {
        Self::Object(value)
    }
}

impl<T> From<Option<T>> for Value
where
    T: Into<Value>,
{
    fn from(value: Option<T>) -> Self {
        value.map(Into::into).unwrap_or(Self::Null)
    }
}

impl TryFrom<u64> for Value {
    type Error = std::num::TryFromIntError;

    fn try_from(value: u64) -> std::result::Result<Self, Self::Error> {
        i64::try_from(value).map(Self::Integer)
    }
}

impl Value {
    pub const fn is_indexable_scalar(&self) -> bool {
        matches!(
            self,
            Self::Null | Self::Bool(_) | Self::Integer(_) | Self::Float(_) | Self::Str(_)
        )
    }
}

/// A decoded record. Top-level fields are sorted lexicographically and never
/// include the synthesized typed `id`, which is represented separately.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub id: RecordId,
    pub fields: Vec<(String, Value)>,
}

impl Record {
    pub fn new(id: RecordId) -> Self {
        Self {
            id,
            fields: Vec::new(),
        }
    }

    pub fn with_field(mut self, name: impl Into<String>, value: Value) -> Self {
        self.fields.push((name.into(), value));
        self.fields.sort_by(|left, right| left.0.cmp(&right.0));
        self
    }

    pub fn object(&self) -> BTreeMap<String, Value> {
        self.fields.iter().cloned().collect()
    }
}

/// Encode one complete user document into canonical JSON text suitable for
/// binding through `jsonb(?)`.
pub fn encode_doc(fields: &BTreeMap<String, Value>) -> Result<String> {
    if fields.contains_key("id") {
        return Err(FastDbError::Schema(
            "top-level field `id` is reserved and synthesized from the record ID".into(),
        ));
    }
    let json = encode_value(&Value::Object(fields.clone()))?;
    serde_json::to_string(&json)
        .map_err(|error| FastDbError::Engine(format!("failed to encode document: {error}")))
}

/// Encode a value for a JSON/JSONB bind. Record IDs use the versioned tag and
/// user objects containing the reserved key are escaped.
pub fn encode_value(value: &Value) -> Result<serde_json::Value> {
    match value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(value) => Ok(serde_json::Value::Bool(*value)),
        Value::Integer(value) => Ok(serde_json::Value::Number((*value).into())),
        Value::Float(value) => {
            let number = Number::from_f64(*value)
                .ok_or_else(|| FastDbError::Schema("non-finite floats cannot be stored".into()))?;
            Ok(serde_json::Value::Number(number))
        }
        Value::Str(value) => Ok(serde_json::Value::String(value.clone())),
        Value::Array(values) => values
            .iter()
            .map(encode_value)
            .collect::<Result<Vec<_>>>()
            .map(serde_json::Value::Array),
        Value::Object(values) => encode_object(values),
        Value::RecordId(record) => {
            let mut tag = Map::new();
            tag.insert("v".into(), TAG_VERSION.into());
            tag.insert("t".into(), "rid".into());
            tag.insert("table".into(), record.table.clone().into());
            tag.insert("id".into(), encode_rid(&record.id).into());
            Ok(tag_envelope(tag))
        }
    }
}

fn encode_object(values: &BTreeMap<String, Value>) -> Result<serde_json::Value> {
    let encoded = values
        .iter()
        .map(|(key, value)| Ok((key.clone(), encode_value(value)?)))
        .collect::<Result<Map<String, serde_json::Value>>>()?;
    if !values.contains_key(TAG_KEY) {
        return Ok(serde_json::Value::Object(encoded));
    }

    let mut tag = Map::new();
    tag.insert("v".into(), TAG_VERSION.into());
    tag.insert("t".into(), "object".into());
    tag.insert("value".into(), serde_json::Value::Object(encoded));
    Ok(tag_envelope(tag))
}

fn tag_envelope(tag: Map<String, serde_json::Value>) -> serde_json::Value {
    let mut envelope = Map::new();
    envelope.insert(TAG_KEY.into(), serde_json::Value::Object(tag));
    serde_json::Value::Object(envelope)
}

/// Decode a complete stored document. Invalid JSON, tags, number ranges, or
/// a stored top-level `id` are format corruption.
pub fn parse_doc(json: &str) -> Result<Vec<(String, Value)>> {
    let stored: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| FastDbError::format(format!("stored doc is not valid JSON: {error}")))?;
    let value = decode_value(stored)?;
    let Value::Object(fields) = value else {
        return Err(FastDbError::format("stored doc is not a FastDB object"));
    };
    if fields.contains_key("id") {
        return Err(FastDbError::format(
            "stored doc illegally contains the synthesized `id` field",
        ));
    }
    Ok(fields.into_iter().collect())
}

pub fn decode_value(value: serde_json::Value) -> Result<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(value) => Ok(Value::Bool(value)),
        serde_json::Value::Number(value) => {
            if let Some(integer) = value.as_i64() {
                Ok(Value::Integer(integer))
            } else if let Some(float) = value.as_f64().filter(|value| value.is_finite()) {
                Ok(Value::Float(float))
            } else {
                Err(FastDbError::format(
                    "stored JSON number is outside the supported finite i64/f64 range",
                ))
            }
        }
        serde_json::Value::String(value) => Ok(Value::Str(value)),
        serde_json::Value::Array(values) => values
            .into_iter()
            .map(decode_value)
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        serde_json::Value::Object(values) => decode_object(values),
    }
}

fn decode_object(mut values: Map<String, serde_json::Value>) -> Result<Value> {
    if !values.contains_key(TAG_KEY) {
        return decode_plain_object(values);
    }
    if values.len() != 1 {
        return Err(FastDbError::format(
            "stored object uses the reserved `$fastdb` key without an envelope",
        ));
    }
    let tag = match values.remove(TAG_KEY) {
        Some(serde_json::Value::Object(tag)) => tag,
        _ => return Err(FastDbError::format("stored FastDB tag is not an object")),
    };
    decode_tag(tag)
}

fn decode_plain_object(values: Map<String, serde_json::Value>) -> Result<Value> {
    values
        .into_iter()
        .map(|(key, value)| Ok((key, decode_value(value)?)))
        .collect::<Result<BTreeMap<_, _>>>()
        .map(Value::Object)
}

fn decode_tag(mut tag: Map<String, serde_json::Value>) -> Result<Value> {
    let version = tag
        .remove("v")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| FastDbError::format("stored FastDB tag has no integer version"))?;
    if version != TAG_VERSION {
        return Err(FastDbError::format(format!(
            "unknown stored FastDB tag version {version}"
        )));
    }
    let kind = tag
        .remove("t")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| FastDbError::format("stored FastDB tag has no string type"))?;
    match kind.as_str() {
        "rid" => {
            let table = take_tag_string(&mut tag, "table")?;
            if table.is_empty() || table.starts_with("__fastdb_") {
                return Err(FastDbError::format(
                    "stored record tag has an invalid logical table",
                ));
            }
            let encoded_id = take_tag_string(&mut tag, "id")?;
            if !tag.is_empty() {
                return Err(FastDbError::format(
                    "stored record tag contains unknown members",
                ));
            }
            Ok(Value::RecordId(RecordId::new(
                table,
                decode_rid(&encoded_id)?,
            )))
        }
        "object" => {
            let value = match tag.remove("value") {
                Some(serde_json::Value::Object(value)) => value,
                _ => return Err(FastDbError::format("stored object tag has no object value")),
            };
            if !tag.is_empty() {
                return Err(FastDbError::format(
                    "stored object tag contains unknown members",
                ));
            }
            decode_plain_object(value)
        }
        _ => Err(FastDbError::format(format!(
            "unknown stored FastDB tag type {kind:?}"
        ))),
    }
}

fn take_tag_string(tag: &mut Map<String, serde_json::Value>, key: &'static str) -> Result<String> {
    tag.remove(key)
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| FastDbError::format(format!("stored FastDB tag has no string {key}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCategory;

    #[test]
    fn p2_codec_001_values_tags_collisions_and_order_round_trip() {
        let rid = RecordId::new("person", "tracy");
        let mut collision = BTreeMap::new();
        collision.insert(TAG_KEY.into(), Value::Str("user".into()));
        let mut doc = BTreeMap::new();
        doc.insert("z".into(), Value::Null);
        doc.insert("rid".into(), Value::RecordId(rid));
        doc.insert("object".into(), Value::Object(collision));
        doc.insert(
            "array".into(),
            Value::Array(vec![
                Value::Bool(true),
                Value::Integer(i64::MIN),
                Value::Float(1.5),
            ]),
        );

        let encoded = encode_doc(&doc).unwrap();
        let decoded: BTreeMap<_, _> = parse_doc(&encoded).unwrap().into_iter().collect();
        assert_eq!(decoded, doc);
        assert_eq!(
            decoded.keys().cloned().collect::<Vec<_>>(),
            vec!["array", "object", "rid", "z"]
        );
    }

    #[test]
    fn p2_codec_002_malformed_and_unknown_tags_are_format_errors() {
        for json in [
            r#"{"$fastdb":1}"#,
            r#"{"$fastdb":{"v":2,"t":"rid","table":"person","id":"v1:s:1:a"}}"#,
            r#"{"$fastdb":{"v":1,"t":"future"}}"#,
            r#"{"$fastdb":{"v":1,"t":"rid","table":"person","id":"bad"}}"#,
            r#"{"$fastdb":{"v":1,"t":"object","value":1}}"#,
        ] {
            assert_eq!(
                parse_doc(json).unwrap_err().category(),
                ErrorCategory::Format
            );
        }
    }
}
