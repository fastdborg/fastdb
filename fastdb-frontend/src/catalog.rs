//! Format-3 catalogs, transactional format-1/2 migration, and validation.

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
pub const FUNCTIONS_TABLE: &str = "__fastdb_functions";
pub const PARAMETERS_TABLE: &str = "__fastdb_parameters";
pub const VIEWS_TABLE: &str = "__fastdb_views";
pub const EVENTS_TABLE: &str = "__fastdb_events";
pub const PERMISSIONS_TABLE: &str = "__fastdb_permissions";
pub const USERS_TABLE: &str = "__fastdb_users";
pub const ACCESSES_TABLE: &str = "__fastdb_accesses";

pub const FORMAT_VERSION: i64 = 3;
pub const DIALECT_VERSION: i64 = 1;
pub const LAST_MIGRATION: i64 = 3;
pub const DOCUMENT_ENCODING_VERSION: i64 = 2;
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
pub const BUILTIN_VECTOR_PROVIDER_VERSION: i64 = 1;
pub const BUILTIN_VECTOR_ENCODING_VERSION: i64 = 1;
pub const BUILTIN_VECTOR_PROVIDER: &str = "BUILTIN_VECTOR_EXACT";
pub const EVENT_RECURSION_LIMIT: i64 = 16;

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
    BuiltinVector,
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
    pub document_encoding_version: i64,
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

#[derive(Debug, Clone, PartialEq)]
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
    pub drop: bool,
    pub permissions: turso_fastdb_parser::SchemaPermissions,
    pub comment: Option<String>,
    pub fields: BTreeMap<String, FieldRule>,
    pub indexes: BTreeMap<String, IndexDefinition>,
    pub events: BTreeMap<String, EventDefinition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CatalogSnapshot {
    pub metadata: Metadata,
    pub tables: BTreeMap<String, TableDefinition>,
    pub analyzers: BTreeMap<String, AnalyzerDefinition>,
    pub parameters: BTreeMap<String, ParameterDefinition>,
    pub functions: BTreeMap<String, FunctionDefinition>,
    pub views: BTreeMap<String, ViewDefinition>,
    pub hidden_columns: BTreeMap<CatalogId, HiddenColumnDefinition>,
    pub capabilities: BTreeMap<String, CapabilityRequirement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ViewDefinition {
    pub id: CatalogId,
    pub logical_name: String,
    pub select: turso_fastdb_parser::SelectStatement,
    pub select_source: String,
    pub dependencies: Vec<CatalogId>,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FunctionArgumentDefinition {
    pub name: String,
    pub ty: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDefinition {
    pub id: CatalogId,
    /// Canonical name without the leading `fn::` namespace.
    pub logical_name: String,
    pub arguments: Vec<FunctionArgumentDefinition>,
    pub body: turso_fastdb_parser::ScriptBlock,
    pub body_source: String,
    pub permissions: turso_fastdb_parser::SchemaPermissions,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventDefinition {
    pub id: CatalogId,
    pub table_id: CatalogId,
    pub logical_name: String,
    pub condition: Option<turso_fastdb_parser::Expr>,
    pub condition_source: String,
    pub action: turso_fastdb_parser::EventAction,
    pub action_source: String,
    pub comment: Option<String>,
    pub definition: String,
}

pub struct NewEventDefinition {
    pub logical_name: String,
    pub condition: Option<turso_fastdb_parser::Expr>,
    pub condition_source: String,
    pub action: turso_fastdb_parser::EventAction,
    pub action_source: String,
    pub comment: Option<String>,
    pub definition: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterDefinition {
    pub id: CatalogId,
    pub logical_name: String,
    pub value: crate::Value,
    pub value_source: String,
    pub permissions: turso_fastdb_parser::SchemaPermissions,
    pub definition: String,
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
    Vector64(usize),
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

#[derive(Debug, Clone, PartialEq)]
pub enum CatalogState {
    Empty,
    Ready(Box<CatalogSnapshot>),
}

impl CatalogState {
    pub fn snapshot(&self) -> Option<&CatalogSnapshot> {
        match self {
            Self::Empty => None,
            Self::Ready(snapshot) => Some(snapshot.as_ref()),
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
        drop: false,
        permissions: turso_fastdb_parser::SchemaPermissions::None,
        comment: None,
        fields: BTreeMap::new(),
        indexes: BTreeMap::new(),
        events: BTreeMap::new(),
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

pub fn allocate_vector_hidden_column(
    table_id: CatalogId,
    field_path_key: String,
    dimension: u32,
    ordinal: usize,
) -> HiddenColumnDefinition {
    let id = CatalogId::new_random();
    HiddenColumnDefinition {
        id,
        table_id,
        index_id: None,
        field_path_key: Some(field_path_key),
        physical_name: physical_hidden_column_name(id),
        provider: Provider::BuiltinVector,
        role: HiddenColumnRole::Vector64(ordinal),
        physical_encoding: "VECTOR64".to_string(),
        dimension: Some(i64::from(dimension)),
        options_json: format!("{{\"ordinal\":{ordinal},\"role\":\"vector64\"}}"),
        state: ProviderState::Ready,
        provider_version: BUILTIN_VECTOR_PROVIDER_VERSION,
        encoding_version: BUILTIN_VECTOR_ENCODING_VERSION,
    }
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
    create_format_three_catalogs(conn)?;
    let database_id = format!("{:032x}", rand::random::<u128>());
    let creation_version = env!("CARGO_PKG_VERSION").to_string();
    let (statement, bindings) = lower::meta_insert(&database_id, &creation_version, LAST_MIGRATION);
    conn.exec_bound(statement, bindings)?;
    Ok(CatalogSnapshot {
        metadata: Metadata {
            database_id,
            creation_version,
            last_migration: LAST_MIGRATION,
            document_encoding_version: DOCUMENT_ENCODING_VERSION,
        },
        tables: BTreeMap::new(),
        analyzers: BTreeMap::new(),
        parameters: BTreeMap::new(),
        functions: BTreeMap::new(),
        views: BTreeMap::new(),
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

pub fn replace_table(conn: &Connection, table: &TableDefinition) -> Result<()> {
    let (statement, bindings) = lower::table_delete(&table.id.to_hex());
    conn.exec_bound(statement, bindings)?;
    persist_table(conn, table)
}

pub fn remove_table_catalog(conn: &Connection, table: &TableDefinition) -> Result<()> {
    let table_id = table.id.to_hex();
    for (statement, bindings) in [
        lower::events_delete_table(&table_id),
        lower::hidden_columns_delete_table(&table_id),
        lower::indexes_delete_table(&table_id),
        lower::fields_delete_table(&table_id),
        lower::table_delete(&table_id),
    ] {
        conn.exec_bound(statement, bindings)?;
    }
    Ok(())
}

pub fn remove_capability(conn: &Connection, provider: &str) -> Result<()> {
    let (statement, bindings) = lower::capability_delete(provider);
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
            Provider::BuiltinVector => BUILTIN_VECTOR_PROVIDER,
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

pub fn allocate_parameter(
    logical_name: &str,
    value: crate::Value,
    value_source: String,
    permissions: turso_fastdb_parser::SchemaPermissions,
    definition: String,
) -> Result<ParameterDefinition> {
    if logical_name.is_empty() || is_reserved_logical_name(logical_name) {
        return Err(FastDbError::Constraint(
            "database parameter has an invalid logical name".into(),
        ));
    }
    Ok(ParameterDefinition {
        id: CatalogId::new_random(),
        logical_name: logical_name.to_string(),
        value,
        value_source,
        permissions,
        definition,
    })
}

pub fn allocate_function(
    logical_name: &str,
    arguments: Vec<FunctionArgumentDefinition>,
    body: turso_fastdb_parser::ScriptBlock,
    body_source: String,
    permissions: turso_fastdb_parser::SchemaPermissions,
    definition: String,
) -> Result<FunctionDefinition> {
    if logical_name.is_empty()
        || logical_name
            .split("::")
            .any(|segment| segment.is_empty() || is_reserved_logical_name(segment))
    {
        return Err(FastDbError::Constraint(
            "custom function has an invalid logical name".into(),
        ));
    }
    Ok(FunctionDefinition {
        id: CatalogId::new_random(),
        logical_name: logical_name.to_string(),
        arguments,
        body,
        body_source,
        permissions,
        definition,
    })
}

pub fn persist_function(conn: &Connection, function: &FunctionDefinition) -> Result<()> {
    let arguments = serde_json::to_string(&function.arguments).map_err(|error| {
        FastDbError::Engine(format!("failed to encode function arguments: {error}"))
    })?;
    let (statement, bindings) = lower::function_insert(
        &function.id.to_hex(),
        &function.logical_name,
        &arguments,
        &function.body_source,
        EXPRESSION_VERSION,
        "{\"call_limit\":10000,\"recursion_limit\":32}",
        &function.definition,
    );
    conn.exec_bound(statement, bindings)
}

pub fn remove_function(conn: &Connection, function: &FunctionDefinition) -> Result<()> {
    let (statement, bindings) = lower::function_delete(&function.id.to_hex());
    conn.exec_bound(statement, bindings)
}

pub fn allocate_view(
    id: CatalogId,
    logical_name: &str,
    select: turso_fastdb_parser::SelectStatement,
    select_source: String,
    dependencies: Vec<CatalogId>,
    definition: String,
) -> Result<ViewDefinition> {
    if logical_name.is_empty() || is_reserved_logical_name(logical_name) {
        return Err(FastDbError::Constraint(
            "view has an invalid logical name".into(),
        ));
    }
    Ok(ViewDefinition {
        id,
        logical_name: logical_name.to_string(),
        select,
        select_source,
        dependencies,
        definition,
    })
}

pub fn persist_view(conn: &Connection, view: &ViewDefinition) -> Result<()> {
    let dependencies = view
        .dependencies
        .iter()
        .map(|id| id.to_hex())
        .collect::<Vec<_>>();
    let dependencies_json = serde_json::to_string(&dependencies).map_err(|error| {
        FastDbError::Engine(format!("failed to encode view dependencies: {error}"))
    })?;
    let (statement, bindings) = lower::view_insert(
        &view.id.to_hex(),
        &view.logical_name,
        &view.definition,
        EXPRESSION_VERSION,
        &dependencies_json,
    );
    conn.exec_bound(statement, bindings)
}

pub fn remove_view(conn: &Connection, view: &ViewDefinition) -> Result<()> {
    let (statement, bindings) = lower::view_delete(&view.id.to_hex());
    conn.exec_bound(statement, bindings)
}

pub fn allocate_event(table_id: CatalogId, input: NewEventDefinition) -> Result<EventDefinition> {
    if input.logical_name.is_empty() || is_reserved_logical_name(&input.logical_name) {
        return Err(FastDbError::Constraint(
            "event has an invalid logical name".into(),
        ));
    }
    Ok(EventDefinition {
        id: CatalogId::new_random(),
        table_id,
        logical_name: input.logical_name,
        condition: input.condition,
        condition_source: input.condition_source,
        action: input.action,
        action_source: input.action_source,
        comment: input.comment,
        definition: input.definition,
    })
}

pub fn persist_event(conn: &Connection, event: &EventDefinition) -> Result<()> {
    let (statement, bindings) = lower::event_insert(
        &event.id.to_hex(),
        &event.table_id.to_hex(),
        &event.logical_name,
        &event.condition_source,
        &event.action_source,
        EXPRESSION_VERSION,
        EVENT_RECURSION_LIMIT,
        &event.definition,
    );
    conn.exec_bound(statement, bindings)
}

pub fn remove_event(conn: &Connection, event: &EventDefinition) -> Result<()> {
    let (statement, bindings) = lower::event_delete(&event.id.to_hex());
    conn.exec_bound(statement, bindings)
}

pub fn persist_parameter(conn: &Connection, parameter: &ParameterDefinition) -> Result<()> {
    let encoded = serde_json::to_string(&crate::decode::encode_value(&parameter.value)?)
        .map_err(|error| FastDbError::Engine(format!("failed to encode parameter: {error}")))?;
    let (statement, bindings) = lower::parameter_insert(
        &parameter.id.to_hex(),
        &parameter.logical_name,
        &encoded,
        DOCUMENT_ENCODING_VERSION,
        &parameter.definition,
    );
    conn.exec_bound(statement, bindings)
}

pub fn remove_parameter(conn: &Connection, parameter: &ParameterDefinition) -> Result<()> {
    let (statement, bindings) = lower::parameter_delete(&parameter.id.to_hex());
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

pub fn persist_vector_capability(conn: &Connection) -> Result<()> {
    let (statement, bindings) = lower::capability_insert(
        BUILTIN_VECTOR_PROVIDER,
        BUILTIN_VECTOR_PROVIDER_VERSION,
        BUILTIN_VECTOR_ENCODING_VERSION,
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

pub fn remove_field(conn: &Connection, table: &TableDefinition, path_key: &str) -> Result<()> {
    let (statement, bindings) = lower::field_delete(&table.id.to_hex(), path_key);
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
            Provider::BuiltinVector => {
                return Err(FastDbError::format(
                    "exact vector provider does not own indexes",
                ));
            }
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
    let meta = conn.collect_rows(lower::meta_schema_stmt(), vec![])?;
    if meta.is_empty() {
        let schema = read_schema(conn)?;
        if schema.is_empty() {
            return Ok(CatalogState::Empty);
        }
        return Err(FastDbError::format(
            "nonempty database has no FastDB format metadata",
        ));
    }
    if meta.len() != 1 {
        return Err(FastDbError::format(
            "FastDB metadata schema lookup is not unique",
        ));
    }

    // Format is deliberately the first catalog value interpreted.
    let format_rows = conn
        .collect_rows(lower::meta_format_stmt(), vec![])
        .map_err(|_| FastDbError::format("FastDB metadata cannot be read"))?;
    let format = singleton_integer(&format_rows, "format_version")?;
    if format == 0 {
        return Err(FastDbError::format(
            "disposable FastDB format 0 cannot be opened as stable format 3",
        ));
    }
    match format {
        1 => {
            migrate_format_one_to_three(conn)?;
        }
        2 => {
            migrate_format_two_to_three(conn)?;
        }
        FORMAT_VERSION => {}
        _ => {
            return Err(FastDbError::format(format!(
                "unknown FastDB format version {format}; supported migration inputs are 1 and 2; current version is {FORMAT_VERSION}"
            )))
        }
    }

    let schema = read_schema(conn)?;
    validate_catalog_schema(&schema)?;
    let metadata = load_metadata(conn)?;
    if metadata.last_migration != LAST_MIGRATION {
        return Err(FastDbError::format(format!(
            "unknown FastDB migration level {}",
            metadata.last_migration
        )));
    }
    if metadata.document_encoding_version != DOCUMENT_ENCODING_VERSION {
        return Err(FastDbError::format(format!(
            "unknown document encoding version {}",
            metadata.document_encoding_version
        )));
    }
    let mut snapshot = CatalogSnapshot {
        metadata,
        tables: load_tables(conn)?,
        analyzers: load_analyzers(conn)?,
        parameters: load_parameters(conn)?,
        functions: load_functions(conn)?,
        views: BTreeMap::new(),
        hidden_columns: load_hidden_columns(conn)?,
        capabilities: load_capabilities(conn)?,
    };
    load_views(conn, &mut snapshot)?;
    validate_future_catalogs_empty(conn)?;
    load_events(conn, &mut snapshot)?;
    load_fields(conn, &mut snapshot)?;
    load_indexes(conn, &mut snapshot)?;
    validate_table_definition_ownership(&snapshot)?;
    validate_graph_catalog(&snapshot)?;
    validate_fts_catalog(&snapshot)?;
    validate_vector_catalog(&snapshot)?;
    validate_physical_objects(&schema, &snapshot, FORMAT_VERSION)?;
    validate_vector_storage(conn, &snapshot)?;
    crate::execute::validate_materialized_views(conn, &snapshot)?;
    Ok(CatalogState::Ready(Box::new(snapshot)))
}

fn load_metadata(conn: &Connection) -> Result<Metadata> {
    let metadata_rows = conn.collect_rows(lower::meta_stmt(), vec![])?;
    let metadata_row = singleton_row(&metadata_rows, "metadata")?;
    if metadata_row.len() != 6 {
        return Err(FastDbError::format("metadata row has wrong width"));
    }
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
        document_encoding_version: format_integer(&metadata_row[5], "document_encoding_version")?,
    })
}

fn load_parameters(conn: &Connection) -> Result<BTreeMap<String, ParameterDefinition>> {
    let rows = conn.collect_rows(lower::parameters_stmt(), vec![])?;
    let mut parameters = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for row in rows {
        if row.len() != 5 {
            return Err(FastDbError::format("parameter catalog row has wrong width"));
        }
        let id = CatalogId::from_hex(&format_text(&row[0], "parameter_id")?)?;
        let logical_name = format_text(&row[1], "logical_name")?;
        if logical_name.is_empty() || is_reserved_logical_name(&logical_name) {
            return Err(FastDbError::format(
                "parameter catalog has an invalid logical name",
            ));
        }
        let encoded = format_text(&row[2], "value_json")?;
        let json: serde_json::Value = serde_json::from_str(&encoded)
            .map_err(|_| FastDbError::format("parameter value is malformed"))?;
        if serde_json::to_string(&json)
            .map_err(|error| FastDbError::Engine(format!("failed to encode parameter: {error}")))?
            != encoded
        {
            return Err(FastDbError::format(
                "parameter value is not canonically encoded",
            ));
        }
        let value = crate::decode::decode_value(json)?;
        if format_integer(&row[3], "encoding_version")? != DOCUMENT_ENCODING_VERSION {
            return Err(FastDbError::format(
                "parameter has an unsupported encoding version",
            ));
        }
        let definition = format_text(&row[4], "definition")?;
        if definition.is_empty() {
            return Err(FastDbError::format("parameter catalog definition is empty"));
        }
        let parsed = turso_fastdb_parser::parse_one(&definition)
            .map_err(|_| FastDbError::format("parameter definition cannot be parsed"))?;
        let turso_fastdb_parser::Statement::DefineParam(parsed) = parsed else {
            return Err(FastDbError::format(
                "parameter definition has the wrong statement kind",
            ));
        };
        if parsed.if_not_exists.is_some()
            || parsed.overwrite.is_some()
            || parsed.name.value != logical_name
        {
            return Err(FastDbError::format(
                "parameter definition does not match catalog ownership",
            ));
        }
        let value_source = definition
            .get(parsed.value.span.offset..parsed.value.span.end())
            .ok_or_else(|| FastDbError::format("parameter value span is invalid"))?
            .to_string();
        let parameter = ParameterDefinition {
            id,
            logical_name: logical_name.clone(),
            value,
            value_source,
            permissions: parsed.permissions,
            definition,
        };
        if !ids.insert(id) || parameters.insert(logical_name, parameter).is_some() {
            return Err(FastDbError::format(
                "parameter catalog contains duplicate ownership",
            ));
        }
    }
    Ok(parameters)
}

fn load_functions(conn: &Connection) -> Result<BTreeMap<String, FunctionDefinition>> {
    const LIMITS: &str = "{\"call_limit\":10000,\"recursion_limit\":32}";
    let rows = conn.collect_rows(lower::functions_stmt(), vec![])?;
    let mut functions = BTreeMap::new();
    let mut ids = BTreeSet::new();
    for row in rows {
        if row.len() != 7 {
            return Err(FastDbError::format("function catalog row has wrong width"));
        }
        let id = CatalogId::from_hex(&format_text(&row[0], "function_id")?)?;
        let logical_name = format_text(&row[1], "logical_name")?;
        let arguments_json = format_text(&row[2], "arguments_ast")?;
        let arguments: Vec<FunctionArgumentDefinition> = serde_json::from_str(&arguments_json)
            .map_err(|_| FastDbError::format("function arguments are malformed"))?;
        if serde_json::to_string(&arguments).map_err(|error| {
            FastDbError::Engine(format!("failed to encode function arguments: {error}"))
        })? != arguments_json
        {
            return Err(FastDbError::format(
                "function arguments are not canonically encoded",
            ));
        }
        let mut argument_names = BTreeSet::new();
        for argument in &arguments {
            if argument.name.is_empty()
                || !argument_names.insert(argument.name.clone())
                || FieldType::parse_canonical(&argument.ty).is_err()
            {
                return Err(FastDbError::format(
                    "function arguments contain an invalid name or type",
                ));
            }
        }
        let body_source = format_text(&row[3], "body_source")?;
        if body_source.is_empty()
            || format_integer(&row[4], "ast_version")? != EXPRESSION_VERSION
            || format_text(&row[5], "limits_json")? != LIMITS
        {
            return Err(FastDbError::format(
                "function AST version or limits are unsupported",
            ));
        }
        let definition = format_text(&row[6], "definition")?;
        let parsed = turso_fastdb_parser::parse_one(&definition)
            .map_err(|_| FastDbError::format("function definition cannot be parsed"))?;
        let turso_fastdb_parser::Statement::DefineFunction(parsed) = parsed else {
            return Err(FastDbError::format(
                "function definition has the wrong statement kind",
            ));
        };
        let parsed_name = parsed
            .name
            .iter()
            .skip(1)
            .map(|segment| segment.value.as_str())
            .collect::<Vec<_>>()
            .join("::");
        let parsed_arguments = parsed
            .arguments
            .iter()
            .map(|argument| FunctionArgumentDefinition {
                name: argument.name.value.clone(),
                ty: FieldType::from_parser(&argument.ty).canonical(),
            })
            .collect::<Vec<_>>();
        let parsed_body = definition
            .get(parsed.body.span.offset..parsed.body.span.end())
            .ok_or_else(|| FastDbError::format("function body span is invalid"))?;
        if parsed.if_not_exists.is_some()
            || parsed.overwrite.is_some()
            || parsed_name != logical_name
            || parsed_arguments != arguments
            || parsed_body != body_source
            || canonical_function_definition(
                &logical_name,
                &arguments,
                &body_source,
                parsed.permissions,
            ) != definition
        {
            return Err(FastDbError::format(
                "function definition does not match catalog ownership",
            ));
        }
        let function = FunctionDefinition {
            id,
            logical_name: logical_name.clone(),
            arguments,
            body: parsed.body,
            body_source,
            permissions: parsed.permissions,
            definition,
        };
        if !ids.insert(id) || functions.insert(logical_name, function).is_some() {
            return Err(FastDbError::format(
                "function catalog contains duplicate ownership",
            ));
        }
    }
    Ok(functions)
}

fn load_views(conn: &Connection, snapshot: &mut CatalogSnapshot) -> Result<()> {
    let rows = conn.collect_rows(lower::views_stmt(), vec![])?;
    let mut ids = BTreeSet::new();
    for row in rows {
        if row.len() != 5 {
            return Err(FastDbError::format("view catalog row has wrong width"));
        }
        let id = CatalogId::from_hex(&format_text(&row[0], "view_id")?)?;
        let logical_name = format_text(&row[1], "logical_name")?;
        let definition = format_text(&row[2], "definition")?;
        if format_integer(&row[3], "ast_version")? != EXPRESSION_VERSION {
            return Err(FastDbError::format(
                "view catalog has an unsupported AST version",
            ));
        }
        let dependencies_json = format_text(&row[4], "dependencies_json")?;
        let dependency_hex: Vec<String> = serde_json::from_str(&dependencies_json)
            .map_err(|_| FastDbError::format("view dependencies are malformed"))?;
        if serde_json::to_string(&dependency_hex).map_err(|error| {
            FastDbError::Engine(format!("failed to encode view dependencies: {error}"))
        })? != dependencies_json
        {
            return Err(FastDbError::format(
                "view dependencies are not canonically encoded",
            ));
        }
        let dependencies = dependency_hex
            .iter()
            .map(|value| CatalogId::from_hex(value))
            .collect::<Result<Vec<_>>>()?;
        if dependencies.is_empty()
            || dependencies.windows(2).any(|pair| pair[0] >= pair[1])
            || dependencies.contains(&id)
        {
            return Err(FastDbError::format(
                "view dependencies are empty, duplicated, unsorted, or self-referential",
            ));
        }
        let table = snapshot.tables.get(&logical_name).ok_or_else(|| {
            FastDbError::format("view catalog belongs to an unknown physical table")
        })?;
        if table.id != id || table.kind != TableKind::Normal {
            return Err(FastDbError::format(
                "view catalog ownership does not match its physical table",
            ));
        }
        let parsed = turso_fastdb_parser::parse_one(&definition)
            .map_err(|_| FastDbError::format("view definition cannot be parsed"))?;
        let turso_fastdb_parser::Statement::DefineTable(parsed) = parsed else {
            return Err(FastDbError::format(
                "view definition has the wrong statement kind",
            ));
        };
        let select = *parsed
            .view
            .ok_or_else(|| FastDbError::format("view definition has no SELECT"))?;
        let source_name = match &select.target {
            turso_fastdb_parser::SelectTarget::Target(turso_fastdb_parser::Target::Table(
                source,
            )) if select.additional_targets.is_empty() => &source.name.value,
            _ => {
                return Err(FastDbError::format(
                    "view definition has unsupported source ownership",
                ));
            }
        };
        let source = snapshot
            .tables
            .get(source_name)
            .ok_or_else(|| FastDbError::format("view depends on an unknown table"))?;
        if dependencies != vec![source.id]
            || parsed.if_not_exists.is_some()
            || parsed.overwrite.is_some()
            || parsed.name.value != logical_name
            || parsed.drop.is_some()
            || parsed.mode.value != TableMode::Schemaless
            || !matches!(
                parsed.kind,
                turso_fastdb_parser::TableKindSyntax::Normal { .. }
            )
            || table.definition.as_deref() != Some(definition.as_str())
        {
            return Err(FastDbError::format(
                "view definition does not match catalog ownership",
            ));
        }
        let select_source = definition
            .get(select.span.offset..select.span.end())
            .ok_or_else(|| FastDbError::format("view SELECT span is invalid"))?
            .to_string();
        let view = ViewDefinition {
            id,
            logical_name: logical_name.clone(),
            select,
            select_source,
            dependencies,
            definition,
        };
        if !ids.insert(id) || snapshot.views.insert(logical_name, view).is_some() {
            return Err(FastDbError::format(
                "view catalog contains duplicate ownership",
            ));
        }
    }
    validate_view_dependency_cycles(snapshot)
}

fn validate_view_dependency_cycles(snapshot: &CatalogSnapshot) -> Result<()> {
    fn visit(
        id: CatalogId,
        snapshot: &CatalogSnapshot,
        visiting: &mut BTreeSet<CatalogId>,
        visited: &mut BTreeSet<CatalogId>,
    ) -> Result<()> {
        if visited.contains(&id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return Err(FastDbError::format(
                "view dependency graph contains a cycle",
            ));
        }
        if let Some(view) = snapshot.views.values().find(|view| view.id == id) {
            for dependency in &view.dependencies {
                if snapshot.views.values().any(|view| view.id == *dependency) {
                    visit(*dependency, snapshot, visiting, visited)?;
                }
            }
        }
        visiting.remove(&id);
        visited.insert(id);
        Ok(())
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for view in snapshot.views.values() {
        visit(view.id, snapshot, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn load_events(conn: &Connection, snapshot: &mut CatalogSnapshot) -> Result<()> {
    let rows = conn.collect_rows(lower::events_stmt(), vec![])?;
    let mut ids = BTreeSet::new();
    for row in rows {
        if row.len() != 8 {
            return Err(FastDbError::format("event catalog row has wrong width"));
        }
        let id = CatalogId::from_hex(&format_text(&row[0], "event_id")?)?;
        let table_id = CatalogId::from_hex(&format_text(&row[1], "table_id")?)?;
        let logical_name = format_text(&row[2], "logical_name")?;
        if logical_name.is_empty() || is_reserved_logical_name(&logical_name) {
            return Err(FastDbError::format(
                "event catalog has an invalid logical name",
            ));
        }
        let condition_source = format_text(&row[3], "when_source")?;
        let action_source = format_text(&row[4], "then_source")?;
        if condition_source.is_empty()
            || action_source.is_empty()
            || format_integer(&row[5], "expression_version")? != EXPRESSION_VERSION
            || format_integer(&row[6], "recursion_limit")? != EVENT_RECURSION_LIMIT
        {
            return Err(FastDbError::format(
                "event expression version or recursion limit is unsupported",
            ));
        }
        let definition = format_text(&row[7], "definition")?;
        let parsed = turso_fastdb_parser::parse_one(&definition)
            .map_err(|_| FastDbError::format("event definition cannot be parsed"))?;
        let turso_fastdb_parser::Statement::DefineEvent(parsed) = parsed else {
            return Err(FastDbError::format(
                "event definition has the wrong statement kind",
            ));
        };
        let parsed_condition_source = parsed
            .condition
            .as_ref()
            .map(|condition| {
                definition
                    .get(condition.span.offset..condition.span.end())
                    .ok_or_else(|| FastDbError::format("event WHEN span is invalid"))
            })
            .transpose()?
            .unwrap_or_default();
        let parsed_action_source = definition
            .get(parsed.action.span.offset..parsed.action.span.end())
            .ok_or_else(|| FastDbError::format("event THEN span is invalid"))?;
        let table = snapshot
            .tables
            .values_mut()
            .find(|table| table.id == table_id)
            .ok_or_else(|| FastDbError::format("event belongs to an unknown table"))?;
        let comment = parsed.comment.as_ref().map(|comment| comment.value.clone());
        if parsed.if_not_exists.is_some()
            || parsed.overwrite.is_some()
            || parsed.name.value != logical_name
            || parsed.table.value != table.logical_name
            || parsed_condition_source != condition_source
            || parsed_action_source != action_source
            || canonical_event_definition(
                &logical_name,
                &table.logical_name,
                &condition_source,
                &action_source,
                comment.as_deref(),
            ) != definition
        {
            return Err(FastDbError::format(
                "event definition does not match catalog ownership",
            ));
        }
        let event = EventDefinition {
            id,
            table_id,
            logical_name: logical_name.clone(),
            condition: parsed.condition,
            condition_source,
            action: parsed.action,
            action_source,
            comment,
            definition,
        };
        if !ids.insert(id) || table.events.insert(logical_name, event).is_some() {
            return Err(FastDbError::format(
                "event catalog contains duplicate ownership",
            ));
        }
    }
    Ok(())
}

pub fn canonical_event_definition(
    logical_name: &str,
    table_name: &str,
    condition_source: &str,
    action_source: &str,
    comment: Option<&str>,
) -> String {
    let mut definition = format!(
        "DEFINE EVENT {} ON TABLE {}",
        render_catalog_identifier(logical_name),
        render_catalog_identifier(table_name),
    );
    definition.push_str(" WHEN ");
    definition.push_str(condition_source.trim());
    definition.push_str(" THEN ");
    definition.push_str(action_source.trim());
    if let Some(comment) = comment {
        definition.push_str(" COMMENT ");
        definition.push_str(&render_catalog_string(comment));
    }
    definition
}

fn render_catalog_identifier(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .next()
            .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && value
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        value.to_string()
    } else {
        format!("`{}`", value.replace('`', "``"))
    }
}

fn render_catalog_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

pub fn canonical_function_definition(
    logical_name: &str,
    arguments: &[FunctionArgumentDefinition],
    body_source: &str,
    permissions: turso_fastdb_parser::SchemaPermissions,
) -> String {
    let arguments = arguments
        .iter()
        .map(|argument| format!("${}: {}", argument.name, argument.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let permissions = match permissions {
        turso_fastdb_parser::SchemaPermissions::Full => "FULL",
        turso_fastdb_parser::SchemaPermissions::None => "NONE",
    };
    format!(
        "DEFINE FUNCTION fn::{logical_name}({arguments}) {} PERMISSIONS {permissions}",
        body_source.trim()
    )
}

fn migrate_format_one_to_three(conn: &Connection) -> Result<()> {
    conn.with_transaction(|| {
        let schema = read_schema(conn)?;
        validate_catalog_schema_v1(&schema)?;
        let metadata_rows = conn.collect_rows(lower::meta_v2_stmt(), vec![])?;
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
                document_encoding_version: 1,
            },
            tables: load_tables_v1(conn)?,
            analyzers: BTreeMap::new(),
            parameters: BTreeMap::new(),
            functions: BTreeMap::new(),
            views: BTreeMap::new(),
            hidden_columns: BTreeMap::new(),
            capabilities: BTreeMap::new(),
        };
        load_fields(conn, &mut prior)?;
        load_indexes_v1(conn, &mut prior)?;
        validate_physical_objects(&schema, &prior, 1)?;

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
        conn.exec_bound(lower::catalog_hidden_columns_v2_ddl(), vec![])?;
        conn.exec_bound(lower::catalog_capabilities_ddl(), vec![])?;
        conn.check_failpoint(crate::Failpoint::AfterFormat2Catalogs)?;

        let migrated_schema = read_schema(conn)?;
        validate_catalog_schema_v2(&migrated_schema)?;
        let mut migrated = CatalogSnapshot {
            metadata: prior.metadata.clone(),
            tables: load_tables(conn)?,
            analyzers: load_analyzers(conn)?,
            parameters: BTreeMap::new(),
            functions: BTreeMap::new(),
            views: BTreeMap::new(),
            hidden_columns: load_hidden_columns_v2(conn)?,
            capabilities: load_capabilities(conn)?,
        };
        load_fields(conn, &mut migrated)?;
        load_indexes_v2(conn, &mut migrated)?;
        validate_physical_objects(&migrated_schema, &migrated, 2)?;
        if migrated.tables != prior.tables {
            return Err(FastDbError::format(
                "format-2 migration changed logical table or index ownership",
            ));
        }
        conn.check_failpoint(crate::Failpoint::AfterFormat2Validation)?;
        apply_format_three(conn, &migrated)
    })
}

fn migrate_format_two_to_three(conn: &Connection) -> Result<()> {
    conn.with_transaction(|| {
        let schema = read_schema(conn)?;
        validate_catalog_schema_v2(&schema)?;
        let metadata_rows = conn.collect_rows(lower::meta_v2_stmt(), vec![])?;
        let metadata_row = singleton_row(&metadata_rows, "metadata")?;
        if format_integer(&metadata_row[0], "format_version")? != 2
            || format_integer(&metadata_row[1], "dialect_version")? != DIALECT_VERSION
            || format_integer(&metadata_row[4], "last_migration")? != 2
        {
            return Err(FastDbError::format(
                "format-2 metadata is incompatible with migration",
            ));
        }
        let database_id = format_text(&metadata_row[2], "database_id")?;
        CatalogId::from_hex(&database_id)?;
        let creation_version = format_text(&metadata_row[3], "creation_version")?;
        if creation_version.is_empty() {
            return Err(FastDbError::format("metadata creation version is empty"));
        }
        let mut prior = CatalogSnapshot {
            metadata: Metadata {
                database_id,
                creation_version,
                last_migration: 2,
                document_encoding_version: 1,
            },
            tables: load_tables(conn)?,
            analyzers: load_analyzers(conn)?,
            parameters: BTreeMap::new(),
            functions: BTreeMap::new(),
            views: BTreeMap::new(),
            hidden_columns: load_hidden_columns_v2(conn)?,
            capabilities: load_capabilities(conn)?,
        };
        load_fields(conn, &mut prior)?;
        load_indexes_v2(conn, &mut prior)?;
        validate_graph_catalog(&prior)?;
        validate_fts_catalog(&prior)?;
        validate_vector_catalog(&prior)?;
        validate_physical_objects(&schema, &prior, 2)?;
        validate_vector_storage(conn, &prior)?;
        apply_format_three(conn, &prior)
    })
}

fn apply_format_three(conn: &Connection, prior: &CatalogSnapshot) -> Result<()> {
    conn.exec_bound(
        lower::add_catalog_column(META_TABLE, lower::format3_meta_column()),
        vec![],
    )?;
    conn.check_failpoint(crate::Failpoint::AfterFormat3Metadata)?;
    conn.exec_bound(
        lower::add_catalog_column(INDEXES_TABLE, lower::format3_provider_column()),
        vec![],
    )?;
    conn.exec_bound(
        lower::add_catalog_column(HIDDEN_COLUMNS_TABLE, lower::format3_provider_column()),
        vec![],
    )?;
    conn.check_failpoint(crate::Failpoint::AfterFormat3ProviderColumns)?;
    create_format_three_catalogs(conn)?;
    conn.check_failpoint(crate::Failpoint::AfterFormat3Catalogs)?;

    let migrated_schema = read_schema(conn)?;
    validate_catalog_schema(&migrated_schema)?;
    let mut migrated = CatalogSnapshot {
        metadata: Metadata {
            document_encoding_version: DOCUMENT_ENCODING_VERSION,
            ..prior.metadata.clone()
        },
        tables: load_tables(conn)?,
        analyzers: load_analyzers(conn)?,
        parameters: load_parameters(conn)?,
        functions: load_functions(conn)?,
        views: BTreeMap::new(),
        hidden_columns: load_hidden_columns(conn)?,
        capabilities: load_capabilities(conn)?,
    };
    load_fields(conn, &mut migrated)?;
    load_indexes(conn, &mut migrated)?;
    validate_graph_catalog(&migrated)?;
    validate_fts_catalog(&migrated)?;
    validate_vector_catalog(&migrated)?;
    validate_physical_objects(&migrated_schema, &migrated, FORMAT_VERSION)?;
    validate_vector_storage(conn, &migrated)?;
    if migrated.tables != prior.tables
        || migrated.analyzers != prior.analyzers
        || migrated.hidden_columns != prior.hidden_columns
        || migrated.capabilities != prior.capabilities
    {
        return Err(FastDbError::format(
            "format-3 migration changed existing catalog ownership",
        ));
    }
    conn.check_failpoint(crate::Failpoint::AfterFormat3Validation)?;
    conn.exec_bound(lower::migrate_to_three_stmt(), vec![])?;
    let metadata = load_metadata(conn)?;
    if metadata.last_migration != LAST_MIGRATION
        || metadata.document_encoding_version != DOCUMENT_ENCODING_VERSION
    {
        return Err(FastDbError::format(
            "format-3 migration failed to publish its complete header",
        ));
    }
    conn.check_failpoint(crate::Failpoint::AfterMigration)
}

fn create_format_three_catalogs(conn: &Connection) -> Result<()> {
    for statement in [
        lower::catalog_functions_ddl(),
        lower::catalog_parameters_ddl(),
        lower::catalog_views_ddl(),
        lower::catalog_events_ddl(),
        lower::catalog_permissions_ddl(),
        lower::catalog_users_ddl(),
        lower::catalog_accesses_ddl(),
    ] {
        conn.exec_bound(statement, vec![])?;
    }
    Ok(())
}

fn validate_future_catalogs_empty(conn: &Connection) -> Result<()> {
    for (table, id_column) in [
        (PERMISSIONS_TABLE, "permission_id"),
        (USERS_TABLE, "user_id"),
        (ACCESSES_TABLE, "access_id"),
    ] {
        if !conn
            .collect_rows(lower::future_catalog_stmt(table, id_column), vec![])?
            .is_empty()
        {
            return Err(FastDbError::format(format!(
                "format-3 catalog {table} contains rows owned by a future phase"
            )));
        }
    }
    Ok(())
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
        let (drop, permissions, comment) = definition.as_deref().map_or_else(
            || Ok((false, turso_fastdb_parser::SchemaPermissions::None, None)),
            |definition| {
                let parsed = turso_fastdb_parser::parse_one(definition)
                    .map_err(|_| FastDbError::format("stored table definition does not parse"))?;
                let turso_fastdb_parser::Statement::DefineTable(statement) = parsed else {
                    return Err(FastDbError::format(
                        "stored table definition has the wrong statement kind",
                    ));
                };
                if statement.name.value != logical_name || statement.mode.value != mode {
                    return Err(FastDbError::format(
                        "stored table definition disagrees with catalog identity or mode",
                    ));
                }
                let definition_kind = match statement.kind {
                    turso_fastdb_parser::TableKindSyntax::Normal { .. } => TableKind::Normal,
                    turso_fastdb_parser::TableKindSyntax::Relation(_) => TableKind::Relation,
                };
                if definition_kind != kind {
                    return Err(FastDbError::format(
                        "stored table definition disagrees with catalog kind",
                    ));
                }
                Ok((
                    statement.drop.is_some(),
                    statement.permissions,
                    statement.comment.map(|comment| comment.value),
                ))
            },
        )?;
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
                        drop,
                        permissions,
                        comment,
                        fields: BTreeMap::new(),
                        indexes: BTreeMap::new(),
                        events: BTreeMap::new(),
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
        let parsed = turso_fastdb_parser::parse_one(&definition)
            .map_err(|_| FastDbError::format("stored field definition does not parse"))?;
        let turso_fastdb_parser::Statement::DefineField(parsed) = parsed else {
            return Err(FastDbError::format(
                "stored field definition has the wrong statement kind",
            ));
        };
        let (_, parsed_path_key) = crate::path::parser_path(&parsed.path)
            .map_err(|_| FastDbError::format("stored field definition has an invalid path"))?;
        if parsed.table.value != table.logical_name
            || parsed_path_key != path_key
            || FieldType::from_parser(&parsed.ty) != ty
        {
            return Err(FastDbError::format(
                "stored field definition disagrees with catalog ownership or type",
            ));
        }
        let expression =
            |value: turso_fastdb_parser::Expr| -> Result<crate::schema::SchemaExpression> {
                let source = definition
                    .get(value.span.offset..value.span.end())
                    .ok_or_else(|| FastDbError::format("stored field expression span is invalid"))?
                    .to_string();
                Ok(crate::schema::SchemaExpression {
                    expression: value,
                    source,
                })
            };
        let default_always = parsed
            .default
            .as_ref()
            .is_some_and(|default| default.always.is_some());
        let default = parsed
            .default
            .map(|default| expression(default.value))
            .transpose()?;
        let value = parsed.value.map(expression).transpose()?;
        let assert = parsed.assert.map(expression).transpose()?;
        for expression in default.iter().chain(value.iter()).chain(assert.iter()) {
            crate::execute::validate_schema_expression_safety(&expression.expression)
                .map_err(|_| FastDbError::format("stored field expression is context-unsafe"))?;
        }
        let rule = FieldRule {
            path,
            path_key: path_key.clone(),
            ty,
            required,
            definition,
            flexible: parsed.flexible.is_some(),
            default,
            default_always,
            value,
            assert,
            readonly: parsed.readonly.is_some(),
            reference: parsed.reference.is_some(),
            permissions: parsed.permissions,
            comment: parsed.comment.map(|comment| comment.value),
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
        true,
    )
}

fn validate_table_definition_ownership(snapshot: &CatalogSnapshot) -> Result<()> {
    for table in snapshot.tables.values() {
        let Some(definition) = &table.definition else {
            continue;
        };
        let parsed = turso_fastdb_parser::parse_one(definition)
            .map_err(|_| FastDbError::format("stored table definition does not parse"))?;
        let turso_fastdb_parser::Statement::DefineTable(statement) = parsed else {
            return Err(FastDbError::format(
                "stored table definition has the wrong statement kind",
            ));
        };
        let turso_fastdb_parser::TableKindSyntax::Relation(relation) = statement.kind else {
            continue;
        };
        if relation.enforced.is_some() != table.relation_enforced {
            return Err(FastDbError::format(
                "stored relation definition disagrees with endpoint enforcement",
            ));
        }
        for (declared, owned) in [
            (relation.input, table.relation_in_table_id),
            (relation.output, table.relation_out_table_id),
        ] {
            let declared = declared
                .map(|endpoint| {
                    snapshot
                        .tables
                        .get(&endpoint.value)
                        .map(|table| table.id)
                        .ok_or_else(|| {
                            FastDbError::format(
                                "stored relation definition names a missing endpoint table",
                            )
                        })
                })
                .transpose()?;
            if declared != owned {
                return Err(FastDbError::format(
                    "stored relation definition disagrees with endpoint ownership",
                ));
            }
        }
    }
    Ok(())
}

fn load_indexes_v2(conn: &Connection, snapshot: &mut CatalogSnapshot) -> Result<()> {
    load_index_rows(
        conn.collect_rows(lower::indexes_v2_stmt(), vec![])?,
        snapshot,
        true,
        false,
    )
}

fn load_indexes_v1(conn: &Connection, snapshot: &mut CatalogSnapshot) -> Result<()> {
    load_index_rows(
        conn.collect_rows(lower::indexes_v1_stmt(), vec![])?,
        snapshot,
        false,
        false,
    )
}

fn load_index_rows(
    rows: Vec<Vec<Value>>,
    snapshot: &mut CatalogSnapshot,
    format_two: bool,
    format_three: bool,
) -> Result<()> {
    let mut ids = BTreeSet::new();
    let mut physical_names = BTreeSet::new();
    for row in rows {
        let expected_width = if format_three {
            15
        } else if format_two {
            14
        } else {
            8
        };
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
                    BUILTIN_VECTOR_PROVIDER => Provider::BuiltinVector,
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
        if format_three && format_integer(&row[14], "auxiliary_version")? != 1 {
            return Err(FastDbError::format(
                "index has an unknown auxiliary-state version",
            ));
        }
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
                    HiddenColumnRole::Vector64(_) => usize::MAX,
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
    load_hidden_column_rows(
        conn.collect_rows(lower::hidden_columns_stmt(), vec![])?,
        true,
    )
}

fn load_hidden_columns_v2(
    conn: &Connection,
) -> Result<BTreeMap<CatalogId, HiddenColumnDefinition>> {
    load_hidden_column_rows(
        conn.collect_rows(lower::hidden_columns_v2_stmt(), vec![])?,
        false,
    )
}

fn load_hidden_column_rows(
    rows: Vec<Vec<Value>>,
    format_three: bool,
) -> Result<BTreeMap<CatalogId, HiddenColumnDefinition>> {
    let mut columns = BTreeMap::new();
    let mut physical_names = BTreeSet::new();
    for row in rows {
        if row.len() != if format_three { 13 } else { 12 } {
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
            BUILTIN_VECTOR_PROVIDER => Provider::BuiltinVector,
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
            Provider::BuiltinVector => {
                if index_id.is_some() || field_path_key.is_none() {
                    return Err(FastDbError::format(
                        "vector hidden column requires field ownership only",
                    ));
                }
                let dimension = dimension
                    .ok_or_else(|| FastDbError::format("vector hidden column has no dimension"))?;
                if !(1..=65_536).contains(&dimension) || physical_encoding != "VECTOR64" {
                    return Err(FastDbError::format(
                        "vector hidden column metadata is incompatible",
                    ));
                }
                let value: serde_json::Value = serde_json::from_str(&options_json)
                    .map_err(|_| FastDbError::format("vector hidden options are malformed"))?;
                let object = value.as_object().ok_or_else(|| {
                    FastDbError::format("vector hidden options must be an object")
                })?;
                let ordinal = object
                    .get("ordinal")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|value| usize::try_from(value).ok())
                    .ok_or_else(|| FastDbError::format("vector hidden ordinal is invalid"))?;
                if object.len() != 2
                    || object.get("role").and_then(serde_json::Value::as_str) != Some("vector64")
                    || options_json != format!("{{\"ordinal\":{ordinal},\"role\":\"vector64\"}}")
                {
                    return Err(FastDbError::format(
                        "vector hidden options are not canonical",
                    ));
                }
                HiddenColumnRole::Vector64(ordinal)
            }
            Provider::BuiltinBtree => unreachable!("matched providers exclude B-tree"),
        };
        let state = match format_text(&row[10], "state")?.as_str() {
            "READY" => ProviderState::Ready,
            "REBUILD_REQUIRED" => ProviderState::RebuildRequired,
            _ => return Err(FastDbError::format("hidden column has unknown state")),
        };
        let encoding_version = format_integer(&row[11], "encoding_version")?;
        if format_three && format_integer(&row[12], "auxiliary_version")? != 1 {
            return Err(FastDbError::format(
                "hidden column has an unknown auxiliary-state version",
            ));
        }
        let expected_versions = match provider {
            Provider::BuiltinGraph => (
                BUILTIN_GRAPH_PROVIDER_VERSION,
                BUILTIN_GRAPH_ENCODING_VERSION,
            ),
            Provider::BuiltinFts => (BUILTIN_FTS_PROVIDER_VERSION, BUILTIN_FTS_ENCODING_VERSION),
            Provider::BuiltinVector => (
                BUILTIN_VECTOR_PROVIDER_VERSION,
                BUILTIN_VECTOR_ENCODING_VERSION,
            ),
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
            BUILTIN_VECTOR_PROVIDER => {
                requirement.min_provider_version == BUILTIN_VECTOR_PROVIDER_VERSION
                    && requirement.min_encoding_version == BUILTIN_VECTOR_ENCODING_VERSION
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
        (FUNCTIONS_TABLE, lower::catalog_functions_ddl()),
        (PARAMETERS_TABLE, lower::catalog_parameters_ddl()),
        (VIEWS_TABLE, lower::catalog_views_ddl()),
        (EVENTS_TABLE, lower::catalog_events_ddl()),
        (PERMISSIONS_TABLE, lower::catalog_permissions_ddl()),
        (USERS_TABLE, lower::catalog_users_ddl()),
        (ACCESSES_TABLE, lower::catalog_accesses_ddl()),
    ] {
        require_exact_schema(schema, "table", name, name, &statement.to_string())?;
    }
    Ok(())
}

fn validate_catalog_schema_v2(schema: &[SchemaObject]) -> Result<()> {
    for (name, statement) in [
        (META_TABLE, lower::catalog_meta_v2_ddl()),
        (TABLES_TABLE, lower::catalog_tables_ddl()),
        (FIELDS_TABLE, lower::catalog_fields_ddl()),
        (INDEXES_TABLE, lower::catalog_indexes_v2_ddl()),
        (ANALYZERS_TABLE, lower::catalog_analyzers_ddl()),
        (HIDDEN_COLUMNS_TABLE, lower::catalog_hidden_columns_v2_ddl()),
        (CAPABILITIES_TABLE, lower::catalog_capabilities_ddl()),
    ] {
        require_exact_schema(schema, "table", name, name, &statement.to_string())?;
    }
    Ok(())
}

fn validate_catalog_schema_v1(schema: &[SchemaObject]) -> Result<()> {
    for (name, statement) in [
        (META_TABLE, lower::catalog_meta_v2_ddl()),
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
    catalog_format: i64,
) -> Result<()> {
    let mut expected_reserved = BTreeSet::from([
        META_TABLE.to_string(),
        TABLES_TABLE.to_string(),
        FIELDS_TABLE.to_string(),
        INDEXES_TABLE.to_string(),
    ]);
    if catalog_format >= 2 {
        expected_reserved.extend([
            ANALYZERS_TABLE.to_string(),
            HIDDEN_COLUMNS_TABLE.to_string(),
            CAPABILITIES_TABLE.to_string(),
        ]);
    }
    if catalog_format >= 3 {
        expected_reserved.extend([
            FUNCTIONS_TABLE.to_string(),
            PARAMETERS_TABLE.to_string(),
            VIEWS_TABLE.to_string(),
            EVENTS_TABLE.to_string(),
            PERMISSIONS_TABLE.to_string(),
            USERS_TABLE.to_string(),
            ACCESSES_TABLE.to_string(),
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
            HiddenColumnRole::Vector64(_) => usize::MAX,
        });
        let vector = snapshot
            .hidden_columns
            .values()
            .filter(|column| {
                column.table_id == table.id && matches!(column.role, HiddenColumnRole::Vector64(_))
            })
            .collect::<Vec<_>>();
        let mut vector = vector;
        vector.sort_by_key(|column| match column.role {
            HiddenColumnRole::Vector64(ordinal) => ordinal,
            _ => usize::MAX,
        });
        let table_ddl = lower::physical_table_with_hidden_ddl(
            &table.physical_name,
            &graph,
            &fts.into_iter()
                .map(|column| column.physical_name.clone())
                .collect::<Vec<_>>(),
            &vector
                .into_iter()
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
            HiddenColumnRole::Vector64(_) => usize::MAX,
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

fn validate_vector_catalog(snapshot: &CatalogSnapshot) -> Result<()> {
    let vector_fields = snapshot
        .tables
        .values()
        .flat_map(|table| {
            table.fields.values().filter_map(move |field| {
                let dimension = match &field.ty {
                    crate::schema::FieldType::Vector { dimension } => Some(*dimension),
                    crate::schema::FieldType::Option(inner) => match inner.as_ref() {
                        crate::schema::FieldType::Vector { dimension } => Some(*dimension),
                        _ => None,
                    },
                    _ => None,
                }?;
                Some((table, field, dimension))
            })
        })
        .collect::<Vec<_>>();
    let vector_columns = snapshot
        .hidden_columns
        .values()
        .filter(|column| matches!(column.role, HiddenColumnRole::Vector64(_)))
        .collect::<Vec<_>>();
    let requires_vector = !vector_fields.is_empty() || !vector_columns.is_empty();
    if requires_vector != snapshot.capabilities.contains_key(BUILTIN_VECTOR_PROVIDER) {
        return Err(FastDbError::format(
            "vector capability requirement disagrees with field ownership",
        ));
    }
    let mut table_ordinals = BTreeMap::<CatalogId, BTreeSet<usize>>::new();
    for column in &vector_columns {
        if column.provider != Provider::BuiltinVector {
            return Err(FastDbError::format(
                "vector hidden column has an incompatible provider",
            ));
        }
        let HiddenColumnRole::Vector64(ordinal) = column.role else {
            unreachable!("filtered vector role");
        };
        if !table_ordinals
            .entry(column.table_id)
            .or_default()
            .insert(ordinal)
        {
            return Err(FastDbError::format(
                "vector hidden columns have duplicate table ordinals",
            ));
        }
        let matches = vector_fields
            .iter()
            .filter(|(table, field, dimension)| {
                column.table_id == table.id
                    && column.field_path_key.as_deref() == Some(field.path_key.as_str())
                    && column.dimension == Some(i64::from(*dimension))
            })
            .count();
        if matches != 1 {
            return Err(FastDbError::format(
                "vector hidden column ownership or dimension is invalid",
            ));
        }
    }
    for ordinals in table_ordinals.values() {
        if ordinals.iter().copied().ne(0..ordinals.len()) {
            return Err(FastDbError::format(
                "vector hidden-column table ordinals are not contiguous",
            ));
        }
    }
    for (table, field, dimension) in vector_fields {
        let matches = vector_columns
            .iter()
            .filter(|column| {
                column.table_id == table.id
                    && column.field_path_key.as_deref() == Some(field.path_key.as_str())
                    && column.dimension == Some(i64::from(dimension))
            })
            .count();
        if matches != 1 {
            return Err(FastDbError::format(
                "vector field must own exactly one hidden vector64 column",
            ));
        }
    }
    Ok(())
}

fn validate_vector_storage(conn: &Connection, snapshot: &CatalogSnapshot) -> Result<()> {
    for table in snapshot.tables.values() {
        for column in snapshot.hidden_columns.values().filter(|column| {
            column.table_id == table.id && matches!(column.role, HiddenColumnRole::Vector64(_))
        }) {
            let path_key = column.field_path_key.as_deref().ok_or_else(|| {
                FastDbError::format("vector hidden column has no field ownership")
            })?;
            let path = decode_canonical_path(path_key)?;
            let dimension = column
                .dimension
                .and_then(|value| usize::try_from(value).ok())
                .ok_or_else(|| FastDbError::format("vector hidden dimension is invalid"))?;
            let mut after_rid = None;
            loop {
                let (statement, bindings) = lower::physical_vector_validation_stmt(
                    &table.physical_name,
                    &column.physical_name,
                    after_rid.as_deref(),
                )?;
                let rows = conn.collect_rows(statement, bindings)?;
                if rows.is_empty() {
                    break;
                }
                let row_count = rows.len();
                for row in rows {
                    if row.len() != 3 {
                        return Err(FastDbError::format("vector validation row has wrong width"));
                    }
                    let rid = format_text(&row[0], "rid")?;
                    let document = crate::decode::parse_doc(&format_text(&row[1], "doc")?)?
                        .into_iter()
                        .collect();
                    let expected = vector_blob_from_document(&document, &path, dimension)?;
                    let actual = match &row[2] {
                        Value::Null => None,
                        Value::Blob(value) => Some(value.as_slice()),
                        _ => {
                            return Err(FastDbError::format(format!(
                                "record {rid:?} has a non-BLOB native vector value"
                            )))
                        }
                    };
                    if actual != expected.as_deref() {
                        return Err(FastDbError::format(format!(
                            "record {rid:?} document and native vector state disagree"
                        )));
                    }
                    after_rid = Some(rid);
                }
                if row_count < 256 {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn vector_blob_from_document(
    document: &BTreeMap<String, crate::decode::Value>,
    path: &[String],
    dimension: usize,
) -> Result<Option<Vec<u8>>> {
    let Some(value) = crate::path::get_path(document, path) else {
        return Ok(None);
    };
    if matches!(value, crate::decode::Value::Null) {
        return Ok(None);
    }
    let crate::decode::Value::Array(elements) = value else {
        return Err(FastDbError::format(
            "stored vector document value is not an array",
        ));
    };
    if elements.len() != dimension {
        return Err(FastDbError::format(
            "stored vector document dimension disagrees with its catalog",
        ));
    }
    let mut encoded = Vec::with_capacity(dimension.saturating_mul(8).saturating_add(1));
    for element in elements {
        let number = match element {
            crate::decode::Value::Integer(value) => *value as f64,
            crate::decode::Value::Float(value) if value.is_finite() => *value,
            _ => {
                return Err(FastDbError::format(
                    "stored vector document contains a non-finite or non-numeric element",
                ))
            }
        };
        encoded.extend_from_slice(&number.to_le_bytes());
    }
    encoded.push(2);
    Ok(Some(encoded))
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
            || (name.starts_with(crate::names::TABLE_NAME_PREFIX)
                && physical_table_sql_matches(actual, expected_sql))
    });
    if object.table_name != table_name || !sql_matches {
        return Err(FastDbError::format(format!(
            "physical object {name:?} has an unexpected structure"
        )));
    }
    Ok(())
}

fn physical_table_sql_matches(actual: &str, expected: &str) -> bool {
    fn columns(sql: &str) -> Option<(&str, Vec<&str>)> {
        let (prefix, body) = sql.split_once(" (")?;
        let body = body.strip_suffix(") STRICT")?;
        Some((prefix, body.split(", ").collect()))
    }
    let Some((actual_prefix, mut actual_columns)) = columns(actual) else {
        return false;
    };
    let Some((expected_prefix, mut expected_columns)) = columns(expected) else {
        return false;
    };
    if actual_prefix != expected_prefix
        || actual_columns.get(..2) != expected_columns.get(..2)
        || actual_columns.len() != expected_columns.len()
    {
        return false;
    }
    actual_columns.drain(..2);
    expected_columns.drain(..2);
    actual_columns.sort_unstable();
    expected_columns.sort_unstable();
    actual_columns == expected_columns
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
