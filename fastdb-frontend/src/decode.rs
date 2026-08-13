//! Stable FastDB value model and format-1 JSON envelope codec.

use crate::error::{FastDbError, Result};
use crate::names::{decode_rid, encode_rid};
use base64::Engine as _;
use chrono::{DateTime, Datelike, SecondsFormat, Utc};
use rust_decimal::prelude::ToPrimitive as _;
use serde_json::{Map, Number};
use std::cmp::Ordering;
use std::collections::BTreeMap;

const TAG_KEY: &str = "$fastdb";
const TAG_VERSION: i64 = 2;
const LEGACY_TAG_VERSION: i64 = 1;
const MAX_VALUE_BYTES: usize = 1 << 20;
const MAX_COLLECTION_ELEMENTS: usize = 1_024;
const MAX_ARRAY_ELEMENTS: usize = 65_536;
const MAX_DOCUMENT_BYTES: usize = 16 << 20;
const MAX_NESTING_DEPTH: usize = 64;

/// A UTC datetime with nanosecond precision in SurrealDB's characterized
/// year `1..=9999` domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DatetimeValue(DateTime<Utc>);

impl DatetimeValue {
    pub fn parse(value: &str) -> Result<Self> {
        let parsed = DateTime::parse_from_rfc3339(value)
            .map_err(|_| FastDbError::Schema("datetime must be valid RFC 3339".into()))?
            .with_timezone(&Utc);
        Self::new(parsed)
    }

    pub fn from_timestamp(seconds: i64, nanoseconds: u32) -> Result<Self> {
        let value = DateTime::from_timestamp(seconds, nanoseconds)
            .ok_or_else(|| FastDbError::Schema("datetime is outside the supported range".into()))?;
        Self::new(value)
    }

    fn new(value: DateTime<Utc>) -> Result<Self> {
        if !(1..=9999).contains(&value.year()) {
            return Err(FastDbError::Schema(
                "datetime year must be in 1..=9999".into(),
            ));
        }
        Ok(Self(value))
    }

    pub fn timestamp(&self) -> i64 {
        self.0.timestamp()
    }

    pub fn nanosecond(&self) -> u32 {
        self.0.timestamp_subsec_nanos()
    }

    pub fn to_canonical(&self) -> String {
        self.0.to_rfc3339_opts(SecondsFormat::AutoSi, true)
    }
}

/// A nonnegative duration with the same range as `std::time::Duration`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DurationValue {
    seconds: u64,
    nanoseconds: u32,
}

impl DurationValue {
    pub fn new(seconds: u64, nanoseconds: u32) -> Result<Self> {
        if nanoseconds >= 1_000_000_000 {
            return Err(FastDbError::Schema(
                "duration nanoseconds must be below one second".into(),
            ));
        }
        Ok(Self {
            seconds,
            nanoseconds,
        })
    }

    pub const fn seconds(self) -> u64 {
        self.seconds
    }

    pub const fn nanoseconds(self) -> u32 {
        self.nanoseconds
    }

    pub fn to_canonical(self) -> String {
        if self.nanoseconds == 0 {
            return format!("{}s", self.seconds);
        }
        let fractional = format!("{:09}", self.nanoseconds)
            .trim_end_matches('0')
            .to_string();
        format!("{}.{fractional}s", self.seconds)
    }

    pub fn parse_canonical(value: &str) -> Result<Self> {
        let value = value
            .strip_suffix('s')
            .ok_or_else(|| FastDbError::format("stored duration has no seconds suffix"))?;
        let (seconds, nanoseconds) = match value.split_once('.') {
            Some((seconds, fractional)) => {
                if fractional.is_empty()
                    || fractional.len() > 9
                    || fractional.ends_with('0')
                    || !fractional.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return Err(FastDbError::format(
                        "stored duration has a noncanonical fraction",
                    ));
                }
                let seconds = parse_canonical_u64(seconds, "duration seconds")?;
                let mut padded = fractional.to_string();
                padded.extend(std::iter::repeat_n('0', 9 - fractional.len()));
                let nanoseconds = padded
                    .parse::<u32>()
                    .map_err(|_| FastDbError::format("stored duration fraction is invalid"))?;
                (seconds, nanoseconds)
            }
            None => (parse_canonical_u64(value, "duration seconds")?, 0),
        };
        let duration = Self::new(seconds, nanoseconds)
            .map_err(|error| FastDbError::format(error.to_string()))?;
        if duration.to_canonical() != format!("{value}s") {
            return Err(FastDbError::format(
                "stored duration is not canonically encoded",
            ));
        }
        Ok(duration)
    }
}

/// A SurrealDB-compatible 96-bit coefficient decimal with scale `0..=28`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DecimalValue(rust_decimal::Decimal);

impl DecimalValue {
    pub fn parse(value: &str) -> Result<Self> {
        if value.contains(['e', 'E']) {
            return Err(FastDbError::Schema(
                "decimal exponent notation is not supported".into(),
            ));
        }
        let decimal = value
            .parse::<rust_decimal::Decimal>()
            .map_err(|_| FastDbError::Schema("decimal is outside the supported range".into()))?;
        Ok(Self(decimal.normalize()))
    }

    fn parse_canonical(value: &str) -> Result<Self> {
        let decimal = Self::parse(value).map_err(|error| FastDbError::format(error.to_string()))?;
        if decimal.to_canonical() != value {
            return Err(FastDbError::format(
                "stored decimal is not canonically encoded",
            ));
        }
        Ok(decimal)
    }

    pub fn to_canonical(self) -> String {
        self.0.normalize().to_string()
    }

    pub const fn as_decimal(self) -> rust_decimal::Decimal {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }
}

/// A compiled, bounded regular-expression value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegexValue(String);

impl RegexValue {
    pub fn new(pattern: impl Into<String>) -> Result<Self> {
        let pattern = pattern.into();
        if pattern.len() > MAX_VALUE_BYTES {
            return Err(FastDbError::Schema(
                "regex exceeds the value byte limit".into(),
            ));
        }
        regex::Regex::new(&pattern)
            .map_err(|_| FastDbError::Schema("regex pattern is invalid".into()))?;
        Ok(Self(pattern))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A logical table value. It is never a physical identifier.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableValue(String);

impl TableValue {
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if !valid_logical_name(&name) || name.starts_with("__fastdb_") {
            return Err(FastDbError::Schema("table value is invalid".into()));
        }
        Ok(Self(name))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A typed file reference. Access remains capability-gated by later phases.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileValue(String);

impl FileValue {
    pub fn new(reference: impl Into<String>) -> Result<Self> {
        let reference = reference.into();
        let Some((bucket, key)) = reference.split_once(":/") else {
            return Err(FastDbError::Schema(
                "file value must use bucket:/key syntax".into(),
            ));
        };
        if !valid_logical_name(bucket)
            || key.is_empty()
            || reference.len() > MAX_VALUE_BYTES
            || reference.contains('\0')
        {
            return Err(FastDbError::Schema("file value is invalid".into()));
        }
        Ok(Self(reference))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One endpoint of a typed range.
#[derive(Debug, Clone, PartialEq)]
pub enum RangeBound {
    Unbounded,
    Included(Box<Value>),
    Excluded(Box<Value>),
}

/// A range retains independent lower/upper bound presence and inclusivity.
#[derive(Debug, Clone, PartialEq)]
pub struct RangeValue {
    start: RangeBound,
    end: RangeBound,
}

impl RangeValue {
    pub const fn new(start: RangeBound, end: RangeBound) -> Self {
        Self { start, end }
    }

    pub const fn start(&self) -> &RangeBound {
        &self.start
    }

    pub const fn end(&self) -> &RangeBound {
        &self.end
    }
}

/// A canonical set. Equal values are deduplicated in FastDB total order.
#[derive(Debug, Clone, PartialEq)]
pub struct SetValue(Vec<Value>);

impl SetValue {
    pub fn new(mut values: Vec<Value>) -> Result<Self> {
        if values.len() > MAX_COLLECTION_ELEMENTS {
            return Err(FastDbError::Schema(format!(
                "set exceeds {MAX_COLLECTION_ELEMENTS} elements"
            )));
        }
        values.sort_by(canonical_value_cmp);
        values.dedup_by(|left, right| canonical_value_cmp(left, right) == Ordering::Equal);
        Ok(Self(values))
    }

    fn from_canonical(values: Vec<Value>) -> Result<Self> {
        let set =
            Self::new(values.clone()).map_err(|error| FastDbError::format(error.to_string()))?;
        if set.0 != values {
            return Err(FastDbError::format(
                "stored set is not canonically ordered and deduplicated",
            ));
        }
        Ok(set)
    }

    pub fn as_slice(&self) -> &[Value] {
        &self.0
    }

    pub fn into_vec(self) -> Vec<Value> {
        self.0
    }
}

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
    None,
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    Decimal(DecimalValue),
    Str(String),
    Bytes(Vec<u8>),
    Duration(DurationValue),
    Datetime(DatetimeValue),
    Uuid(uuid::Uuid),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
    Set(SetValue),
    Range(RangeValue),
    Regex(RegexValue),
    RecordId(RecordId),
    Table(TableValue),
    File(FileValue),
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
    encode_value_at(value, 0)
}

fn encode_value_at(value: &Value, depth: usize) -> Result<serde_json::Value> {
    if depth > MAX_NESTING_DEPTH {
        return Err(FastDbError::Schema(format!(
            "value nesting exceeds {MAX_NESTING_DEPTH}"
        )));
    }
    match value {
        Value::None => Ok(simple_tag("none")),
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(value) => Ok(serde_json::Value::Bool(*value)),
        Value::Integer(value) => Ok(serde_json::Value::Number((*value).into())),
        Value::Float(value) => {
            let number = Number::from_f64(*value)
                .ok_or_else(|| FastDbError::Schema("non-finite floats cannot be stored".into()))?;
            Ok(serde_json::Value::Number(number))
        }
        Value::Decimal(value) => Ok(string_tag("decimal", value.to_canonical())),
        Value::Str(value) if value.len() <= MAX_VALUE_BYTES => {
            Ok(serde_json::Value::String(value.clone()))
        }
        Value::Str(_) => Err(FastDbError::Schema(
            "string exceeds the value byte limit".into(),
        )),
        Value::Bytes(value) if value.len() <= MAX_VALUE_BYTES => Ok(string_tag(
            "bytes",
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value),
        )),
        Value::Bytes(_) => Err(FastDbError::Schema(
            "bytes exceed the value byte limit".into(),
        )),
        Value::Duration(value) => Ok(string_tag("duration", value.to_canonical())),
        Value::Datetime(value) => Ok(string_tag("datetime", value.to_canonical())),
        Value::Uuid(value) => Ok(string_tag("uuid", value.hyphenated().to_string())),
        Value::Array(values) if values.len() <= MAX_ARRAY_ELEMENTS => values
            .iter()
            .map(|value| encode_value_at(value, depth + 1))
            .collect::<Result<Vec<_>>>()
            .map(serde_json::Value::Array),
        Value::Array(_) => Err(FastDbError::Schema(format!(
            "array exceeds {MAX_ARRAY_ELEMENTS} elements"
        ))),
        Value::Object(values) => encode_object(values, depth),
        Value::Set(values) => {
            let encoded = values
                .as_slice()
                .iter()
                .map(|value| encode_value_at(value, depth + 1))
                .collect::<Result<Vec<_>>>()?;
            Ok(value_tag("set", serde_json::Value::Array(encoded)))
        }
        Value::Range(value) => {
            let mut tag = new_tag("range");
            tag.insert("start".into(), encode_bound(value.start(), depth + 1)?);
            tag.insert("end".into(), encode_bound(value.end(), depth + 1)?);
            Ok(tag_envelope(tag))
        }
        Value::Regex(value) => Ok(string_tag("regex", value.as_str().to_string())),
        Value::RecordId(record) => {
            let mut tag = new_tag("rid");
            tag.insert("table".into(), record.table.clone().into());
            tag.insert("id".into(), encode_rid(&record.id).into());
            Ok(tag_envelope(tag))
        }
        Value::Table(value) => Ok(string_tag("table", value.as_str().to_string())),
        Value::File(value) => Ok(string_tag("file", value.as_str().to_string())),
    }
}

fn encode_object(values: &BTreeMap<String, Value>, depth: usize) -> Result<serde_json::Value> {
    if values.len() > MAX_COLLECTION_ELEMENTS {
        return Err(FastDbError::Schema(format!(
            "object exceeds {MAX_COLLECTION_ELEMENTS} elements"
        )));
    }
    let encoded = values
        .iter()
        .map(|(key, value)| {
            if key.len() > 256 {
                return Err(FastDbError::Schema(
                    "object key exceeds the identifier byte limit".into(),
                ));
            }
            Ok((key.clone(), encode_value_at(value, depth + 1)?))
        })
        .collect::<Result<Map<String, serde_json::Value>>>()?;
    if !values.contains_key(TAG_KEY) {
        return Ok(serde_json::Value::Object(encoded));
    }

    let mut tag = new_tag("object");
    tag.insert("value".into(), serde_json::Value::Object(encoded));
    Ok(tag_envelope(tag))
}

fn new_tag(kind: &str) -> Map<String, serde_json::Value> {
    let mut tag = Map::new();
    tag.insert("v".into(), TAG_VERSION.into());
    tag.insert("t".into(), kind.into());
    tag
}

fn simple_tag(kind: &str) -> serde_json::Value {
    tag_envelope(new_tag(kind))
}

fn string_tag(kind: &str, value: String) -> serde_json::Value {
    value_tag(kind, serde_json::Value::String(value))
}

fn value_tag(kind: &str, value: serde_json::Value) -> serde_json::Value {
    let mut tag = new_tag(kind);
    tag.insert("value".into(), value);
    tag_envelope(tag)
}

fn encode_bound(bound: &RangeBound, depth: usize) -> Result<serde_json::Value> {
    let (kind, value) = match bound {
        RangeBound::Unbounded => ("unbounded", None),
        RangeBound::Included(value) => ("included", Some(encode_value_at(value, depth)?)),
        RangeBound::Excluded(value) => ("excluded", Some(encode_value_at(value, depth)?)),
    };
    let mut object = Map::new();
    object.insert("kind".into(), kind.into());
    if let Some(value) = value {
        object.insert("value".into(), value);
    }
    Ok(serde_json::Value::Object(object))
}

fn tag_envelope(tag: Map<String, serde_json::Value>) -> serde_json::Value {
    let mut envelope = Map::new();
    envelope.insert(TAG_KEY.into(), serde_json::Value::Object(tag));
    serde_json::Value::Object(envelope)
}

/// Decode a complete stored document. Invalid JSON, tags, number ranges, or
/// a stored top-level `id` are format corruption.
pub fn parse_doc(json: &str) -> Result<Vec<(String, Value)>> {
    if json.len() > MAX_DOCUMENT_BYTES {
        return Err(FastDbError::format(
            "stored doc exceeds the document byte limit",
        ));
    }
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
    decode_value_at(value, 0)
}

fn decode_value_at(value: serde_json::Value, depth: usize) -> Result<Value> {
    if depth > MAX_NESTING_DEPTH {
        return Err(FastDbError::format(format!(
            "stored value nesting exceeds {MAX_NESTING_DEPTH}"
        )));
    }
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
        serde_json::Value::String(value) if value.len() <= MAX_VALUE_BYTES => Ok(Value::Str(value)),
        serde_json::Value::String(_) => Err(FastDbError::format(
            "stored string exceeds the value byte limit",
        )),
        serde_json::Value::Array(values) if values.len() <= MAX_ARRAY_ELEMENTS => values
            .into_iter()
            .map(|value| decode_value_at(value, depth + 1))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        serde_json::Value::Array(_) => Err(FastDbError::format(format!(
            "stored array exceeds {MAX_ARRAY_ELEMENTS} elements"
        ))),
        serde_json::Value::Object(values) => decode_object(values, depth),
    }
}

fn decode_object(mut values: Map<String, serde_json::Value>, depth: usize) -> Result<Value> {
    if values.len() > MAX_COLLECTION_ELEMENTS {
        return Err(FastDbError::format(format!(
            "stored object exceeds {MAX_COLLECTION_ELEMENTS} elements"
        )));
    }
    if !values.contains_key(TAG_KEY) {
        return decode_plain_object(values, depth);
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
    decode_tag(tag, depth)
}

fn decode_plain_object(values: Map<String, serde_json::Value>, depth: usize) -> Result<Value> {
    values
        .into_iter()
        .map(|(key, value)| {
            if key.len() > 256 {
                return Err(FastDbError::format(
                    "stored object key exceeds the identifier byte limit",
                ));
            }
            Ok((key, decode_value_at(value, depth + 1)?))
        })
        .collect::<Result<BTreeMap<_, _>>>()
        .map(Value::Object)
}

fn decode_tag(mut tag: Map<String, serde_json::Value>, depth: usize) -> Result<Value> {
    let version = tag
        .remove("v")
        .and_then(|value| value.as_i64())
        .ok_or_else(|| FastDbError::format("stored FastDB tag has no integer version"))?;
    if !matches!(version, LEGACY_TAG_VERSION | TAG_VERSION) {
        return Err(FastDbError::format(format!(
            "unknown stored FastDB tag version {version}"
        )));
    }
    let kind = tag
        .remove("t")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| FastDbError::format("stored FastDB tag has no string type"))?;
    if version == LEGACY_TAG_VERSION && !matches!(kind.as_str(), "rid" | "object") {
        return Err(FastDbError::format(format!(
            "stored FastDB tag type {kind:?} requires envelope version {TAG_VERSION}"
        )));
    }
    match kind.as_str() {
        "none" => {
            require_empty_tag(&tag, "NONE")?;
            Ok(Value::None)
        }
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
            decode_plain_object(value, depth)
        }
        "bytes" => {
            let value = take_only_tag_string(&mut tag, "bytes")?;
            let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(&value)
                .map_err(|_| FastDbError::format("stored bytes tag is invalid base64url"))?;
            if decoded.len() > MAX_VALUE_BYTES
                || base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&decoded) != value
            {
                return Err(FastDbError::format(
                    "stored bytes tag is not canonically encoded",
                ));
            }
            Ok(Value::Bytes(decoded))
        }
        "datetime" => {
            let value = take_only_tag_string(&mut tag, "datetime")?;
            let datetime = DatetimeValue::parse(&value)
                .map_err(|error| FastDbError::format(error.to_string()))?;
            if datetime.to_canonical() != value {
                return Err(FastDbError::format(
                    "stored datetime is not canonically encoded",
                ));
            }
            Ok(Value::Datetime(datetime))
        }
        "decimal" => take_only_tag_string(&mut tag, "decimal")
            .and_then(|value| DecimalValue::parse_canonical(&value))
            .map(Value::Decimal),
        "duration" => take_only_tag_string(&mut tag, "duration")
            .and_then(|value| DurationValue::parse_canonical(&value))
            .map(Value::Duration),
        "file" => take_only_tag_string(&mut tag, "file")
            .and_then(|value| {
                FileValue::new(value).map_err(|error| FastDbError::format(error.to_string()))
            })
            .map(Value::File),
        "range" => {
            let start = tag
                .remove("start")
                .ok_or_else(|| FastDbError::format("stored range has no start bound"))?;
            let end = tag
                .remove("end")
                .ok_or_else(|| FastDbError::format("stored range has no end bound"))?;
            if !tag.is_empty() {
                return Err(FastDbError::format(
                    "stored range tag contains unknown members",
                ));
            }
            Ok(Value::Range(RangeValue::new(
                decode_bound(start, depth + 1)?,
                decode_bound(end, depth + 1)?,
            )))
        }
        "regex" => take_only_tag_string(&mut tag, "regex")
            .and_then(|value| {
                RegexValue::new(value).map_err(|error| FastDbError::format(error.to_string()))
            })
            .map(Value::Regex),
        "set" => {
            let values = match tag.remove("value") {
                Some(serde_json::Value::Array(values)) => values,
                _ => return Err(FastDbError::format("stored set tag has no array value")),
            };
            if !tag.is_empty() {
                return Err(FastDbError::format(
                    "stored set tag contains unknown members",
                ));
            }
            let values = values
                .into_iter()
                .map(|value| decode_value_at(value, depth + 1))
                .collect::<Result<Vec<_>>>()?;
            SetValue::from_canonical(values).map(Value::Set)
        }
        "table" => take_only_tag_string(&mut tag, "table")
            .and_then(|value| {
                TableValue::new(value).map_err(|error| FastDbError::format(error.to_string()))
            })
            .map(Value::Table),
        "uuid" => {
            let value = take_only_tag_string(&mut tag, "UUID")?;
            let uuid = uuid::Uuid::parse_str(&value)
                .map_err(|_| FastDbError::format("stored UUID tag is invalid"))?;
            if uuid.hyphenated().to_string() != value {
                return Err(FastDbError::format(
                    "stored UUID tag is not canonically encoded",
                ));
            }
            Ok(Value::Uuid(uuid))
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

fn take_only_tag_string(
    tag: &mut Map<String, serde_json::Value>,
    kind: &'static str,
) -> Result<String> {
    let value = take_tag_string(tag, "value")?;
    if !tag.is_empty() {
        return Err(FastDbError::format(format!(
            "stored {kind} tag contains unknown members"
        )));
    }
    Ok(value)
}

fn require_empty_tag(tag: &Map<String, serde_json::Value>, kind: &'static str) -> Result<()> {
    if tag.is_empty() {
        Ok(())
    } else {
        Err(FastDbError::format(format!(
            "stored {kind} tag contains unknown members"
        )))
    }
}

fn decode_bound(value: serde_json::Value, depth: usize) -> Result<RangeBound> {
    let serde_json::Value::Object(mut object) = value else {
        return Err(FastDbError::format("stored range bound is not an object"));
    };
    let kind = object
        .remove("kind")
        .and_then(|value| value.as_str().map(str::to_owned))
        .ok_or_else(|| FastDbError::format("stored range bound has no string kind"))?;
    let value = object.remove("value");
    if !object.is_empty() {
        return Err(FastDbError::format(
            "stored range bound contains unknown members",
        ));
    }
    match (kind.as_str(), value) {
        ("unbounded", None) => Ok(RangeBound::Unbounded),
        ("included", Some(value)) => decode_value_at(value, depth)
            .map(Box::new)
            .map(RangeBound::Included),
        ("excluded", Some(value)) => decode_value_at(value, depth)
            .map(Box::new)
            .map(RangeBound::Excluded),
        _ => Err(FastDbError::format(
            "stored range bound has mismatched kind and value",
        )),
    }
}

fn parse_canonical_u64(value: &str, label: &str) -> Result<u64> {
    if value.is_empty()
        || (value.len() > 1 && value.starts_with('0'))
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(FastDbError::format(format!(
            "stored {label} is not a canonical unsigned integer"
        )));
    }
    value
        .parse::<u64>()
        .map_err(|_| FastDbError::format(format!("stored {label} is outside u64")))
}

fn valid_logical_name(value: &str) -> bool {
    if value.is_empty() || value.len() > 256 {
        return false;
    }
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && chars.all(|value| value == '_' || value.is_alphanumeric())
}

pub(crate) fn canonical_value_cmp(left: &Value, right: &Value) -> Ordering {
    let left_rank = value_rank(left);
    let right_rank = value_rank(right);
    if left_rank != right_rank {
        return left_rank.cmp(&right_rank);
    }
    match (left, right) {
        (Value::None | Value::Null, Value::None | Value::Null) => Ordering::Equal,
        (Value::Bool(left), Value::Bool(right)) => left.cmp(right),
        (
            left @ (Value::Integer(_) | Value::Float(_) | Value::Decimal(_)),
            right @ (Value::Integer(_) | Value::Float(_) | Value::Decimal(_)),
        ) => numeric_cmp(left, right),
        (Value::Str(left), Value::Str(right)) => left.cmp(right),
        (Value::Duration(left), Value::Duration(right)) => left.cmp(right),
        (Value::Datetime(left), Value::Datetime(right)) => left.cmp(right),
        (Value::Uuid(left), Value::Uuid(right)) => left.as_bytes().cmp(right.as_bytes()),
        (Value::Array(left), Value::Array(right)) => sequence_cmp(left, right),
        (Value::Object(left), Value::Object(right)) => object_cmp(left, right),
        (Value::Set(left), Value::Set(right)) => sequence_cmp(left.as_slice(), right.as_slice()),
        (Value::Bytes(left), Value::Bytes(right)) => left.cmp(right),
        (Value::RecordId(left), Value::RecordId(right)) => record_cmp(left, right),
        (Value::Range(left), Value::Range(right)) => range_cmp(left, right),
        (Value::Regex(left), Value::Regex(right)) => left.cmp(right),
        (Value::Table(left), Value::Table(right)) => left.cmp(right),
        (Value::File(left), Value::File(right)) => left.cmp(right),
        _ => Ordering::Equal,
    }
}

const fn value_rank(value: &Value) -> u8 {
    match value {
        Value::None => 0,
        Value::Null => 1,
        Value::Bool(_) => 2,
        Value::Integer(_) | Value::Float(_) | Value::Decimal(_) => 3,
        Value::Str(_) => 4,
        Value::Duration(_) => 5,
        Value::Datetime(_) => 6,
        Value::Uuid(_) => 7,
        Value::Array(_) => 8,
        Value::Object(_) => 9,
        Value::Set(_) => 10,
        Value::Bytes(_) => 11,
        Value::RecordId(_) => 12,
        Value::Range(_) => 13,
        Value::Regex(_) => 14,
        Value::Table(_) => 15,
        Value::File(_) => 16,
    }
}

fn numeric_cmp(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => left.cmp(right),
        (Value::Decimal(left), Value::Decimal(right)) => left.cmp(right),
        (Value::Float(left), Value::Float(right)) => {
            left.partial_cmp(right).expect("stored floats are finite")
        }
        (Value::Integer(left), Value::Float(right)) => compare_integer_float(*left, *right),
        (Value::Float(left), Value::Integer(right)) => {
            compare_integer_float(*right, *left).reverse()
        }
        (Value::Integer(left), Value::Decimal(right)) => {
            rust_decimal::Decimal::from(*left).cmp(&right.as_decimal())
        }
        (Value::Decimal(left), Value::Integer(right)) => {
            left.as_decimal().cmp(&rust_decimal::Decimal::from(*right))
        }
        (Value::Decimal(left), Value::Float(right)) => compare_decimal_float(*left, *right),
        (Value::Float(left), Value::Decimal(right)) => {
            compare_decimal_float(*right, *left).reverse()
        }
        _ => unreachable!("numeric comparison received a non-number"),
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

fn compare_decimal_float(decimal: DecimalValue, float: f64) -> Ordering {
    let rendered = float.to_string();
    let converted = if rendered.contains(['e', 'E']) {
        rust_decimal::Decimal::from_scientific(&rendered).ok()
    } else {
        rendered.parse::<rust_decimal::Decimal>().ok()
    };
    if let Some(converted) = converted {
        return decimal.as_decimal().cmp(&converted);
    }

    let approximate = decimal
        .as_decimal()
        .to_f64()
        .expect("96-bit decimal always fits finite f64")
        .total_cmp(&float);
    if approximate != Ordering::Equal {
        return approximate;
    }
    if float.is_sign_positive() {
        Ordering::Less
    } else {
        Ordering::Greater
    }
}

fn sequence_cmp(left: &[Value], right: &[Value]) -> Ordering {
    left.iter()
        .zip(right)
        .map(|(left, right)| canonical_value_cmp(left, right))
        .find(|ordering| *ordering != Ordering::Equal)
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

fn object_cmp(left: &BTreeMap<String, Value>, right: &BTreeMap<String, Value>) -> Ordering {
    left.iter()
        .zip(right)
        .map(|((left_key, left_value), (right_key, right_value))| {
            left_key
                .cmp(right_key)
                .then_with(|| canonical_value_cmp(left_value, right_value))
        })
        .find(|ordering| *ordering != Ordering::Equal)
        .unwrap_or_else(|| left.len().cmp(&right.len()))
}

fn record_cmp(left: &RecordId, right: &RecordId) -> Ordering {
    left.table
        .cmp(&right.table)
        .then_with(|| record_component_cmp(&left.id, &right.id))
}

fn record_component_cmp(left: &RecordIdValue, right: &RecordIdValue) -> Ordering {
    let rank = |value: &RecordIdValue| match value {
        RecordIdValue::Integer(_) => 0,
        RecordIdValue::String(_) => 1,
        RecordIdValue::Uuid(_) => 2,
    };
    rank(left)
        .cmp(&rank(right))
        .then_with(|| match (left, right) {
            (RecordIdValue::Integer(left), RecordIdValue::Integer(right)) => left.cmp(right),
            (RecordIdValue::String(left), RecordIdValue::String(right)) => left.cmp(right),
            (RecordIdValue::Uuid(left), RecordIdValue::Uuid(right)) => {
                left.as_bytes().cmp(right.as_bytes())
            }
            _ => Ordering::Equal,
        })
}

fn range_cmp(left: &RangeValue, right: &RangeValue) -> Ordering {
    bound_cmp(left.start(), right.start()).then_with(|| bound_cmp(left.end(), right.end()))
}

fn bound_cmp(left: &RangeBound, right: &RangeBound) -> Ordering {
    let rank = |bound: &RangeBound| match bound {
        RangeBound::Unbounded => 0,
        RangeBound::Included(_) => 1,
        RangeBound::Excluded(_) => 2,
    };
    rank(left)
        .cmp(&rank(right))
        .then_with(|| match (left, right) {
            (RangeBound::Included(left), RangeBound::Included(right))
            | (RangeBound::Excluded(left), RangeBound::Excluded(right)) => {
                canonical_value_cmp(left, right)
            }
            _ => Ordering::Equal,
        })
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

    #[test]
    fn p12_value_001_format_three_values_round_trip_canonically() {
        let uuid = uuid::Uuid::parse_str("018f1f12-7b42-7cc7-98ad-dbdc0d501234").unwrap();
        let set = SetValue::new(vec![
            Value::Decimal(DecimalValue::parse("1.0").unwrap()),
            Value::Integer(1),
            Value::Str("one".into()),
        ])
        .unwrap();
        assert_eq!(
            set.as_slice(),
            &[
                Value::Decimal(DecimalValue::parse("1").unwrap()),
                Value::Str("one".into())
            ]
        );

        let values = vec![
            Value::None,
            Value::Bytes(vec![0, 1, 254, 255]),
            Value::Datetime(DatetimeValue::parse("2026-08-13T23:00:01.123456789+07:00").unwrap()),
            Value::Duration(DurationValue::new(u64::MAX, 999_999_999).unwrap()),
            Value::Decimal(DecimalValue::parse("-123456789.1200").unwrap()),
            Value::Set(set),
            Value::Range(RangeValue::new(
                RangeBound::Excluded(Box::new(Value::Integer(1))),
                RangeBound::Included(Box::new(Value::Integer(3))),
            )),
            Value::Regex(RegexValue::new("^[a-z]+$").unwrap()),
            Value::Uuid(uuid),
            Value::Table(TableValue::new("person").unwrap()),
            Value::File(FileValue::new("bucket:/folder/object").unwrap()),
        ];
        for value in values {
            let encoded = encode_value(&value).unwrap();
            assert_eq!(encoded["$fastdb"]["v"], TAG_VERSION);
            assert_eq!(decode_value(encoded.clone()).unwrap(), value);
            assert_eq!(
                encode_value(&decode_value(encoded).unwrap()).unwrap(),
                encode_value(&value).unwrap()
            );
        }
    }

    #[test]
    fn p12_value_002_legacy_envelopes_read_but_reencode_as_version_two() {
        let legacy = serde_json::json!({
            "$fastdb": {
                "v": 1,
                "t": "object",
                "value": {
                    "rid": {"$fastdb": {"v": 1, "t": "rid", "table": "person", "id": "v1:i:7"}}
                }
            }
        });
        let decoded = decode_value(legacy).unwrap();
        let encoded = encode_value(&decoded).unwrap();
        assert_eq!(encoded["rid"]["$fastdb"]["v"], 2);
    }

    #[test]
    fn p12_value_003_noncanonical_and_bounded_envelopes_fail_closed() {
        for value in [
            serde_json::json!({"$fastdb":{"v":2,"t":"bytes","value":"YQ=="}}),
            serde_json::json!({"$fastdb":{"v":2,"t":"datetime","value":"2026-08-13T16:00:00+00:00"}}),
            serde_json::json!({"$fastdb":{"v":2,"t":"decimal","value":"1.0"}}),
            serde_json::json!({"$fastdb":{"v":2,"t":"duration","value":"01s"}}),
            serde_json::json!({"$fastdb":{"v":2,"t":"set","value":[2,1]}}),
            serde_json::json!({"$fastdb":{"v":2,"t":"range","start":{"kind":"unbounded","value":1},"end":{"kind":"unbounded"}}}),
            serde_json::json!({"$fastdb":{"v":2,"t":"uuid","value":"018F1F12-7B42-7CC7-98AD-DBDC0D501234"}}),
            serde_json::json!({"$fastdb":{"v":2,"t":"future"}}),
        ] {
            assert_eq!(
                decode_value(value).unwrap_err().category(),
                ErrorCategory::Format
            );
        }

        let mut nested = serde_json::Value::Null;
        for _ in 0..=MAX_NESTING_DEPTH {
            nested = serde_json::Value::Array(vec![nested]);
        }
        assert_eq!(
            decode_value(nested).unwrap_err().category(),
            ErrorCategory::Format
        );
    }

    #[test]
    fn p12_value_006_decimal_scale_rounding_matches_the_reference_boundary() {
        for (source, expected) in [
            (
                "1.23456789012345678901234567894",
                "1.2345678901234567890123456789",
            ),
            (
                "1.23456789012345678901234567895",
                "1.234567890123456789012345679",
            ),
            ("9.99999999999999999999999999995", "10"),
            (
                "-1.23456789012345678901234567895",
                "-1.234567890123456789012345679",
            ),
            (
                "0.00000000000000000000000000005",
                "0.0000000000000000000000000001",
            ),
        ] {
            assert_eq!(
                DecimalValue::parse(source).unwrap().to_canonical(),
                expected
            );
        }
        assert!(DecimalValue::parse("1e2").is_err());
        assert!(DecimalValue::parse("79228162514264337593543950336").is_err());
    }

    #[test]
    fn p12_value_007_set_numeric_equivalence_retains_distinct_large_values() {
        let set = SetValue::new(vec![
            Value::Float(0.1),
            Value::Decimal(DecimalValue::parse("0.1").unwrap()),
            Value::Integer(9_007_199_254_740_992),
            Value::Decimal(DecimalValue::parse("9007199254740993").unwrap()),
            Value::Float(9_007_199_254_740_992.0),
        ])
        .unwrap();
        assert_eq!(set.as_slice().len(), 3);
        assert_eq!(
            set.as_slice(),
            &[
                Value::Float(0.1),
                Value::Integer(9_007_199_254_740_992),
                Value::Decimal(DecimalValue::parse("9007199254740993").unwrap()),
            ]
        );
    }
}
