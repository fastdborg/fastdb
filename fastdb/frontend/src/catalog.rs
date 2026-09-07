//! Transactional lifecycle of logical collections, fields, and managed indexes.
use crate::{
    canonical, quote, text, Collection, Connection, Document, Error, FieldType, Parameters,
    QueryResult, Result, Value,
};
use turso_core::Value as EngineValue;
use turso_parser::ast::{Cmd, Stmt};

// Compare our fixed generated DDL lexically, avoiding recursive parsing of
// externally modified schema SQL. Whitespace, keyword case and trailing
// semicolons are immaterial; constraints and identifier quoting remain exact.
fn schema_tokens(sql: &str) -> Result<Vec<(fastql_parser::Kind, String)>> {
    use fastql_parser::Kind;
    let mut tokens = fastql_parser::tokenize(sql)
        .map_err(|e| Error::Storage(format!("invalid managed schema SQL: {e}")))?
        .into_iter()
        .filter(|t| !(t.kind == Kind::Symbol && t.text == ";"))
        .map(|t| {
            let text = if t.kind == Kind::Word {
                t.text.to_ascii_lowercase()
            } else {
                t.text
            };
            (t.kind, text)
        })
        .collect::<Vec<_>>();
    // SQLite may omit the creation-time IF NOT EXISTS flag in stored DDL.
    if tokens.len() >= 5
        && tokens[0].1 == "create"
        && tokens[1].1 == "table"
        && tokens[2..5]
            .iter()
            .map(|t| t.1.as_str())
            .eq(["if", "not", "exists"])
    {
        tokens.drain(2..5);
    }
    Ok(tokens)
}

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
    pub(crate) fn validate_storage_schema(&self) -> Result<()> {
        self.schema_object("__fastdb_catalog", "table", "__fastdb_catalog", "CREATE TABLE IF NOT EXISTS __fastdb_catalog (name TEXT PRIMARY KEY, metadata TEXT NOT NULL)")?;
        self.managed_dependencies("__fastdb_catalog", None)?;
        let mut storage = std::collections::BTreeSet::new();
        for c in self.collections()? {
            storage.insert(c.storage.clone());
            self.schema_object(
                &c.storage,
                "table",
                &c.storage,
                &format!(
                    "CREATE TABLE {} (id BLOB PRIMARY KEY, doc BLOB NOT NULL)",
                    quote(&c.storage)
                ),
            )?;
            self.managed_dependencies(&c.storage, None)?;
            for index in &c.indexes {
                if !storage.insert(index.storage.clone()) {
                    return Err(Error::Storage(format!(
                        "managed index storage {} has multiple owners",
                        index.storage
                    )));
                }
                self.schema_object(
                    &index.storage,
                    "table",
                    &index.storage,
                    &format!(
                        "CREATE TABLE {} (\"key\", id BLOB NOT NULL)",
                        quote(&index.storage)
                    ),
                )?;
                self.schema_object(
                    &index.name,
                    "index",
                    &index.storage,
                    &format!(
                        "CREATE {} INDEX {} ON {} (\"key\")",
                        if index.unique { "UNIQUE" } else { "" },
                        quote(&index.name),
                        quote(&index.storage)
                    ),
                )?;
                self.managed_dependencies(&index.storage, Some(&index.name))?;
            }
        }
        for row in self.run("SELECT name FROM sqlite_schema", &[])? {
            let [EngineValue::Text(name)] = row.as_slice() else {
                return Err(Error::Storage("invalid schema name".into()));
            };
            let canonical = name.as_str().to_ascii_lowercase();
            if (canonical.starts_with("__fastdb_c_") || canonical.starts_with("__fastdb_i_"))
                && !storage.contains(name.as_str())
            {
                return Err(Error::Storage(format!(
                    "orphan managed storage object {}",
                    name.as_str()
                )));
            }
        }
        Ok(())
    }
    pub(super) fn schema_object(
        &self,
        name: &str,
        kind: &str,
        table: &str,
        sql: &str,
    ) -> Result<()> {
        let rows = self.run(
            "SELECT type,tbl_name,sql FROM sqlite_schema WHERE name=?1",
            &[text(name)],
        )?;
        let valid = match rows.as_slice() {
            [row] => match row.as_slice() {
                [EngineValue::Text(actual_kind), EngineValue::Text(actual_table), EngineValue::Text(actual_sql)] => {
                    actual_kind.as_str() == kind
                        && actual_table.as_str() == table
                        && schema_tokens(actual_sql.as_str())? == schema_tokens(sql)?
                }
                _ => false,
            },
            _ => false,
        };
        if !valid {
            return Err(Error::Storage(format!(
                "missing or incompatible managed schema object {name}"
            )));
        }
        Ok(())
    }
    pub(super) fn managed_dependencies(
        &self,
        table: &str,
        expected_index: Option<&str>,
    ) -> Result<()> {
        for row in self.run("SELECT name,type FROM sqlite_schema WHERE tbl_name=?1 AND (type='trigger' OR (type='index' AND sql IS NOT NULL))", &[text(table)])? {
            if !matches!(row.as_slice(), [EngineValue::Text(name), EngineValue::Text(kind)] if kind.as_str()=="index" && Some(name.as_str())==expected_index) {
                return Err(Error::Storage(format!("unexpected dependency on managed table {table}")));
            }
        }
        Ok(())
    }
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

#[cfg(test)]
mod storage_schema_tests {
    use super::*;
    #[test]
    fn new_connections_reject_missing_or_modified_managed_schema() {
        for mutation in [
            "DROP TABLE __fastdb_c_646f6373",
            "ALTER TABLE __fastdb_c_646f6373 ADD COLUMN extra TEXT",
            "DROP TABLE __fastdb_i_76616c75655f696478",
            "DROP INDEX value_idx",
            "CREATE UNIQUE INDEX extra ON __fastdb_c_646f6373(doc)",
            "CREATE TRIGGER extra AFTER INSERT ON __fastdb_c_646f6373 BEGIN INSERT INTO sentinel VALUES (100); END",
            "ALTER TABLE __fastdb_catalog ADD COLUMN extra TEXT",
            "CREATE TRIGGER extra AFTER UPDATE ON __fastdb_catalog BEGIN INSERT INTO sentinel VALUES (100); END",
        ] {
            let db=crate::Database::open(":memory:").unwrap();
            let c=db.connect().unwrap();
            for sql in ["CREATE TABLE docs", "CREATE UNIQUE INDEX value_idx ON docs(value)", "INSERT INTO docs {value:1}", "CREATE TABLE sentinel(value INTEGER)", "INSERT INTO sentinel VALUES (99)"] {
                c.execute(sql,&Parameters::new()).unwrap();
            }
            drop(db.connect().expect("valid managed schema"));
            c.run(mutation,&[]).unwrap();
            let error=db.connect().err().expect("reject modified schema");
            assert_eq!(error.code(),"FDB_STORAGE","{mutation}: {error}");
            assert_eq!(c.execute("SELECT * FROM sentinel",&Parameters::new()).unwrap().rows,vec![vec![Value::Integer(99)]]);
        }
    }
    #[test]
    fn changed_unique_index_definition_is_rejected() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        c.execute("CREATE TABLE docs", &Parameters::new()).unwrap();
        c.execute(
            "CREATE UNIQUE INDEX value_idx ON docs(value)",
            &Parameters::new(),
        )
        .unwrap();
        c.run("DROP INDEX value_idx", &[]).unwrap();
        c.run(
            "CREATE INDEX value_idx ON __fastdb_i_76616c75655f696478(key)",
            &[],
        )
        .unwrap();
        assert_eq!(db.connect().err().unwrap().code(), "FDB_STORAGE");
        c.run("DROP INDEX value_idx", &[]).unwrap();
        c.run(
            "CREATE UNIQUE INDEX \"value_idx\" ON \"__fastdb_i_76616c75655f696478\" (key)",
            &[],
        )
        .unwrap();
        drop(db.connect().expect("restored schema"));
    }
}

#[cfg(test)]
mod orphan_schema_tests {
    use super::*;
    #[test]
    fn managed_index_storage_cannot_be_shared_by_collections() {
        let db = crate::Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        for sql in [
            "CREATE TABLE docs",
            "CREATE TABLE other",
            "CREATE INDEX value_idx ON docs(value)",
            "INSERT INTO docs {value:1}",
        ] {
            c.execute(sql, &Parameters::new()).unwrap();
        }
        let mut other = c.catalog("other").unwrap();
        other.indexes = c.catalog("docs").unwrap().indexes;
        c.save_catalog(&other).unwrap();
        let error = db
            .connect()
            .err()
            .expect("shared index storage must reject connection");
        assert_eq!(error.code(), "FDB_STORAGE");
        assert!(error.to_string().contains("multiple owners"), "{error}");
        other.indexes.clear();
        c.save_catalog(&other).unwrap();
        drop(db.connect().expect("restored ownership"));
    }

    #[test]
    fn removed_metadata_and_orphan_reserved_objects_fail_connection_validation() {
        for mutation in [
            "DELETE FROM __fastdb_catalog WHERE name='docs'",
            "UPDATE __fastdb_catalog SET metadata=json_set(metadata,'$.indexes',json('[]')) WHERE name='docs'",
            "CREATE TABLE __fastdb_c_orphan(value)",
            "CREATE VIEW __fastdb_i_orphan AS SELECT 1",
            "CREATE TABLE __FASTDB_C_ORPHAN(value)",
        ] {
            let db=crate::Database::open(":memory:").unwrap();
            let c=db.connect().unwrap();
            for sql in ["CREATE TABLE docs", "CREATE INDEX value_idx ON docs(value)", "INSERT INTO docs {value:1}"] {
                c.execute(sql,&Parameters::new()).unwrap();
            }
            c.run(mutation,&[]).unwrap();
            let error=db.connect().err().expect("orphan must reject connection");
            assert_eq!(error.code(),"FDB_STORAGE","{error}");
            assert!(error.to_string().contains("orphan managed storage"),"{mutation}: {error}");
            assert_eq!(c.run("SELECT count(*) FROM __fastdb_c_646f6373",&[]).unwrap()[0][0], crate::scalar(&Value::Integer(1)).unwrap());
        }
    }
}
