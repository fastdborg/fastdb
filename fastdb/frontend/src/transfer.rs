//! Versioned, lossless document transfer. Not a schema or database backup format.
use crate::{Connection, Document, Error, Key, Record, Result, Value};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_DOCUMENTS: usize = 100_000;
#[derive(Clone, Copy, Debug)]
pub enum TransferFormat {
    Json,
    Ndjson,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    format: String,
    version: u32,
}
impl Header {
    fn new() -> Self {
        Self {
            format: "fastdb.documents".into(),
            version: 1,
        }
    }
    fn check(&self) -> Result<()> {
        if self.format != "fastdb.documents" || self.version != 1 {
            return Err(Error::Validation(
                "unsupported document transfer format/version".into(),
            ));
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Bundle {
    header: Header,
    documents: Vec<Portable>,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value", deny_unknown_fields)]
enum Portable {
    Null,
    Boolean(bool),
    Integer(#[serde(with = "decimal")] i64),
    Number(#[serde(with = "float_bits")] f64),
    String(String),
    Binary(Vec<u8>),
    Record(PortableRecord),
    Object(#[serde(deserialize_with = "unique_fields")] BTreeMap<String, Portable>),
    Array(Vec<Portable>),
    Vector(Vec<u8>),
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortableRecord {
    table: String,
    key: PortableKey,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "value", deny_unknown_fields)]
enum PortableKey {
    Integer(#[serde(with = "decimal")] i64),
    String(String),
}
mod decimal {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &i64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
        let text = String::deserialize(deserializer)?;
        let value = text.parse::<i64>().map_err(serde::de::Error::custom)?;
        if value.to_string() != text {
            return Err(serde::de::Error::custom("expected canonical decimal int64"));
        }
        Ok(value)
    }
}
impl From<Value> for Portable {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Boolean(v) => Self::Boolean(v),
            Value::Integer(v) => Self::Integer(v),
            Value::Number(v) => Self::Number(v),
            Value::String(v) => Self::String(v),
            Value::Binary(v) => Self::Binary(v),
            Value::Vector(v) => Self::Vector(v),
            Value::Array(v) => Self::Array(v.into_iter().map(Self::from).collect()),
            Value::Object(v) => {
                Self::Object(v.into_iter().map(|(k, v)| (k, Self::from(v))).collect())
            }
            Value::Record(v) => Self::Record(PortableRecord {
                table: v.table,
                key: match v.key {
                    Key::Integer(v) => PortableKey::Integer(v),
                    Key::String(v) => PortableKey::String(v),
                },
            }),
        }
    }
}
impl From<Portable> for Value {
    fn from(value: Portable) -> Self {
        match value {
            Portable::Null => Self::Null,
            Portable::Boolean(v) => Self::Boolean(v),
            Portable::Integer(v) => Self::Integer(v),
            Portable::Number(v) => Self::Number(v),
            Portable::String(v) => Self::String(v),
            Portable::Binary(v) => Self::Binary(v),
            Portable::Vector(v) => Self::Vector(v),
            Portable::Array(v) => Self::Array(v.into_iter().map(Self::from).collect()),
            Portable::Object(v) => {
                Self::Object(v.into_iter().map(|(k, v)| (k, Self::from(v))).collect())
            }
            Portable::Record(v) => Self::Record(Record {
                table: v.table,
                key: match v.key {
                    PortableKey::Integer(v) => Key::Integer(v),
                    PortableKey::String(v) => Key::String(v),
                },
            }),
        }
    }
}
// Bound the encoded output while serde emits it, without a temporary JSON
// string for each document. The engine and decoded current row have separate costs.
struct TransferWriter {
    bytes: Vec<u8>,
    limit: usize,
    exceeded: bool,
}
impl std::io::Write for TransferWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            self.exceeded = true;
            return Err(std::io::Error::other("transfer byte limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl TransferWriter {
    fn append(&mut self, bytes: &[u8]) -> Result<()> {
        use std::io::Write;
        self.write_all(bytes)
            .map_err(|_| Error::Limit("transfer byte limit exceeded".into()))
    }
    fn json(&mut self, value: &impl Serialize) -> Result<()> {
        serde_json::to_writer(&mut *self, value).map_err(|error| {
            if self.exceeded {
                Error::Limit("transfer byte limit exceeded".into())
            } else {
                error.into()
            }
        })
    }
}

impl Connection {
    /// Export one collection's documents in a versioned typed format.
    pub fn export_documents(&self, table: &str, format: TransferFormat) -> Result<String> {
        self.export_documents_bounded(table, format, MAX_BYTES, MAX_DOCUMENTS)
    }

    fn export_documents_bounded(
        &self,
        table: &str,
        format: TransferFormat,
        max_bytes: usize,
        max_documents: usize,
    ) -> Result<String> {
        self.atomic(|| {
            let collection = self.catalog(table)?;
            let mut output = TransferWriter {
                bytes: Vec::new(),
                limit: max_bytes,
                exceeded: false,
            };
            match format {
                TransferFormat::Json => {
                    output.append(b"{\"header\":")?;
                    output.json(&Header::new())?;
                    output.append(b",\"documents\":[")?;
                }
                TransferFormat::Ndjson => {
                    output.json(&Header::new())?;
                    output.append(b"\n")?;
                }
            }
            let mut count = 0;
            let mut failure = None;
            let mut statement = self.prepare(format!(
                "SELECT doc FROM {} ORDER BY id LIMIT {}",
                crate::quote(&collection.storage),
                max_documents.saturating_add(1)
            ))?;
            let execution = crate::parser_stack(|| {
                statement.run_with_row_callback(|row| {
                    let result = (|| -> Result<()> {
                        if count >= max_documents {
                            return Err(Error::Limit("transfer document limit exceeded".into()));
                        }
                        let mut values = row.get_values();
                        let value = values
                            .next()
                            .ok_or_else(|| Error::Storage("missing transfer document".into()))?;
                        let document =
                            Portable::from(Value::Object(crate::decode_document(value)?));
                        if matches!(format, TransferFormat::Json) && count != 0 {
                            output.append(b",")?;
                        }
                        output.json(&document)?;
                        if matches!(format, TransferFormat::Ndjson) {
                            output.append(b"\n")?;
                        }
                        count += 1;
                        Ok(())
                    })();
                    if let Err(error) = result {
                        failure = Some(error);
                        return Err(turso_core::LimboError::Interrupt);
                    }
                    Ok(())
                })
            });
            drop(statement);
            if let Some(error) = failure {
                return Err(error);
            }
            execution?;
            if matches!(format, TransferFormat::Json) {
                output.append(b"]}")?;
            }
            String::from_utf8(output.bytes)
                .map_err(|_| Error::Storage("invalid transfer UTF-8".into()))
        })
    }
    /// Insert all transferred documents atomically into an existing collection.
    pub fn import_documents(
        &self,
        table: &str,
        input: &str,
        format: TransferFormat,
    ) -> Result<usize> {
        if input.len() > MAX_BYTES {
            return Err(Error::Limit("transfer exceeds 64 MiB".into()));
        }
        let parse_error =
            |e: serde_json::Error| Error::Validation(format!("document transfer: {e}"));
        let portable =
            match format {
                TransferFormat::Json => {
                    let bundle: Bundle = serde_json::from_str(input).map_err(parse_error)?;
                    bundle.header.check()?;
                    bundle.documents
                }
                TransferFormat::Ndjson => {
                    let mut lines = input.lines();
                    let header: Header = serde_json::from_str(
                        lines
                            .next()
                            .ok_or_else(|| Error::Validation("missing transfer header".into()))?,
                    )
                    .map_err(parse_error)?;
                    header.check()?;
                    let mut documents = Vec::new();
                    for (i, line) in lines.enumerate() {
                        documents.push(serde_json::from_str(line).map_err(|e| {
                            Error::Validation(format!("transfer line {}: {e}", i + 2))
                        })?);
                        if documents.len() > MAX_DOCUMENTS {
                            return Err(Error::Limit("transfer exceeds 100000 documents".into()));
                        }
                    }
                    documents
                }
            };
        if portable.len() > MAX_DOCUMENTS {
            return Err(Error::Limit("transfer exceeds 100000 documents".into()));
        }
        let documents = portable
            .into_iter()
            .map(|v| {
                let value = Value::from(v);
                value.validate()?;
                let Value::Object(doc) = value else {
                    return Err(Error::Validation(
                        "transfer entries must be typed objects".into(),
                    ));
                };
                Ok(doc)
            })
            .collect::<Result<Vec<Document>>>()?;
        self.atomic(|| {
            self.catalog(table)?;
            let count = documents.len();
            for document in documents {
                self.insert(table, document)?;
            }
            Ok(count)
        })
    }
}

fn unique_fields<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, Portable>, D::Error> {
    struct Fields;
    impl<'de> serde::de::Visitor<'de> for Fields {
        type Value = BTreeMap<String, Portable>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an object with unique field names")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut fields = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, Portable>()? {
                if fields.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom("duplicate document field"));
                }
            }
            Ok(fields)
        }
    }
    deserializer.deserialize_map(Fields)
}

mod float_bits {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{:016x}", value.to_bits()))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
        let text = String::deserialize(deserializer)?;
        let bits = u64::from_str_radix(&text, 16).map_err(serde::de::Error::custom)?;
        let value = f64::from_bits(bits);
        if text != format!("{bits:016x}") || !value.is_finite() {
            return Err(serde::de::Error::custom(
                "expected 16 lowercase hexadecimal digits encoding finite binary64",
            ));
        }
        Ok(value)
    }
}

impl Value {
    /// A single tagged value using the document transfer v1 value encoding.
    pub fn to_portable_value(&self) -> Result<serde_json::Value> {
        self.validate()?;
        Ok(serde_json::to_value(Portable::from(self.clone()))?)
    }
    /// Decode a tagged transfer v1 value and validate its logical type.
    pub fn from_portable_value(value: serde_json::Value) -> Result<Self> {
        let value = Self::from(serde_json::from_value::<Portable>(value)?);
        value.validate()?;
        Ok(value)
    }
}

#[cfg(test)]
mod export_tests {
    use super::*;
    #[test]
    fn incremental_export_preserves_format_and_enforces_exact_limits() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        c.execute("CREATE TABLE docs", &crate::Parameters::new())
            .unwrap();
        c.execute("BEGIN", &crate::Parameters::new()).unwrap();
        c.execute(
            "INSERT INTO docs {id:docs:a,text:'ไทย',flag:true};",
            &crate::Parameters::new(),
        )
        .unwrap();
        c.execute(
            "INSERT INTO docs {id:docs:b,text:'line',flag:false};",
            &crate::Parameters::new(),
        )
        .unwrap();
        let docs = ["a", "b"]
            .into_iter()
            .map(|key| {
                let record = Record {
                    table: "docs".into(),
                    key: Key::String(key.into()),
                };
                Portable::from(Value::Object(c.get(&record).unwrap().unwrap()))
            })
            .collect::<Vec<_>>();
        let expected_json = serde_json::to_string(&Bundle {
            header: Header::new(),
            documents: docs,
        })
        .unwrap();
        for format in [TransferFormat::Json, TransferFormat::Ndjson] {
            let output = c.export_documents("docs", format).unwrap();
            if matches!(format, TransferFormat::Json) {
                assert_eq!(output, expected_json);
            }
            assert_eq!(
                c.export_documents_bounded("docs", format, output.len(), 2)
                    .unwrap(),
                output
            );
            for bytes in [0, 1, output.len() - 1] {
                assert_eq!(
                    c.export_documents_bounded("docs", format, bytes, 2)
                        .unwrap_err()
                        .code(),
                    "FDB_LIMIT"
                );
                assert_eq!(c.transaction_state(), crate::TransactionState::Active);
            }
            assert_eq!(
                c.export_documents_bounded("docs", format, MAX_BYTES, 1)
                    .unwrap_err()
                    .code(),
                "FDB_LIMIT"
            );
            assert_eq!(
                c.check_collection_integrity("docs", Default::default())
                    .unwrap()
                    .documents,
                2
            );
            let other = crate::Database::open(":memory:").unwrap();
            let other = other.connect().unwrap();
            other
                .execute("CREATE TABLE docs", &crate::Parameters::new())
                .unwrap();
            assert_eq!(other.import_documents("docs", &output, format).unwrap(), 2);
            assert_eq!(other.export_documents("docs", format).unwrap(), output);
        }
        c.execute("ROLLBACK", &crate::Parameters::new()).unwrap();
    }
}
