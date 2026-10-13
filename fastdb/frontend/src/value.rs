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
        serde_json::to_writer(&mut bytes, self)?;
        Ok(bytes)
    }
    /// Check the complete tagged encoding before allocating its byte buffer.
    pub(crate) fn encode_with_limit(&self, max_bytes: Option<usize>) -> Result<Vec<u8>> {
        if let Some(limit) = max_bytes {
            self.validate()?;
            struct Counter {
                used: usize,
                limit: usize,
            }
            impl std::io::Write for Counter {
                fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                    if bytes.len() > self.limit.saturating_sub(self.used) {
                        return Err(std::io::Error::other("encoded value limit exceeded"));
                    }
                    self.used += bytes.len();
                    Ok(bytes.len())
                }
                fn flush(&mut self) -> std::io::Result<()> {
                    Ok(())
                }
            }
            let mut counter = Counter { used: 0, limit };
            std::io::Write::write_all(&mut counter, b"FDB\x01")
                .map_err(|_| Error::Limit("encoded value limit exceeded".into()))?;
            serde_json::to_writer(&mut counter, self)
                .map_err(|_| Error::Limit("encoded value limit exceeded".into()))?;
        }
        self.encode()
    }
    pub(crate) fn decode(bytes: &[u8]) -> Result<Self> {
        let payload = bytes
            .strip_prefix(b"FDB\x01")
            .ok_or_else(|| Error::Storage("unknown value format".into()))?;
        let input = std::str::from_utf8(payload)
            .map_err(|error| Error::Storage(format!("invalid value UTF-8: {error}")))?;
        let value: Self = crate::decode_wire_json(input)?;
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
    fn bounded_encoding_preserves_bytes_and_counts_header_and_escapes() {
        for value in [
            Value::Null,
            Value::Record(Record {
                table: "docs".into(),
                key: Key::String("\"\\é".into()),
            }),
            Value::Binary(vec![0, 255]),
        ] {
            let expected = value.encode().unwrap();
            assert_eq!(value.encode_with_limit(None).unwrap(), expected);
            assert_eq!(
                value.encode_with_limit(Some(expected.len())).unwrap(),
                expected
            );
            for limit in [0, 3, expected.len() - 1] {
                assert_eq!(
                    value.encode_with_limit(Some(limit)).unwrap_err().code(),
                    "FDB_LIMIT"
                );
            }
        }
    }

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
                let mut previous_encoding = b"FDB\x01".to_vec();
                previous_encoding
                    .extend(serde_json::to_vec(&Value::Object(document.clone())).unwrap());
                assert_eq!(bytes, previous_encoding);
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

/// A field read from an encoded document.
#[derive(Debug, PartialEq)]
pub(crate) enum Field {
    Found(Value),
    /// The path is missing or crosses a non-object parent.
    Missing,
    /// The encoded value is not an object.
    RootNotObject,
}

/// Read one nonempty field path from an encoded document, deserializing only
/// that field. Encodings outside the encoder's layout (another key order,
/// nesting beyond the parser's recursion limit) fall back to a full decode,
/// so the result always equals reading the fully decoded document.
pub(crate) fn decode_field(bytes: &[u8], path: &[String]) -> Result<Field> {
    if let Some(found) = field::read(bytes, path) {
        return Ok(found);
    }
    let Value::Object(document) = Value::decode(bytes)? else {
        return Ok(Field::RootNotObject);
    };
    match crate::path_value(&document, path) {
        Ok(Some(found)) => Ok(Field::Found(found.clone())),
        Ok(None) | Err(Error::Validation(_)) => Ok(Field::Missing),
        Err(error) => Err(error),
    }
}

mod field {
    use super::{Document, Field, Record, Value};
    use serde::de::{DeserializeSeed, Error as _, IgnoredAny, MapAccess, Visitor};
    use std::fmt;

    pub(super) fn read(bytes: &[u8], path: &[String]) -> Option<Field> {
        let payload = bytes.strip_prefix(b"FDB\x01")?;
        if path.is_empty() {
            return None;
        }
        let mut decoder = serde_json::Deserializer::from_slice(payload);
        let found = Tagged { path, root: true }.deserialize(&mut decoder).ok()?;
        decoder.end().ok()?;
        match found {
            Lookup::Found(value) => value.validate().ok().map(|()| Field::Found(value)),
            Lookup::Missing => Some(Field::Missing),
            Lookup::RootNotObject => None,
        }
    }

    enum Lookup {
        Found(Value),
        Missing,
        RootNotObject,
    }

    /// One encoded value: `{"type": kind}` or `{"type": kind, "value": ...}`.
    struct Tagged<'p> {
        path: &'p [String],
        root: bool,
    }

    impl<'de> DeserializeSeed<'de> for Tagged<'_> {
        type Value = Lookup;
        fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Lookup, D::Error> {
            d.deserialize_map(self)
        }
    }

    impl<'de> Visitor<'de> for Tagged<'_> {
        type Value = Lookup;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an encoded value")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Lookup, A::Error> {
            if map.next_key_seed(Is("type"))? != Some(true) {
                return Err(A::Error::custom("type must come first"));
            }
            let kind = map.next_value::<Kind>()?;
            let content = match map.next_key_seed(Is("value"))? {
                None => false,
                Some(true) => true,
                Some(false) => return Err(A::Error::custom("unexpected member")),
            };
            let found = if self.path.is_empty() {
                Lookup::Found(match (kind, content) {
                    (Kind::Null, false) => Value::Null,
                    (Kind::Null, true) => {
                        map.next_value::<IgnoredAny>()?;
                        Value::Null
                    }
                    (_, false) => return Err(A::Error::custom("missing value")),
                    (Kind::Boolean, true) => Value::Boolean(map.next_value()?),
                    (Kind::Integer, true) => Value::Integer(map.next_value()?),
                    (Kind::Number, true) => Value::Number(map.next_value()?),
                    (Kind::String, true) => Value::String(map.next_value()?),
                    (Kind::Binary, true) => Value::Binary(map.next_value()?),
                    (Kind::Record, true) => Value::Record(map.next_value::<Record>()?),
                    (Kind::Object, true) => Value::Object(map.next_value::<Document>()?),
                    (Kind::Array, true) => Value::Array(map.next_value()?),
                    (Kind::Vector, true) => Value::Vector(map.next_value()?),
                })
            } else if kind == Kind::Object && content {
                map.next_value_seed(Fields(self.path))?
            } else {
                if content {
                    map.next_value::<IgnoredAny>()?;
                }
                if self.root {
                    Lookup::RootNotObject
                } else {
                    Lookup::Missing
                }
            };
            if map.next_key::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("unexpected member"));
            }
            Ok(found)
        }
    }

    /// Object members: descend into the first path step, skip the others.
    struct Fields<'p>(&'p [String]);

    impl<'de> DeserializeSeed<'de> for Fields<'_> {
        type Value = Lookup;
        fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Lookup, D::Error> {
            d.deserialize_map(self)
        }
    }

    impl<'de> Visitor<'de> for Fields<'_> {
        type Value = Lookup;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("document fields")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Lookup, A::Error> {
            let (first, rest) = self.0.split_first().expect("nonempty path");
            let mut found = Lookup::Missing;
            while let Some(matches) = map.next_key_seed(Is(first))? {
                if matches {
                    found = map.next_value_seed(Tagged {
                        path: rest,
                        root: false,
                    })?;
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(found)
        }
    }

    /// A string key compared with an expected text, without allocating.
    struct Is<'a>(&'a str);

    impl<'de> DeserializeSeed<'de> for Is<'_> {
        type Value = bool;
        fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<bool, D::Error> {
            d.deserialize_str(self)
        }
    }

    impl<'de> Visitor<'de> for Is<'_> {
        type Value = bool;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a string key")
        }
        fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<bool, E> {
            Ok(v == self.0)
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Null,
        Boolean,
        Integer,
        Number,
        String,
        Binary,
        Record,
        Object,
        Array,
        Vector,
    }

    impl<'de> serde::Deserialize<'de> for Kind {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Kind, D::Error> {
            struct KindVisitor;
            impl<'de> Visitor<'de> for KindVisitor {
                type Value = Kind;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("a value type")
                }
                fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Kind, E> {
                    Ok(match v {
                        "Null" => Kind::Null,
                        "Boolean" => Kind::Boolean,
                        "Integer" => Kind::Integer,
                        "Number" => Kind::Number,
                        "String" => Kind::String,
                        "Binary" => Kind::Binary,
                        "Record" => Kind::Record,
                        "Object" => Kind::Object,
                        "Array" => Kind::Array,
                        "Vector" => Kind::Vector,
                        _ => return Err(E::custom("unknown value type")),
                    })
                }
            }
            d.deserialize_str(KindVisitor)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::super::{decode_field, Field, Value};
        use crate::Document;

        fn sample() -> Value {
            Value::Object(Document::from([
                ("a".into(), Value::Integer(1)),
                ("name".into(), Value::String("quote \" and \\ slash".into())),
                (
                    "nested".into(),
                    Value::Object(Document::from([
                        (
                            "deep".into(),
                            Value::Array(vec![
                                Value::Number(1.5),
                                Value::Null,
                                Value::Boolean(true),
                            ]),
                        ),
                        ("bin".into(), Value::Binary(vec![0, 1, 255])),
                    ])),
                ),
                (
                    "rec".into(),
                    Value::Record(crate::Record {
                        table: "t".into(),
                        key: crate::Key::String("k".into()),
                    }),
                ),
                ("nothing".into(), Value::Null),
                ("vec".into(), Value::vector32(&[1.0, 2.0]).unwrap()),
                ("with \"escape\"".into(), Value::Integer(7)),
            ]))
        }

        #[test]
        fn field_reads_match_full_decoding() {
            let value = sample();
            let bytes = value.encode().unwrap();
            let Value::Object(document) = &value else {
                panic!()
            };
            let paths: Vec<Vec<&str>> = vec![
                vec!["a"],
                vec!["name"],
                vec!["nested"],
                vec!["nested", "deep"],
                vec!["nested", "bin"],
                vec!["rec"],
                vec!["nothing"],
                vec!["vec"],
                vec!["with \"escape\""],
                vec!["missing"],
                vec!["a", "below_scalar"],
                vec!["nested", "missing"],
                vec!["nested", "deep", "x"],
                vec!["nothing", "x"],
            ];
            for path in paths {
                let path = path.into_iter().map(String::from).collect::<Vec<_>>();
                let expected = match crate::path_value(document, &path) {
                    Ok(Some(found)) => Field::Found(found.clone()),
                    _ => Field::Missing,
                };
                assert_eq!(super::read(&bytes, &path), Some(expected), "{path:?}");
                let expected = match crate::path_value(document, &path) {
                    Ok(Some(found)) => Field::Found(found.clone()),
                    _ => Field::Missing,
                };
                assert_eq!(decode_field(&bytes, &path).unwrap(), expected, "{path:?}");
            }
        }

        #[test]
        fn noncanonical_encodings_fall_back_to_full_decoding() {
            let reordered = b"FDB\x01{\"value\":{\"a\":{\"type\":\"Integer\",\"value\":3}},\"type\":\"Object\"}";
            assert_eq!(super::read(reordered, &["a".into()]), None);
            assert_eq!(
                decode_field(reordered, &["a".into()]).unwrap(),
                Field::Found(Value::Integer(3))
            );
            let array_root = Value::Array(vec![Value::Integer(1)]).encode().unwrap();
            assert_eq!(super::read(&array_root, &["a".into()]), None);
            assert_eq!(
                decode_field(&array_root, &["a".into()]).unwrap(),
                Field::RootNotObject
            );
            assert!(decode_field(
                b"FDB\x01{\"type\":\"Object\",\"value\":{\"a\":",
                &["a".into()]
            )
            .is_err());
        }
    }
}
