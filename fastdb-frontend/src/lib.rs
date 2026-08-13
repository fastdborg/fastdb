//! # turso_fastdb
//!
//! FastDB Phase 3 frontend: parses SurrealQL-subset input with the
//! independent parser crate, lowers it directly into Turso AST, executes
//! via `Connection::prepare_translated_stmt_with_options`, and decodes
//! results into FastDB value/record types.
//!
//! The request path is:
//! ```text
//! FastDB source -> parser AST -> frontend plan
//!   -> Turso AST + bound values -> prepare_translated_stmt_with_options
//!   -> Turso execution -> FastDB result decoding
//! ```
//! FastDB user input is never parsed by Turso's SQLite parser and never
//! rendered into SQLite text. Logical identifiers resolve through the
//! catalog; physical names are opaque.

// Deny warnings in-crate so that FastDB code stays lint-clean. This is
// scoped to FastDB crates only: upstream workspace-member dependencies
// (e.g. turso_core) carry their own, pre-existing lint state and are not
// fixed by Phase 0 (per plan-phase0.md: "do not fix upstream failures").
// The verified clippy command is therefore `cargo clippy -p <fastdb crate>
// --all-targets` WITHOUT a global `-D warnings`, which would otherwise
// fatalize pre-existing upstream warnings. See docs/phase0-engine-audit.md.
#![forbid(unsafe_code)]
#![deny(warnings)]

mod builtins;
pub mod catalog;
pub mod connection;
pub mod decode;
pub mod error;
mod eval;
pub mod execute;
pub mod lower;
pub mod names;
mod password_functions;
pub mod path;
mod provider;
pub mod schema;
mod string_functions;
pub mod test_failpoints;
mod value_functions;

pub use connection::{CheckReport, Connection, Database};
pub use decode::{parse_doc, Record, RecordId, RecordIdValue, Value};
pub use error::{ErrorCategory, FastDbError};
pub use test_failpoints::Failpoint;

use std::collections::BTreeMap;

/// Named value bindings. Keys do not include the `$` source prefix.
pub type Params = BTreeMap<String, Value>;

/// The exact result of one statement in an executed script.
#[derive(Debug, Clone, PartialEq)]
pub enum StatementResult {
    None,
    Rows(Vec<Value>),
    Value(Value),
}

/// Ordered results for a successfully executed request.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResponse {
    pub statements: Vec<StatementResult>,
    /// Exact number of records created, updated, or deleted by the request.
    pub mutation_count: u64,
}

impl QueryResponse {
    pub(crate) fn new(statements: Vec<StatementResult>, mutation_count: u64) -> Self {
        Self {
            statements,
            mutation_count,
        }
    }

    /// Phase 0-2 test adapter. New code must inspect [`Self::statements`].
    #[cfg(any(test, feature = "testing"))]
    #[doc(hidden)]
    pub fn legacy_records(&self) -> Vec<Record> {
        self.statements
            .first()
            .map(statement_records)
            .unwrap_or_default()
    }
}

#[cfg(any(test, feature = "testing"))]
fn statement_records(result: &StatementResult) -> Vec<Record> {
    let values = match result {
        StatementResult::Rows(values) => values.as_slice(),
        StatementResult::Value(value) => std::slice::from_ref(value),
        StatementResult::None => &[],
    };
    values.iter().filter_map(value_record).collect()
}

#[cfg(any(test, feature = "testing"))]
fn value_record(value: &Value) -> Option<Record> {
    let Value::Object(object) = value else {
        return None;
    };
    let Value::RecordId(id) = object.get("id")? else {
        return None;
    };
    Some(Record {
        id: id.clone(),
        fields: object
            .iter()
            .filter(|(key, _)| key.as_str() != "id")
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    })
}

pub(crate) fn validate_params(params: &Params) -> error::Result<()> {
    for (name, value) in params {
        if !valid_parameter_name(name) {
            return Err(FastDbError::Schema(format!(
                "invalid parameter name {name:?}; names omit `$` and use identifier syntax"
            )));
        }
        validate_bound_value(value, 0)?;
    }
    Ok(())
}

fn valid_parameter_name(name: &str) -> bool {
    if name.is_empty()
        || name.len() > turso_fastdb_parser::ParserLimits::default().max_identifier_bytes
    {
        return false;
    }
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_alphabetic())
        && chars.all(|value| value == '_' || value.is_alphanumeric())
}

fn validate_bound_value(value: &Value, depth: usize) -> error::Result<()> {
    let limits = turso_fastdb_parser::ParserLimits::default();
    if depth > limits.max_nesting_depth {
        return Err(FastDbError::Schema(format!(
            "parameter value nesting exceeds {}",
            limits.max_nesting_depth
        )));
    }
    match value {
        Value::Float(value) if !value.is_finite() => Err(FastDbError::Schema(
            "parameter contains a non-finite float".into(),
        )),
        Value::Array(values) => {
            let max_elements = if depth == 0 {
                65_536
            } else {
                limits.max_collection_elements
            };
            if values.len() > max_elements {
                return Err(FastDbError::Schema(format!(
                    "parameter array exceeds {max_elements} elements"
                )));
            }
            for value in values {
                validate_bound_value(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            if values.len() > limits.max_collection_elements {
                return Err(FastDbError::Schema(format!(
                    "parameter object exceeds {} elements",
                    limits.max_collection_elements
                )));
            }
            for (key, value) in values {
                if key.len() > limits.max_identifier_bytes {
                    return Err(FastDbError::Schema(
                        "parameter object key exceeds the identifier byte limit".into(),
                    ));
                }
                validate_bound_value(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Set(values) => {
            if values.as_slice().len() > limits.max_collection_elements {
                return Err(FastDbError::Schema(format!(
                    "parameter set exceeds {} elements",
                    limits.max_collection_elements
                )));
            }
            for value in values.as_slice() {
                validate_bound_value(value, depth + 1)?;
            }
            Ok(())
        }
        Value::Range(value) => {
            for bound in [value.start(), value.end()] {
                match bound {
                    decode::RangeBound::Unbounded => {}
                    decode::RangeBound::Included(value) | decode::RangeBound::Excluded(value) => {
                        validate_bound_value(value, depth + 1)?;
                    }
                }
            }
            Ok(())
        }
        Value::RecordId(record) => {
            if !valid_parameter_name(&record.table) || record.table.starts_with("__fastdb_") {
                return Err(FastDbError::Schema(
                    "parameter contains an invalid record-ID table".into(),
                ));
            }
            if matches!(&record.id, RecordIdValue::String(value) if value.len() > limits.max_identifier_bytes)
            {
                return Err(FastDbError::Schema(
                    "parameter record-ID component exceeds the byte limit".into(),
                ));
            }
            if matches!(&record.id, RecordIdValue::Uuid(value) if !matches!(value.get_version_num(), 4 | 7))
            {
                return Err(FastDbError::Schema(
                    "parameter record-ID UUID must be UUIDv4 or UUIDv7".into(),
                ));
            }
            Ok(())
        }
        Value::Str(value) if value.len() > 1 << 20 => Err(FastDbError::Schema(
            "parameter string exceeds the value byte limit".into(),
        )),
        Value::Bytes(value) if value.len() > 1 << 20 => Err(FastDbError::Schema(
            "parameter bytes exceed the value byte limit".into(),
        )),
        Value::None
        | Value::Null
        | Value::Bool(_)
        | Value::Integer(_)
        | Value::Float(_)
        | Value::Decimal(_)
        | Value::Str(_)
        | Value::Bytes(_)
        | Value::Duration(_)
        | Value::Datetime(_)
        | Value::Uuid(_)
        | Value::Regex(_)
        | Value::Table(_)
        | Value::File(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_create_select_filter_delete_memory() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();

        let r = conn
            .execute("CREATE person:tracy SET name = 'Tracy';")
            .unwrap();
        let records = r.legacy_records();
        assert_eq!(records.len(), 1);
        let rec = &records[0];
        assert_eq!(rec.id.table, "person");
        assert_eq!(rec.id.id, "tracy");
        assert_eq!(
            rec.fields,
            vec![("name".to_string(), Value::Str("Tracy".to_string()))]
        );

        let r = conn.execute("SELECT * FROM person:tracy;").unwrap();
        let records = r.legacy_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id.id, "tracy");

        let r = conn
            .execute("SELECT * FROM person WHERE name = 'Tracy';")
            .unwrap();
        assert_eq!(r.legacy_records().len(), 1);

        let r = conn.execute("DELETE person:tracy;").unwrap();
        assert!(r.legacy_records().is_empty());

        let r = conn.execute("SELECT * FROM person:tracy;").unwrap();
        assert!(r.legacy_records().is_empty());
    }

    #[test]
    fn smoke_read_of_empty_db_does_not_mutate() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        let r = conn.execute("SELECT * FROM person:tracy;").unwrap();
        assert!(r.legacy_records().is_empty());
        // No catalog should have been created by the read.
        let exists = crate::catalog::catalog_exists(&conn, crate::catalog::META_TABLE).unwrap();
        assert!(!exists, "read of empty db must not create catalog");
    }

    #[test]
    fn smoke_duplicate_create_is_constraint_error() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE person:tracy SET name = 'Tracy';")
            .unwrap();
        let err = conn
            .execute("CREATE person:tracy SET name = 'Other';")
            .unwrap_err();
        assert_eq!(err.category(), ErrorCategory::Constraint);
    }

    #[test]
    fn p7_graph_001_define_relate_and_decode_synthesized_endpoints() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("DEFINE TABLE wrote SCHEMAFULL TYPE RELATION FROM person TO post ENFORCED")
            .unwrap();
        conn.execute("DEFINE FIELD role ON wrote TYPE string")
            .unwrap();
        conn.execute("CREATE person:one CONTENT {}").unwrap();
        conn.execute("CREATE post:two CONTENT {}").unwrap();
        let related = conn
            .execute("RELATE ONLY person:one->wrote->post:two SET role = 'author'")
            .unwrap();
        let StatementResult::Value(Value::Object(edge)) = &related.statements[0] else {
            panic!("expected one edge object")
        };
        assert_eq!(
            edge.get("in"),
            Some(&Value::RecordId(RecordId::new("person", "one")))
        );
        assert_eq!(
            edge.get("out"),
            Some(&Value::RecordId(RecordId::new("post", "two")))
        );
        assert_eq!(edge.get("role"), Some(&Value::Str("author".into())));

        let selected = conn.execute("SELECT * FROM wrote").unwrap();
        let StatementResult::Rows(rows) = &selected.statements[0] else {
            panic!("expected edge rows")
        };
        assert_eq!(rows, &vec![Value::Object(edge.clone())]);

        let traversed = conn
            .execute(
                "SELECT ->wrote->post AS ids, ->wrote->post.* AS docs FROM person:one; \
                 SELECT <-wrote<-person AS authors FROM post:two",
            )
            .unwrap();
        let StatementResult::Rows(forward) = &traversed.statements[0] else {
            panic!("expected forward traversal rows")
        };
        let Value::Object(forward) = &forward[0] else {
            panic!("expected projected object")
        };
        assert_eq!(
            forward.get("ids"),
            Some(&Value::Array(vec![Value::RecordId(RecordId::new(
                "post", "two"
            ))]))
        );
        assert!(matches!(
            forward.get("docs"),
            Some(Value::Array(values)) if values.len() == 1
        ));
        let StatementResult::Rows(reverse) = &traversed.statements[1] else {
            panic!("expected reverse traversal rows")
        };
        let Value::Object(reverse) = &reverse[0] else {
            panic!("expected projected object")
        };
        assert_eq!(
            reverse.get("authors"),
            Some(&Value::Array(vec![Value::RecordId(RecordId::new(
                "person", "one"
            ))]))
        );

        let explained = conn
            .execute(
                "EXPLAIN SELECT ->wrote->post AS forward, <-wrote<-person AS reverse \
                 FROM person:one",
            )
            .unwrap();
        let StatementResult::Rows(plans) = &explained.statements[0] else {
            panic!("expected explain rows")
        };
        let details = plans
            .iter()
            .filter_map(|value| match value {
                Value::Object(value) => match value.get("detail") {
                    Some(Value::Str(value)) => Some(value.as_str()),
                    _ => None,
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        let catalog = conn.coordinator.catalog.read().unwrap();
        let relation = catalog
            .as_ref()
            .and_then(crate::catalog::CatalogState::snapshot)
            .unwrap()
            .tables
            .get("wrote")
            .unwrap();
        for index in relation.indexes.values() {
            assert!(
                details
                    .iter()
                    .any(|detail| detail.contains(&index.physical_name)),
                "missing adjacency plan for {}: {details:?}",
                index.options_json
            );
        }
        drop(catalog);

        let deleted = conn.execute("DELETE person:one").unwrap();
        assert_eq!(deleted.mutation_count, 2, "node and connected edge");
        let remaining = conn.execute("SELECT * FROM wrote").unwrap();
        assert_eq!(remaining.statements, vec![StatementResult::Rows(vec![])]);
    }

    #[test]
    fn p8_fts_001_analyzer_and_index_create_sealed_provider_storage() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE doc:one SET text = 'Rust web programming'")
            .unwrap();
        conn.execute("DEFINE ANALYZER blankish TOKENIZERS blank")
            .unwrap();
        conn.execute(
            "DEFINE INDEX text_idx ON doc FIELDS text FULLTEXT ANALYZER blankish HIGHLIGHTS",
        )
        .unwrap();

        let catalog = conn.coordinator.catalog.read().unwrap();
        let snapshot = catalog
            .as_ref()
            .and_then(crate::catalog::CatalogState::snapshot)
            .unwrap();
        assert_eq!(snapshot.analyzers.len(), 1);
        assert!(snapshot
            .capabilities
            .contains_key(crate::catalog::BUILTIN_FTS_PROVIDER));
        let index = &snapshot.tables["doc"].indexes["text_idx"];
        assert_eq!(index.kind, crate::catalog::IndexKind::Fts);
        assert_eq!(index.physical_columns.len(), 1);
        assert_eq!(
            snapshot
                .hidden_columns
                .values()
                .filter(|column| column.index_id == Some(index.id))
                .count(),
            1
        );
        drop(catalog);

        let result = conn
            .execute(
                "SELECT text, search::score(1) AS score, \
                 search::highlight('<b>', '</b>', 1) AS marked \
                 FROM doc WHERE text @1@ 'Rust web'",
            )
            .unwrap();
        let StatementResult::Rows(rows) = &result.statements[0] else {
            panic!("expected FTS rows");
        };
        assert_eq!(rows.len(), 1);
        let Value::Object(row) = &rows[0] else {
            panic!("expected projected object");
        };
        assert!(matches!(row.get("score"), Some(Value::Float(value)) if *value >= 0.0));
        assert_eq!(
            row.get("marked"),
            Some(&Value::Str(
                "<b>Rust</b> <b>web</b> programming".to_string()
            ))
        );

        conn.execute("UPDATE doc:one SET text = 'database internals'")
            .unwrap();
        let stale = conn
            .execute("SELECT * FROM doc WHERE text @@ 'Rust'")
            .unwrap();
        assert_eq!(stale.statements, vec![StatementResult::Rows(vec![])]);
        let fresh = conn
            .execute("SELECT * FROM doc WHERE text @@ 'database'")
            .unwrap();
        assert!(matches!(&fresh.statements[0], StatementResult::Rows(rows) if rows.len() == 1));

        conn.execute("CREATE INDEX native_idx ON doc USING fts (text) WITH (tokenizer = 'simple')")
            .unwrap();
        let native = conn
            .execute(
                "SELECT fts_score(text, 'database') AS score, \
                 fts_highlight(text, '<i>', '</i>', 'database') AS marked \
                 FROM doc WHERE fts_match(text, 'database')",
            )
            .unwrap();
        assert!(matches!(&native.statements[0], StatementResult::Rows(rows) if rows.len() == 1));
    }

    #[test]
    fn p8_fts_002_explicit_writer_rejects_stale_search_and_rolls_back() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE doc:one SET text = 'committed term'")
            .unwrap();
        conn.execute("DEFINE ANALYZER blankish TOKENIZERS blank")
            .unwrap();
        conn.execute("DEFINE INDEX text_idx ON doc FIELDS text FULLTEXT ANALYZER blankish")
            .unwrap();

        conn.execute("BEGIN").unwrap();
        conn.execute("UPDATE doc:one SET text = 'uncommitted term'")
            .unwrap();
        let error = conn
            .execute("SELECT * FROM doc WHERE text @@ 'uncommitted'")
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Transaction);
        assert!(error.to_string().contains("until commit"));
        conn.execute("CANCEL").unwrap();

        let committed = conn
            .execute("SELECT * FROM doc WHERE text @@ 'committed'")
            .unwrap();
        assert!(matches!(&committed.statements[0], StatementResult::Rows(rows) if rows.len() == 1));
        let absent = conn
            .execute("SELECT * FROM doc WHERE text @@ 'uncommitted'")
            .unwrap();
        assert_eq!(absent.statements, vec![StatementResult::Rows(vec![])]);
    }

    #[test]
    fn p9_vector_001_exact_knn_prefilters_and_projects_distance() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute(
            "CREATE item:a SET embedding = [1, 0], active = true; \
             CREATE item:b SET embedding = [0, 1], active = true; \
             CREATE item:c SET embedding = [0.9, 0.1], active = false; \
             DEFINE FIELD embedding ON item TYPE array<float, 2>",
        )
        .unwrap();

        let catalog = conn.coordinator.catalog.read().unwrap();
        let snapshot = catalog
            .as_ref()
            .and_then(crate::catalog::CatalogState::snapshot)
            .unwrap();
        assert!(snapshot
            .capabilities
            .contains_key(crate::catalog::BUILTIN_VECTOR_PROVIDER));
        let column = snapshot
            .hidden_columns
            .values()
            .find(|column| matches!(column.role, crate::catalog::HiddenColumnRole::Vector64(_)))
            .unwrap();
        assert_eq!(column.dimension, Some(2));
        drop(catalog);

        let result = conn
            .execute(
                "SELECT id, vector::distance::knn() AS distance FROM item \
                 WHERE active = true AND embedding <|2,COSINE|> [1,0]",
            )
            .unwrap();
        let StatementResult::Rows(rows) = &result.statements[0] else {
            panic!("expected KNN rows");
        };
        assert_eq!(rows.len(), 2);
        let ids = rows
            .iter()
            .map(|row| match row {
                Value::Object(row) => row.get("id").cloned().unwrap(),
                _ => panic!("expected projected object"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![
                Value::RecordId(RecordId::new("item", "a")),
                Value::RecordId(RecordId::new("item", "b")),
            ]
        );
        let Value::Object(first) = &rows[0] else {
            panic!("expected object")
        };
        assert!(
            matches!(first.get("distance"), Some(Value::Float(value)) if value.abs() < 1e-6),
            "unexpected first row: {first:?}"
        );

        let functions = conn
            .execute(
                "SELECT vector::distance::euclidean(embedding, [1,1]) AS euclidean, \
                 vector::similarity::cosine(embedding, [1,1]) AS cosine FROM item:a",
            )
            .unwrap();
        let StatementResult::Rows(rows) = &functions.statements[0] else {
            panic!("expected function rows")
        };
        let Value::Object(row) = &rows[0] else {
            panic!("expected function object")
        };
        assert!(
            matches!(row.get("euclidean"), Some(Value::Float(value)) if (*value - 1.0).abs() < 1e-12)
        );
        assert!(
            matches!(row.get("cosine"), Some(Value::Float(value)) if (*value - std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-12)
        );
    }
}
