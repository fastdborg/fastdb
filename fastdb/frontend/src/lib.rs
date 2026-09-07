//! Embedded FastDB frontend over the pinned Turso engine.
mod bundled;
mod catalog;
mod check;
mod expression;
mod functions;
mod guard;
mod integrity;
pub use integrity::{IntegrityLimits, IntegrityReport};
mod interrupt;
mod links;
pub use interrupt::{CancellationToken, InterruptHandle};
mod migration;
pub use migration::{Migration, MigrationReport};
mod path;
mod profile;
pub use profile::{ProfiledQuery, QueryMetrics};
mod select;
mod transaction;
mod transfer;
pub use transfer::TransferFormat;
mod update;
mod value;
mod vectors;
mod write;
use serde::{Deserialize, Serialize};
use std::{num::NonZeroUsize, sync::Arc};
pub use transaction::{BatchExecution, ExecutionReport, TransactionState};
use turso_core::{
    Connection as EngineConnection, Database as EngineDatabase, Value as EngineValue,
};
pub use value::{Document, Key, Parameters, Record, Value};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Syntax(#[from] fastql_parser::Error),
    #[error("engine: {0}")]
    Engine(#[from] turso_core::LimboError),
    #[error("encoding: {0}")]
    Encoding(#[from] serde_json::Error),
    #[error("validation: {0}")]
    Validation(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("already exists: {0}")]
    AlreadyExists(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("parameter missing: {0}")]
    Parameter(String),
    #[error("storage: {0}")]
    Storage(String),
    #[error("resource limit: {0}")]
    Limit(String),
    #[error("expected exactly one row, got {0}")]
    Cardinality(usize),
    #[error("migration {version} at byte {offset}: {source}")]
    Migration {
        version: i64,
        offset: usize,
        source: Box<Error>,
    },
    #[error("rollback failed after {cause}: {rollback}")]
    Rollback { cause: String, rollback: String },
}
impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Syntax(_) => "FDB_SYNTAX",
            Self::Engine(turso_core::LimboError::Interrupt) => "FDB_CANCELLED",
            Self::Engine(turso_core::LimboError::Busy) => "FDB_BUSY",
            Self::Engine(turso_core::LimboError::BusySnapshot) => "FDB_BUSY_SNAPSHOT",
            Self::Engine(
                turso_core::LimboError::Constraint(_)
                | turso_core::LimboError::ForeignKeyConstraint(_)
                | turso_core::LimboError::Raise(..),
            ) => "FDB_CONSTRAINT",
            Self::Engine(_) => "FDB_ENGINE",
            Self::Encoding(_) | Self::Storage(_) => "FDB_STORAGE",
            Self::Validation(_) => "FDB_VALIDATION",
            Self::NotFound(_) => "FDB_NOT_FOUND",
            Self::AlreadyExists(_) => "FDB_ALREADY_EXISTS",
            Self::Unsupported(_) => "FDB_UNSUPPORTED",
            Self::Parameter(_) => "FDB_PARAMETER",
            Self::Limit(_) => "FDB_LIMIT",
            Self::Cardinality(_) => "FDB_CARDINALITY",
            Self::Migration { .. } => "FDB_MIGRATION",
            Self::Rollback { .. } => "FDB_ROLLBACK",
        }
    }
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Serialize)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub affected: i64,
}
impl QueryResult {
    pub fn all(self) -> Vec<Vec<Value>> {
        self.rows
    }
    pub fn first(self) -> Option<Vec<Value>> {
        self.rows.into_iter().next()
    }
    pub fn exactly_one(mut self) -> Result<Vec<Value>> {
        if self.rows.len() != 1 {
            return Err(Error::Cardinality(self.rows.len()));
        }
        Ok(self.rows.remove(0))
    }
    fn documents(docs: Vec<Document>, affected: i64) -> Self {
        Self {
            columns: vec!["document".into()],
            rows: docs.into_iter().map(|d| vec![Value::Object(d)]).collect(),
            affected,
        }
    }
    fn command(affected: i64) -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            affected,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FieldType {
    String,
    Integer,
    Number,
    Boolean,
    Object,
    Array,
    Record(String),
    Vector(usize),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub path: Vec<String>,
    pub kind: FieldType,
    pub required: bool,
    pub nullable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Index {
    name: String,
    path: Vec<String>,
    unique: bool,
    storage: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Collection {
    #[serde(default = "catalog::legacy_version")]
    version: u32,
    name: String,
    storage: String,
    fields: Vec<Field>,
    indexes: Vec<Index>,
}

pub struct Database {
    engine: Arc<EngineDatabase>,
}
impl Database {
    pub fn open(path: &str) -> Result<Self> {
        let io = EngineDatabase::io_for_path(path)?;
        let engine = EngineDatabase::open_file(io, path)?;
        let conn = engine.connect()?;
        conn.execute("CREATE TABLE IF NOT EXISTS __fastdb_catalog (name TEXT PRIMARY KEY, metadata TEXT NOT NULL)")?;
        Ok(Self { engine })
    }
    pub fn connect(&self) -> Result<Connection> {
        let connection = Connection {
            engine: self.engine.connect()?,
        };
        functions::register(&connection)?;
        connection.atomic(|| connection.validate_storage_schema())?;
        Ok(connection)
    }
}
/// Connections expose only checked frontend operations, never raw engine handles.
pub struct Connection {
    engine: Arc<EngineConnection>,
}
fn text(value: &str) -> EngineValue {
    EngineValue::Text(value.to_owned().into())
}
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn canonical(name: &str) -> Result<String> {
    if name.is_empty()
        || name.contains('\0')
        || name.to_ascii_lowercase().starts_with("__fastdb_")
        || name.to_ascii_lowercase().starts_with("sqlite_")
    {
        return Err(Error::Validation("invalid or reserved name".into()));
    }
    Ok(name.to_ascii_lowercase())
}
impl Connection {
    fn prepare(&self, sql: impl AsRef<str>) -> turso_core::Result<turso_core::Statement> {
        parser_stack(|| self.engine.prepare(sql))
    }

    fn run(&self, sql: &str, params: &[EngineValue]) -> Result<Vec<Vec<EngineValue>>> {
        let mut statement = self.prepare(sql)?;
        for (i, value) in params.iter().enumerate() {
            statement.bind_at(
                NonZeroUsize::new(i + 1).expect("one-based parameter"),
                value.clone(),
            )?;
        }
        collect_rows(&mut statement)
    }
    fn atomic<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        self.run("SAVEPOINT __fastdb_statement", &[])?;
        let result = f().and_then(|v| {
            self.run("RELEASE __fastdb_statement", &[])?;
            Ok(v)
        });
        match result {
            Ok(value) => Ok(value),
            Err(cause) => {
                if self.engine.get_auto_commit() {
                    return Err(cause);
                }
                let rollback = self
                    .run("ROLLBACK TO __fastdb_statement", &[])
                    .and_then(|_| self.run("RELEASE __fastdb_statement", &[]));
                match rollback {
                    Ok(_) => Err(cause),
                    Err(rollback) => Err(Error::Rollback {
                        cause: cause.to_string(),
                        rollback: rollback.to_string(),
                    }),
                }
            }
        }
    }
    fn catalog(&self, name: &str) -> Result<Collection> {
        let name = canonical(name)?;
        let rows = self.run(
            "SELECT metadata FROM __fastdb_catalog WHERE name = ?1",
            &[text(&name)],
        )?;
        match rows.first().and_then(|r| r.first()) {
            Some(EngineValue::Text(t)) => catalog::decode(t.as_str(), &name),
            None => Err(Error::NotFound(name)),
            _ => Err(Error::Storage("invalid collection metadata".into())),
        }
    }
    fn save_catalog(&self, collection: &Collection) -> Result<()> {
        let mut collection = collection.clone();
        collection.version = catalog::version();
        self.run(
            "UPDATE __fastdb_catalog SET metadata = ?1 WHERE name = ?2",
            &[
                text(&serde_json::to_string(&collection)?),
                text(&collection.name),
            ],
        )?;
        Ok(())
    }
    pub fn create_collection(&self, name: &str, if_not_exists: bool) -> Result<()> {
        let name = canonical(name)?;
        self.atomic(|| {
            let exists = !self.run("SELECT name FROM sqlite_schema WHERE name = ?1 COLLATE NOCASE UNION ALL SELECT name FROM __fastdb_catalog WHERE name = ?1", &[text(&name)])?.is_empty();
            if exists { return if if_not_exists { Ok(()) } else { Err(Error::AlreadyExists(name.clone())) }; }
            // Names are collision-free UTF-8 hex, independent of user quoting.
            let storage = format!("__fastdb_c_{}", name.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>());
            self.run(&format!("CREATE TABLE {} (id BLOB PRIMARY KEY, doc BLOB NOT NULL)", quote(&storage)), &[])?;
            let collection = Collection { version:catalog::version(), name: name.clone(), storage, fields: Vec::new(), indexes: Vec::new() };
            self.run("INSERT INTO __fastdb_catalog VALUES (?1, ?2)", &[text(&name), text(&serde_json::to_string(&collection)?)])?; Ok(())
        })
    }
    fn documents(&self, collection: &Collection) -> Result<Vec<Document>> {
        self.run(
            &format!("SELECT doc FROM {}", quote(&collection.storage)),
            &[],
        )?
        .into_iter()
        .map(|row| decode_document(&row[0]))
        .collect()
    }
    pub fn get(&self, record: &Record) -> Result<Option<Document>> {
        let collection = self.catalog(&record.table)?;
        self.get_in(&collection, record)
    }
    fn get_in(&self, c: &Collection, record: &Record) -> Result<Option<Document>> {
        let id = normalized_id(record, &c.name)?;
        self.run(
            &format!("SELECT doc FROM {} WHERE id = ?1", quote(&c.storage)),
            &[EngineValue::Blob(Value::Record(id).encode()?)],
        )?
        .first()
        .map(|r| decode_document(&r[0]))
        .transpose()
    }
    pub fn define_field(&self, table: &str, field: Field, overwrite: bool) -> Result<()> {
        validate_path(&field.path)?;
        if let FieldType::Vector(dims) = field.kind {
            vectors::validate_dimension(dims)?;
        }
        if field.path[0] == "id" {
            return Err(Error::Validation("id has a fixed type".into()));
        }
        if let FieldType::Record(target) = &field.kind {
            canonical(target)?;
        }
        self.check_definition(&field)?;
        self.atomic(|| {
            let mut c = self.catalog(table)?;
            let existing = c.fields.iter().position(|f| f.path == field.path);
            match (existing, overwrite) {
                (Some(i), true) => {
                    c.fields[i] = field.clone();
                }
                (None, false) => c.fields.push(field.clone()),
                (Some(_), false) => return Err(Error::AlreadyExists(field.path.join("."))),
                (None, true) => return Err(Error::NotFound(field.path.join("."))),
            }
            for index in &c.indexes {
                catalog::compatible_index(&c, &index.path)?;
            }
            for doc in self.documents(&c)? {
                self.validate_candidate(&c, &doc)?;
            }
            self.save_catalog(&c)
        })
    }
    pub fn create_index(
        &self,
        table: &str,
        name: &str,
        path: Vec<String>,
        unique: bool,
    ) -> Result<()> {
        self.create_index_if(table, name, path, unique, false)
    }
    pub fn create_index_if(
        &self,
        table: &str,
        name: &str,
        path: Vec<String>,
        unique: bool,
        if_not_exists: bool,
    ) -> Result<()> {
        validate_path(&path)?;
        let name = canonical(name)?;
        self.atomic(|| {
            let mut c = self.catalog(table)?;
            let existing = self.run(
                "SELECT type FROM sqlite_schema WHERE name=?1 COLLATE NOCASE",
                &[text(&name)],
            )?;
            if !existing.is_empty() {
                if if_not_exists
                    && matches!(&existing[0][0],EngineValue::Text(t) if t.as_str()=="index")
                {
                    return Ok(());
                }
                return Err(Error::AlreadyExists(name.clone()));
            }
            if !self
                .run(
                    "SELECT name FROM __fastdb_catalog WHERE name=?1",
                    &[text(&name)],
                )?
                .is_empty()
            {
                return Err(Error::AlreadyExists(name.clone()));
            }
            catalog::compatible_index(&c, &path)?;
            let storage = format!(
                "__fastdb_i_{}",
                name.as_bytes()
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
            // No affinity on key: numeric equality and binary text semantics match SQL expressions.
            self.run(
                &format!("CREATE TABLE {} (key, id BLOB NOT NULL)", quote(&storage)),
                &[],
            )?;
            self.run(
                &format!(
                    "CREATE {} INDEX {} ON {} (key)",
                    if unique { "UNIQUE" } else { "" },
                    quote(&name),
                    quote(&storage)
                ),
                &[],
            )?;
            let index = Index {
                name: name.clone(),
                path: path.clone(),
                unique,
                storage,
            };
            for doc in self.documents(&c)? {
                self.insert_index(&index, &doc)?;
            }
            c.indexes.push(index);
            self.save_catalog(&c)
        })
    }
    fn insert_index(&self, index: &Index, doc: &Document) -> Result<()> {
        let key = index_scalar(path_value(doc, &index.path)?.unwrap_or(&Value::Null))?;
        let id = doc
            .get("id")
            .ok_or_else(|| Error::Storage("document has no id".into()))?;
        self.run(
            &format!("INSERT INTO {} VALUES (?1, ?2)", quote(&index.storage)),
            &[key, EngineValue::Blob(id.encode()?)],
        )?;
        Ok(())
    }
    pub fn insert(&self, table: &str, mut doc: Document) -> Result<Document> {
        self.atomic(|| {
            let c = self.catalog(table)?;
            if !doc.contains_key("id") {
                let rows = self.run("SELECT uuid7_str()", &[])?;
                let Some(EngineValue::Text(key)) = rows.first().and_then(|r| r.first()) else {
                    return Err(Error::Storage("UUID generator returned non-text".into()));
                };
                doc.insert(
                    "id".into(),
                    Value::Record(Record {
                        table: c.name.clone(),
                        key: Key::String(key.as_str().into()),
                    }),
                );
            }
            normalize_document_id(&c, &mut doc)?;
            self.validate_candidate(&c, &doc)?;
            self.run(
                &format!("INSERT INTO {} VALUES (?1, ?2)", quote(&c.storage)),
                &[
                    EngineValue::Blob(doc["id"].encode()?),
                    EngineValue::Blob(Value::Object(doc.clone()).encode()?),
                ],
            )?;
            for index in &c.indexes {
                self.insert_index(index, &doc)?;
            }
            Ok(doc.clone())
        })
    }
    pub fn patch(&self, record: &Record, patch: Document) -> Result<Option<Document>> {
        if patch.contains_key("id") {
            return Err(Error::Validation("id is immutable".into()));
        }
        self.atomic(|| {
            let c = self.catalog(&record.table)?;
            let Some(mut doc) = self.get_in(&c, record)? else {
                return Ok(None);
            };
            doc.extend(patch.clone());
            self.replace_document(&c, &doc)?;
            Ok(Some(doc))
        })
    }
    fn replace_document(&self, c: &Collection, doc: &Document) -> Result<()> {
        self.validate_candidate(c, doc)?;
        let id = EngineValue::Blob(doc["id"].encode()?);
        self.run(
            &format!("UPDATE {} SET doc = ?1 WHERE id = ?2", quote(&c.storage)),
            &[
                EngineValue::Blob(Value::Object(doc.clone()).encode()?),
                id.clone(),
            ],
        )?;
        for index in &c.indexes {
            self.run(
                &format!("DELETE FROM {} WHERE id = ?1", quote(&index.storage)),
                std::slice::from_ref(&id),
            )?;
            self.insert_index(index, doc)?;
        }
        Ok(())
    }
    pub fn delete(&self, record: &Record) -> Result<Option<Document>> {
        self.atomic(|| {
            let c = self.catalog(&record.table)?;
            let Some(doc) = self.get_in(&c, record)? else {
                return Ok(None);
            };
            let id = EngineValue::Blob(doc["id"].encode()?);
            for index in &c.indexes {
                self.run(
                    &format!("DELETE FROM {} WHERE id = ?1", quote(&index.storage)),
                    std::slice::from_ref(&id),
                )?;
            }
            self.run(
                &format!("DELETE FROM {} WHERE id = ?1", quote(&c.storage)),
                &[id],
            )?;
            Ok(Some(doc))
        })
    }
    pub fn lookup_index(&self, table: &str, name: &str, value: &Value) -> Result<Vec<Document>> {
        self.atomic(|| {
            let c = self.catalog(table)?;
            let index = c
                .indexes
                .iter()
                .find(|i| i.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| Error::NotFound(name.into()))?;
            let rows = self.run(
                &format!(
                    "SELECT c.doc FROM {} AS i JOIN {} AS c ON c.id = i.id WHERE i.key = ?1",
                    quote(&index.storage),
                    quote(&c.storage)
                ),
                &[index_scalar(value)?],
            )?;
            rows.iter().map(|r| decode_document(&r[0])).collect()
        })
    }
    pub fn execute(&self, sql: &str, params: &Parameters) -> Result<QueryResult> {
        parser_stack(|| self.execute_inner(sql, params))
    }
    fn execute_inner(&self, sql: &str, params: &Parameters) -> Result<QueryResult> {
        use fastql_parser::Statement;
        let object = |expr| match self.evaluate(expr, params, None)? {
            Value::Object(doc) => Ok(doc),
            _ => Err(Error::Validation("expected a typed document object".into())),
        };
        match fastql_parser::parse(sql)? {
            Statement::Upsert {
                table,
                target,
                value,
                returning,
            } => self.atomic(|| {
                let fastql_parser::Expr::Object(mut fields) = value else {
                    return Err(Error::Validation("UPSERT requires object".into()));
                };
                let record = if let Some(target) = target {
                    if fields.contains_key("id") {
                        return Err(Error::Validation("target UPSERT body must omit id".into()));
                    }
                    target
                } else {
                    let expr = fields.remove("id").ok_or_else(|| {
                        Error::Validation("UPSERT requires an explicit id".into())
                    })?;
                    let Value::Record(id) = self.evaluate(expr, params, None)? else {
                        return Err(Error::Validation("UPSERT id must be typed record".into()));
                    };
                    id
                };
                let record = normalized_id(&record, &canonical(&table)?)?;
                let before = self.get(&record)?.unwrap_or_default();
                let Value::Object(mut doc) =
                    self.evaluate(fastql_parser::Expr::Object(fields), params, Some(&before))?
                else {
                    unreachable!("object expression");
                };
                doc.insert("id".into(), Value::Record(record));
                let doc = self.upsert(&table, doc)?;
                self.object_returning(&table, returning, vec![doc], params)
            }),
            Statement::PatchWhere {
                table,
                value,
                predicate,
                returning,
            } => self.atomic(|| {
                let suffix = predicate.map_or(String::new(), |p| format!(" WHERE {p}"));
                let query = format!("SELECT * FROM {}{suffix}", quote(&table));
                let rows = self
                    .collection_select_subset(&query, params)?
                    .ok_or_else(|| Error::NotFound(table.clone()))?
                    .rows;
                let mut candidates = Vec::new();
                for row in rows {
                    let Some(Value::Object(before)) = row.into_iter().next() else {
                        return Err(Error::Storage("expected candidate document".into()));
                    };
                    let Value::Object(patch) =
                        self.evaluate(value.clone(), params, Some(&before))?
                    else {
                        return Err(Error::Validation("expected object patch".into()));
                    };
                    let Some(Value::Record(id)) = before.get("id") else {
                        return Err(Error::Storage("expected typed id".into()));
                    };
                    candidates.push((id.clone(), patch));
                }
                let mut docs = Vec::new();
                for (id, patch) in candidates {
                    docs.push(
                        self.patch(&id, patch)?
                            .ok_or_else(|| Error::Storage("candidate disappeared".into()))?,
                    );
                }
                self.object_returning(&table, returning, docs, params)
            }),
            Statement::RemoveField { table, path } => {
                self.remove_field(&table, &path)?;
                Ok(QueryResult::command(0))
            }
            Statement::Info { scope, name } => self.info(&scope, name.as_deref()),

            Statement::DefineField {
                table,
                path,
                kind,
                target,
                required,
                nullable,
                check,
                overwrite,
            } => {
                let kind = match (kind.as_str(), target) {
                    ("string", None) => FieldType::String,
                    ("integer", None) => FieldType::Integer,
                    ("number", None) => FieldType::Number,
                    ("boolean", None) => FieldType::Boolean,
                    ("object", None) => FieldType::Object,
                    ("array", None) => FieldType::Array,
                    ("record", Some(target)) => FieldType::Record(canonical(&target)?),
                    ("vector", Some(target)) => FieldType::Vector(
                        target
                            .parse()
                            .map_err(|_| Error::Validation("invalid vector dimension".into()))?,
                    ),
                    _ => return Err(Error::Unsupported("field type is not implemented".into())),
                };
                self.define_field(
                    &table,
                    Field {
                        path,
                        kind,
                        required,
                        nullable,
                        check,
                    },
                    overwrite,
                )?;
                Ok(QueryResult::command(0))
            }
            Statement::CreateIndex {
                if_not_exists,
                table,
                name,
                path,
                unique,
                sql,
            } => match self.catalog(&table) {
                Ok(_) => {
                    self.create_index_if(&table, &name, path, unique, if_not_exists)?;
                    Ok(QueryResult::command(0))
                }
                Err(Error::NotFound(_)) => self.sql(&sql, params),
                Err(error) => Err(error),
            },
            Statement::CreateCollection {
                name,
                if_not_exists,
            } => {
                self.create_collection(&name, if_not_exists)?;
                Ok(QueryResult::command(0))
            }
            Statement::Insert {
                table,
                value,
                returning,
            } => self.atomic(|| {
                let doc = self.insert(&table, object(value)?)?;
                self.object_returning(&table, returning, vec![doc], params)
            }),
            Statement::SelectRecord(record) => Ok(QueryResult::documents(
                self.get(&record)?.into_iter().collect(),
                0,
            )),
            Statement::Patch {
                target,
                value,
                returning,
            } => self.atomic(|| {
                let Some(before) = self.get(&target)? else {
                    return self.object_returning(&target.table, returning, Vec::new(), params);
                };
                let Value::Object(patch) = self.evaluate(value, params, Some(&before))? else {
                    return Err(Error::Validation("expected object patch".into()));
                };
                let doc = self.patch(&target, patch)?;
                self.object_returning(&target.table, returning, doc.into_iter().collect(), params)
            }),
            Statement::Delete { target, returning } => self.atomic(|| {
                let doc = self.delete(&target)?;
                self.object_returning(&target.table, returning, doc.into_iter().collect(), params)
            }),
            Statement::Sql(sql) => self.sql(&sql, params),
        }
    }
    pub(crate) fn guard_native_sql(&self, sql: &str) -> Result<()> {
        let tokens = guard::native_tokens(sql)?;
        let collections = self.run("SELECT name FROM __fastdb_catalog", &[])?;
        for token in &tokens {
            // Reject unresolved managed references; value literals in covered AST
            // contexts have been removed from this guard-only token stream.
            if matches!(
                token.kind,
                fastql_parser::Kind::Word
                    | fastql_parser::Kind::Identifier
                    | fastql_parser::Kind::String
            ) {
                let name = token.text.to_ascii_lowercase();
                if name.starts_with("__fastdb_")
                    || name == "writable_schema"
                    || collections
                        .iter()
                        .any(|r| matches!(&r[0], EngineValue::Text(t) if t.as_str() == name))
                {
                    return Err(Error::Unsupported(
                        "managed collection SQL requires logical lowering".into(),
                    ));
                }
            }
        }
        Ok(())
    }
    fn sql(&self, sql: &str, params: &Parameters) -> Result<QueryResult> {
        if let Some(result) = self.catalog_statement(sql)? {
            return Ok(result);
        }
        if let Some(result) = self.collection_write(sql, params)? {
            return Ok(result);
        }
        if let Some(result) = self.collection_select(sql, params)? {
            return Ok(result);
        }
        self.native_profiled(sql, params)
            .map(|profile| profile.result)
    }
    fn native_profiled(&self, sql: &str, params: &Parameters) -> Result<ProfiledQuery> {
        self.guard_native_sql(sql)?;
        let mut stmt = self.prepare(sql)?;
        if !fastql_parser::tokenize(&sql[stmt.tail_offset()..])?.is_empty() {
            return Err(Error::Unsupported("execute accepts one statement".into()));
        }
        for (name, value) in params {
            let index = bind_index(&stmt, name).ok_or_else(|| Error::Parameter(name.clone()))?;
            stmt.bind_at(index, scalar(value)?)?;
        }
        let columns = (0..stmt.num_columns())
            .map(|i| stmt.get_column_name(i).into_owned())
            .collect();
        let rows = collect_rows(&mut stmt)?
            .into_iter()
            .map(|row| row.into_iter().map(from_engine).collect())
            .collect();
        Ok(ProfiledQuery {
            result: QueryResult {
                columns,
                rows,
                affected: stmt.n_change(),
            },
            metrics: QueryMetrics::from_statement(&stmt),
        })
    }
}
fn bind_index(statement: &turso_core::Statement, name: &str) -> Option<NonZeroUsize> {
    statement.parameter_index(name).or_else(|| {
        let index = NonZeroUsize::new(name.strip_prefix('?')?.parse().ok()?)?;
        statement.parameters().has_index(index).then_some(index)
    })
}
fn from_engine(value: EngineValue) -> Value {
    use turso_core::Numeric;
    match value {
        EngineValue::Null => Value::Null,
        EngineValue::Text(t) => Value::String(t.as_str().into()),
        EngineValue::Blob(b) => Value::Binary(b),
        EngineValue::Numeric(Numeric::Integer(i)) => Value::Integer(i),
        EngineValue::Numeric(Numeric::Float(n)) => Value::Number(n.into()),
    }
}
fn scalar(value: &Value) -> Result<EngineValue> {
    value.validate()?;
    Ok(match value {
        Value::Null => EngineValue::Null,
        Value::Boolean(v) => EngineValue::Numeric(turso_core::Numeric::Integer(i64::from(*v))),
        Value::Integer(v) => EngineValue::Numeric(turso_core::Numeric::Integer(*v)),
        Value::Number(v) => EngineValue::Numeric(turso_core::Numeric::Float(
            turso_core::NonNan::new(*v).expect("validated finite number"),
        )),
        Value::String(v) => text(v),
        Value::Binary(v) => EngineValue::Blob(v.clone()),
        Value::Record(r) => {
            let mut r = r.clone();
            r.table = canonical(&r.table)?;
            EngineValue::Blob(Value::Record(r).encode()?)
        }
        _ => {
            return Err(Error::Validation(
                "expected scalar or record index value".into(),
            ))
        }
    })
}
fn index_scalar(value: &Value) -> Result<EngineValue> {
    // Binary data and record IDs share the engine BLOB storage class, but must
    // never alias each other even when user bytes imitate our record encoding.
    if matches!(value, Value::Binary(_)) {
        return Ok(EngineValue::Blob(value.encode()?));
    }
    scalar(value)
}
fn decode_document(value: &EngineValue) -> Result<Document> {
    if let EngineValue::Blob(bytes) = value {
        if let Value::Object(doc) = Value::decode(bytes)? {
            return Ok(doc);
        }
    }
    Err(Error::Storage("invalid stored document".into()))
}
fn normalized_id(record: &Record, table: &str) -> Result<Record> {
    value::validate_record(record)?;
    if canonical(&record.table)? != table {
        return Err(Error::Validation(
            "id targets a different collection".into(),
        ));
    }
    Ok(Record {
        table: table.into(),
        key: record.key.clone(),
    })
}
fn normalize_document_id(c: &Collection, doc: &mut Document) -> Result<()> {
    let Some(Value::Record(id)) = doc.get("id") else {
        return Err(Error::Validation("id must be a typed record".into()));
    };
    let id = normalized_id(id, &c.name)?;
    doc.insert("id".into(), Value::Record(id));
    Ok(())
}
fn validate_path(path: &[String]) -> Result<()> {
    if path.is_empty() || path.iter().any(String::is_empty) {
        return Err(Error::Validation("empty field path".into()));
    }
    Ok(())
}
fn path_value<'a>(doc: &'a Document, path: &[String]) -> Result<Option<&'a Value>> {
    let mut fields = doc;
    for (i, part) in path.iter().enumerate() {
        let value = fields.get(part);
        if i == path.len() - 1 || value.is_none() {
            return Ok(value);
        }
        match value {
            Some(Value::Object(object)) => fields = object,
            _ => return Err(Error::Validation(format!("non-object parent at {part}"))),
        }
    }
    Err(Error::Validation("empty field path".into()))
}
fn validate_document(c: &Collection, doc: &Document) -> Result<()> {
    Value::Object(doc.clone()).validate()?;
    for f in &c.fields {
        // A required nested child applies only when its parent object exists.
        if f.path.len() > 1 && path_value(doc, &f.path[..f.path.len() - 1])?.is_none() {
            continue;
        }
        let value = path_value(doc, &f.path)?;
        match value {
            None if !f.required => continue,
            Some(Value::Null) if f.nullable => continue,
            Some(v)
                if matches!(
                    (&f.kind, v),
                    (FieldType::String, Value::String(_))
                        | (FieldType::Integer, Value::Integer(_))
                        | (FieldType::Number, Value::Integer(_) | Value::Number(_))
                        | (FieldType::Boolean, Value::Boolean(_))
                        | (FieldType::Object, Value::Object(_))
                        | (FieldType::Array, Value::Array(_))
                ) =>
            {
                continue
            }
            Some(Value::Vector(bytes)) if matches!(&f.kind, FieldType::Vector(dims) if vectors::dimensions(bytes)? == *dims) => {
                continue
            }
            Some(Value::Record(r)) if matches!(&f.kind, FieldType::Record(target) if target.eq_ignore_ascii_case(&r.table)) => {
                continue
            }
            _ => {
                return Err(Error::Validation(format!(
                    "field {} failed {:?} validation",
                    f.path.join("."),
                    f.kind
                )))
            }
        }
    }
    for index in &c.indexes {
        scalar(path_value(doc, &index.path)?.unwrap_or(&Value::Null))?;
    }
    Ok(())
}

// The pinned run_collect_rows helper conflates Interrupt with Busy. Its callback
// counterpart preserves those errors, so all frontend reads use this adapter.
fn collect_rows(statement: &mut turso_core::Statement) -> Result<Vec<Vec<EngineValue>>> {
    let mut rows = Vec::new();
    // Execution can reprepare after a concurrent schema change.
    parser_stack(|| {
        statement.run_with_row_callback(|row| {
            rows.push(row.get_values().cloned().collect());
            Ok(())
        })
    })?;
    Ok(rows)
}

// The pinned parser's recursion guard can require more than a caller's native
// stack in debug builds. Keep parsing/preparation on this thread while giving
// that guard room to return an error. Nested calls reuse the auxiliary stack.
fn parser_stack<T>(work: impl FnOnce() -> T) -> T {
    stacker::maybe_grow(16 * 1024 * 1024, 32 * 1024 * 1024, work)
}

#[cfg(test)]
mod stack_tests {
    use super::*;
    #[test]
    fn deep_native_statements_reprepare_on_a_small_caller_stack() {
        std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(|| {
                let db = Database::open(":memory:").unwrap();
                let c = db.connect().unwrap();
                c.run("CREATE TABLE native(v INTEGER)", &[]).unwrap();
                c.run("INSERT INTO native VALUES (1)", &[]).unwrap();
                for (i, expr) in [
                    format!("{}1", "NOT ".repeat(80)),
                    format!(
                        "{}1{}",
                        "CASE WHEN 1 THEN ".repeat(80),
                        " ELSE 0 END".repeat(80)
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let mut statement = c.prepare(format!("SELECT {expr} FROM native")).unwrap();
                    c.run(&format!("CREATE TABLE change_{i}(v INTEGER)"), &[])
                        .unwrap();
                    assert_eq!(
                        collect_rows(&mut statement).unwrap(),
                        vec![vec![EngineValue::Numeric(turso_core::Numeric::Integer(1))]]
                    );
                    assert!(statement.metrics().reprepares > 0);
                }
            })
            .unwrap()
            .join()
            .unwrap();
    }
}
