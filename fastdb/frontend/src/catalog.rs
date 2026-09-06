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
    for field in &collection.fields {
        if let FieldType::Vector(dims) = field.kind {
            crate::vectors::validate_dimension(dims)?;
        }
    }
    if collection.version == 1 && collection.fields.iter().any(|f| f.check.is_some()) {
        return Err(Error::Storage(
            "CHECK metadata requires catalog version 2".into(),
        ));
    }
    Ok(())
}
pub(crate) fn decode(metadata: &str, name: &str) -> Result<Collection> {
    let c: Collection = serde_json::from_str(metadata)
        .map_err(|e| Error::Storage(format!("invalid collection metadata: {e}")))?;
    let validate = || -> Result<()> {
        validate_version(&c)?;
        if canonical(name)? != name || c.name != name {
            return Err(Error::Storage("collection metadata name mismatch".into()));
        }
        let storage = |prefix: &str, name: &str| {
            format!(
                "{prefix}{}",
                name.as_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            )
        };
        if c.storage != storage("__fastdb_c_", name) {
            return Err(Error::Storage(
                "collection storage identity mismatch".into(),
            ));
        }
        let mut paths = std::collections::BTreeSet::new();
        for field in &c.fields {
            crate::validate_path(&field.path)?;
            if let Some(check) = &field.check {
                fastql_parser::validate_delimiter_depth(&fastql_parser::tokenize(check)?)?;
            }
            if field.path[0] == "id" || !paths.insert(&field.path) {
                return Err(Error::Storage(
                    "invalid or duplicate field definition".into(),
                ));
            }
            if let FieldType::Record(target) = &field.kind {
                // The Rust field API has always accepted case-insensitive
                // record targets; validate the name without rejecting that form.
                canonical(target)?;
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for index in &c.indexes {
            crate::validate_path(&index.path)?;
            if canonical(&index.name)? != index.name
                || !names.insert(&index.name)
                || index.storage != storage("__fastdb_i_", &index.name)
            {
                return Err(Error::Storage("invalid index identity".into()));
            }
            compatible_index(&c, &index.path)?;
        }
        Ok(())
    };
    validate().map_err(|e| Error::Storage(format!("invalid collection metadata: {e}")))?;
    Ok(c)
}
pub(crate) fn compatible_index(c: &Collection, path: &[String]) -> Result<()> {
    for field in &c.fields {
        let incompatible = if field.path == path {
            matches!(
                field.kind,
                FieldType::Object | FieldType::Array | FieldType::Vector(_)
            )
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
        self.run(
            "SELECT name,metadata FROM __fastdb_catalog ORDER BY name",
            &[],
        )?
        .into_iter()
        .map(|row| match row.as_slice() {
            [EngineValue::Text(name), EngineValue::Text(metadata)] => {
                decode(metadata.as_str(), name.as_str())
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
                    let views = self.run("SELECT name FROM sqlite_schema WHERE type='view' AND substr(lower(name),1,9) != '__fastdb_' AND substr(lower(name),1,7) != 'sqlite_' ORDER BY name", &[])?.into_iter().map(|row| object([("name", crate::from_engine(row[0].clone())), ("model", Value::String("relational".into()))])).collect();
                    object([("catalog_version",Value::Integer(i64::from(version()))),("tables",Value::Array(entries)),("views",Value::Array(views))])
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
                                    FieldType::Vector(dims) => format!("vector<{dims}>"),
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
                let schema = self.run("SELECT type FROM sqlite_schema WHERE type IN ('table','view') AND name=?1 COLLATE NOCASE", &[text(&name)])?;
                let Some(row) = schema.first() else {
                    return Err(Error::NotFound(name));
                };
                Ok(object([
                    ("name", Value::String(name.clone())),
                    ("model", Value::String("relational".into())),
                    ("kind", crate::from_engine(row[0].clone())),
                    ("columns", self.pragma_info("table_xinfo", &name)?),
                    ("indexes", self.pragma_info("index_list", &name)?),
                ]))
            }
            Err(e) => Err(e),
        }
    }
    fn pragma_info(&self, pragma: &str, name: &str) -> Result<Value> {
        let result = self.sql(
            &format!("PRAGMA {pragma}({})", quote(name)),
            &Parameters::new(),
        )?;
        Ok(Value::Array(
            result
                .rows
                .into_iter()
                .map(|row| object_row(&result.columns, row))
                .collect(),
        ))
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
            ("name", Value::String(name.clone())),
            ("model", Value::String("relational".into())),
            ("table", crate::from_engine(row[0].clone())),
            ("sql", crate::from_engine(row[1].clone())),
            ("columns", self.pragma_info("index_xinfo", &name)?),
        ]))
    }
}
fn object_row(names: &[String], row: Vec<Value>) -> Value {
    Value::Object(names.iter().cloned().zip(row).collect())
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    #[test]
    fn malformed_metadata_cannot_redirect_storage_or_weaken_definitions() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        let q = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
        q("CREATE TABLE docs");
        q("DEFINE FIELD value ON docs TYPE integer REQUIRED");
        q("CREATE UNIQUE INDEX value_idx ON docs(value)");
        q("INSERT INTO docs {value:1}");
        q("CREATE TABLE sentinel(value INTEGER)");
        q("INSERT INTO sentinel VALUES (99)");
        let original = serde_json::to_value(c.catalog("docs").unwrap()).unwrap();
        let mutations: Vec<(&str, serde_json::Value)> = vec![
            ("/name", "other".into()),
            ("/storage", "sentinel".into()),
            ("/storage", "__fastdb_c_6f74686572".into()),
            ("/fields/0/path", serde_json::json!([])),
            ("/fields/0/path", serde_json::json!(["id"])),
            (
                "/fields/0/kind",
                serde_json::json!({"Record":"__fastdb_bad"}),
            ),
            ("/fields/0/kind", serde_json::json!({"Vector":0})),
            ("/fields/0/kind", "Object".into()),
            (
                "/fields",
                serde_json::json!([original["fields"][0], original["fields"][0]]),
            ),
            ("/indexes/0/name", "VALUE_IDX".into()),
            ("/indexes/0/storage", "sentinel".into()),
            ("/indexes/0/path", serde_json::json!([""])),
            (
                "/indexes",
                serde_json::json!([original["indexes"][0], original["indexes"][0]]),
            ),
        ];
        let mut invalid = vec!["{".to_owned(), "null".into()];
        for (path, value) in mutations {
            let mut metadata = original.clone();
            *metadata.pointer_mut(path).unwrap() = value;
            invalid.push(metadata.to_string());
        }
        let mut nested = original.clone();
        nested["fields"][0]["check"] =
            format!("{}value>0{}", "(".repeat(128), ")".repeat(128)).into();
        invalid.push(nested.to_string());
        for metadata in invalid {
            c.run(
                "UPDATE __fastdb_catalog SET metadata=?1 WHERE name='docs'",
                &[text(&metadata)],
            )
            .unwrap();
            assert_eq!(
                c.catalog("docs").unwrap_err().code(),
                "FDB_STORAGE",
                "{metadata}"
            );
            assert_eq!(
                c.collections().unwrap_err().code(),
                "FDB_STORAGE",
                "{metadata}"
            );
            assert_eq!(
                c.execute("DROP TABLE docs", &Parameters::new())
                    .unwrap_err()
                    .code(),
                "FDB_STORAGE",
                "{metadata}"
            );
            assert_eq!(
                c.run("SELECT value FROM sentinel", &[]).unwrap(),
                vec![vec![EngineValue::from_i64(99)]]
            );
        }
        c.run(
            "UPDATE __fastdb_catalog SET metadata=?1 WHERE name='docs'",
            &[text(&original.to_string())],
        )
        .unwrap();
        assert_eq!(
            c.lookup_index("docs", "value_idx", &Value::Integer(1))
                .unwrap()
                .len(),
            1
        );
        assert!(c
            .execute("INSERT INTO docs {value:1.5}", &Parameters::new())
            .is_err());
        let mut legacy = original;
        legacy.as_object_mut().unwrap().remove("version");
        c.run(
            "UPDATE __fastdb_catalog SET metadata=?1 WHERE name='docs'",
            &[text(&legacy.to_string())],
        )
        .unwrap();
        assert_eq!(c.catalog("docs").unwrap().version, 1);
        assert_eq!(
            c.lookup_index("docs", "value_idx", &Value::Integer(1))
                .unwrap()
                .len(),
            1
        );
        let reference = |target: &str| crate::Field {
            path: vec!["parent".into()],
            kind: FieldType::Record(target.into()),
            required: false,
            nullable: true,
            check: None,
        };
        assert_eq!(
            c.define_field("docs", reference("__fastdb_bad"), false)
                .unwrap_err()
                .code(),
            "FDB_VALIDATION"
        );
        c.define_field("docs", reference("Docs"), false).unwrap();
        assert!(
            matches!(&c.catalog("docs").unwrap().fields[1].kind,FieldType::Record(target) if target=="Docs")
        );
    }
}
