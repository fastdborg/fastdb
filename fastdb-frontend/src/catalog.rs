//! Format-2 catalogs, transactional format-1 migration, and validation.

use crate::connection::Connection;
use crate::error::{FastDbError, Result};
use crate::lower;
use crate::names::{
    physical_hidden_column_name, physical_index_name, physical_table_name, validate_physical_name,
    CatalogId, HIDDEN_COLUMN_NAME_PREFIX, INDEX_NAME_PREFIX, TABLE_NAME_PREFIX,
};
use crate::path::{canonical_path, decode_canonical_path};
use crate::schema::{FieldRule, FieldType};
use std::collections::{BTreeMap, BTreeSet};
use turso_core::Value;
use turso_fastdb_parser::TableMode;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FtsIndexOptions {
    pub surface: String,
    pub tokenizer: String,
    pub weights: Vec<f64>,
    pub analyzer: Option<String>,
    pub highlights: bool,
}

impl FtsIndexOptions {
    pub fn canonical_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|error| FastDbError::Engine(format!("failed to encode FTS options: {error}")))
    }

    pub fn parse_canonical(value: &str) -> Result<Self> {
        let options: Self = serde_json::from_str(value)
            .map_err(|_| FastDbError::format("FTS index options are malformed"))?;
        if options.canonical_json()? != value {
            return Err(FastDbError::format(
                "FTS index options are not canonically encoded",
            ));
        }
        Ok(options)
    }
}

pub const META_TABLE: &str = "__fastdb_meta";
pub const TABLES_TABLE: &str = "__fastdb_tables";
pub const FIELDS_TABLE: &str = "__fastdb_fields";
pub const INDEXES_TABLE: &str = "__fastdb_indexes";
pub const ANALYZERS_TABLE: &str = "__fastdb_analyzers";
pub const HIDDEN_COLUMNS_TABLE: &str = "__fastdb_hidden_columns";
pub const CAPABILITIES_TABLE: &str = "__fastdb_capabilities";

pub const FORMAT_VERSION: i64 = 2;
pub const DIALECT_VERSION: i64 = 1;
pub const LAST_MIGRATION: i64 = 2;
pub const EXPRESSION_VERSION: i64 = 1;
pub const BUILTIN_BTREE_PROVIDER_VERSION: i64 = 1;
pub const BUILTIN_BTREE_ENCODING_VERSION: i64 = 1;
pub const BUILTIN_GRAPH_PROVIDER_VERSION: i64 = 1;
pub const BUILTIN_GRAPH_ENCODING_VERSION: i64 = 1;
pub const BUILTIN_GRAPH_PROVIDER: &str = "BUILTIN_GRAPH";
pub const BUILTIN_FTS_PROVIDER_VERSION: i64 = 1;
pub const BUILTIN_FTS_ENCODING_VERSION: i64 = 1;
pub const BUILTIN_FTS_PROVIDER: &str = "BUILTIN_FTS";
pub const BUILTIN_FTS_ANALYZER_PROVIDER: &str = "BUILTIN_FTS_SURREAL_BLANK";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableKind {
    Normal,
    Relation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexKind {
    Btree,
    GraphAdjacency,
    Fts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    BuiltinBtree,
    BuiltinGraph,
    BuiltinFts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderState {
    Ready,
    RebuildRequired,
}

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
    pub kind: IndexKind,
    pub provider: Provider,
    pub provider_version: i64,
    pub options_json: String,
    pub state: ProviderState,
    pub encoding_version: i64,
    /// Opaque provider input columns. Empty for ordinary B-tree indexes.
    pub physical_columns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDefinition {
    pub id: CatalogId,
    pub logical_name: String,
    pub physical_name: String,
    pub mode: TableMode,
    pub definition: Option<String>,
    pub kind: TableKind,
    pub relation_in_table_id: Option<CatalogId>,
    pub relation_out_table_id: Option<CatalogId>,
    pub relation_enforced: bool,
    pub fields: BTreeMap<String, FieldRule>,
    pub indexes: BTreeMap<String, IndexDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSnapshot {
    pub metadata: Metadata,
    pub tables: BTreeMap<String, TableDefinition>,
    pub analyzers: BTreeMap<String, AnalyzerDefinition>,
    pub hidden_columns: BTreeMap<CatalogId, HiddenColumnDefinition>,
    pub capabilities: BTreeMap<String, CapabilityRequirement>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzerDefinition {
    pub id: CatalogId,
    pub logical_name: String,
    pub provider: String,
    pub provider_version: i64,
    pub options_json: String,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HiddenColumnDefinition {
    pub id: CatalogId,
    pub table_id: CatalogId,
    pub index_id: Option<CatalogId>,
    pub field_path_key: Option<String>,
    pub physical_name: String,
    pub provider: Provider,
    pub role: HiddenColumnRole,
    pub physical_encoding: String,
    pub dimension: Option<i64>,
    pub options_json: String,
    pub state: ProviderState,
    pub provider_version: i64,
    pub encoding_version: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HiddenColumnRole {
    Graph(GraphColumnRole),
    FtsText(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GraphColumnRole {
    InTable,
    InRid,
    OutTable,
    OutRid,
}

impl GraphColumnRole {
    pub const ALL: [Self; 4] = [Self::InTable, Self::InRid, Self::OutTable, Self::OutRid];

    pub const fn options_json(self) -> &'static str {
        match self {
            Self::InTable => "{\"role\":\"in_table\"}",
            Self::InRid => "{\"role\":\"in_rid\"}",
            Self::OutTable => "{\"role\":\"out_table\"}",
            Self::OutRid => "{\"role\":\"out_rid\"}",
        }
    }

    pub const fn encoding(self) -> &'static str {
        match self {
            Self::InTable | Self::OutTable => "GRAPH_TABLE_ID",
            Self::InRid | Self::OutRid => "GRAPH_RID",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapabilityRequirement {
    pub provider: String,
    pub min_provider_version: i64,
    pub min_encoding_version: i64,
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
        kind: TableKind::Normal,
        relation_in_table_id: None,
        relation_out_table_id: None,
        relation_enforced: false,
        fields: BTreeMap::new(),
        indexes: BTreeMap::new(),
    })
}

pub fn allocate_relation_table(
    logical_name: &str,
    mode: TableMode,
    definition: Option<String>,
    relation_in_table_id: Option<CatalogId>,
    relation_out_table_id: Option<CatalogId>,
    relation_enforced: bool,
) -> Result<TableDefinition> {
    let mut table = allocate_table(logical_name, mode, definition)?;
    table.kind = TableKind::Relation;
    table.relation_in_table_id = relation_in_table_id;
    table.relation_out_table_id = relation_out_table_id;
    table.relation_enforced = relation_enforced;
    Ok(table)
}

pub fn allocate_graph_hidden_columns(table_id: CatalogId) -> Vec<HiddenColumnDefinition> {
    GraphColumnRole::ALL
        .into_iter()
        .map(|role| {
            let id = CatalogId::new_random();
            HiddenColumnDefinition {
                id,
                table_id,
                index_id: None,
                field_path_key: None,
                physical_name: physical_hidden_column_name(id),
                provider: Provider::BuiltinGraph,
                role: HiddenColumnRole::Graph(role),
                physical_encoding: role.encoding().to_string(),
                dimension: None,
                options_json: role.options_json().to_string(),
                state: ProviderState::Ready,
                provider_version: BUILTIN_GRAPH_PROVIDER_VERSION,
                encoding_version: BUILTIN_GRAPH_ENCODING_VERSION,
            }
        })
        .collect()
}

pub fn allocate_graph_index(
    logical_name: &str,
    columns: &[&HiddenColumnDefinition],
    direction: &str,
) -> Result<IndexDefinition> {
    let paths = columns
        .iter()
        .map(|column| vec![column.physical_name.clone()])
        .collect::<Vec<_>>();
    let mut index = allocate_index(logical_name, paths, false, String::new())?;
    index.kind = IndexKind::GraphAdjacency;
    index.provider = Provider::BuiltinGraph;
    index.provider_version = BUILTIN_GRAPH_PROVIDER_VERSION;
    index.options_json = format!("{{\"direction\":\"{direction}\"}}");
    index.encoding_version = BUILTIN_GRAPH_ENCODING_VERSION;
    index.physical_columns = columns
        .iter()
        .map(|column| column.physical_name.clone())
        .collect();
    Ok(index)
}

pub fn allocate_analyzer(logical_name: &str, definition: String) -> Result<AnalyzerDefinition> {
    if is_reserved_logical_name(logical_name) {
        return Err(FastDbError::Constraint(format!(
            "logical analyzer name {logical_name:?} uses the reserved FastDB prefix"
        )));
    }
    Ok(AnalyzerDefinition {
        id: CatalogId::new_random(),
        logical_name: logical_name.to_string(),
        provider: BUILTIN_FTS_ANALYZER_PROVIDER.to_string(),
        provider_version: BUILTIN_FTS_PROVIDER_VERSION,
        options_json: "{\"tokenizer\":\"blank\"}".to_string(),
        definition,
    })
}

pub fn allocate_fts_index(
    table_id: CatalogId,
    first_ordinal: usize,
    logical_name: &str,
    paths: Vec<Vec<String>>,
    definition: String,
    options_json: String,
) -> Result<(IndexDefinition, Vec<HiddenColumnDefinition>)> {
    let mut index = allocate_index(logical_name, paths, false, definition)?;
    index.kind = IndexKind::Fts;
    index.provider = Provider::BuiltinFts;
    index.provider_version = BUILTIN_FTS_PROVIDER_VERSION;
    index.options_json = options_json;
    index.encoding_version = BUILTIN_FTS_ENCODING_VERSION;
    let hidden = index
        .path_keys
        .iter()
        .enumerate()
        .map(|(field_ordinal, path_key)| {
            let ordinal = first_ordinal + field_ordinal;
            let id = CatalogId::new_random();
            HiddenColumnDefinition {
                id,
                table_id,
                index_id: Some(index.id),
                field_path_key: Some(path_key.clone()),
                physical_name: physical_hidden_column_name(id),
                provider: Provider::BuiltinFts,
                role: HiddenColumnRole::FtsText(ordinal),
                physical_encoding: "FTS_TEXT_UTF8".to_string(),
                dimension: None,
                options_json: format!("{{\"ordinal\":{ordinal},\"role\":\"fts_text\"}}"),
                state: ProviderState::Ready,
                provider_version: BUILTIN_FTS_PROVIDER_VERSION,
                encoding_version: BUILTIN_FTS_ENCODING_VERSION,
            }
        })
        .collect::<Vec<_>>();
    index.physical_columns = hidden
        .iter()
        .map(|column| column.physical_name.clone())
        .collect();
    Ok((index, hidden))
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
        kind: IndexKind::Btree,
        provider: Provider::BuiltinBtree,
        provider_version: BUILTIN_BTREE_PROVIDER_VERSION,
        options_json: "{}".to_string(),
        state: ProviderState::Ready,
        encoding_version: BUILTIN_BTREE_ENCODING_VERSION,
        physical_columns: Vec::new(),
    })
}

pub fn bootstrap(conn: &Connection) -> Result<CatalogSnapshot> {
    conn.exec_bound(lower::catalog_meta_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_tables_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_fields_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_indexes_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_analyzers_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_hidden_columns_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_capabilities_ddl(), vec![])?;
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
        analyzers: BTreeMap::new(),
        hidden_columns: BTreeMap::new(),
        capabilities: BTreeMap::new(),
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
        match table.kind {
            TableKind::Normal => "NORMAL",
            TableKind::Relation => "RELATION",
        },
        table.relation_in_table_id.map(CatalogId::to_hex).as_deref(),
        table
            .relation_out_table_id
            .map(CatalogId::to_hex)
            .as_deref(),
        table.relation_enforced,
    );
    conn.exec_bound(statement, bindings)
}

pub fn persist_hidden_column(conn: &Connection, column: &HiddenColumnDefinition) -> Result<()> {
    let index_id = column.index_id.map(CatalogId::to_hex);
    let (statement, bindings) = lower::hidden_column_insert(
        &column.id.to_hex(),
        &column.table_id.to_hex(),
        index_id.as_deref(),
        column.field_path_key.as_deref(),
        &column.physical_name,
        match column.provider {
            Provider::BuiltinGraph => BUILTIN_GRAPH_PROVIDER,
            Provider::BuiltinFts => BUILTIN_FTS_PROVIDER,
            Provider::BuiltinBtree => {
                return Err(FastDbError::format(
                    "ordinary B-tree provider cannot own a hidden column",
                ));
            }
        },
        column.provider_version,
        &column.physical_encoding,
        column.dimension,
        &column.options_json,
        "READY",
        column.encoding_version,
    );
    conn.exec_bound(statement, bindings)
}

pub fn persist_analyzer(conn: &Connection, analyzer: &AnalyzerDefinition) -> Result<()> {
    let (statement, bindings) = lower::analyzer_insert(
        &analyzer.id.to_hex(),
        &analyzer.logical_name,
        &analyzer.provider,
        analyzer.provider_version,
        &analyzer.options_json,
        &analyzer.definition,
    );
    conn.exec_bound(statement, bindings)
}

pub fn persist_graph_capability(conn: &Connection) -> Result<()> {
    let (statement, bindings) = lower::capability_insert(
        BUILTIN_GRAPH_PROVIDER,
        BUILTIN_GRAPH_PROVIDER_VERSION,
        BUILTIN_GRAPH_ENCODING_VERSION,
    );
    conn.exec_bound(statement, bindings)
}

pub fn persist_fts_capability(conn: &Connection) -> Result<()> {
    let (statement, bindings) = lower::capability_insert(
        BUILTIN_FTS_PROVIDER,
        BUILTIN_FTS_PROVIDER_VERSION,
        BUILTIN_FTS_ENCODING_VERSION,
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
        match index.kind {
            IndexKind::Btree => "BTREE",
            IndexKind::GraphAdjacency => "GRAPH_ADJACENCY",
            IndexKind::Fts => "FTS",
        },
        match index.provider {
            Provider::BuiltinBtree => "BUILTIN_BTREE",
            Provider::BuiltinGraph => BUILTIN_GRAPH_PROVIDER,
            Provider::BuiltinFts => BUILTIN_FTS_PROVIDER,
        },
        index.provider_version,
        &index.options_json,
        match index.state {
            ProviderState::Ready => "READY",
            ProviderState::RebuildRequired => "REBUILD_REQUIRED",
        },
        index.encoding_version,
    );
    conn.exec_bound(statement, bindings)
}

pub fn remove_index(conn: &Connection, index: &IndexDefinition) -> Result<()> {
    let (statement, bindings) = lower::index_delete(&index.id.to_hex());
    conn.exec_bound(statement, bindings)
}

pub fn load_and_validate(conn: &Connection) -> Result<CatalogState> {
    let mut schema = read_schema(conn)?;
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
            "disposable FastDB format 0 cannot be opened as stable format 2",
        ));
    }
    match format {
        1 => {
            migrate_format_one(conn, &schema)?;
            schema = read_schema(conn)?;
        }
        FORMAT_VERSION => {}
        _ => {
            return Err(FastDbError::format(format!(
                "unknown FastDB format version {format}; supported versions are 1 and \
                 {FORMAT_VERSION}"
            )))
        }
    }

    validate_catalog_schema(&schema)?;
    let metadata = load_metadata(conn)?;
    if metadata.last_migration != LAST_MIGRATION {
        return Err(FastDbError::format(format!(
            "unknown FastDB migration level {}",
            metadata.last_migration
        )));
    }
    let mut snapshot = CatalogSnapshot {
        metadata,
        tables: load_tables(conn)?,
        analyzers: load_analyzers(conn)?,
        hidden_columns: load_hidden_columns(conn)?,
        capabilities: load_capabilities(conn)?,
    };
    load_fields(conn, &mut snapshot)?;
    load_indexes(conn, &mut snapshot)?;
    validate_graph_catalog(&snapshot)?;
    validate_fts_catalog(&snapshot)?;
    validate_physical_objects(&schema, &snapshot, true)?;
    Ok(CatalogState::Ready(snapshot))
}

fn load_metadata(conn: &Connection) -> Result<Metadata> {
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
    Ok(Metadata {
        database_id,
        creation_version,
        last_migration: format_integer(&metadata_row[4], "last_migration")?,
    })
}

fn migrate_format_one(conn: &Connection, schema: &[SchemaObject]) -> Result<()> {
    validate_catalog_schema_v1(schema)?;
    let metadata_rows = conn.collect_rows(lower::meta_stmt(), vec![])?;
    let metadata_row = singleton_row(&metadata_rows, "metadata")?;
    if format_integer(&metadata_row[0], "format_version")? != 1 {
        return Err(FastDbError::format(
            "format changed while preparing migration",
        ));
    }
    if format_integer(&metadata_row[1], "dialect_version")? != DIALECT_VERSION {
        return Err(FastDbError::format("format-1 dialect is unsupported"));
    }
    let database_id = format_text(&metadata_row[2], "database_id")?;
    CatalogId::from_hex(&database_id)?;
    let creation_version = format_text(&metadata_row[3], "creation_version")?;
    if creation_version.is_empty() {
        return Err(FastDbError::format("metadata creation version is empty"));
    }
    let last_migration = format_integer(&metadata_row[4], "last_migration")?;
    if !matches!(last_migration, 0 | 1) {
        return Err(FastDbError::format(format!(
            "unknown format-1 migration level {last_migration}"
        )));
    }

    let mut prior = CatalogSnapshot {
        metadata: Metadata {
            database_id,
            creation_version,
            last_migration,
        },
        tables: load_tables_v1(conn)?,
        analyzers: BTreeMap::new(),
        hidden_columns: BTreeMap::new(),
        capabilities: BTreeMap::new(),
    };
    load_fields(conn, &mut prior)?;
    load_indexes_v1(conn, &mut prior)?;
    validate_physical_objects(schema, &prior, false)?;

    conn.with_transaction(|| {
        if last_migration == 0 {
            conn.exec_bound(lower::migrate_to_one_stmt(), vec![])?;
        }
        for column in lower::format2_table_columns() {
            conn.exec_bound(lower::add_catalog_column(TABLES_TABLE, column), vec![])?;
        }
        conn.check_failpoint(crate::Failpoint::AfterFormat2TableColumns)?;
        for column in lower::format2_index_columns() {
            conn.exec_bound(lower::add_catalog_column(INDEXES_TABLE, column), vec![])?;
        }
        conn.check_failpoint(crate::Failpoint::AfterFormat2IndexColumns)?;
        conn.exec_bound(lower::catalog_analyzers_ddl(), vec![])?;
        conn.exec_bound(lower::catalog_hidden_columns_ddl(), vec![])?;
        conn.exec_bound(lower::catalog_capabilities_ddl(), vec![])?;
        conn.check_failpoint(crate::Failpoint::AfterFormat2Catalogs)?;

        let migrated_schema = read_schema(conn)?;
        validate_catalog_schema(&migrated_schema)?;
        let mut migrated = CatalogSnapshot {
            metadata: prior.metadata.clone(),
            tables: load_tables(conn)?,
            analyzers: load_analyzers(conn)?,
            hidden_columns: load_hidden_columns(conn)?,
            capabilities: load_capabilities(conn)?,
        };
        load_fields(conn, &mut migrated)?;
        load_indexes(conn, &mut migrated)?;
        validate_physical_objects(&migrated_schema, &migrated, true)?;
        if migrated.tables != prior.tables {
            return Err(FastDbError::format(
                "format-2 migration changed logical table or index ownership",
            ));
        }
        conn.check_failpoint(crate::Failpoint::AfterFormat2Validation)?;
        conn.exec_bound(lower::migrate_to_two_stmt(), vec![])?;
        conn.check_failpoint(crate::Failpoint::AfterMigration)
    })
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
    load_table_rows(conn.collect_rows(lower::tables_stmt(), vec![])?, true)
}

fn load_tables_v1(conn: &Connection) -> Result<BTreeMap<String, TableDefinition>> {
    load_table_rows(conn.collect_rows(lower::tables_v1_stmt(), vec![])?, false)
}

fn load_table_rows(
    rows: Vec<Vec<Value>>,
    format_two: bool,
) -> Result<BTreeMap<String, TableDefinition>> {
    let mut tables = BTreeMap::new();
    let mut ids = BTreeSet::new();
    let mut physical_names = BTreeSet::new();
    for row in rows {
        let expected_width = if format_two { 9 } else { 5 };
        if row.len() != expected_width {
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
        let (kind, relation_in_table_id, relation_out_table_id, relation_enforced) = if format_two {
            let kind = match format_text(&row[5], "kind")?.as_str() {
                "NORMAL" => TableKind::Normal,
                "RELATION" => TableKind::Relation,
                _ => return Err(FastDbError::format("table catalog has unknown kind")),
            };
            let relation_in = format_optional_catalog_id(&row[6], "relation_in_table_id")?;
            let relation_out = format_optional_catalog_id(&row[7], "relation_out_table_id")?;
            let enforced = format_boolean_integer(&row[8], "relation_enforced")?;
            if kind == TableKind::Normal
                && (relation_in.is_some() || relation_out.is_some() || enforced)
            {
                return Err(FastDbError::format(
                    "normal table has relation-only metadata",
                ));
            }
            (kind, relation_in, relation_out, enforced)
        } else {
            (TableKind::Normal, None, None, false)
        };
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
                        kind,
                        relation_in_table_id,
                        relation_out_table_id,
                        relation_enforced,
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
    load_index_rows(
        conn.collect_rows(lower::indexes_stmt(), vec![])?,
        snapshot,
        true,
    )
}

fn load_indexes_v1(conn: &Connection, snapshot: &mut CatalogSnapshot) -> Result<()> {
    load_index_rows(
        conn.collect_rows(lower::indexes_v1_stmt(), vec![])?,
        snapshot,
        false,
    )
}

fn load_index_rows(
    rows: Vec<Vec<Value>>,
    snapshot: &mut CatalogSnapshot,
    format_two: bool,
) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut physical_names = BTreeSet::new();
    for row in rows {
        let expected_width = if format_two { 14 } else { 8 };
        if row.len() != expected_width {
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
        let (kind, provider, provider_version, options_json, state, encoding_version) =
            if format_two {
                let kind = match format_text(&row[8], "index_kind")?.as_str() {
                    "BTREE" => IndexKind::Btree,
                    "GRAPH_ADJACENCY" => IndexKind::GraphAdjacency,
                    "FTS" => IndexKind::Fts,
                    _ => return Err(FastDbError::format("index catalog has unknown kind")),
                };
                let provider = match format_text(&row[9], "provider")?.as_str() {
                    "BUILTIN_BTREE" => Provider::BuiltinBtree,
                    BUILTIN_GRAPH_PROVIDER => Provider::BuiltinGraph,
                    BUILTIN_FTS_PROVIDER => Provider::BuiltinFts,
                    _ => {
                        return Err(FastDbError::format(
                            "index requires an unknown or unavailable provider",
                        ))
                    }
                };
                let provider_version = format_integer(&row[10], "provider_version")?;
                let options_json = format_text(&row[11], "options_json")?;
                let state = match format_text(&row[12], "state")?.as_str() {
                    "READY" => ProviderState::Ready,
                    "REBUILD_REQUIRED" => ProviderState::RebuildRequired,
                    _ => return Err(FastDbError::format("index has unknown lifecycle state")),
                };
                let encoding_version = format_integer(&row[13], "encoding_version")?;
                (
                    kind,
                    provider,
                    provider_version,
                    options_json,
                    state,
                    encoding_version,
                )
            } else {
                (
                    IndexKind::Btree,
                    Provider::BuiltinBtree,
                    BUILTIN_BTREE_PROVIDER_VERSION,
                    "{}".to_string(),
                    ProviderState::Ready,
                    BUILTIN_BTREE_ENCODING_VERSION,
                )
            };
        let index = IndexDefinition {
            id,
            logical_name: logical_name.clone(),
            physical_name: physical_name.clone(),
            paths,
            path_keys,
            unique,
            expression_version,
            definition,
            kind,
            provider,
            provider_version,
            options_json,
            state,
            encoding_version,
            physical_columns: if kind == IndexKind::Fts {
                let mut columns = snapshot
                    .hidden_columns
                    .values()
                    .filter(|column| column.index_id == Some(id))
                    .collect::<Vec<_>>();
                columns.sort_by_key(|column| match column.role {
                    HiddenColumnRole::FtsText(ordinal) => ordinal,
                    HiddenColumnRole::Graph(_) => usize::MAX,
                });
                columns
                    .into_iter()
                    .map(|column| column.physical_name.clone())
                    .collect()
            } else {
                Vec::new()
            },
        };
        crate::provider::index_provider(&index)?;
        if !ids.insert(id)
            || !physical_names.insert(physical_name)
            || table.indexes.insert(logical_name, index).is_some()
        {
            return Err(FastDbError::format("index catalog contains duplicates"));
        }
    }
    Ok(())
}

fn load_analyzers(conn: &Connection) -> Result<BTreeMap<String, AnalyzerDefinition>> {
    let rows = conn.collect_rows(lower::analyzers_stmt(), vec![])?;
    let mut analyzers = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for row in rows {
        if row.len() != 6 {
            return Err(FastDbError::format("analyzer catalog row has wrong width"));
        }
        let id = CatalogId::from_hex(&format_text(&row[0], "analyzer_id")?)?;
        let logical_name = format_text(&row[1], "logical_name")?;
        if logical_name.is_empty() || is_reserved_logical_name(&logical_name) {
            return Err(FastDbError::format(
                "analyzer catalog has invalid logical name",
            ));
        }
        let analyzer = AnalyzerDefinition {
            id,
            logical_name: logical_name.clone(),
            provider: format_text(&row[2], "provider")?,
            provider_version: format_integer(&row[3], "provider_version")?,
            options_json: format_text(&row[4], "options_json")?,
            definition: format_text(&row[5], "definition")?,
        };
        if analyzer.provider != BUILTIN_FTS_ANALYZER_PROVIDER
            || analyzer.provider_version != BUILTIN_FTS_PROVIDER_VERSION
            || analyzer.options_json != "{\"tokenizer\":\"blank\"}"
        {
            return Err(FastDbError::format(
                "analyzer requires an unknown or incompatible provider",
            ));
        }
        if !ids.insert(id) || analyzers.insert(logical_name, analyzer).is_some() {
            return Err(FastDbError::format("analyzer catalog contains duplicates"));
        }
    }
    Ok(analyzers)
}

fn load_hidden_columns(conn: &Connection) -> Result<BTreeMap<CatalogId, HiddenColumnDefinition>> {
    let rows = conn.collect_rows(lower::hidden_columns_stmt(), vec![])?;
    let mut columns = BTreeMap::new();
    let mut physical_names = BTreeSet::new();
    for row in rows {
        if row.len() != 12 {
            return Err(FastDbError::format(
                "hidden-column catalog row has wrong width",
            ));
        }
        let id = CatalogId::from_hex(&format_text(&row[0], "column_id")?)?;
        let table_id = CatalogId::from_hex(&format_text(&row[1], "table_id")?)?;
        let index_id = format_optional_catalog_id(&row[2], "index_id")?;
        let field_path_key = format_optional_text(&row[3], "field_path_key")?;
        if let Some(path_key) = &field_path_key {
            decode_canonical_path(path_key)?;
        }
        let physical_name = format_text(&row[4], "physical_name")?;
        validate_physical_name(&physical_name, HIDDEN_COLUMN_NAME_PREFIX)?;
        if physical_name != physical_hidden_column_name(id) {
            return Err(FastDbError::format(
                "hidden-column physical name does not match its immutable ID",
            ));
        }
        let provider = match format_text(&row[5], "provider")?.as_str() {
            BUILTIN_GRAPH_PROVIDER => Provider::BuiltinGraph,
            BUILTIN_FTS_PROVIDER => Provider::BuiltinFts,
            _ => {
                return Err(FastDbError::format(
                    "hidden typed column requires an unknown provider",
                ));
            }
        };
        let provider_version = format_integer(&row[6], "provider_version")?;
        let physical_encoding = format_text(&row[7], "physical_encoding")?;
        let dimension = format_optional_integer(&row[8], "dimension")?;
        let options_json = format_text(&row[9], "options_json")?;
        let role = match provider {
            Provider::BuiltinGraph => {
                if index_id.is_some() || field_path_key.is_some() || dimension.is_some() {
                    return Err(FastDbError::format(
                        "graph hidden column has index, field, or dimension metadata",
                    ));
                }
                let graph_role = match options_json.as_str() {
                    "{\"role\":\"in_table\"}" => GraphColumnRole::InTable,
                    "{\"role\":\"in_rid\"}" => GraphColumnRole::InRid,
                    "{\"role\":\"out_table\"}" => GraphColumnRole::OutTable,
                    "{\"role\":\"out_rid\"}" => GraphColumnRole::OutRid,
                    _ => return Err(FastDbError::format("unknown graph hidden-column role")),
                };
                if physical_encoding != graph_role.encoding() {
                    return Err(FastDbError::format(
                        "graph hidden-column role and encoding disagree",
                    ));
                }
                HiddenColumnRole::Graph(graph_role)
            }
            Provider::BuiltinFts => {
                if index_id.is_none() || field_path_key.is_none() || dimension.is_some() {
                    return Err(FastDbError::format(
                        "FTS hidden column requires index and field ownership only",
                    ));
                }
                if physical_encoding != "FTS_TEXT_UTF8" {
                    return Err(FastDbError::format(
                        "FTS hidden column has an incompatible encoding",
                    ));
                }
                let value: serde_json::Value = serde_json::from_str(&options_json)
                    .map_err(|_| FastDbError::format("FTS hidden options are malformed"))?;
                let object = value
                    .as_object()
                    .ok_or_else(|| FastDbError::format("FTS hidden options must be an object"))?;
                if object.len() != 2
                    || object.get("role").and_then(serde_json::Value::as_str) != Some("fts_text")
                {
                    return Err(FastDbError::format(
                        "FTS hidden-column role options are not canonical",
                    ));
                }
                let ordinal = object
                    .get("ordinal")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| FastDbError::format("FTS hidden ordinal is invalid"))?;
                if options_json != format!("{{\"ordinal\":{ordinal},\"role\":\"fts_text\"}}") {
                    return Err(FastDbError::format(
                        "FTS hidden options are not canonically encoded",
                    ));
                }
                HiddenColumnRole::FtsText(ordinal)
            }
            Provider::BuiltinBtree => unreachable!("matched providers exclude B-tree"),
        };
        let state = match format_text(&row[10], "state")?.as_str() {
            "READY" => ProviderState::Ready,
            "REBUILD_REQUIRED" => ProviderState::RebuildRequired,
            _ => return Err(FastDbError::format("hidden column has unknown state")),
        };
        let encoding_version = format_integer(&row[11], "encoding_version")?;
        let expected_versions = match provider {
            Provider::BuiltinGraph => (
                BUILTIN_GRAPH_PROVIDER_VERSION,
                BUILTIN_GRAPH_ENCODING_VERSION,
            ),
            Provider::BuiltinFts => (BUILTIN_FTS_PROVIDER_VERSION, BUILTIN_FTS_ENCODING_VERSION),
            Provider::BuiltinBtree => unreachable!("hidden B-tree excluded"),
        };
        if provider_version != expected_versions.0
            || encoding_version != expected_versions.1
            || state != ProviderState::Ready
        {
            return Err(FastDbError::format(
                "graph hidden column has an unsupported version or state",
            ));
        }
        let column = HiddenColumnDefinition {
            id,
            table_id,
            index_id,
            field_path_key,
            physical_name: physical_name.clone(),
            provider,
            role,
            physical_encoding,
            dimension,
            options_json,
            state,
            provider_version,
            encoding_version,
        };
        if !physical_names.insert(physical_name) || columns.insert(id, column).is_some() {
            return Err(FastDbError::format(
                "hidden-column catalog contains duplicate ownership",
            ));
        }
    }
    Ok(columns)
}

fn load_capabilities(conn: &Connection) -> Result<BTreeMap<String, CapabilityRequirement>> {
    let rows = conn.collect_rows(lower::capabilities_stmt(), vec![])?;
    let mut capabilities = BTreeMap::new();
    for row in rows {
        if row.len() != 3 {
            return Err(FastDbError::format(
                "capability catalog row has wrong width",
            ));
        }
        let provider = format_text(&row[0], "provider")?;
        let requirement = CapabilityRequirement {
            provider: provider.clone(),
            min_provider_version: format_integer(&row[1], "min_provider_version")?,
            min_encoding_version: format_integer(&row[2], "min_encoding_version")?,
        };
        let supported = match provider.as_str() {
            BUILTIN_GRAPH_PROVIDER => {
                requirement.min_provider_version == BUILTIN_GRAPH_PROVIDER_VERSION
                    && requirement.min_encoding_version == BUILTIN_GRAPH_ENCODING_VERSION
            }
            BUILTIN_FTS_PROVIDER => {
                cfg!(not(target_family = "wasm"))
                    && requirement.min_provider_version == BUILTIN_FTS_PROVIDER_VERSION
                    && requirement.min_encoding_version == BUILTIN_FTS_ENCODING_VERSION
            }
            _ => false,
        };
        if !supported {
            return Err(FastDbError::format(
                "database requires an unknown or unavailable capability",
            ));
        }
        if capabilities.insert(provider, requirement).is_some() {
            return Err(FastDbError::format(
                "capability catalog contains duplicate providers",
            ));
        }
    }
    Ok(capabilities)
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
        (ANALYZERS_TABLE, lower::catalog_analyzers_ddl()),
        (HIDDEN_COLUMNS_TABLE, lower::catalog_hidden_columns_ddl()),
        (CAPABILITIES_TABLE, lower::catalog_capabilities_ddl()),
    ] {
        require_exact_schema(schema, "table", name, name, &statement.to_string())?;
    }
    Ok(())
}

fn validate_catalog_schema_v1(schema: &[SchemaObject]) -> Result<()> {
    for (name, statement) in [
        (META_TABLE, lower::catalog_meta_ddl()),
        (TABLES_TABLE, lower::catalog_tables_v1_ddl()),
        (FIELDS_TABLE, lower::catalog_fields_ddl()),
        (INDEXES_TABLE, lower::catalog_indexes_v1_ddl()),
    ] {
        require_exact_schema(schema, "table", name, name, &statement.to_string())?;
    }
    Ok(())
}

fn validate_physical_objects(
    schema: &[SchemaObject],
    snapshot: &CatalogSnapshot,
    format_two: bool,
) -> Result<()> {
    let mut expected_reserved = BTreeSet::from([
        META_TABLE.to_string(),
        TABLES_TABLE.to_string(),
        FIELDS_TABLE.to_string(),
        INDEXES_TABLE.to_string(),
    ]);
    if format_two {
        expected_reserved.extend([
            ANALYZERS_TABLE.to_string(),
            HIDDEN_COLUMNS_TABLE.to_string(),
            CAPABILITIES_TABLE.to_string(),
        ]);
    }
    for table in snapshot.tables.values() {
        expected_reserved.insert(table.physical_name.clone());
        let graph = if table.kind == TableKind::Relation {
            graph_columns(snapshot, table)?
                .into_iter()
                .map(|column| column.physical_name.clone())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut fts = snapshot
            .hidden_columns
            .values()
            .filter(|column| {
                column.table_id == table.id && matches!(column.role, HiddenColumnRole::FtsText(_))
            })
            .collect::<Vec<_>>();
        fts.sort_by_key(|column| match column.role {
            HiddenColumnRole::FtsText(ordinal) => ordinal,
            HiddenColumnRole::Graph(_) => usize::MAX,
        });
        let table_ddl = lower::physical_table_with_hidden_ddl(
            &table.physical_name,
            &graph,
            &fts.into_iter()
                .map(|column| column.physical_name.clone())
                .collect::<Vec<_>>(),
        )?;
        require_exact_schema(
            schema,
            "table",
            &table.physical_name,
            &table.physical_name,
            &table_ddl.to_string(),
        )?;
        for index in table.indexes.values() {
            expected_reserved.insert(index.physical_name.clone());
            let provider = crate::provider::index_provider(index)?;
            require_exact_schema(
                schema,
                "index",
                &index.physical_name,
                &table.physical_name,
                &provider
                    .create_statement(index, &table.physical_name)?
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

pub(crate) fn graph_columns<'a>(
    snapshot: &'a CatalogSnapshot,
    table: &TableDefinition,
) -> Result<Vec<&'a HiddenColumnDefinition>> {
    let mut by_role = BTreeMap::new();
    for column in snapshot.hidden_columns.values().filter(|column| {
        column.table_id == table.id && matches!(column.role, HiddenColumnRole::Graph(_))
    }) {
        let HiddenColumnRole::Graph(role) = column.role else {
            unreachable!("filtered graph role");
        };
        if by_role.insert(role, column).is_some() {
            return Err(FastDbError::format(
                "relation table has duplicate hidden-column roles",
            ));
        }
    }
    GraphColumnRole::ALL
        .into_iter()
        .map(|role| {
            by_role.get(&role).copied().ok_or_else(|| {
                FastDbError::format("relation table is missing a hidden endpoint column")
            })
        })
        .collect()
}

fn validate_graph_catalog(snapshot: &CatalogSnapshot) -> Result<()> {
    let has_relations = snapshot
        .tables
        .values()
        .any(|table| table.kind == TableKind::Relation);
    if has_relations != snapshot.capabilities.contains_key(BUILTIN_GRAPH_PROVIDER) {
        return Err(FastDbError::format(
            "graph capability requirement disagrees with relation ownership",
        ));
    }
    for column in snapshot
        .hidden_columns
        .values()
        .filter(|column| matches!(column.role, HiddenColumnRole::Graph(_)))
    {
        let owner = snapshot
            .tables
            .values()
            .find(|table| table.id == column.table_id)
            .ok_or_else(|| FastDbError::format("hidden column has an orphan table owner"))?;
        if owner.kind != TableKind::Relation {
            return Err(FastDbError::format(
                "normal table owns a graph hidden column",
            ));
        }
    }
    for table in snapshot.tables.values() {
        let graph_indexes = table
            .indexes
            .values()
            .filter(|index| index.kind == IndexKind::GraphAdjacency)
            .collect::<Vec<_>>();
        if table.kind == TableKind::Normal {
            if !graph_indexes.is_empty() {
                return Err(FastDbError::format(
                    "normal table owns a graph adjacency index",
                ));
            }
            continue;
        }
        for endpoint in [table.relation_in_table_id, table.relation_out_table_id]
            .into_iter()
            .flatten()
        {
            let endpoint_table = snapshot
                .tables
                .values()
                .find(|candidate| candidate.id == endpoint)
                .ok_or_else(|| FastDbError::format("relation endpoint table is missing"))?;
            if endpoint_table.kind != TableKind::Normal {
                return Err(FastDbError::format(
                    "relation endpoint constraint must reference a normal table",
                ));
            }
        }
        let columns = graph_columns(snapshot, table)?;
        if snapshot
            .hidden_columns
            .values()
            .filter(|column| {
                column.table_id == table.id && matches!(column.role, HiddenColumnRole::Graph(_))
            })
            .count()
            != 4
        {
            return Err(FastDbError::format(
                "relation table must own exactly four hidden columns",
            ));
        }
        if graph_indexes.len() != 2 {
            return Err(FastDbError::format(
                "relation table must own forward and reverse adjacency indexes",
            ));
        }
        let forward = columns
            .iter()
            .map(|column| vec![column.physical_name.clone()])
            .collect::<Vec<_>>();
        let reverse = [columns[2], columns[3], columns[0], columns[1]]
            .into_iter()
            .map(|column| vec![column.physical_name.clone()])
            .collect::<Vec<_>>();
        for (logical_name, options, expected) in [
            ("__graph_forward", "{\"direction\":\"forward\"}", forward),
            ("__graph_reverse", "{\"direction\":\"reverse\"}", reverse),
        ] {
            let matching = graph_indexes
                .iter()
                .filter(|index| {
                    index.logical_name == logical_name
                        && index.options_json == options
                        && index.paths == expected
                })
                .count();
            if matching != 1 {
                return Err(FastDbError::format(
                    "relation adjacency index direction or column order is invalid",
                ));
            }
        }
    }
    Ok(())
}

fn validate_fts_catalog(snapshot: &CatalogSnapshot) -> Result<()> {
    let fts_indexes = snapshot
        .tables
        .values()
        .flat_map(|table| table.indexes.values().map(move |index| (table, index)))
        .filter(|(_, index)| index.kind == IndexKind::Fts)
        .collect::<Vec<_>>();
    let fts_columns = snapshot
        .hidden_columns
        .values()
        .filter(|column| matches!(column.role, HiddenColumnRole::FtsText(_)))
        .collect::<Vec<_>>();
    let requires_fts =
        !snapshot.analyzers.is_empty() || !fts_indexes.is_empty() || !fts_columns.is_empty();
    if requires_fts != snapshot.capabilities.contains_key(BUILTIN_FTS_PROVIDER) {
        return Err(FastDbError::format(
            "FTS capability requirement disagrees with analyzer/index ownership",
        ));
    }

    let mut table_ordinals = BTreeMap::<CatalogId, BTreeSet<usize>>::new();
    for column in &fts_columns {
        let owner = snapshot
            .tables
            .values()
            .find(|table| table.id == column.table_id)
            .ok_or_else(|| FastDbError::format("FTS hidden column has an orphan table owner"))?;
        let index_id = column
            .index_id
            .ok_or_else(|| FastDbError::format("FTS hidden column has no index owner"))?;
        let index = owner
            .indexes
            .values()
            .find(|index| index.id == index_id)
            .ok_or_else(|| FastDbError::format("FTS hidden column has an orphan index owner"))?;
        if index.kind != IndexKind::Fts || column.provider != Provider::BuiltinFts {
            return Err(FastDbError::format(
                "FTS hidden column is owned by an incompatible index",
            ));
        }
        let HiddenColumnRole::FtsText(ordinal) = column.role else {
            unreachable!("filtered FTS role");
        };
        if !table_ordinals.entry(owner.id).or_default().insert(ordinal) {
            return Err(FastDbError::format(
                "FTS hidden columns have duplicate table ordinals",
            ));
        }
    }
    for ordinals in table_ordinals.values() {
        if ordinals.iter().copied().ne(0..ordinals.len()) {
            return Err(FastDbError::format(
                "FTS hidden-column table ordinals are not contiguous",
            ));
        }
    }

    for (table, index) in fts_indexes {
        crate::provider::index_provider(index)?;
        let options = FtsIndexOptions::parse_canonical(&index.options_json)?;
        if options.surface == "surreal" {
            let analyzer = options
                .analyzer
                .as_ref()
                .and_then(|name| snapshot.analyzers.get(name))
                .ok_or_else(|| FastDbError::format("FTS index analyzer is missing"))?;
            if analyzer.provider != BUILTIN_FTS_ANALYZER_PROVIDER {
                return Err(FastDbError::format(
                    "FTS index analyzer provider is incompatible",
                ));
            }
        }
        let mut owned = snapshot
            .hidden_columns
            .values()
            .filter(|column| column.index_id == Some(index.id))
            .collect::<Vec<_>>();
        owned.sort_by_key(|column| match column.role {
            HiddenColumnRole::FtsText(ordinal) => ordinal,
            HiddenColumnRole::Graph(_) => usize::MAX,
        });
        if owned.len() != index.paths.len()
            || owned.iter().zip(&index.path_keys).any(|(column, path)| {
                column.table_id != table.id
                    || column.field_path_key.as_deref() != Some(path.as_str())
            })
            || owned
                .iter()
                .map(|column| column.physical_name.as_str())
                .ne(index.physical_columns.iter().map(String::as_str))
        {
            return Err(FastDbError::format(
                "FTS index hidden-column ownership or order is invalid",
            ));
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
    let actual_sql = object.sql.as_deref();
    let sql_matches = actual_sql.is_some_and(|actual| {
        actual == expected_sql
            || actual.replace("CHECK (", "CHECK(") == expected_sql.replace("CHECK (", "CHECK(")
    });
    if object.table_name != table_name || !sql_matches {
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

fn format_optional_catalog_id(value: &Value, field: &str) -> Result<Option<CatalogId>> {
    format_optional_text(value, field)?
        .map(|value| CatalogId::from_hex(&value))
        .transpose()
}

fn format_integer(value: &Value, field: &str) -> Result<i64> {
    match value {
        Value::Numeric(turso_core::Numeric::Integer(value)) => Ok(*value),
        _ => Err(FastDbError::format(format!(
            "catalog field {field} is not INTEGER"
        ))),
    }
}

fn format_optional_integer(value: &Value, field: &str) -> Result<Option<i64>> {
    match value {
        Value::Null => Ok(None),
        Value::Numeric(turso_core::Numeric::Integer(value)) => Ok(Some(*value)),
        _ => Err(FastDbError::format(format!(
            "catalog field {field} is neither INTEGER nor NULL"
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
