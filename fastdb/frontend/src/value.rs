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
            Self::Vector(bytes) => crate::vectors::dimensions(bytes).map(|_| ()),
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
pub(crate) fn validate_document_value(document: &Document) -> Result<()> {
    for value in document.values() {
        value.validate_at(1)?;
    }
    Ok(())
}

pub(crate) fn encode_document(document: &Document) -> Result<Vec<u8>> {
    validate_document_value(document)?;
    #[derive(Serialize)]
    #[serde(tag = "type", content = "value")]
    enum BorrowedValue<'a> {
        Object(&'a Document),
    }
    let mut bytes = b"FDB\x01".to_vec();
    serde_json::to_writer(&mut bytes, &BorrowedValue::Object(document))?;
    Ok(bytes)
}

pub(crate) fn validate_record(r: &Record) -> Result<()> {
    if r.table.is_empty() || matches!(&r.key, Key::String(s) if s.is_empty()) {
        return Err(Error::Validation(
            "record target and string key must be nonempty".into(),
        ));
    }
    // Use the same logical target-name rules even when the reference is nested
    // or not indexed; indexed writes already pass through canonical().
    crate::canonical(&r.table)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stored_numbers_round_trip_binary64_bits() {
        let mut bits = 0x1234_5678_9abc_def0u64;
        let edges = [
            0,
            1,
            0x8000_0000_0000_0000,
            0x000f_ffff_ffff_ffff,
            0x0010_0000_0000_0000,
            0x7fef_ffff_ffff_ffff,
            0xffef_ffff_ffff_ffff,
        ];
        for candidate in edges.into_iter().chain((0..4096).map(|_| {
            bits ^= bits << 13;
            bits ^= bits >> 7;
            bits ^= bits << 17;
            bits
        })) {
            let number = f64::from_bits(candidate);
            if !number.is_finite() {
                continue;
            }
            let encoded = Value::Number(number).encode().unwrap();
            let Value::Number(actual) = Value::decode(&encoded).unwrap() else {
                panic!("number type lost")
            };
            assert_eq!(
                actual.to_bits(),
                candidate,
                "encoding {}",
                String::from_utf8_lossy(&encoded)
            );
        }
    }
}

#[cfg(test)]
mod borrowed_document_tests {
    use super::*;

    #[test]
    fn borrowed_encoding_preserves_storage_bytes_and_validation() {
        for vector in [
            Value::vector32(&[1.0, 0.0, -1.0]).unwrap(),
            Value::vector64(&[1.0, 0.0, -1.0]).unwrap(),
            Value::vector32_sparse(&[1.0, 0.0, -1.0]).unwrap(),
            Value::vector8(&[1.0, 0.0, -1.0]).unwrap(),
            Value::vector1bit(&[1.0, 0.0, -1.0]).unwrap(),
        ] {
            for key in [Key::Integer(i64::MIN), Key::String("ไทย:\"\n".into())] {
                let document = Document::from([
                    (
                        "id".into(),
                        Value::Record(Record {
                            table: "docs".into(),
                            key,
                        }),
                    ),
                    ("vector".into(), vector.clone()),
                    (
                        "nested".into(),
                        Value::Object(Document::from([(
                            "escaped\"\n".into(),
                            Value::String("\0\t\\".into()),
                        )])),
                    ),
                ]);
                let bytes = encode_document(&document).unwrap();
                assert_eq!(bytes, Value::Object(document.clone()).encode().unwrap());
                assert_eq!(Value::decode(&bytes).unwrap(), Value::Object(document));
            }
        }
        let document = Document::from([
            (
                "id".into(),
                Value::Record(Record {
                    table: "docs".into(),
                    key: Key::Integer(i64::MAX),
                }),
            ),
            (
                "é".into(),
                Value::Array(vec![
                    Value::Null,
                    Value::Boolean(true),
                    Value::Integer(i64::MIN),
                    Value::Number(-0.0),
                    Value::String("ไทย".into()),
                    Value::Binary(vec![0, 255]),
                    Value::Object(Document::new()),
                ]),
            ),
        ]);
        let bytes = encode_document(&document).unwrap();
        assert_eq!(bytes, Value::Object(document.clone()).encode().unwrap());
        assert_eq!(Value::decode(&bytes).unwrap(), Value::Object(document));
        for value in [
            Value::Number(f64::NAN),
            Value::Vector(vec![]),
            Value::Record(Record {
                table: String::new(),
                key: Key::Integer(1),
            }),
        ] {
            let document = Document::from([("v".into(), value)]);
            assert_eq!(
                encode_document(&document).unwrap_err().code(),
                Value::Object(document.clone()).encode().unwrap_err().code()
            );
        }
        let mut value = Value::Null;
        for depth in 0..66 {
            let document = Document::from([("v".into(), value.clone())]);
            let borrowed = encode_document(&document);
            let owned = Value::Object(document).encode();
            assert_eq!(borrowed.is_ok(), owned.is_ok(), "depth {depth}");
            if let (Ok(a), Ok(b)) = (borrowed, owned) {
                assert_eq!(a, b);
            }
            value = Value::Array(vec![value]);
        }
    }
}
