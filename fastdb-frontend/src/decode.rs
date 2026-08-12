//! Phase 0 result model: typed record ids and decoded records.
//!
//! The decoder synthesizes the typed [`RecordId`] from the catalog (table
//! name) and the canonical `rid`; `id` is never stored inside `doc`.

use crate::error::{FastDbError, Result};

/// Parse a canonical JSON object (produced by `json(doc)` on the engine) into
/// ordered Phase 0 fields. Phase 0 supports only string field values; any
/// other JSON value is an explicit error.
pub fn parse_doc(json: &str) -> Result<Vec<(String, Value)>> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| FastDbError::Engine(format!("stored doc is not valid JSON: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| FastDbError::Engine("stored doc is not a JSON object".to_string()))?;
    let mut fields = Vec::with_capacity(obj.len());
    for (k, v) in obj {
        match v {
            serde_json::Value::String(s) => fields.push((k.clone(), Value::Str(s.clone()))),
            _ => {
                return Err(FastDbError::Engine(
                    "Phase 0 supports only string field values in doc".to_string(),
                ))
            }
        }
    }
    Ok(fields)
}

/// A typed record id. Phase 0 supports a single bare-string id component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordId {
    pub table: String,
    pub id: String,
}

impl RecordId {
    pub fn new(table: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            id: id.into(),
        }
    }
}

impl std::fmt::Display for RecordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.table, self.id)
    }
}

/// A decoded FastDB value. Phase 0 supports only strings in user content;
/// richer value types arrive in later phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Str(String),
}

/// A decoded record: a typed id plus ordered user-content fields.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        self
    }
}
