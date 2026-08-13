//! Direct FastDB-to-Turso AST lowering and reviewed internal schema builders.

use crate::decode::Value as FastValue;
use crate::error::FastDbError;
#[cfg(feature = "testing")]
use crate::names::HIDDEN_COLUMN_NAME_PREFIX;
use crate::names::{validate_physical_name, INDEX_NAME_PREFIX, TABLE_NAME_PREFIX};
use std::num::NonZeroU32;
use turso_core::Value;
use turso_parser::ast::*;

pub type Bindings = Vec<Value>;
pub(crate) const TRANSLATED_INPUT: &str = "<fastdb-translated>";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PredicateOperator {
    Equal,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}

fn nm(value: &str) -> Name {
    Name::from_string(value)
}

fn qnm(value: &str) -> QualifiedName {
    QualifiedName::single(nm(value))
}

fn id(value: &str) -> Expr {
    Expr::Id(nm(value))
}

fn var(index: u32) -> Expr {
    Expr::Variable(Variable::indexed(
        NonZeroU32::new(index).expect("binding indexes are one-based"),
    ))
}

fn strlit(value: &str) -> Expr {
    Expr::Literal(Literal::String(format!("'{}'", value.replace('\'', "''"))))
}

fn numlit(value: impl ToString) -> Expr {
    Expr::Literal(Literal::Numeric(value.to_string()))
}

fn fcall(name: &str, arguments: Vec<Expr>) -> Expr {
    Expr::FunctionCall {
        name: nm(name),
        distinctness: None,
        args: arguments.into_iter().map(Box::new).collect(),
        order_by: vec![],
        within_group: vec![],
        filter_over: FunctionTail {
            filter_clause: None,
            over_clause: None,
        },
    }
}

fn ty(name: &str) -> Type {
    Type {
        name: name.to_string(),
        size: None,
        array_dimensions: 0,
    }
}

fn column(
    name: &str,
    type_name: &str,
    constraints: Vec<NamedColumnConstraint>,
) -> ColumnDefinition {
    ColumnDefinition {
        col_name: nm(name),
        col_type: Some(ty(type_name)),
        constraints,
    }
}

fn primary_key() -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::PrimaryKey {
            order: None,
            conflict_clause: None,
            auto_increment: false,
        },
    }
}

fn not_null() -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::NotNull {
            nullable: false,
            conflict_clause: None,
        },
    }
}

fn unique() -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::Unique(None),
    }
}

fn default(expression: Expr) -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::Default(Box::new(expression)),
    }
}

fn check(expression: Expr) -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::Check(Box::new(expression)),
    }
}

fn sorted_id(name: &str) -> SortedColumn {
    SortedColumn {
        expr: Box::new(id(name)),
        order: None,
        nulls: None,
    }
}

fn table_unique(columns: &[&str]) -> NamedTableConstraint {
    NamedTableConstraint {
        name: None,
        constraint: TableConstraint::Unique {
            columns: columns.iter().map(|name| sorted_id(name)).collect(),
            conflict_clause: None,
        },
    }
}

fn table_primary_key(columns: &[&str]) -> NamedTableConstraint {
    NamedTableConstraint {
        name: None,
        constraint: TableConstraint::PrimaryKey {
            columns: columns.iter().map(|name| sorted_id(name)).collect(),
            auto_increment: false,
            conflict_clause: None,
        },
    }
}

fn strict_options() -> TableOptions {
    TableOptions {
        without_rowid_text: None,
        strict_text: Some("STRICT".into()),
    }
}

fn create_table(
    table: &str,
    columns: Vec<ColumnDefinition>,
    constraints: Vec<NamedTableConstraint>,
) -> Stmt {
    Stmt::CreateTable {
        temporary: false,
        if_not_exists: false,
        tbl_name: qnm(table),
        body: CreateTableBody::ColumnsAndConstraints {
            columns,
            constraints,
            options: strict_options(),
        },
    }
}

pub fn begin_immediate() -> Stmt {
    Stmt::Begin {
        typ: Some(TransactionType::Immediate),
        name: None,
    }
}

pub fn commit() -> Stmt {
    Stmt::Commit { name: None }
}

pub fn rollback() -> Stmt {
    Stmt::Rollback {
        tx_name: None,
        savepoint_name: None,
    }
}

pub fn catalog_meta_v2_ddl() -> Stmt {
    create_table(
        crate::catalog::META_TABLE,
        vec![
            column(
                "singleton",
                "INTEGER",
                vec![
                    primary_key(),
                    check(Expr::binary(id("singleton"), Operator::Equals, numlit(1))),
                ],
            ),
            column("format_version", "INTEGER", vec![not_null()]),
            column("dialect_version", "INTEGER", vec![not_null()]),
            column("database_id", "TEXT", vec![not_null(), unique()]),
            column("creation_version", "TEXT", vec![not_null()]),
            column("last_migration", "INTEGER", vec![not_null()]),
        ],
        vec![],
    )
}

pub fn catalog_meta_ddl() -> Stmt {
    let Stmt::CreateTable {
        tbl_name,
        body:
            CreateTableBody::ColumnsAndConstraints {
                mut columns,
                constraints,
                options,
            },
        temporary,
        if_not_exists,
    } = catalog_meta_v2_ddl()
    else {
        unreachable!("metadata DDL is a column table")
    };
    columns.push(column(
        "document_encoding_version",
        "INTEGER",
        vec![
            not_null(),
            default(numlit(crate::catalog::DOCUMENT_ENCODING_VERSION)),
            check(Expr::binary(
                id("document_encoding_version"),
                Operator::Equals,
                numlit(crate::catalog::DOCUMENT_ENCODING_VERSION),
            )),
        ],
    ));
    Stmt::CreateTable {
        tbl_name,
        body: CreateTableBody::ColumnsAndConstraints {
            columns,
            constraints,
            options,
        },
        temporary,
        if_not_exists,
    }
}

pub fn catalog_tables_v1_ddl() -> Stmt {
    let mode_check = Expr::InList {
        lhs: Box::new(id("mode")),
        not: false,
        rhs: vec![
            Box::new(strlit("SCHEMALESS")),
            Box::new(strlit("SCHEMAFULL")),
        ],
    };
    create_table(
        crate::catalog::TABLES_TABLE,
        vec![
            column("table_id", "TEXT", vec![primary_key()]),
            column("logical_name", "TEXT", vec![not_null(), unique()]),
            column("physical_name", "TEXT", vec![not_null(), unique()]),
            column("mode", "TEXT", vec![not_null(), check(mode_check)]),
            column("definition", "TEXT", vec![]),
        ],
        vec![],
    )
}

pub fn catalog_tables_ddl() -> Stmt {
    let mode_check = Expr::InList {
        lhs: Box::new(id("mode")),
        not: false,
        rhs: vec![
            Box::new(strlit("SCHEMALESS")),
            Box::new(strlit("SCHEMAFULL")),
        ],
    };
    create_table(
        crate::catalog::TABLES_TABLE,
        vec![
            column("table_id", "TEXT", vec![primary_key()]),
            column("logical_name", "TEXT", vec![not_null(), unique()]),
            column("physical_name", "TEXT", vec![not_null(), unique()]),
            column("mode", "TEXT", vec![not_null(), check(mode_check)]),
            column("definition", "TEXT", vec![]),
            column("kind", "TEXT", vec![not_null(), default(strlit("NORMAL"))]),
            column("relation_in_table_id", "TEXT", vec![]),
            column("relation_out_table_id", "TEXT", vec![]),
            column(
                "relation_enforced",
                "INTEGER",
                vec![not_null(), default(numlit(0))],
            ),
        ],
        vec![],
    )
}

pub fn catalog_fields_ddl() -> Stmt {
    create_table(
        crate::catalog::FIELDS_TABLE,
        vec![
            column("table_id", "TEXT", vec![not_null()]),
            column("path_key", "TEXT", vec![not_null()]),
            column("type_ast", "TEXT", vec![not_null()]),
            column(
                "required",
                "INTEGER",
                vec![
                    not_null(),
                    check(Expr::InList {
                        lhs: Box::new(id("required")),
                        not: false,
                        rhs: vec![Box::new(numlit(0)), Box::new(numlit(1))],
                    }),
                ],
            ),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![table_primary_key(&["table_id", "path_key"])],
    )
}

pub fn catalog_indexes_v1_ddl() -> Stmt {
    create_table(
        crate::catalog::INDEXES_TABLE,
        vec![
            column("index_id", "TEXT", vec![primary_key()]),
            column("table_id", "TEXT", vec![not_null()]),
            column("logical_name", "TEXT", vec![not_null()]),
            column("physical_name", "TEXT", vec![not_null(), unique()]),
            column("paths_json", "TEXT", vec![not_null()]),
            column(
                "unique_flag",
                "INTEGER",
                vec![
                    not_null(),
                    check(Expr::InList {
                        lhs: Box::new(id("unique_flag")),
                        not: false,
                        rhs: vec![Box::new(numlit(0)), Box::new(numlit(1))],
                    }),
                ],
            ),
            column("expression_version", "INTEGER", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![table_unique(&["table_id", "logical_name"])],
    )
}

pub fn catalog_indexes_v2_ddl() -> Stmt {
    create_table(
        crate::catalog::INDEXES_TABLE,
        vec![
            column("index_id", "TEXT", vec![primary_key()]),
            column("table_id", "TEXT", vec![not_null()]),
            column("logical_name", "TEXT", vec![not_null()]),
            column("physical_name", "TEXT", vec![not_null(), unique()]),
            column("paths_json", "TEXT", vec![not_null()]),
            column(
                "unique_flag",
                "INTEGER",
                vec![
                    not_null(),
                    check(Expr::InList {
                        lhs: Box::new(id("unique_flag")),
                        not: false,
                        rhs: vec![Box::new(numlit(0)), Box::new(numlit(1))],
                    }),
                ],
            ),
            column("expression_version", "INTEGER", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
            column(
                "index_kind",
                "TEXT",
                vec![not_null(), default(strlit("BTREE"))],
            ),
            column(
                "provider",
                "TEXT",
                vec![not_null(), default(strlit("BUILTIN_BTREE"))],
            ),
            column(
                "provider_version",
                "INTEGER",
                vec![not_null(), default(numlit(1))],
            ),
            column(
                "options_json",
                "TEXT",
                vec![not_null(), default(strlit("{}"))],
            ),
            column("state", "TEXT", vec![not_null(), default(strlit("READY"))]),
            column(
                "encoding_version",
                "INTEGER",
                vec![not_null(), default(numlit(1))],
            ),
        ],
        vec![table_unique(&["table_id", "logical_name"])],
    )
}

pub fn catalog_indexes_ddl() -> Stmt {
    let Stmt::CreateTable {
        tbl_name,
        body:
            CreateTableBody::ColumnsAndConstraints {
                mut columns,
                constraints,
                options,
            },
        temporary,
        if_not_exists,
    } = catalog_indexes_v2_ddl()
    else {
        unreachable!("index catalog DDL is a column table")
    };
    columns.push(column(
        "auxiliary_version",
        "INTEGER",
        vec![not_null(), default(numlit(1))],
    ));
    Stmt::CreateTable {
        tbl_name,
        body: CreateTableBody::ColumnsAndConstraints {
            columns,
            constraints,
            options,
        },
        temporary,
        if_not_exists,
    }
}

pub fn catalog_analyzers_ddl() -> Stmt {
    create_table(
        crate::catalog::ANALYZERS_TABLE,
        vec![
            column("analyzer_id", "TEXT", vec![primary_key()]),
            column("logical_name", "TEXT", vec![not_null(), unique()]),
            column("provider", "TEXT", vec![not_null()]),
            column("provider_version", "INTEGER", vec![not_null()]),
            column("options_json", "TEXT", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![],
    )
}

pub fn catalog_hidden_columns_v2_ddl() -> Stmt {
    create_table(
        crate::catalog::HIDDEN_COLUMNS_TABLE,
        vec![
            column("column_id", "TEXT", vec![primary_key()]),
            column("table_id", "TEXT", vec![not_null()]),
            column("index_id", "TEXT", vec![]),
            column("field_path_key", "TEXT", vec![]),
            column("physical_name", "TEXT", vec![not_null(), unique()]),
            column("provider", "TEXT", vec![not_null()]),
            column("provider_version", "INTEGER", vec![not_null()]),
            column("physical_encoding", "TEXT", vec![not_null()]),
            column("dimension", "INTEGER", vec![]),
            column("options_json", "TEXT", vec![not_null()]),
            column("state", "TEXT", vec![not_null()]),
            column("encoding_version", "INTEGER", vec![not_null()]),
        ],
        vec![],
    )
}

pub fn catalog_hidden_columns_ddl() -> Stmt {
    let Stmt::CreateTable {
        tbl_name,
        body:
            CreateTableBody::ColumnsAndConstraints {
                mut columns,
                constraints,
                options,
            },
        temporary,
        if_not_exists,
    } = catalog_hidden_columns_v2_ddl()
    else {
        unreachable!("hidden-column catalog DDL is a column table")
    };
    columns.push(column(
        "auxiliary_version",
        "INTEGER",
        vec![not_null(), default(numlit(1))],
    ));
    Stmt::CreateTable {
        tbl_name,
        body: CreateTableBody::ColumnsAndConstraints {
            columns,
            constraints,
            options,
        },
        temporary,
        if_not_exists,
    }
}

pub fn catalog_capabilities_ddl() -> Stmt {
    create_table(
        crate::catalog::CAPABILITIES_TABLE,
        vec![
            column("provider", "TEXT", vec![primary_key()]),
            column("min_provider_version", "INTEGER", vec![not_null()]),
            column("min_encoding_version", "INTEGER", vec![not_null()]),
        ],
        vec![],
    )
}

pub fn catalog_functions_ddl() -> Stmt {
    create_table(
        crate::catalog::FUNCTIONS_TABLE,
        vec![
            column("function_id", "TEXT", vec![primary_key()]),
            column("logical_name", "TEXT", vec![not_null(), unique()]),
            column("arguments_ast", "TEXT", vec![not_null()]),
            column("body_source", "TEXT", vec![not_null()]),
            column("ast_version", "INTEGER", vec![not_null()]),
            column("limits_json", "TEXT", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![],
    )
}

pub fn catalog_parameters_ddl() -> Stmt {
    create_table(
        crate::catalog::PARAMETERS_TABLE,
        vec![
            column("parameter_id", "TEXT", vec![primary_key()]),
            column("logical_name", "TEXT", vec![not_null(), unique()]),
            column("value_json", "TEXT", vec![not_null()]),
            column("encoding_version", "INTEGER", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![],
    )
}

pub fn catalog_views_ddl() -> Stmt {
    create_table(
        crate::catalog::VIEWS_TABLE,
        vec![
            column("view_id", "TEXT", vec![primary_key()]),
            column("logical_name", "TEXT", vec![not_null(), unique()]),
            column("definition", "TEXT", vec![not_null()]),
            column("ast_version", "INTEGER", vec![not_null()]),
            column("dependencies_json", "TEXT", vec![not_null()]),
        ],
        vec![],
    )
}

pub fn catalog_events_ddl() -> Stmt {
    create_table(
        crate::catalog::EVENTS_TABLE,
        vec![
            column("event_id", "TEXT", vec![primary_key()]),
            column("table_id", "TEXT", vec![not_null()]),
            column("logical_name", "TEXT", vec![not_null()]),
            column("when_source", "TEXT", vec![not_null()]),
            column("then_source", "TEXT", vec![not_null()]),
            column("expression_version", "INTEGER", vec![not_null()]),
            column("recursion_limit", "INTEGER", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![table_unique(&["table_id", "logical_name"])],
    )
}

pub fn catalog_permissions_ddl() -> Stmt {
    let owner_kind = Expr::InList {
        lhs: Box::new(id("owner_kind")),
        not: false,
        rhs: vec![
            Box::new(strlit("TABLE")),
            Box::new(strlit("FIELD")),
            Box::new(strlit("FUNCTION")),
        ],
    };
    create_table(
        crate::catalog::PERMISSIONS_TABLE,
        vec![
            column("permission_id", "TEXT", vec![primary_key()]),
            column("owner_kind", "TEXT", vec![not_null(), check(owner_kind)]),
            column("owner_id", "TEXT", vec![not_null()]),
            column("action", "TEXT", vec![not_null()]),
            column("predicate_source", "TEXT", vec![not_null()]),
            column("expression_version", "INTEGER", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![],
    )
}

pub fn catalog_users_ddl() -> Stmt {
    let scope_kind = Expr::InList {
        lhs: Box::new(id("scope_kind")),
        not: false,
        rhs: vec![Box::new(strlit("DATABASE")), Box::new(strlit("RECORD"))],
    };
    create_table(
        crate::catalog::USERS_TABLE,
        vec![
            column("user_id", "TEXT", vec![primary_key()]),
            column("scope_kind", "TEXT", vec![not_null(), check(scope_kind)]),
            column("scope_id", "TEXT", vec![]),
            column("logical_name", "TEXT", vec![not_null()]),
            column("roles_json", "TEXT", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![table_unique(&["scope_kind", "scope_id", "logical_name"])],
    )
}

pub fn catalog_accesses_ddl() -> Stmt {
    let scope_kind = Expr::InList {
        lhs: Box::new(id("scope_kind")),
        not: false,
        rhs: vec![Box::new(strlit("DATABASE")), Box::new(strlit("RECORD"))],
    };
    create_table(
        crate::catalog::ACCESSES_TABLE,
        vec![
            column("access_id", "TEXT", vec![primary_key()]),
            column("scope_kind", "TEXT", vec![not_null(), check(scope_kind)]),
            column("scope_id", "TEXT", vec![]),
            column("logical_name", "TEXT", vec![not_null()]),
            column("access_kind", "TEXT", vec![not_null()]),
            column("options_json", "TEXT", vec![not_null()]),
            column("definition", "TEXT", vec![not_null()]),
        ],
        vec![table_unique(&["scope_kind", "scope_id", "logical_name"])],
    )
}

pub fn format3_meta_column() -> ColumnDefinition {
    column(
        "document_encoding_version",
        "INTEGER",
        vec![
            not_null(),
            default(numlit(crate::catalog::DOCUMENT_ENCODING_VERSION)),
            check(Expr::binary(
                id("document_encoding_version"),
                Operator::Equals,
                numlit(crate::catalog::DOCUMENT_ENCODING_VERSION),
            )),
        ],
    )
}

pub fn format3_provider_column() -> ColumnDefinition {
    column(
        "auxiliary_version",
        "INTEGER",
        vec![not_null(), default(numlit(1))],
    )
}

pub fn add_catalog_column(table: &str, definition: ColumnDefinition) -> Stmt {
    Stmt::AlterTable(AlterTable {
        name: qnm(table),
        body: AlterTableBody::AddColumn(definition),
    })
}

pub fn format2_table_columns() -> Vec<ColumnDefinition> {
    vec![
        column("kind", "TEXT", vec![not_null(), default(strlit("NORMAL"))]),
        column("relation_in_table_id", "TEXT", vec![]),
        column("relation_out_table_id", "TEXT", vec![]),
        column(
            "relation_enforced",
            "INTEGER",
            vec![not_null(), default(numlit(0))],
        ),
    ]
}

pub fn format2_index_columns() -> Vec<ColumnDefinition> {
    vec![
        column(
            "index_kind",
            "TEXT",
            vec![not_null(), default(strlit("BTREE"))],
        ),
        column(
            "provider",
            "TEXT",
            vec![not_null(), default(strlit("BUILTIN_BTREE"))],
        ),
        column(
            "provider_version",
            "INTEGER",
            vec![not_null(), default(numlit(1))],
        ),
        column(
            "options_json",
            "TEXT",
            vec![not_null(), default(strlit("{}"))],
        ),
        column("state", "TEXT", vec![not_null(), default(strlit("READY"))]),
        column(
            "encoding_version",
            "INTEGER",
            vec![not_null(), default(numlit(1))],
        ),
    ]
}

pub fn physical_table_ddl(opaque_name: &str) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_name, TABLE_NAME_PREFIX)?;
    Ok(create_table(
        opaque_name,
        vec![
            column("rid", "TEXT", vec![primary_key()]),
            column("doc", "BLOB", vec![not_null()]),
        ],
        vec![],
    ))
}

pub fn physical_relation_table_ddl(
    opaque_name: &str,
    hidden_columns: &[String],
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_name, TABLE_NAME_PREFIX)?;
    if hidden_columns.len() != 4 {
        return Err(FastDbError::format(
            "relation table requires exactly four hidden endpoint columns",
        ));
    }
    let mut columns = vec![
        column("rid", "TEXT", vec![primary_key()]),
        column("doc", "BLOB", vec![not_null()]),
    ];
    for hidden in hidden_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        columns.push(column(hidden, "TEXT", vec![not_null()]));
    }
    Ok(create_table(opaque_name, columns, vec![]))
}

pub fn physical_table_with_hidden_ddl(
    opaque_name: &str,
    graph_columns: &[String],
    fts_columns: &[String],
    vector_columns: &[String],
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_name, TABLE_NAME_PREFIX)?;
    if !graph_columns.is_empty() && graph_columns.len() != 4 {
        return Err(FastDbError::format(
            "physical table requires zero or four graph columns",
        ));
    }
    let mut columns = vec![
        column("rid", "TEXT", vec![primary_key()]),
        column("doc", "BLOB", vec![not_null()]),
    ];
    for hidden in graph_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        columns.push(column(hidden, "TEXT", vec![not_null()]));
    }
    for hidden in fts_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        columns.push(column(hidden, "TEXT", vec![]));
    }
    for hidden in vector_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        columns.push(column(hidden, "BLOB", vec![]));
    }
    Ok(create_table(opaque_name, columns, vec![]))
}

pub fn physical_add_vector_column_ddl(
    opaque_table: &str,
    opaque_hidden_column: &str,
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(
        opaque_hidden_column,
        crate::names::HIDDEN_COLUMN_NAME_PREFIX,
    )?;
    Ok(Stmt::AlterTable(AlterTable {
        name: qnm(opaque_table),
        body: AlterTableBody::AddColumn(column(opaque_hidden_column, "BLOB", vec![])),
    }))
}

pub fn physical_add_fts_column_ddl(
    opaque_table: &str,
    opaque_hidden_column: &str,
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(
        opaque_hidden_column,
        crate::names::HIDDEN_COLUMN_NAME_PREFIX,
    )?;
    Ok(Stmt::AlterTable(AlterTable {
        name: qnm(opaque_table),
        body: AlterTableBody::AddColumn(column(opaque_hidden_column, "TEXT", vec![])),
    }))
}

pub fn physical_update_fts_column_stmt(
    opaque_table: &str,
    opaque_hidden_column: &str,
    encoded_rid: &str,
    value: Option<&str>,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(
        opaque_hidden_column,
        crate::names::HIDDEN_COLUMN_NAME_PREFIX,
    )?;
    let (value_expr, bindings) = if let Some(value) = value {
        (var(2), vec![text(encoded_rid), text(value)])
    } else {
        (Expr::Literal(Literal::Null), vec![text(encoded_rid)])
    };
    Ok((
        Stmt::Update(Update {
            with: None,
            or_conflict: None,
            tbl_name: qnm(opaque_table),
            indexed: None,
            sets: vec![Set {
                col_names: vec![nm(opaque_hidden_column)],
                expr: Box::new(value_expr),
            }],
            from: None,
            where_clause: Some(Box::new(Expr::binary(id("rid"), Operator::Equals, var(1)))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        }),
        bindings,
    ))
}

pub fn physical_update_vector_column_stmt(
    opaque_table: &str,
    opaque_hidden_column: &str,
    encoded_rid: &str,
    value: Option<Value>,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(
        opaque_hidden_column,
        crate::names::HIDDEN_COLUMN_NAME_PREFIX,
    )?;
    let (value_expr, bindings) = if let Some(value) = value {
        (var(2), vec![text(encoded_rid), value])
    } else {
        (Expr::Literal(Literal::Null), vec![text(encoded_rid)])
    };
    Ok((
        Stmt::Update(Update {
            with: None,
            or_conflict: None,
            tbl_name: qnm(opaque_table),
            indexed: None,
            sets: vec![Set {
                col_names: vec![nm(opaque_hidden_column)],
                expr: Box::new(value_expr),
            }],
            from: None,
            where_clause: Some(Box::new(Expr::binary(id("rid"), Operator::Equals, var(1)))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        }),
        bindings,
    ))
}

pub fn physical_graph_index_ddl(
    opaque_index: &str,
    opaque_table: &str,
    hidden_columns: &[String],
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_index, INDEX_NAME_PREFIX)?;
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    if hidden_columns.len() != 4 {
        return Err(FastDbError::format(
            "graph adjacency index requires exactly four hidden columns",
        ));
    }
    for hidden in hidden_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    }
    Ok(Stmt::CreateIndex {
        unique: false,
        if_not_exists: false,
        idx_name: qnm(opaque_index),
        tbl_name: nm(opaque_table),
        using: None,
        columns: hidden_columns.iter().map(|name| sorted_id(name)).collect(),
        with_clause: vec![],
        where_clause: None,
    })
}

pub fn physical_fts_index_ddl(
    opaque_index: &str,
    opaque_table: &str,
    hidden_columns: &[String],
    tokenizer: &str,
    weights: &[f64],
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_index, INDEX_NAME_PREFIX)?;
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    if hidden_columns.is_empty() {
        return Err(FastDbError::format("FTS index has no hidden input columns"));
    }
    for hidden in hidden_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    }
    let mut with_clause = vec![(
        nm("tokenizer"),
        Box::new(Expr::Literal(Literal::String(format!("'{tokenizer}'")))),
    )];
    if !weights.is_empty() {
        let encoded = hidden_columns
            .iter()
            .zip(weights)
            .map(|(column, weight)| format!("{column}={weight}"))
            .collect::<Vec<_>>()
            .join(",");
        with_clause.push((
            nm("weights"),
            Box::new(Expr::Literal(Literal::String(format!("'{encoded}'")))),
        ));
    }
    Ok(Stmt::CreateIndex {
        unique: false,
        if_not_exists: false,
        idx_name: qnm(opaque_index),
        tbl_name: nm(opaque_table),
        using: Some(nm("fts")),
        columns: hidden_columns.iter().map(|name| sorted_id(name)).collect(),
        with_clause,
        where_clause: None,
    })
}

pub fn physical_optimize_index_stmt(opaque_index: &str) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_index, INDEX_NAME_PREFIX)?;
    Ok(Stmt::Optimize {
        idx_name: Some(qnm(opaque_index)),
    })
}

#[cfg(feature = "testing")]
pub(crate) fn test_provider_table_ddl(
    opaque_table: &str,
    opaque_hidden_column: &str,
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(opaque_hidden_column, HIDDEN_COLUMN_NAME_PREFIX)?;
    Ok(create_table(
        opaque_table,
        vec![
            column("rid", "TEXT", vec![primary_key()]),
            column("doc", "BLOB", vec![not_null()]),
            column(opaque_hidden_column, "INTEGER", vec![not_null()]),
        ],
        vec![],
    ))
}

#[cfg(feature = "testing")]
pub(crate) fn test_provider_insert_stmt(
    opaque_table: &str,
    opaque_hidden_column: &str,
    encoded_document: &str,
    derived: i64,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(opaque_hidden_column, HIDDEN_COLUMN_NAME_PREFIX)?;
    Ok(insert_values(
        opaque_table,
        &["rid", "doc", opaque_hidden_column],
        vec![strlit("test"), fcall("jsonb", vec![var(1)]), var(2)],
        vec![text(encoded_document), Value::from_i64(derived)],
    ))
}

#[cfg(feature = "testing")]
pub(crate) fn test_provider_update_document_stmt(
    opaque_table: &str,
    encoded_document: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    Ok((
        Stmt::Update(Update {
            with: None,
            or_conflict: None,
            tbl_name: qnm(opaque_table),
            indexed: None,
            sets: vec![Set {
                col_names: vec![nm("doc")],
                expr: Box::new(fcall("jsonb", vec![var(1)])),
            }],
            from: None,
            where_clause: Some(Box::new(Expr::binary(
                id("rid"),
                Operator::Equals,
                strlit("test"),
            ))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        }),
        vec![text(encoded_document)],
    ))
}

#[cfg(feature = "testing")]
pub(crate) fn test_provider_update_hidden_stmt(
    opaque_table: &str,
    opaque_hidden_column: &str,
    derived: i64,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(opaque_hidden_column, HIDDEN_COLUMN_NAME_PREFIX)?;
    Ok((
        Stmt::Update(Update {
            with: None,
            or_conflict: None,
            tbl_name: qnm(opaque_table),
            indexed: None,
            sets: vec![Set {
                col_names: vec![nm(opaque_hidden_column)],
                expr: Box::new(var(1)),
            }],
            from: None,
            where_clause: Some(Box::new(Expr::binary(
                id("rid"),
                Operator::Equals,
                strlit("test"),
            ))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        }),
        vec![Value::from_i64(derived)],
    ))
}

#[cfg(feature = "testing")]
pub(crate) fn test_provider_select_stmt(
    opaque_table: &str,
    opaque_hidden_column: &str,
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(opaque_hidden_column, HIDDEN_COLUMN_NAME_PREFIX)?;
    Ok(one_select(
        vec![
            ResultColumn::Expr(Box::new(fcall("json", vec![id("doc")])), None),
            ResultColumn::Expr(Box::new(id(opaque_hidden_column)), None),
        ],
        opaque_table,
        Some(Expr::binary(id("rid"), Operator::Equals, strlit("test"))),
    ))
}

/// This is the only expression builder used by filters and index DDL.
pub fn json_extract_doc(path: &str) -> Expr {
    fcall("json_extract", vec![id("doc"), strlit(path)])
}

pub fn physical_index_ddl(
    opaque_index: &str,
    opaque_table: &str,
    paths: &[String],
    is_unique: bool,
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_index, INDEX_NAME_PREFIX)?;
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    if paths.is_empty() {
        return Err(FastDbError::Schema("index has no field paths".into()));
    }
    Ok(Stmt::CreateIndex {
        unique: is_unique,
        if_not_exists: false,
        idx_name: qnm(opaque_index),
        tbl_name: nm(opaque_table),
        using: None,
        columns: paths
            .iter()
            .map(|path| SortedColumn {
                expr: Box::new(json_extract_doc(path)),
                order: None,
                nulls: None,
            })
            .collect(),
        with_clause: vec![],
        where_clause: None,
    })
}

pub fn sqlite_schema_stmt() -> Stmt {
    one_select(
        vec!["type", "name", "tbl_name", "sql"]
            .into_iter()
            .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
            .collect(),
        "sqlite_schema",
        None,
    )
}

pub fn meta_schema_stmt() -> Stmt {
    one_select(
        vec![ResultColumn::Expr(Box::new(id("name")), None)],
        "sqlite_schema",
        Some(Expr::binary(
            Expr::binary(id("type"), Operator::Equals, strlit("table")),
            Operator::And,
            Expr::binary(
                id("name"),
                Operator::Equals,
                strlit(crate::catalog::META_TABLE),
            ),
        )),
    )
}

pub fn meta_format_stmt() -> Stmt {
    one_select(
        vec![ResultColumn::Expr(Box::new(id("format_version")), None)],
        crate::catalog::META_TABLE,
        Some(Expr::binary(id("singleton"), Operator::Equals, numlit(1))),
    )
}

pub fn meta_stmt() -> Stmt {
    one_select(
        [
            "format_version",
            "dialect_version",
            "database_id",
            "creation_version",
            "last_migration",
            "document_encoding_version",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::META_TABLE,
        Some(Expr::binary(id("singleton"), Operator::Equals, numlit(1))),
    )
}

pub fn meta_v2_stmt() -> Stmt {
    one_select(
        [
            "format_version",
            "dialect_version",
            "database_id",
            "creation_version",
            "last_migration",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::META_TABLE,
        Some(Expr::binary(id("singleton"), Operator::Equals, numlit(1))),
    )
}

pub fn tables_stmt() -> Stmt {
    one_select(
        [
            "table_id",
            "logical_name",
            "physical_name",
            "mode",
            "definition",
            "kind",
            "relation_in_table_id",
            "relation_out_table_id",
            "relation_enforced",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::TABLES_TABLE,
        None,
    )
}

pub fn fields_stmt() -> Stmt {
    one_select(
        ["table_id", "path_key", "type_ast", "required", "definition"]
            .into_iter()
            .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
            .collect(),
        crate::catalog::FIELDS_TABLE,
        None,
    )
}

pub fn indexes_stmt() -> Stmt {
    one_select(
        [
            "index_id",
            "table_id",
            "logical_name",
            "physical_name",
            "paths_json",
            "unique_flag",
            "expression_version",
            "definition",
            "index_kind",
            "provider",
            "provider_version",
            "options_json",
            "state",
            "encoding_version",
            "auxiliary_version",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::INDEXES_TABLE,
        None,
    )
}

pub fn indexes_v2_stmt() -> Stmt {
    one_select(
        [
            "index_id",
            "table_id",
            "logical_name",
            "physical_name",
            "paths_json",
            "unique_flag",
            "expression_version",
            "definition",
            "index_kind",
            "provider",
            "provider_version",
            "options_json",
            "state",
            "encoding_version",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::INDEXES_TABLE,
        None,
    )
}

pub fn tables_v1_stmt() -> Stmt {
    one_select(
        [
            "table_id",
            "logical_name",
            "physical_name",
            "mode",
            "definition",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::TABLES_TABLE,
        None,
    )
}

pub fn indexes_v1_stmt() -> Stmt {
    one_select(
        [
            "index_id",
            "table_id",
            "logical_name",
            "physical_name",
            "paths_json",
            "unique_flag",
            "expression_version",
            "definition",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::INDEXES_TABLE,
        None,
    )
}

pub fn analyzers_stmt() -> Stmt {
    one_select(
        [
            "analyzer_id",
            "logical_name",
            "provider",
            "provider_version",
            "options_json",
            "definition",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::ANALYZERS_TABLE,
        None,
    )
}

pub fn hidden_columns_stmt() -> Stmt {
    one_select(
        [
            "column_id",
            "table_id",
            "index_id",
            "field_path_key",
            "physical_name",
            "provider",
            "provider_version",
            "physical_encoding",
            "dimension",
            "options_json",
            "state",
            "encoding_version",
            "auxiliary_version",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::HIDDEN_COLUMNS_TABLE,
        None,
    )
}

pub fn hidden_columns_v2_stmt() -> Stmt {
    one_select(
        [
            "column_id",
            "table_id",
            "index_id",
            "field_path_key",
            "physical_name",
            "provider",
            "provider_version",
            "physical_encoding",
            "dimension",
            "options_json",
            "state",
            "encoding_version",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::HIDDEN_COLUMNS_TABLE,
        None,
    )
}

pub fn future_catalog_stmt(table: &str, id_column: &str) -> Stmt {
    one_select(
        vec![ResultColumn::Expr(Box::new(id(id_column)), None)],
        table,
        None,
    )
}

pub fn parameters_stmt() -> Stmt {
    one_select(
        [
            "parameter_id",
            "logical_name",
            "value_json",
            "encoding_version",
            "definition",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::PARAMETERS_TABLE,
        None,
    )
}

pub fn functions_stmt() -> Stmt {
    one_select(
        [
            "function_id",
            "logical_name",
            "arguments_ast",
            "body_source",
            "ast_version",
            "limits_json",
            "definition",
        ]
        .into_iter()
        .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
        .collect(),
        crate::catalog::FUNCTIONS_TABLE,
        None,
    )
}

pub fn capabilities_stmt() -> Stmt {
    one_select(
        ["provider", "min_provider_version", "min_encoding_version"]
            .into_iter()
            .map(|name| ResultColumn::Expr(Box::new(id(name)), None))
            .collect(),
        crate::catalog::CAPABILITIES_TABLE,
        None,
    )
}

pub fn meta_insert(
    database_id: &str,
    creation_version: &str,
    last_migration: i64,
) -> (Stmt, Bindings) {
    insert_values(
        crate::catalog::META_TABLE,
        &[
            "singleton",
            "format_version",
            "dialect_version",
            "database_id",
            "creation_version",
            "last_migration",
            "document_encoding_version",
        ],
        vec![
            numlit(1),
            numlit(crate::catalog::FORMAT_VERSION),
            numlit(crate::catalog::DIALECT_VERSION),
            var(1),
            var(2),
            numlit(last_migration),
            numlit(crate::catalog::DOCUMENT_ENCODING_VERSION),
        ],
        vec![text(database_id), text(creation_version)],
    )
}

pub fn migrate_to_one_stmt() -> Stmt {
    Stmt::Update(Update {
        with: None,
        or_conflict: None,
        tbl_name: qnm(crate::catalog::META_TABLE),
        indexed: None,
        sets: vec![Set {
            col_names: vec![nm("last_migration")],
            expr: Box::new(numlit(1)),
        }],
        from: None,
        where_clause: Some(Box::new(Expr::binary(
            Expr::binary(id("singleton"), Operator::Equals, numlit(1)),
            Operator::And,
            Expr::binary(id("last_migration"), Operator::Equals, numlit(0)),
        ))),
        returning: vec![],
        order_by: vec![],
        limit: None,
    })
}

pub fn migrate_to_two_stmt() -> Stmt {
    Stmt::Update(Update {
        with: None,
        or_conflict: None,
        tbl_name: qnm(crate::catalog::META_TABLE),
        indexed: None,
        sets: vec![
            Set {
                col_names: vec![nm("last_migration")],
                expr: Box::new(numlit(crate::catalog::LAST_MIGRATION)),
            },
            Set {
                col_names: vec![nm("format_version")],
                expr: Box::new(numlit(crate::catalog::FORMAT_VERSION)),
            },
        ],
        from: None,
        where_clause: Some(Box::new(Expr::binary(
            Expr::binary(id("singleton"), Operator::Equals, numlit(1)),
            Operator::And,
            Expr::binary(id("format_version"), Operator::Equals, numlit(1)),
        ))),
        returning: vec![],
        order_by: vec![],
        limit: None,
    })
}

pub fn migrate_to_three_stmt() -> Stmt {
    Stmt::Update(Update {
        with: None,
        or_conflict: None,
        tbl_name: qnm(crate::catalog::META_TABLE),
        indexed: None,
        sets: vec![
            Set {
                col_names: vec![nm("last_migration")],
                expr: Box::new(numlit(crate::catalog::LAST_MIGRATION)),
            },
            Set {
                col_names: vec![nm("format_version")],
                expr: Box::new(numlit(crate::catalog::FORMAT_VERSION)),
            },
        ],
        from: None,
        where_clause: Some(Box::new(Expr::binary(
            Expr::binary(id("singleton"), Operator::Equals, numlit(1)),
            Operator::And,
            Expr::InList {
                lhs: Box::new(id("format_version")),
                not: false,
                rhs: vec![Box::new(numlit(1)), Box::new(numlit(2))],
            },
        ))),
        returning: vec![],
        order_by: vec![],
        limit: None,
    })
}

#[allow(clippy::too_many_arguments)]
pub fn table_insert(
    table_id: &str,
    logical_name: &str,
    physical_name: &str,
    mode: &str,
    definition: Option<&str>,
    kind: &str,
    relation_in_table_id: Option<&str>,
    relation_out_table_id: Option<&str>,
    relation_enforced: bool,
) -> (Stmt, Bindings) {
    let mut bindings = vec![
        text(table_id),
        text(logical_name),
        text(physical_name),
        text(mode),
    ];
    let definition_expr = if let Some(definition) = definition {
        bindings.push(text(definition));
        var(5)
    } else {
        Expr::Literal(Literal::Null)
    };
    bindings.push(text(kind));
    let kind_index = u32::try_from(bindings.len()).expect("fixed table catalog binding count");
    let relation_in = optional_text_binding(&mut bindings, relation_in_table_id);
    let relation_out = optional_text_binding(&mut bindings, relation_out_table_id);
    insert_values(
        crate::catalog::TABLES_TABLE,
        &[
            "table_id",
            "logical_name",
            "physical_name",
            "mode",
            "definition",
            "kind",
            "relation_in_table_id",
            "relation_out_table_id",
            "relation_enforced",
        ],
        vec![
            var(1),
            var(2),
            var(3),
            var(4),
            definition_expr,
            var(kind_index),
            relation_in,
            relation_out,
            numlit(i64::from(relation_enforced)),
        ],
        bindings,
    )
}

pub fn field_insert(
    table_id: &str,
    path_key: &str,
    type_ast: &str,
    required: bool,
    definition: &str,
) -> (Stmt, Bindings) {
    insert_values(
        crate::catalog::FIELDS_TABLE,
        &["table_id", "path_key", "type_ast", "required", "definition"],
        vec![var(1), var(2), var(3), numlit(i64::from(required)), var(4)],
        vec![
            text(table_id),
            text(path_key),
            text(type_ast),
            text(definition),
        ],
    )
}

#[allow(clippy::too_many_arguments)]
pub fn index_insert(
    index_id: &str,
    table_id: &str,
    logical_name: &str,
    physical_name: &str,
    paths_json: &str,
    is_unique: bool,
    expression_version: i64,
    definition: &str,
    index_kind: &str,
    provider: &str,
    provider_version: i64,
    options_json: &str,
    state: &str,
    encoding_version: i64,
) -> (Stmt, Bindings) {
    insert_values(
        crate::catalog::INDEXES_TABLE,
        &[
            "index_id",
            "table_id",
            "logical_name",
            "physical_name",
            "paths_json",
            "unique_flag",
            "expression_version",
            "definition",
            "index_kind",
            "provider",
            "provider_version",
            "options_json",
            "state",
            "encoding_version",
        ],
        vec![
            var(1),
            var(2),
            var(3),
            var(4),
            var(5),
            numlit(i64::from(is_unique)),
            numlit(expression_version),
            var(6),
            var(7),
            var(8),
            numlit(provider_version),
            var(9),
            var(10),
            numlit(encoding_version),
        ],
        vec![
            text(index_id),
            text(table_id),
            text(logical_name),
            text(physical_name),
            text(paths_json),
            text(definition),
            text(index_kind),
            text(provider),
            text(options_json),
            text(state),
        ],
    )
}

pub fn index_delete(index_id: &str) -> (Stmt, Bindings) {
    (
        Stmt::Delete {
            with: None,
            tbl_name: qnm(crate::catalog::INDEXES_TABLE),
            indexed: None,
            where_clause: Some(Box::new(Expr::binary(
                id("index_id"),
                Operator::Equals,
                var(1),
            ))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        },
        vec![text(index_id)],
    )
}

pub fn analyzer_insert(
    analyzer_id: &str,
    logical_name: &str,
    provider: &str,
    provider_version: i64,
    options_json: &str,
    definition: &str,
) -> (Stmt, Bindings) {
    insert_values(
        crate::catalog::ANALYZERS_TABLE,
        &[
            "analyzer_id",
            "logical_name",
            "provider",
            "provider_version",
            "options_json",
            "definition",
        ],
        vec![
            var(1),
            var(2),
            var(3),
            numlit(provider_version),
            var(4),
            var(5),
        ],
        vec![
            text(analyzer_id),
            text(logical_name),
            text(provider),
            text(options_json),
            text(definition),
        ],
    )
}

pub fn parameter_insert(
    parameter_id: &str,
    logical_name: &str,
    value_json: &str,
    encoding_version: i64,
    definition: &str,
) -> (Stmt, Bindings) {
    insert_values(
        crate::catalog::PARAMETERS_TABLE,
        &[
            "parameter_id",
            "logical_name",
            "value_json",
            "encoding_version",
            "definition",
        ],
        vec![var(1), var(2), var(3), numlit(encoding_version), var(4)],
        vec![
            text(parameter_id),
            text(logical_name),
            text(value_json),
            text(definition),
        ],
    )
}

pub fn function_insert(
    function_id: &str,
    logical_name: &str,
    arguments_ast: &str,
    body_source: &str,
    ast_version: i64,
    limits_json: &str,
    definition: &str,
) -> (Stmt, Bindings) {
    insert_values(
        crate::catalog::FUNCTIONS_TABLE,
        &[
            "function_id",
            "logical_name",
            "arguments_ast",
            "body_source",
            "ast_version",
            "limits_json",
            "definition",
        ],
        vec![
            var(1),
            var(2),
            var(3),
            var(4),
            numlit(ast_version),
            var(5),
            var(6),
        ],
        vec![
            text(function_id),
            text(logical_name),
            text(arguments_ast),
            text(body_source),
            text(limits_json),
            text(definition),
        ],
    )
}

pub fn function_delete(function_id: &str) -> (Stmt, Bindings) {
    (
        Stmt::Delete {
            with: None,
            tbl_name: qnm(crate::catalog::FUNCTIONS_TABLE),
            indexed: None,
            where_clause: Some(Box::new(Expr::binary(
                id("function_id"),
                Operator::Equals,
                var(1),
            ))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        },
        vec![text(function_id)],
    )
}

pub fn parameter_delete(parameter_id: &str) -> (Stmt, Bindings) {
    (
        Stmt::Delete {
            with: None,
            tbl_name: qnm(crate::catalog::PARAMETERS_TABLE),
            indexed: None,
            where_clause: Some(Box::new(Expr::binary(
                id("parameter_id"),
                Operator::Equals,
                var(1),
            ))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        },
        vec![text(parameter_id)],
    )
}

#[allow(clippy::too_many_arguments)]
pub fn hidden_column_insert(
    column_id: &str,
    table_id: &str,
    index_id: Option<&str>,
    field_path_key: Option<&str>,
    physical_name: &str,
    provider: &str,
    provider_version: i64,
    physical_encoding: &str,
    dimension: Option<i64>,
    options_json: &str,
    state: &str,
    encoding_version: i64,
) -> (Stmt, Bindings) {
    let mut bindings = vec![text(column_id), text(table_id)];
    let index_id = optional_text_binding(&mut bindings, index_id);
    let field_path_key = optional_text_binding(&mut bindings, field_path_key);
    bindings.push(text(physical_name));
    let physical_name_index = u32::try_from(bindings.len()).expect("fixed hidden bindings");
    bindings.push(text(provider));
    let provider_index = u32::try_from(bindings.len()).expect("fixed hidden bindings");
    bindings.push(text(physical_encoding));
    let encoding_index = u32::try_from(bindings.len()).expect("fixed hidden bindings");
    let dimension = dimension.map_or(Expr::Literal(Literal::Null), numlit);
    bindings.push(text(options_json));
    let options_index = u32::try_from(bindings.len()).expect("fixed hidden bindings");
    bindings.push(text(state));
    let state_index = u32::try_from(bindings.len()).expect("fixed hidden bindings");
    insert_values(
        crate::catalog::HIDDEN_COLUMNS_TABLE,
        &[
            "column_id",
            "table_id",
            "index_id",
            "field_path_key",
            "physical_name",
            "provider",
            "provider_version",
            "physical_encoding",
            "dimension",
            "options_json",
            "state",
            "encoding_version",
        ],
        vec![
            var(1),
            var(2),
            index_id,
            field_path_key,
            var(physical_name_index),
            var(provider_index),
            numlit(provider_version),
            var(encoding_index),
            dimension,
            var(options_index),
            var(state_index),
            numlit(encoding_version),
        ],
        bindings,
    )
}

pub fn capability_insert(
    provider: &str,
    min_provider_version: i64,
    min_encoding_version: i64,
) -> (Stmt, Bindings) {
    insert_values(
        crate::catalog::CAPABILITIES_TABLE,
        &["provider", "min_provider_version", "min_encoding_version"],
        vec![
            var(1),
            numlit(min_provider_version),
            numlit(min_encoding_version),
        ],
        vec![text(provider)],
    )
}

pub fn physical_drop_index_ddl(opaque_index: &str) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_index, INDEX_NAME_PREFIX)?;
    Ok(Stmt::DropIndex {
        if_exists: false,
        idx_name: qnm(opaque_index),
    })
}

pub fn physical_rebuild_index_stmt(opaque_index: &str) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_index, INDEX_NAME_PREFIX)?;
    Ok(Stmt::Reindex {
        name: Some(qnm(opaque_index)),
    })
}

pub fn physical_insert_content_stmt(
    opaque_table: &str,
    encoded_rid: &str,
    encoded_doc: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let jsonb = fcall("jsonb", vec![var(2)]);
    Ok(insert_values(
        opaque_table,
        &["rid", "doc"],
        vec![var(1), jsonb],
        vec![text(encoded_rid), text(encoded_doc)],
    ))
}

pub fn physical_insert_document_with_hidden_stmt(
    opaque_table: &str,
    encoded_rid: &str,
    encoded_doc: &str,
    hidden: &[(String, Option<Value>)],
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let mut names = vec!["rid".to_string(), "doc".to_string()];
    let mut expressions = vec![var(1), fcall("jsonb", vec![var(2)])];
    let mut bindings = vec![text(encoded_rid), text(encoded_doc)];
    for (name, value) in hidden {
        validate_physical_name(name, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        names.push(name.clone());
        if let Some(value) = value {
            bindings.push(value.clone());
            let index = u32::try_from(bindings.len())
                .map_err(|_| FastDbError::Engine("too many hidden bindings".into()))?;
            expressions.push(var(index));
        } else {
            expressions.push(Expr::Literal(Literal::Null));
        }
    }
    Ok(insert_values_owned(
        opaque_table,
        &names,
        expressions,
        bindings,
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn physical_relation_insert_stmt(
    opaque_table: &str,
    hidden_columns: &[String],
    encoded_rid: &str,
    encoded_doc: &str,
    in_table_id: &str,
    in_rid: &str,
    out_table_id: &str,
    out_rid: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    if hidden_columns.len() != 4 {
        return Err(FastDbError::format(
            "relation insert requires four endpoint columns",
        ));
    }
    for hidden in hidden_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    }
    let mut names = vec!["rid".to_string(), "doc".to_string()];
    names.extend(hidden_columns.iter().cloned());
    Ok(insert_values_owned(
        opaque_table,
        &names,
        vec![
            var(1),
            fcall("jsonb", vec![var(2)]),
            var(3),
            var(4),
            var(5),
            var(6),
        ],
        vec![
            text(encoded_rid),
            text(encoded_doc),
            text(in_table_id),
            text(in_rid),
            text(out_table_id),
            text(out_rid),
        ],
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn physical_relation_insert_with_hidden_stmt(
    opaque_table: &str,
    graph_columns: &[String],
    derived_hidden: &[(String, Option<Value>)],
    encoded_rid: &str,
    encoded_doc: &str,
    in_table_id: &str,
    in_rid: &str,
    out_table_id: &str,
    out_rid: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    if graph_columns.len() != 4 {
        return Err(FastDbError::format(
            "relation insert requires four endpoint columns",
        ));
    }
    let mut names = vec!["rid".to_string(), "doc".to_string()];
    let mut expressions = vec![var(1), fcall("jsonb", vec![var(2)])];
    let mut bindings = vec![
        text(encoded_rid),
        text(encoded_doc),
        text(in_table_id),
        text(in_rid),
        text(out_table_id),
        text(out_rid),
    ];
    for (ordinal, name) in graph_columns.iter().enumerate() {
        validate_physical_name(name, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        names.push(name.clone());
        expressions.push(var(
            u32::try_from(ordinal + 3).expect("four graph bindings fit")
        ));
    }
    for (name, value) in derived_hidden {
        validate_physical_name(name, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        names.push(name.clone());
        if let Some(value) = value {
            bindings.push(value.clone());
            expressions.push(var(u32::try_from(bindings.len())
                .map_err(|_| FastDbError::Engine("too many hidden bindings".into()))?));
        } else {
            expressions.push(Expr::Literal(Literal::Null));
        }
    }
    Ok(insert_values_owned(
        opaque_table,
        &names,
        expressions,
        bindings,
    ))
}

pub fn physical_insert_set_stmt(
    opaque_table: &str,
    encoded_rid: &str,
    path: &str,
    encoded_value: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let empty = fcall("jsonb", vec![strlit("{}")]);
    let value = fcall("json", vec![var(3)]);
    let document = fcall("jsonb", vec![fcall("json_set", vec![empty, var(2), value])]);
    Ok(insert_values(
        opaque_table,
        &["rid", "doc"],
        vec![var(1), document],
        vec![text(encoded_rid), text(path), text(encoded_value)],
    ))
}

pub fn physical_update_doc_stmt(
    opaque_table: &str,
    encoded_rid: &str,
    encoded_doc: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    Ok((
        Stmt::Update(Update {
            with: None,
            or_conflict: None,
            tbl_name: qnm(opaque_table),
            indexed: None,
            sets: vec![Set {
                col_names: vec![nm("doc")],
                expr: Box::new(fcall("jsonb", vec![var(2)])),
            }],
            from: None,
            where_clause: Some(Box::new(Expr::binary(id("rid"), Operator::Equals, var(1)))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        }),
        vec![text(encoded_rid), text(encoded_doc)],
    ))
}

pub fn physical_update_document_with_hidden_stmt(
    opaque_table: &str,
    encoded_rid: &str,
    encoded_doc: &str,
    hidden: &[(String, Option<Value>)],
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let mut bindings = vec![text(encoded_rid), text(encoded_doc)];
    let mut sets = vec![Set {
        col_names: vec![nm("doc")],
        expr: Box::new(fcall("jsonb", vec![var(2)])),
    }];
    for (name, value) in hidden {
        validate_physical_name(name, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        let expression = if let Some(value) = value {
            bindings.push(value.clone());
            var(u32::try_from(bindings.len())
                .map_err(|_| FastDbError::Engine("too many hidden bindings".into()))?)
        } else {
            Expr::Literal(Literal::Null)
        };
        sets.push(Set {
            col_names: vec![nm(name)],
            expr: Box::new(expression),
        });
    }
    Ok((
        Stmt::Update(Update {
            with: None,
            or_conflict: None,
            tbl_name: qnm(opaque_table),
            indexed: None,
            sets,
            from: None,
            where_clause: Some(Box::new(Expr::binary(id("rid"), Operator::Equals, var(1)))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        }),
        bindings,
    ))
}

pub fn physical_select_stmt(
    opaque_table: &str,
    encoded_rid: Option<&str>,
    filters: &[(String, FastValue)],
) -> Result<(Stmt, Bindings), FastDbError> {
    let predicates = filters
        .iter()
        .map(|(path, value)| (path.clone(), PredicateOperator::Equal, value.clone()))
        .collect::<Vec<_>>();
    physical_select_predicates_stmt(opaque_table, encoded_rid, &predicates)
}

pub fn physical_select_predicates_stmt(
    opaque_table: &str,
    encoded_rid: Option<&str>,
    predicates: &[(String, PredicateOperator, FastValue)],
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let mut bindings = Vec::new();
    let mut condition = None;
    if let Some(rid) = encoded_rid {
        bindings.push(text(rid));
        condition = Some(Expr::binary(id("rid"), Operator::Equals, var(1)));
    }
    for (path, predicate_operator, value) in predicates {
        bindings.push(engine_scalar(value)?);
        let index = u32::try_from(bindings.len())
            .map_err(|_| FastDbError::Engine("too many translated bindings".into()))?;
        let operator = match predicate_operator {
            PredicateOperator::Equal if matches!(value, FastValue::Null) => Operator::Is,
            PredicateOperator::Equal => Operator::Equals,
            PredicateOperator::Less => Operator::Less,
            PredicateOperator::LessEqual => Operator::LessEquals,
            PredicateOperator::Greater => Operator::Greater,
            PredicateOperator::GreaterEqual => Operator::GreaterEquals,
        };
        let predicate = Expr::binary(json_extract_doc(path), operator, var(index));
        condition = Some(match condition {
            None => predicate,
            Some(previous) => Expr::binary(previous, Operator::And, predicate),
        });
    }
    let doc_json = fcall("json", vec![id("doc")]);
    Ok((
        one_select(
            vec![
                ResultColumn::Expr(Box::new(id("rid")), None),
                ResultColumn::Expr(Box::new(doc_json), None),
            ],
            opaque_table,
            condition,
        ),
        bindings,
    ))
}

pub fn physical_fts_select_stmt(
    opaque_table: &str,
    fts_columns: &[String],
    graph_columns: &[String],
    encoded_rid: Option<&str>,
    query: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    if fts_columns.is_empty() {
        return Err(FastDbError::format("FTS query has no provider columns"));
    }
    for column in fts_columns.iter().chain(graph_columns) {
        validate_physical_name(column, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    }
    if !graph_columns.is_empty() && graph_columns.len() != 4 {
        return Err(FastDbError::format(
            "FTS relation query requires four graph columns",
        ));
    }
    let mut bindings = Vec::new();
    let mut condition = if let Some(rid) = encoded_rid {
        bindings.push(text(rid));
        Some(Expr::binary(id("rid"), Operator::Equals, var(1)))
    } else {
        None
    };
    bindings.push(text(query));
    let query_index = u32::try_from(bindings.len())
        .map_err(|_| FastDbError::Engine("too many FTS bindings".into()))?;
    let arguments = fts_columns
        .iter()
        .map(|column| id(column))
        .chain(std::iter::once(var(query_index)))
        .collect::<Vec<_>>();
    let matched = fcall("fts_match", arguments.clone());
    condition = Some(match condition {
        None => matched,
        Some(previous) => Expr::binary(previous, Operator::And, matched),
    });
    let mut columns = vec![
        ResultColumn::Expr(Box::new(id("rid")), None),
        ResultColumn::Expr(Box::new(fcall("json", vec![id("doc")])), None),
    ];
    columns.extend(
        graph_columns
            .iter()
            .map(|column| ResultColumn::Expr(Box::new(id(column)), None)),
    );
    columns.push(ResultColumn::Expr(
        Box::new(fcall("fts_score", arguments)),
        Some(As::As(nm("__fastdb_fts_score"))),
    ));
    let mut statement = one_select(columns, opaque_table, condition);
    let Stmt::Select(select) = &mut statement else {
        unreachable!("one_select returns SELECT");
    };
    select.limit = Some(Limit {
        expr: Box::new(numlit(10_001)),
        offset: None,
    });
    Ok((statement, bindings))
}

#[allow(clippy::too_many_arguments)]
pub fn physical_vector_select_stmt(
    opaque_table: &str,
    vector_column: &str,
    graph_columns: &[String],
    encoded_rid: Option<&str>,
    predicates: &[(String, PredicateOperator, FastValue)],
    query: Value,
    metric: turso_fastdb_parser::KnnMetric,
    k: u64,
    include_document: bool,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(vector_column, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    for column in graph_columns {
        validate_physical_name(column, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    }
    if !graph_columns.is_empty() && graph_columns.len() != 4 {
        return Err(FastDbError::format(
            "vector relation query requires four graph columns",
        ));
    }
    let mut bindings = Vec::new();
    let mut condition = None;
    if let Some(rid) = encoded_rid {
        bindings.push(text(rid));
        condition = Some(Expr::binary(id("rid"), Operator::Equals, var(1)));
    }
    for (path, predicate_operator, value) in predicates {
        bindings.push(engine_scalar(value)?);
        let index = u32::try_from(bindings.len())
            .map_err(|_| FastDbError::Engine("too many translated bindings".into()))?;
        let operator = match predicate_operator {
            PredicateOperator::Equal if matches!(value, FastValue::Null) => Operator::Is,
            PredicateOperator::Equal => Operator::Equals,
            PredicateOperator::Less => Operator::Less,
            PredicateOperator::LessEqual => Operator::LessEquals,
            PredicateOperator::Greater => Operator::Greater,
            PredicateOperator::GreaterEqual => Operator::GreaterEquals,
        };
        let predicate = Expr::binary(json_extract_doc(path), operator, var(index));
        condition = Some(match condition {
            None => predicate,
            Some(previous) => Expr::binary(previous, Operator::And, predicate),
        });
    }
    condition = Some(match condition {
        None => Expr::binary(
            id(vector_column),
            Operator::IsNot,
            Expr::Literal(Literal::Null),
        ),
        Some(previous) => Expr::binary(
            previous,
            Operator::And,
            Expr::binary(
                id(vector_column),
                Operator::IsNot,
                Expr::Literal(Literal::Null),
            ),
        ),
    });
    bindings.push(query);
    let query_index = u32::try_from(bindings.len())
        .map_err(|_| FastDbError::Engine("too many vector bindings".into()))?;
    let function = match metric {
        turso_fastdb_parser::KnnMetric::Cosine => "vector_distance_cos",
        turso_fastdb_parser::KnnMetric::Euclidean => "vector_distance_l2",
    };
    let distance = fcall(function, vec![id(vector_column), var(query_index)]);
    let document = if include_document {
        fcall("json", vec![id("doc")])
    } else {
        Expr::Literal(Literal::Null)
    };
    let mut columns = vec![
        ResultColumn::Expr(Box::new(id("rid")), None),
        ResultColumn::Expr(Box::new(document), None),
    ];
    columns.extend(
        graph_columns
            .iter()
            .map(|column| ResultColumn::Expr(Box::new(id(column)), None)),
    );
    columns.push(ResultColumn::Expr(
        Box::new(distance.clone()),
        Some(As::As(nm("__fastdb_vector_distance"))),
    ));
    let mut statement = one_select(columns, opaque_table, condition);
    let Stmt::Select(select) = &mut statement else {
        unreachable!("one_select returns SELECT");
    };
    select.order_by = vec![
        SortedColumn {
            expr: Box::new(distance),
            order: Some(SortOrder::Asc),
            nulls: None,
        },
        SortedColumn {
            expr: Box::new(id("rid")),
            order: Some(SortOrder::Asc),
            nulls: None,
        },
    ];
    select.limit = Some(Limit {
        expr: Box::new(numlit(k)),
        offset: None,
    });
    Ok((statement, bindings))
}

pub fn physical_vector_validation_stmt(
    opaque_table: &str,
    vector_column: &str,
    after_rid: Option<&str>,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    validate_physical_name(vector_column, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    let (condition, bindings) = if let Some(rid) = after_rid {
        (
            Some(Expr::binary(id("rid"), Operator::Greater, var(1))),
            vec![text(rid)],
        )
    } else {
        (None, Vec::new())
    };
    let mut statement = one_select(
        vec![
            ResultColumn::Expr(Box::new(id("rid")), None),
            ResultColumn::Expr(Box::new(fcall("json", vec![id("doc")])), None),
            ResultColumn::Expr(Box::new(id(vector_column)), None),
        ],
        opaque_table,
        condition,
    );
    let Stmt::Select(select) = &mut statement else {
        unreachable!("one_select returns SELECT");
    };
    select.order_by = vec![SortedColumn {
        expr: Box::new(id("rid")),
        order: Some(SortOrder::Asc),
        nulls: None,
    }];
    select.limit = Some(Limit {
        expr: Box::new(numlit(256)),
        offset: None,
    });
    Ok((statement, bindings))
}

pub fn physical_relation_select_predicates_stmt(
    opaque_table: &str,
    hidden_columns: &[String],
    encoded_rid: Option<&str>,
    predicates: &[(String, PredicateOperator, FastValue)],
) -> Result<(Stmt, Bindings), FastDbError> {
    let (statement, bindings) =
        physical_select_predicates_stmt(opaque_table, encoded_rid, predicates)?;
    let Stmt::Select(mut select) = statement else {
        unreachable!("physical select builder returns SELECT")
    };
    if hidden_columns.len() != 4 {
        return Err(FastDbError::format(
            "relation select requires four endpoint columns",
        ));
    }
    let OneSelect::Select { columns, .. } = &mut select.body.select else {
        unreachable!("physical select builder returns simple SELECT")
    };
    for hidden in hidden_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
        columns.push(ResultColumn::Expr(Box::new(id(hidden)), None));
    }
    Ok((Stmt::Select(select), bindings))
}

pub fn physical_graph_neighbors_stmt(
    opaque_table: &str,
    hidden_columns: &[String],
    forward: bool,
    endpoint_table_id: &str,
    endpoint_rid: &str,
    other_table_id: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    if hidden_columns.len() != 4 {
        return Err(FastDbError::format(
            "graph neighbor lookup requires four endpoint columns",
        ));
    }
    for hidden in hidden_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    }
    let (table_column, rid_column, other_table_column, result_column) = if forward {
        (
            &hidden_columns[0],
            &hidden_columns[1],
            &hidden_columns[2],
            &hidden_columns[3],
        )
    } else {
        (
            &hidden_columns[2],
            &hidden_columns[3],
            &hidden_columns[0],
            &hidden_columns[1],
        )
    };
    let condition = Expr::binary(
        Expr::binary(
            Expr::binary(id(table_column), Operator::Equals, var(1)),
            Operator::And,
            Expr::binary(id(rid_column), Operator::Equals, var(2)),
        ),
        Operator::And,
        Expr::binary(id(other_table_column), Operator::Equals, var(3)),
    );
    Ok((
        one_select(
            vec![ResultColumn::Expr(Box::new(id(result_column)), None)],
            opaque_table,
            Some(condition),
        ),
        vec![
            text(endpoint_table_id),
            text(endpoint_rid),
            text(other_table_id),
        ],
    ))
}

pub fn physical_graph_connected_edge_ids_stmt(
    opaque_table: &str,
    hidden_columns: &[String],
    forward: bool,
    endpoint_table_id: &str,
    endpoint_rid: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    if hidden_columns.len() != 4 {
        return Err(FastDbError::format(
            "graph cascade lookup requires four endpoint columns",
        ));
    }
    for hidden in hidden_columns {
        validate_physical_name(hidden, crate::names::HIDDEN_COLUMN_NAME_PREFIX)?;
    }
    let (table_column, rid_column) = if forward {
        (&hidden_columns[0], &hidden_columns[1])
    } else {
        (&hidden_columns[2], &hidden_columns[3])
    };
    let condition = Expr::binary(
        Expr::binary(id(table_column), Operator::Equals, var(1)),
        Operator::And,
        Expr::binary(id(rid_column), Operator::Equals, var(2)),
    );
    Ok((
        one_select(
            vec![ResultColumn::Expr(Box::new(id("rid")), None)],
            opaque_table,
            Some(condition),
        ),
        vec![text(endpoint_table_id), text(endpoint_rid)],
    ))
}

pub fn physical_all_rows_stmt(opaque_table: &str) -> Result<Stmt, FastDbError> {
    physical_select_stmt(opaque_table, None, &[]).map(|(statement, _)| statement)
}

pub fn physical_delete_by_rid_stmt(
    opaque_table: &str,
    encoded_rid: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    Ok((
        Stmt::Delete {
            with: None,
            tbl_name: qnm(opaque_table),
            indexed: None,
            where_clause: Some(Box::new(Expr::binary(id("rid"), Operator::Equals, var(1)))),
            returning: vec![],
            order_by: vec![],
            limit: None,
        },
        vec![text(encoded_rid)],
    ))
}

fn insert_values(
    table: &str,
    columns: &[&str],
    expressions: Vec<Expr>,
    bindings: Bindings,
) -> (Stmt, Bindings) {
    (
        Stmt::Insert {
            with: None,
            or_conflict: None,
            tbl_name: qnm(table),
            columns: columns.iter().map(|name| nm(name)).collect(),
            body: InsertBody::Select(
                Select {
                    with: None,
                    body: SelectBody {
                        select: OneSelect::Values(vec![expressions
                            .into_iter()
                            .map(Box::new)
                            .collect()]),
                        compounds: vec![],
                    },
                    order_by: vec![],
                    limit: None,
                },
                None,
            ),
            returning: vec![],
        },
        bindings,
    )
}

fn insert_values_owned(
    table: &str,
    columns: &[String],
    expressions: Vec<Expr>,
    bindings: Bindings,
) -> (Stmt, Bindings) {
    (
        Stmt::Insert {
            with: None,
            or_conflict: None,
            tbl_name: qnm(table),
            columns: columns.iter().map(|name| nm(name)).collect(),
            body: InsertBody::Select(
                Select {
                    with: None,
                    body: SelectBody {
                        select: OneSelect::Values(vec![expressions
                            .into_iter()
                            .map(Box::new)
                            .collect()]),
                        compounds: vec![],
                    },
                    order_by: vec![],
                    limit: None,
                },
                None,
            ),
            returning: vec![],
        },
        bindings,
    )
}

fn one_select(columns: Vec<ResultColumn>, table: &str, where_clause: Option<Expr>) -> Stmt {
    Stmt::Select(Select {
        with: None,
        body: SelectBody {
            select: OneSelect::Select {
                distinctness: None,
                columns,
                from: Some(FromClause {
                    select: Box::new(SelectTable::Table(qnm(table), None, None)),
                    joins: vec![],
                }),
                where_clause: where_clause.map(Box::new),
                group_by: None,
                window_clause: vec![],
            },
            compounds: vec![],
        },
        order_by: vec![],
        limit: None,
    })
}

fn text(value: &str) -> Value {
    Value::build_text(value.to_string())
}

fn optional_text_binding(bindings: &mut Bindings, value: Option<&str>) -> Expr {
    if let Some(value) = value {
        bindings.push(text(value));
        let index = u32::try_from(bindings.len()).expect("catalog binding count fits u32");
        var(index)
    } else {
        Expr::Literal(Literal::Null)
    }
}

fn engine_scalar(value: &FastValue) -> Result<Value, FastDbError> {
    match value {
        FastValue::Null => Ok(Value::Null),
        FastValue::Bool(value) => Ok(Value::from_i64(i64::from(*value))),
        FastValue::Integer(value) => Ok(Value::from_i64(*value)),
        FastValue::Float(value) if value.is_finite() => Ok(Value::from_f64(*value)),
        FastValue::Str(value) => Ok(text(value)),
        FastValue::Float(_) => Err(FastDbError::Schema("non-finite filter value".into())),
        FastValue::None
        | FastValue::Decimal(_)
        | FastValue::Bytes(_)
        | FastValue::Duration(_)
        | FastValue::Datetime(_)
        | FastValue::Uuid(_)
        | FastValue::Array(_)
        | FastValue::Object(_)
        | FastValue::Set(_)
        | FastValue::Range(_)
        | FastValue::Regex(_)
        | FastValue::RecordId(_)
        | FastValue::Table(_)
        | FastValue::File(_) => Err(FastDbError::UnsupportedSyntax(
            turso_fastdb_parser::ParseError::unsupported(
                "filters support only scalar constants in this phase",
                turso_fastdb_parser::Span::default(),
            ),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn p2_path_003_filter_and_index_use_structurally_identical_expression_ast() {
        let table = "__fastdb_t_00000000000000000000000000000001";
        let index = "__fastdb_i_00000000000000000000000000000002";
        let path = r#"$."profile"."age""#.to_string();
        let Stmt::CreateIndex { columns, .. } =
            physical_index_ddl(index, table, std::slice::from_ref(&path), false).unwrap()
        else {
            panic!("index lowering returned the wrong statement kind");
        };
        let index_expression = columns[0].expr.as_ref();

        let (select, _) =
            physical_select_stmt(table, None, &[(path, FastValue::Integer(42))]).unwrap();
        let Stmt::Select(select) = select else {
            panic!("filter lowering returned the wrong statement kind");
        };
        let OneSelect::Select {
            where_clause: Some(predicate),
            ..
        } = &select.body.select
        else {
            panic!("filter lowering omitted its predicate");
        };
        let Expr::Binary(filter_expression, Operator::Equals, _) = predicate.as_ref() else {
            panic!("filter lowering returned the wrong predicate shape");
        };

        assert_eq!(filter_expression.as_ref(), index_expression);
    }
}
