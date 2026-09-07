//! Batched one-hop reads. These execute in the frontend, never in a UDF.
use crate::{
    canonical, from_engine, quote, text, Connection, Document, EngineValue, Error, Key, Record,
    Result, Value,
};
use std::collections::BTreeMap;
const CHUNK: usize = 128;
impl Connection {
    /// Resolve typed references in order, preserving duplicates and nulls.
    /// Missing targets become Null. This reads within one transaction snapshot.
    pub fn fetch_records(&self, references: &[Value]) -> Result<Vec<Value>> {
        if references.len() > 16_384 {
            return Err(Error::Limit("fetch reference count exceeds 16384".into()));
        }
        self.atomic(|| self.fetch_records_inner(references))
    }
    fn fetch_records_inner(&self, references: &[Value]) -> Result<Vec<Value>> {
        let mut groups: BTreeMap<String, BTreeMap<Vec<u8>, Record>> = BTreeMap::new();
        let mut identities = Vec::new();
        for value in references {
            let record = match value {
                Value::Null => {
                    identities.push(None);
                    continue;
                }
                Value::Record(record) => Record {
                    table: canonical(&record.table)?,
                    key: record.key.clone(),
                },
                _ => {
                    return Err(Error::Validation(
                        "record::fetch expects a typed record or null".into(),
                    ))
                }
            };
            let encoded = Value::Record(record.clone()).encode()?;
            groups
                .entry(record.table.clone())
                .or_default()
                .insert(encoded.clone(), record);
            identities.push(Some(encoded));
        }
        let mut found = BTreeMap::new();
        for (table, records) in groups {
            let records = records.into_iter().collect::<Vec<_>>();
            match self.catalog(&table) {
                Ok(collection) => {
                    for chunk in records.chunks(CHUNK) {
                        let slots = (1..=chunk.len())
                            .map(|i| format!("?{i}"))
                            .collect::<Vec<_>>()
                            .join(",");
                        let params = chunk
                            .iter()
                            .map(|(id, _)| EngineValue::Blob(id.clone()))
                            .collect::<Vec<_>>();
                        let rows = self.run(
                            &format!(
                                "SELECT id,doc FROM {} WHERE id IN ({slots})",
                                quote(&collection.storage)
                            ),
                            &params,
                        )?;
                        for row in rows {
                            let EngineValue::Blob(id) = &row[0] else {
                                return Err(Error::Storage("invalid fetched id".into()));
                            };
                            found.insert(
                                id.clone(),
                                Value::Object(crate::decode_document(&row[1])?),
                            );
                        }
                    }
                }
                Err(Error::NotFound(_)) => {
                    let schema = self.run(
                        "SELECT type FROM main.sqlite_schema WHERE name=?1 COLLATE NOCASE",
                        &[text(&table)],
                    )?;
                    let Some(row) = schema.first() else {
                        continue;
                    };
                    if from_engine(row[0].clone()) != Value::String("table".into()) {
                        return Err(Error::Unsupported(
                            "record::fetch requires a table target".into(),
                        ));
                    }
                    let info =
                        self.run(&format!("PRAGMA main.table_info({})", quote(&table)), &[])?;
                    let keys = info
                        .iter()
                        .filter(|r| matches!(from_engine(r[5].clone()),Value::Integer(i) if i>0))
                        .collect::<Vec<_>>();
                    let [key] = keys.as_slice() else {
                        return Err(Error::Unsupported(
                            "record::fetch requires a single explicit TEXT or INTEGER primary key"
                                .into(),
                        ));
                    };
                    let (Value::String(key_name), Value::String(key_type)) =
                        (from_engine(key[1].clone()), from_engine(key[2].clone()))
                    else {
                        return Err(Error::Storage("invalid primary key metadata".into()));
                    };
                    let integer = match key_type.trim().to_ascii_uppercase().as_str() {
                        "INTEGER" => true,
                        "TEXT" => false,
                        _ => {
                            return Err(Error::Unsupported(
                                "record::fetch requires TEXT or INTEGER primary key".into(),
                            ))
                        }
                    };
                    for (_, record) in &records {
                        if matches!(record.key, Key::Integer(_)) != integer {
                            return Err(Error::Validation(
                                "reference key type differs from target primary key".into(),
                            ));
                        }
                    }
                    for chunk in records.chunks(CHUNK) {
                        let slots = (1..=chunk.len())
                            .map(|i| format!("?{i}"))
                            .collect::<Vec<_>>()
                            .join(",");
                        let sql = format!(
                            "SELECT * FROM main.{} WHERE {} IN ({slots}) AND typeof({})='{}'",
                            quote(&table),
                            quote(&key_name),
                            quote(&key_name),
                            if integer { "integer" } else { "text" }
                        );
                        let mut statement = self.prepare(sql)?;
                        for (i, (_, record)) in chunk.iter().enumerate() {
                            let value = match &record.key {
                                Key::Integer(i) => crate::scalar(&Value::Integer(*i))?,
                                Key::String(s) => text(s),
                            };
                            statement.bind_at(
                                std::num::NonZeroUsize::new(i + 1).expect("one based"),
                                value,
                            )?;
                        }
                        let names = (0..statement.num_columns())
                            .map(|i| statement.get_column_name(i).into_owned())
                            .collect::<Vec<_>>();
                        for row in crate::collect_rows(&mut statement)? {
                            let doc = names
                                .iter()
                                .cloned()
                                .zip(row.into_iter().map(from_engine))
                                .collect::<Document>();
                            let key = match doc.get(&key_name) {
                                Some(Value::Integer(i)) => Key::Integer(*i),
                                Some(Value::String(s)) => Key::String(s.clone()),
                                _ => return Err(Error::Storage("invalid fetched key".into())),
                            };
                            let id = Value::Record(Record {
                                table: table.clone(),
                                key,
                            })
                            .encode()?;
                            found.insert(id, Value::Object(doc));
                        }
                    }
                }
                Err(e) => return Err(e),
            }
        }
        Ok(identities
            .into_iter()
            .map(|id| {
                id.and_then(|id| found.get(&id).cloned())
                    .unwrap_or(Value::Null)
            })
            .collect())
    }
}
