//! Transactional lifecycle of logical collections, fields, and managed indexes.
use crate::{
    canonical, quote, text, Collection, Connection, Document, Error, FieldType, Parameters,
    QueryResult, Result, Value,
};
use turso_core::Value as EngineValue;
use turso_parser::ast::{Cmd, Stmt};

pub(crate) const fn version() -> u32 {
    2
}
pub(crate) const fn legacy_version() -> u32 {
    1
}
pub(crate) fn validate_version(collection: &Collection) -> Result<()> {
    if !matches!(collection.version, 1 | 2) {
        return Err(Error::Storage(format!(
            "unsupported collection metadata version {}",
            collection.version
        )));
    }
    if collection.version == 1 && collection.fields.iter().any(|f| f.check.is_some()) {
        return Err(Error::Storage(
            "CHECK metadata requires catalog version 2".into(),
        ));
    }
    Ok(())
}
pub(crate) fn compatible_index(c: &Collection, path: &[String]) -> Result<()> {
    for field in &c.fields {
        let incompatible = if field.path == path {
            matches!(field.kind, FieldType::Object | FieldType::Array)
        } else if path.starts_with(&field.path) {
            !matches!(field.kind, FieldType::Object)
        } else {
            field.path.starts_with(path)
        };
        if incompatible {
            return Err(Error::Validation(format!(
                "field {} is incompatible with index path {}",
                field.path.join("."),
                path.join(".")
            )));
        }
    }
    Ok(())
}
fn strings(values: &[String]) -> Value {
    Value::Array(values.iter().cloned().map(Value::String).collect())
}
fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
}
fn index_info(index: &crate::Index, table: &str) -> Value {
    object([
        ("name", Value::String(index.name.clone())),
        ("table", Value::String(table.into())),
        ("model", Value::String("document".into())),
        ("path", strings(&index.path)),
        ("unique", Value::Boolean(index.unique)),
    ])
}
impl Connection {
    fn collections(&self) -> Result<Vec<Collection>> {
        self.run("SELECT metadata FROM __fastdb_catalog ORDER BY name", &[])?
            .into_iter()
            .map(|row| match &row[0] {
                EngineValue::Text(t) => {
                    let collection: Collection = serde_json::from_str(t.as_str())?;
                    validate_version(&collection)?;
                    Ok(collection)
                }
                _ => Err(Error::Storage("invalid catalog entry".into())),
            })
            .collect()
    }
    pub fn upsert(&self, table: &str, mut doc: Document) -> Result<Document> {
        self.atomic(|| {
            let c = self.catalog(table)?;
            crate::normalize_document_id(&c, &mut doc)?;
            let Value::Record(id) = &doc["id"] else {
                unreachable!("normalized id");
            };
            if let Some(mut existing) = self.get_in(&c, id)? {
                existing.extend(doc.clone());
                self.replace_document(&c, &existing)?;
                Ok(existing)
            } else {
                self.insert(&c.name, doc.clone())
            }
        })
    }
    pub fn remove_field(&self, table: &str, path: &[String]) -> Result<()> {
        crate::validate_path(path)?;
        self.atomic(|| {
            let mut c = self.catalog(table)?;
            let position = c
                .fields
                .iter()
                .position(|f| f.path == path)
                .ok_or_else(|| Error::NotFound(path.join(".")))?;
            c.fields.remove(position);
            self.save_catalog(&c)
        })
    }
    pub fn drop_collection(&self, name: &str, if_exists: bool) -> Result<()> {
        self.atomic(|| {
            let c = match self.catalog(name) {
                Ok(c) => c,
                Err(Error::NotFound(_)) if if_exists => return Ok(()),
                Err(e) => return Err(e),
            };
            for index in &c.indexes {
                self.run(&format!("DROP TABLE {}", quote(&index.storage)), &[])?;
            }
            self.run(&format!("DROP TABLE {}", quote(&c.storage)), &[])?;
            self.run(
                "DELETE FROM __fastdb_catalog WHERE name=?1",
                &[text(&c.name)],
            )?;
            Ok(())
        })
    }
    /// Returns false when the name is not a managed index, allowing SQL dispatch
    /// to preserve the baseline operation for an ordinary relational index.
    pub fn drop_managed_index(&self, name: &str) -> Result<bool> {
        let name = canonical(name)?;
        self.atomic(|| {
            for mut c in self.collections()? {
                if let Some(position) = c.indexes.iter().position(|i| i.name == name) {
                    let index = c.indexes.remove(position);
                    self.run(&format!("DROP TABLE {}", quote(&index.storage)), &[])?;
                    self.save_catalog(&c)?;
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }
    pub(crate) fn catalog_statement(&self, sql: &str) -> Result<Option<QueryResult>> {
        let Ok(Cmd::Stmt(stmt)) = crate::select::parsed(sql) else {
            return Ok(None);
        };
        match stmt {
            Stmt::DropTable {
                tbl_name,
                if_exists,
            } => {
                if tbl_name
                    .db_name
                    .as_ref()
                    .is_some_and(|n| !n.as_str().eq_ignore_ascii_case("main"))
                {
                    return Ok(None);
                }
                match self.catalog(tbl_name.name.as_str()) {
                    Ok(_) => {
                        self.drop_collection(tbl_name.name.as_str(), if_exists)?;
                        Ok(Some(QueryResult::command(0)))
                    }
                    Err(Error::NotFound(_)) => Ok(None),
                    Err(e) => Err(e),
                }
            }
            Stmt::DropIndex { idx_name, .. } => {
                if idx_name
                    .db_name
                    .as_ref()
                    .is_some_and(|n| !n.as_str().eq_ignore_ascii_case("main"))
                {
                    return Ok(None);
                }
                if self.drop_managed_index(idx_name.name.as_str())? {
                    Ok(Some(QueryResult::command(0)))
                } else {
                    Ok(None)
                }
            }
            Stmt::CreateTable {
                if_not_exists,
                temporary,
                tbl_name,
                ..
            } if !temporary
                && tbl_name
                    .db_name
                    .as_ref()
                    .is_none_or(|n| n.as_str().eq_ignore_ascii_case("main")) =>
            {
                match self.catalog(tbl_name.name.as_str()) {
                    Ok(_) if if_not_exists => Ok(Some(QueryResult::command(0))),
                    Ok(_) => Err(Error::AlreadyExists(tbl_name.name.as_str().into())),
                    Err(Error::NotFound(_)) => Ok(None),
                    Err(e) => Err(e),
                }
            }
            _ => Ok(None),
        }
    }
    pub fn info(&self, scope: &str, name: Option<&str>) -> Result<QueryResult> {
        self.atomic(|| {
            let info=match (scope,name) {
                ("db",None)=>{
                    let mut entries=Vec::new();
                    for c in self.collections()? {entries.push(object([("name",Value::String(c.name)),("model",Value::String("document".into()))]));}
                    for row in self.run("SELECT name FROM sqlite_schema WHERE type='table' AND substr(lower(name),1,9) != '__fastdb_' AND substr(lower(name),1,7) != 'sqlite_' ORDER BY name",&[])? {
                        if let EngineValue::Text(t)=&row[0] {entries.push(object([("name",Value::String(t.as_str().into())),("model",Value::String("relational".into()))]));}
                    }
                    object([("catalog_version",Value::Integer(i64::from(version()))),("tables",Value::Array(entries))])
                }
                ("table",Some(name))=>self.table_info(name)?,
                ("index",Some(name))=>self.logical_index_info(name)?,
                _=>return Err(Error::Validation("INFO requires DB, TABLE name, or INDEX name".into())),
            };
            Ok(QueryResult {columns:vec!["info".into()],rows:vec![vec![info]],affected:0})
        })
    }
    fn table_info(&self, name: &str) -> Result<Value> {
        let name = canonical(name)?;
        match self.catalog(&name) {
            Ok(c) => {
                let fields = c
                    .fields
                    .iter()
                    .map(|f| {
                        object([
                            ("path", strings(&f.path)),
                            (
                                "type",
                                Value::String(match &f.kind {
                                    FieldType::String => "string".into(),
                                    FieldType::Integer => "integer".into(),
                                    FieldType::Number => "number".into(),
                                    FieldType::Boolean => "boolean".into(),
                                    FieldType::Object => "object".into(),
                                    FieldType::Array => "array".into(),
                                    FieldType::Record(target) => format!("record<{target}>"),
                                }),
                            ),
                            ("required", Value::Boolean(f.required)),
                            ("nullable", Value::Boolean(f.nullable)),
                            (
                                "check",
                                f.check
                                    .as_ref()
                                    .map_or(Value::Null, |s| Value::String(s.clone())),
                            ),
                        ])
                    })
                    .collect();
                let indexes = c.indexes.iter().map(|i| index_info(i, &c.name)).collect();
                Ok(object([
                    ("name", Value::String(c.name)),
                    ("model", Value::String("document".into())),
                    ("fields", Value::Array(fields)),
                    ("indexes", Value::Array(indexes)),
                    (
                        "capabilities",
                        strings(&[
                            "typed_ids".into(),
                            "field_validation".into(),
                            "scalar_indexes".into(),
                            "transactions".into(),
                        ]),
                    ),
                ]))
            }
            Err(Error::NotFound(_)) => {
                if self.run("SELECT name FROM sqlite_schema WHERE type IN ('table','view') AND name=?1 COLLATE NOCASE",&[text(&name)])?.is_empty() {return Err(Error::NotFound(name));}
                let columns = self.sql(
                    &format!("PRAGMA table_info({})", quote(&name)),
                    &Parameters::new(),
                )?;
                Ok(object([
                    ("name", Value::String(name)),
                    ("model", Value::String("relational".into())),
                    (
                        "columns",
                        Value::Array(
                            columns
                                .rows
                                .into_iter()
                                .map(|row| object_row(&columns.columns, row))
                                .collect(),
                        ),
                    ),
                ]))
            }
            Err(e) => Err(e),
        }
    }
    fn logical_index_info(&self, name: &str) -> Result<Value> {
        let name = canonical(name)?;
        for c in self.collections()? {
            if let Some(index) = c.indexes.iter().find(|i| i.name == name) {
                return Ok(index_info(index, &c.name));
            }
        }
        let rows = self.run(
            "SELECT tbl_name, sql FROM sqlite_schema WHERE type='index' AND name=?1 COLLATE NOCASE",
            &[text(&name)],
        )?;
        let Some(row) = rows.first() else {
            return Err(Error::NotFound(name));
        };
        Ok(object([
            ("name", Value::String(name)),
            ("model", Value::String("relational".into())),
            ("table", crate::from_engine(row[0].clone())),
            ("sql", crate::from_engine(row[1].clone())),
        ]))
    }
}
fn object_row(names: &[String], row: Vec<Value>) -> Value {
    Value::Object(names.iter().cloned().zip(row).collect())
}
