//! Explicit, bounded logical collection audit without data repair.
use crate::{quote, Connection, Error, Result, Value};
use serde::Serialize;
use turso_core::{Numeric, Value as EngineValue};

#[derive(Debug, Clone, Copy)]
pub struct IntegrityLimits {
    pub max_documents: u64,
    pub max_encoded_bytes: u64,
}
impl Default for IntegrityLimits {
    fn default() -> Self {
        Self {
            max_documents: 100_000,
            max_encoded_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Default, Serialize)]
pub struct IntegrityReport {
    pub documents: u64,
    pub indexes: u64,
    pub index_entries: u64,
    pub encoded_bytes: u64,
}
fn stored(error: Error) -> Error {
    match error {
        Error::Engine(_) | Error::Limit(_) | Error::Storage(_) => error,
        _ => Error::Storage(format!("invalid stored collection value: {error}")),
    }
}
impl Connection {
    /// Audit one collection's documents and index entries in one snapshot.
    /// Limits bound processed records/bytes, not engine allocations or time.
    pub fn check_collection_integrity(
        &self,
        table: &str,
        limits: IntegrityLimits,
    ) -> Result<IntegrityReport> {
        crate::parser_stack(|| self.check_collection_integrity_inner(table, limits))
    }
    fn check_collection_integrity_inner(
        &self,
        table: &str,
        limits: IntegrityLimits,
    ) -> Result<IntegrityReport> {
        self.atomic(|| {
            self.validate_storage_schema()?;
            let c = self.catalog(table)?;
            let mut report = IntegrityReport {
                indexes: c.indexes.len() as u64,
                ..Default::default()
            };
            let mut statement =
                self.prepare(format!("SELECT id,doc FROM {}", quote(&c.storage)))?;
            let mut failure = None;
            let execution = crate::parser_stack(|| {
                statement.run_with_row_callback(|row| {
                    let check = (|| -> Result<()> {
                        let mut values = row.get_values();
                        let (Some(EngineValue::Blob(id)), Some(EngineValue::Blob(body))) =
                            (values.next(), values.next())
                        else {
                            return Err(Error::Storage("invalid physical document row".into()));
                        };
                        if report.documents >= limits.max_documents {
                            return Err(Error::Limit("integrity document limit exceeded".into()));
                        }
                        let bytes = (id.len() as u64)
                            .checked_add(body.len() as u64)
                            .ok_or_else(|| Error::Limit("integrity byte limit exceeded".into()))?;
                        if bytes
                            > limits
                                .max_encoded_bytes
                                .saturating_sub(report.encoded_bytes)
                        {
                            return Err(Error::Limit("integrity byte limit exceeded".into()));
                        }
                        report.documents += 1;
                        report.encoded_bytes += bytes;
                        let doc = crate::decode_document(&EngineValue::Blob(body.clone()))
                            .map_err(stored)?;
                        let Some(Value::Record(record)) = doc.get("id") else {
                            return Err(Error::Storage("stored document has no typed id".into()));
                        };
                        let canonical =
                            Value::Record(crate::normalized_id(record, &c.name).map_err(stored)?);
                        if doc.get("id") != Some(&canonical)
                            || canonical.encode().map_err(stored)? != *id
                        {
                            return Err(Error::Storage("physical/document id mismatch".into()));
                        }
                        self.validate_integrity_candidate(&c, &doc)
                            .map_err(stored)?;
                        for index in &c.indexes {
                            let key = crate::index_scalar(
                                crate::path_value(&doc, &index.path)
                                    .map_err(stored)?
                                    .unwrap_or(&Value::Null),
                            )
                            .map_err(stored)?;
                            let rows = self.run(
                                &format!(
                                    "SELECT count(*) FROM {} WHERE \"key\" IS ?1 AND id=?2",
                                    quote(&index.storage)
                                ),
                                &[key, EngineValue::Blob(id.clone())],
                            )?;
                            if count(&rows)? != 1 {
                                return Err(Error::Storage(format!(
                                    "missing or duplicate entry in index {}",
                                    index.name
                                )));
                            }
                        }
                        Ok(())
                    })();
                    match check {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            failure = Some(error);
                            // Stop this callback scan; no engine interrupt flag is
                            // set. Preserve the actual frontend error below.
                            Err(turso_core::LimboError::Interrupt)
                        }
                    }
                })
            });
            drop(statement);
            if let Some(error) = failure {
                return Err(error);
            }
            execution?;
            for index in &c.indexes {
                let entries = count(&self.run(
                    &format!("SELECT count(*) FROM {}", quote(&index.storage)),
                    &[],
                )?)?;
                if entries != report.documents {
                    return Err(Error::Storage(format!(
                        "extra or missing entries in index {}",
                        index.name
                    )));
                }
                report.index_entries = report
                    .index_entries
                    .checked_add(entries)
                    .ok_or_else(|| Error::Limit("integrity entry count overflow".into()))?;
            }
            Ok(report)
        })
    }
}
fn count(rows: &[Vec<EngineValue>]) -> Result<u64> {
    match rows {
        [row] => match row.as_slice() {
            [EngineValue::Numeric(Numeric::Integer(value))] if *value >= 0 => Ok(*value as u64),
            _ => Err(Error::Storage("invalid integrity count result".into())),
        },
        _ => Err(Error::Storage("invalid integrity count result".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, Parameters};
    fn setup() -> (Database, Connection) {
        let db = Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        for sql in [
            "CREATE TABLE docs",
            "DEFINE FIELD n ON docs TYPE integer REQUIRED CHECK(n>0)",
            "CREATE UNIQUE INDEX docs_n ON docs(n)",
            "CREATE INDEX docs_g ON docs(g)",
            "INSERT INTO docs {id:docs:a,n:1,g:null}",
            "INSERT INTO docs {id:docs:b,n:2}",
        ] {
            c.execute(sql, &Parameters::new()).unwrap();
        }
        (db, c)
    }
    #[test]
    fn checks_documents_indexes_and_limits_without_changing_outer_work() {
        let (_db, c) = setup();
        let report = c
            .check_collection_integrity("docs", IntegrityLimits::default())
            .unwrap();
        assert_eq!(
            (report.documents, report.indexes, report.index_entries),
            (2, 2, 4)
        );
        assert!(report.encoded_bytes > 0);
        c.execute("BEGIN", &Parameters::new()).unwrap();
        c.execute("INSERT INTO docs {id:docs:c,n:3,g:'x'}", &Parameters::new())
            .unwrap();
        for limits in [
            IntegrityLimits {
                max_documents: 2,
                ..Default::default()
            },
            IntegrityLimits {
                max_encoded_bytes: 1,
                ..Default::default()
            },
        ] {
            assert_eq!(
                c.check_collection_integrity("docs", limits)
                    .unwrap_err()
                    .code(),
                "FDB_LIMIT"
            );
            assert_eq!(c.transaction_state(), crate::TransactionState::Active);
        }
        assert_eq!(
            c.check_collection_integrity("docs", IntegrityLimits::default())
                .unwrap()
                .documents,
            3
        );
        c.execute("ROLLBACK", &Parameters::new()).unwrap();
        assert_eq!(
            c.check_collection_integrity("docs", IntegrityLimits::default())
                .unwrap()
                .documents,
            2
        );
    }
    #[test]
    fn rejects_missing_duplicate_stale_and_extra_index_entries() {
        for sql in [
            "DELETE FROM __fastdb_i_646f63735f67",
            "INSERT INTO __fastdb_i_646f63735f67 SELECT * FROM __fastdb_i_646f63735f67 LIMIT 1",
            "UPDATE __fastdb_i_646f63735f67 SET key='stale'",
            "INSERT INTO __fastdb_i_646f63735f67 VALUES (NULL,X'00')",
        ] {
            let (_db, c) = setup();
            c.run(sql, &[]).unwrap();
            assert_eq!(
                c.check_collection_integrity("docs", IntegrityLimits::default())
                    .unwrap_err()
                    .code(),
                "FDB_STORAGE",
                "{sql}"
            );
            assert_eq!(
                c.execute("SELECT count(*) FROM docs", &Parameters::new())
                    .unwrap()
                    .rows,
                vec![vec![Value::Integer(2)]]
            );
        }
    }
    #[test]
    fn audit_check_evaluation_preserves_engine_cancellation() {
        let (_db, c) = setup();
        let collection = c.catalog("docs").unwrap();
        let doc = c
            .get(&crate::Record {
                table: "docs".into(),
                key: crate::Key::String("a".into()),
            })
            .unwrap()
            .unwrap();
        c.engine.set_progress_handler(1, Some(Box::new(|| true)));
        let error = c
            .validate_integrity_candidate(&collection, &doc)
            .unwrap_err();
        c.engine.set_progress_handler(0, None);
        assert_eq!(error.code(), "FDB_CANCELLED");
        assert_eq!(
            c.check_collection_integrity("docs", IntegrityLimits::default())
                .unwrap()
                .documents,
            2
        );
    }
    #[test]
    fn rejects_invalid_encoding_ids_types_and_checks() {
        for mutation in 0..4 {
            let (_db, c) = setup();
            let record = crate::Record {
                table: "docs".into(),
                key: crate::Key::String("a".into()),
            };
            let mut doc = c.get(&record).unwrap().unwrap();
            let bytes = match mutation {
                0 => vec![0],
                1 => {
                    doc.insert(
                        "id".into(),
                        Value::Record(crate::Record {
                            table: "docs".into(),
                            key: crate::Key::String("other".into()),
                        }),
                    );
                    Value::Object(doc).encode().unwrap()
                }
                2 => {
                    doc.insert("n".into(), Value::String("bad".into()));
                    Value::Object(doc).encode().unwrap()
                }
                _ => {
                    doc.insert("n".into(), Value::Integer(0));
                    Value::Object(doc).encode().unwrap()
                }
            };
            c.run(
                "UPDATE __fastdb_c_646f6373 SET doc=?1 WHERE id=?2",
                &[
                    EngineValue::Blob(bytes),
                    EngineValue::Blob(Value::Record(record).encode().unwrap()),
                ],
            )
            .unwrap();
            let error = c
                .check_collection_integrity("docs", IntegrityLimits::default())
                .unwrap_err();
            assert_eq!(error.code(), "FDB_STORAGE", "case {mutation}: {error}");
        }
    }
}
