//! Stable format-1 catalogs, open-time validation, and catalog snapshots.

use crate::connection::Connection;
use crate::error::{FastDbError, Result};
use crate::lower;
use crate::names::{
    physical_index_name, physical_table_name, validate_physical_name, CatalogId, INDEX_NAME_PREFIX,
    TABLE_NAME_PREFIX,
};
use crate::path::{canonical_path, decode_canonical_path};
use crate::schema::{FieldRule, FieldType};
use std::collections::{BTreeMap, BTreeSet};
use turso_core::Value;
use turso_fastdb_parser::TableMode;

pub const META_TABLE: &str = "__fastdb_meta";
pub const TABLES_TABLE: &str = "__fastdb_tables";
pub const FIELDS_TABLE: &str = "__fastdb_fields";
pub const INDEXES_TABLE: &str = "__fastdb_indexes";

pub const FORMAT_VERSION: i64 = 1;
pub const DIALECT_VERSION: i64 = 1;
pub const LAST_MIGRATION: i64 = 1;
pub const EXPRESSION_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub database_id: String,
    pub creation_version: String,
    pub last_migration: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDefinition {
    pub id: CatalogId,
    pub logical_name: String,
    pub physical_name: String,
    pub paths: Vec<Vec<String>>,
    pub path_keys: Vec<String>,
    pub unique: bool,
    pub expression_version: i64,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDefinition {
    pub id: CatalogId,
    pub logical_name: String,
    pub physical_name: String,
    pub mode: TableMode,
    pub definition: Option<String>,
    pub fields: BTreeMap<String, FieldRule>,
    pub indexes: BTreeMap<String, IndexDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    pub metadata: Metadata,
    pub tables: BTreeMap<String, TableDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogState {
    Empty,
    Ready(CatalogSnapshot),
}

impl CatalogState {
    pub fn snapshot(&self) -> Option<&CatalogSnapshot> {
        match self {
            Self::Empty => None,
            Self::Ready(snapshot) => Some(snapshot),
        }
    }
}

#[derive(Debug, Clone)]
struct SchemaObject {
    object_type: String,
    name: String,
    table_name: String,
    sql: Option<String>,
}

pub fn is_reserved_logical_name(name: &str) -> bool {
    name.starts_with("__fastdb_")
}

pub fn allocate_table(
    logical_name: &str,
    mode: TableMode,
    definition: Option<String>,
) -> Result<TableDefinition> {
    if is_reserved_logical_name(logical_name) {
        return Err(FastDbError::Constraint(format!(
            "logical table name {logical_name:?} uses the reserved FastDB prefix"
        )));
    }
    let id = CatalogId::new_random();
    Ok(TableDefinition {
        id,
        logical_name: logical_name.to_string(),
        physical_name: physical_table_name(id),
        mode,
        definition,
        fields: BTreeMap::new(),
        indexes: BTreeMap::new(),
    })
}

pub fn allocate_index(
    logical_name: &str,
    paths: Vec<Vec<String>>,
    unique: bool,
    definition: String,
) -> Result<IndexDefinition> {
    if is_reserved_logical_name(logical_name) {
        return Err(FastDbError::Constraint(format!(
            "logical index name {logical_name:?} uses the reserved FastDB prefix"
        )));
    }
    let path_keys = paths
        .iter()
        .map(canonical_path)
        .collect::<Result<Vec<_>>>()?;
    let id = CatalogId::new_random();
    Ok(IndexDefinition {
        id,
        logical_name: logical_name.to_string(),
        physical_name: physical_index_name(id),
        paths,
        path_keys,
        unique,
        expression_version: EXPRESSION_VERSION,
        definition,
    })
}

pub fn bootstrap(conn: &Connection) -> Result<CatalogSnapshot> {
    conn.exec_bound(lower::catalog_meta_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_tables_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_fields_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_indexes_ddl(), vec![])?;
    let database_id = format!("{:032x}", rand::random::<u128>());
    let creation_version = env!("CARGO_PKG_VERSION").to_string();
    let (statement, bindings) = lower::meta_insert(&database_id, &creation_version, LAST_MIGRATION);
    conn.exec_bound(statement, bindings)?;
    Ok(CatalogSnapshot {
        metadata: Metadata {
            database_id,
            creation_version,
            last_migration: LAST_MIGRATION,
        },
        tables: BTreeMap::new(),
    })
}

pub fn persist_table(conn: &Connection, table: &TableDefinition) -> Result<()> {
    let mode = match table.mode {
        TableMode::Schemaless => "SCHEMALESS",
        TableMode::Schemafull => "SCHEMAFULL",
    };
    let (statement, bindings) = lower::table_insert(
        &table.id.to_hex(),
        &table.logical_name,
        &table.physical_name,
        mode,
        table.definition.as_deref(),
    );
    conn.exec_bound(statement, bindings)
}

pub fn persist_field(conn: &Connection, table: &TableDefinition, field: &FieldRule) -> Result<()> {
    let (statement, bindings) = lower::field_insert(
        &table.id.to_hex(),
        &field.path_key,
        &field.ty.canonical(),
        field.required,
        &field.definition,
    );
    conn.exec_bound(statement, bindings)
}

pub fn persist_index(
    conn: &Connection,
    table: &TableDefinition,
    index: &IndexDefinition,
) -> Result<()> {
    let paths_json = serde_json::to_string(&index.path_keys)
        .map_err(|error| FastDbError::Engine(format!("failed to encode index paths: {error}")))?;
    let (statement, bindings) = lower::index_insert(
        &index.id.to_hex(),
        &table.id.to_hex(),
        &index.logical_name,
        &index.physical_name,
        &paths_json,
        index.unique,
        index.expression_version,
        &index.definition,
    );
    conn.exec_bound(statement, bindings)
}

pub fn load_and_validate(conn: &Connection) -> Result<CatalogState> {
    let schema = read_schema(conn)?;
    let meta = schema
        .iter()
        .find(|object| object.object_type == "table" && object.name == META_TABLE);
    if meta.is_none() {
        if schema.is_empty() {
            return Ok(CatalogState::Empty);
        }
        return Err(FastDbError::format(
            "nonempty database has no FastDB format metadata",
        ));
    }

    // Format is deliberately the first catalog value interpreted.
    let format_rows = conn
        .collect_rows(lower::meta_format_stmt(), vec![])
        .map_err(|_| FastDbError::format("FastDB metadata cannot be read"))?;
    let format = singleton_integer(&format_rows, "format_version")?;
    if format == 0 {
        return Err(FastDbError::format(
            "disposable FastDB format 0 cannot be opened as stable format 1",
        ));
    }
    if format != FORMAT_VERSION {
        return Err(FastDbError::format(format!(
            "unknown FastDB format version {format}; supported version is {FORMAT_VERSION}"
        )));
    }

    validate_catalog_schema(&schema)?;
    let metadata_rows = conn.collect_rows(lower::meta_stmt(), vec![])?;
    let metadata_row = singleton_row(&metadata_rows, "metadata")?;
    let persisted_format = format_integer(&metadata_row[0], "format_version")?;
    if persisted_format != FORMAT_VERSION {
        return Err(FastDbError::format("metadata format changed while opening"));
    }
    let dialect = format_integer(&metadata_row[1], "dialect_version")?;
    if dialect != DIALECT_VERSION {
        return Err(FastDbError::format(format!(
            "unknown FastDB dialect version {dialect}; supported version is {DIALECT_VERSION}"
        )));
    }
    let database_id = format_text(&metadata_row[2], "database_id")?;
    CatalogId::from_hex(&database_id)?;
    let creation_version = format_text(&metadata_row[3], "creation_version")?;
    if creation_version.is_empty() {
        return Err(FastDbError::format("metadata creation version is empty"));
    }
    let mut last_migration = format_integer(&metadata_row[4], "last_migration")?;
    if !(0..=LAST_MIGRATION).contains(&last_migration) {
        return Err(FastDbError::format(format!(
            "unknown FastDB migration level {last_migration}"
        )));
    }
    if last_migration == 0 {
        conn.with_transaction(|| {
            conn.exec_bound(lower::migrate_to_one_stmt(), vec![])?;
            conn.check_failpoint(crate::Failpoint::AfterMigration)
        })?;
        last_migration = 1;
    }

    let mut snapshot = CatalogSnapshot {
        metadata: Metadata {
            database_id,
            creation_version,
            last_migration,
        },
        tables: load_tables(conn)?,
    };
    load_fields(conn, &mut snapshot)?;
    load_indexes(conn, &mut snapshot)?;
    validate_physical_objects(&schema, &snapshot)?;
    Ok(CatalogState::Ready(snapshot))
}

pub fn catalog_exists(conn: &Connection, name: &str) -> Result<bool> {
    Ok(read_schema(conn)?.iter().any(|object| object.name == name))
}

pub fn ensure_catalog_compatible(conn: &Connection) -> Result<()> {
    match load_and_validate(conn)? {
        CatalogState::Ready(_) => Ok(()),
        CatalogState::Empty => Err(FastDbError::format("FastDB catalogs are absent")),
    }
}

fn load_tables(conn: &Connection) -> Result<BTreeMap<String, TableDefinition>> {
    let rows = conn.collect_rows(lower::tables_stmt(), vec![])?;
    let mut tables = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut physical_names = BTreeSet::new();
    for row in rows {
        if row.len() != 5 {
            return Err(FastDbError::format("table catalog row has wrong width"));
        }
        let id_text = format_text(&row[0], "table_id")?;
        let id = CatalogId::from_hex(&id_text)?;
        let logical_name = format_text(&row[1], "logical_name")?;
        if logical_name.is_empty() || is_reserved_logical_name(&logical_name) {
            return Err(FastDbError::format(
                "table catalog has invalid logical name",
            ));
        }
        let physical_name = format_text(&row[2], "physical_name")?;
        validate_physical_name(&physical_name, TABLE_NAME_PREFIX)?;
        if physical_name != physical_table_name(id) {
            return Err(FastDbError::format(
                "table catalog physical name does not match its immutable ID",
            ));
        }
        let mode = match format_text(&row[3], "mode")?.as_str() {
            "SCHEMALESS" => TableMode::Schemaless,
            "SCHEMAFULL" => TableMode::Schemafull,
            _ => return Err(FastDbError::format("table catalog has unknown mode")),
        };
        let definition = format_optional_text(&row[4], "definition")?;
        if !ids.insert(id)
            || !physical_names.insert(physical_name.clone())
            || tables
                .insert(
                    logical_name.clone(),
                    TableDefinition {
                        id,
                        logical_name,
                        physical_name,
                        mode,
                        definition,
                        fields: BTreeMap::new(),
                        indexes: BTreeMap::new(),
                    },
                )
                .is_some()
        {
            return Err(FastDbError::format(
                "table catalog contains duplicate ownership",
            ));
        }
    }
    Ok(tables)
}

fn load_fields(conn: &Connection, snapshot: &mut CatalogSnapshot) -> Result<()> {
    for row in conn.collect_rows(lower::fields_stmt(), vec![])? {
        if row.len() != 5 {
            return Err(FastDbError::format("field catalog row has wrong width"));
        }
        let table_id = CatalogId::from_hex(&format_text(&row[0], "table_id")?)?;
        let table = snapshot
            .tables
            .values_mut()
            .find(|table| table.id == table_id)
            .ok_or_else(|| FastDbError::format("field catalog has an orphan table owner"))?;
        let path_key = format_text(&row[1], "path_key")?;
        let path = decode_canonical_path(&path_key)?;
        let ty = FieldType::parse_canonical(&format_text(&row[2], "type_ast")?)?;
        let required = format_boolean_integer(&row[3], "required")?;
        if required != ty.required() {
            return Err(FastDbError::format(
                "field catalog required flag disagrees with its type AST",
            ));
        }
        let definition = format_text(&row[4], "definition")?;
        let rule = FieldRule {
            path,
            path_key: path_key.clone(),
            ty,
            required,
            definition,
        };
        crate::schema::validate_field_relationships(table.fields.values(), &rule).map_err(
            |_| FastDbError::format("field catalog contains structurally contradictory paths"),
        )?;
        if table.fields.insert(path_key, rule).is_some() {
            return Err(FastDbError::format("field catalog contains duplicates"));
        }
    }
    Ok(())
}

fn load_indexes(conn: &Connection, snapshot: &mut CatalogSnapshot) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut physical_names = BTreeSet::new();
    for row in conn.collect_rows(lower::indexes_stmt(), vec![])? {
        if row.len() != 8 {
            return Err(FastDbError::format("index catalog row has wrong width"));
        }
        let id = CatalogId::from_hex(&format_text(&row[0], "index_id")?)?;
        let table_id = CatalogId::from_hex(&format_text(&row[1], "table_id")?)?;
        let table = snapshot
            .tables
            .values_mut()
            .find(|table| table.id == table_id)
            .ok_or_else(|| FastDbError::format("index catalog has an orphan table owner"))?;
        let logical_name = format_text(&row[2], "logical_name")?;
        if logical_name.is_empty() || is_reserved_logical_name(&logical_name) {
            return Err(FastDbError::format(
                "index catalog has invalid logical name",
            ));
        }
        let physical_name = format_text(&row[3], "physical_name")?;
        validate_physical_name(&physical_name, INDEX_NAME_PREFIX)?;
        if physical_name != physical_index_name(id) {
            return Err(FastDbError::format(
                "index catalog physical name does not match its immutable ID",
            ));
        }
        let path_keys: Vec<String> = serde_json::from_str(&format_text(&row[4], "paths_json")?)
            .map_err(|_| FastDbError::format("index path list is malformed JSON"))?;
        if path_keys.is_empty() {
            return Err(FastDbError::format("index catalog has no paths"));
        }
        let paths = path_keys
            .iter()
            .map(|path| decode_canonical_path(path))
            .collect::<Result<Vec<_>>>()?;
        let unique = format_boolean_integer(&row[5], "unique_flag")?;
        let expression_version = format_integer(&row[6], "expression_version")?;
        if expression_version != EXPRESSION_VERSION {
            return Err(FastDbError::format(format!(
                "unknown index expression version {expression_version}"
            )));
        }
        let definition = format_text(&row[7], "definition")?;
        let index = IndexDefinition {
            id,
            logical_name: logical_name.clone(),
            physical_name: physical_name.clone(),
            paths,
            path_keys,
            unique,
            expression_version,
            definition,
        };
        if !ids.insert(id)
            || !physical_names.insert(physical_name)
            || table.indexes.insert(logical_name, index).is_some()
        {
            return Err(FastDbError::format("index catalog contains duplicates"));
        }
    }
    Ok(())
}

fn read_schema(conn: &Connection) -> Result<Vec<SchemaObject>> {
    conn.collect_rows(lower::sqlite_schema_stmt(), vec![])?
        .into_iter()
        .map(|row| {
            if row.len() != 4 {
                return Err(FastDbError::format("sqlite_schema row has wrong width"));
            }
            Ok(SchemaObject {
                object_type: format_text(&row[0], "sqlite_schema.type")?,
                name: format_text(&row[1], "sqlite_schema.name")?,
                table_name: format_text(&row[2], "sqlite_schema.tbl_name")?,
                sql: format_optional_text(&row[3], "sqlite_schema.sql")?,
            })
        })
        .collect()
}

fn validate_catalog_schema(schema: &[SchemaObject]) -> Result<()> {
    for (name, statement) in [
        (META_TABLE, lower::catalog_meta_ddl()),
        (TABLES_TABLE, lower::catalog_tables_ddl()),
        (FIELDS_TABLE, lower::catalog_fields_ddl()),
        (INDEXES_TABLE, lower::catalog_indexes_ddl()),
    ] {
        require_exact_schema(schema, "table", name, name, &statement.to_string())?;
    }
    Ok(())
}

fn validate_physical_objects(schema: &[SchemaObject], snapshot: &CatalogSnapshot) -> Result<()> {
    let mut expected_reserved = BTreeSet::from([
        META_TABLE.to_string(),
        TABLES_TABLE.to_string(),
        FIELDS_TABLE.to_string(),
        INDEXES_TABLE.to_string(),
    ]);
    for table in snapshot.tables.values() {
        expected_reserved.insert(table.physical_name.clone());
        require_exact_schema(
            schema,
            "table",
            &table.physical_name,
            &table.physical_name,
            &lower::physical_table_ddl(&table.physical_name)?.to_string(),
        )?;
        for index in table.indexes.values() {
            expected_reserved.insert(index.physical_name.clone());
            require_exact_schema(
                schema,
                "index",
                &index.physical_name,
                &table.physical_name,
                &lower::physical_index_ddl(
                    &index.physical_name,
                    &table.physical_name,
                    &index.path_keys,
                    index.unique,
                )?
                .to_string(),
            )?;
        }
    }
    for object in schema {
        if object.name.starts_with("__fastdb_") && !expected_reserved.contains(&object.name) {
            return Err(FastDbError::format(format!(
                "orphan reserved physical object {:?}",
                object.name
            )));
        }
    }
    Ok(())
}

fn require_exact_schema(
    schema: &[SchemaObject],
    object_type: &str,
    name: &str,
    table_name: &str,
    expected_sql: &str,
) -> Result<()> {
    let object = schema
        .iter()
        .find(|object| object.object_type == object_type && object.name == name)
        .ok_or_else(|| {
            FastDbError::format(format!("required physical object {name:?} is missing"))
        })?;
    if object.table_name != table_name || object.sql.as_deref() != Some(expected_sql) {
        return Err(FastDbError::format(format!(
            "physical object {name:?} has an unexpected structure"
        )));
    }
    Ok(())
}

fn singleton_integer(rows: &[Vec<Value>], field: &str) -> Result<i64> {
    let row = singleton_row(rows, field)?;
    let value = row
        .first()
        .ok_or_else(|| FastDbError::format(format!("metadata {field} is absent")))?;
    format_integer(value, field)
}

fn singleton_row<'a>(rows: &'a [Vec<Value>], label: &str) -> Result<&'a Vec<Value>> {
    if rows.len() != 1 {
        return Err(FastDbError::format(format!(
            "FastDB {label} must contain exactly one singleton row"
        )));
    }
    Ok(&rows[0])
}

fn format_text(value: &Value, field: &str) -> Result<String> {
    match value {
        Value::Text(value) => Ok(value.as_str().to_string()),
        _ => Err(FastDbError::format(format!(
            "catalog field {field} is not TEXT"
        ))),
    }
}

fn format_optional_text(value: &Value, field: &str) -> Result<Option<String>> {
    match value {
        Value::Null => Ok(None),
        Value::Text(value) => Ok(Some(value.as_str().to_string())),
        _ => Err(FastDbError::format(format!(
            "catalog field {field} is neither TEXT nor NULL"
        ))),
    }
}

fn format_integer(value: &Value, field: &str) -> Result<i64> {
    match value {
        Value::Numeric(turso_core::Numeric::Integer(value)) => Ok(*value),
        _ => Err(FastDbError::format(format!(
            "catalog field {field} is not INTEGER"
        ))),
    }
}

fn format_boolean_integer(value: &Value, field: &str) -> Result<bool> {
    match format_integer(value, field)? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(FastDbError::format(format!(
            "catalog field {field} is not canonical boolean INTEGER"
        ))),
    }
}
