use crate::{Error, Result};
pub use fastql_parser::{Key, Record};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Document = BTreeMap<String, Value>;
pub type Parameters = BTreeMap<String, Value>;

/// Tagged values prevent collisions with user objects and preserve scalar types.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Value {
    Null,
    Boolean(bool),
    Integer(i64),
    Number(f64),
    String(String),
    Binary(Vec<u8>),
    Record(Record),
    Object(Document),
    Array(Vec<Value>),
    Vector(Vec<u8>),
}
impl Value {
    pub fn validate(&self) -> Result<()> {
        self.validate_at(0)
    }
    fn validate_at(&self, depth: usize) -> Result<()> {
        if depth > 64 {
            return Err(Error::Limit("value nesting exceeds 64".into()));
        }
        match self {
            Self::Number(n) if !n.is_finite() => Err(Error::Validation("non-finite number".into())),
            Self::Record(r) => validate_record(r),
            Self::Object(fields) => {
                for value in fields.values() {
                    value.validate_at(depth + 1)?;
                }
                Ok(())
            }
            Self::Array(values) => {
                for value in values {
                    value.validate_at(depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut bytes = b"FDB\x01".to_vec();
        bytes.extend(serde_json::to_vec(self)?);
        Ok(bytes)
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        let payload = bytes
            .strip_prefix(b"FDB\x01")
            .ok_or_else(|| Error::Storage("unknown value format".into()))?;
        let value: Self = serde_json::from_slice(payload)?;
        value.validate()?;
        Ok(value)
    }
}
pub(crate) fn validate_record(r: &Record) -> Result<()> {
    if r.table.is_empty() || matches!(&r.key, Key::String(s) if s.is_empty()) {
        return Err(Error::Validation(
            "record target and string key must be nonempty".into(),
        ));
    }
    Ok(())
}
