//! Direct FastDB → Turso AST lowering and the internal catalog statement
//! builders.
//!
//! Every statement produced here is a directly-constructed
//! `turso_parser::ast::Stmt` executed via
//! `Connection::prepare_translated_stmt_with_options`. No FastDB input is
//! ever parsed by Turso's SQLite parser, and no user value or logical
//! identifier is interpolated into SQL text:
//!
//! - user values are `Expr::Variable` bound with `Statement::bind_at`;
//! - logical table names resolve through the catalog and never appear in
//!   generated SQL;
//! - physical table/index names are validated opaque names from P0.4;
//! - the only static text that appears is the canonical JSON path and the
//!   static internal catalog DDL, both built in this one reviewed module.

use crate::error::FastDbError;
use crate::names::{validate_physical_name, INDEX_NAME_PREFIX, TABLE_NAME_PREFIX};
use std::num::NonZeroU32;
use turso_core::Value;
use turso_parser::ast::*;

/// 1-based parameter index used both in `Expr::Variable` and for
/// `Statement::bind_at`. A lowering returns a [`Stmt`] plus an ordered
/// `Vec<Value>` whose position `i` is bound at index `i+1`.
pub type Bindings = Vec<Value>;

/// Internal-only source string attached to translated statements for
/// diagnostics. It never becomes persisted DDL.
pub(crate) const TRANSLATED_INPUT: &str = "<fastdb-translated>";

// ---------- small constructors ----------

fn nm(s: &str) -> Name {
    Name::from_string(s)
}
fn qnm(s: &str) -> QualifiedName {
    QualifiedName::single(nm(s))
}
fn id(s: &str) -> Expr {
    Expr::Id(nm(s))
}
fn var(idx: u32) -> Expr {
    Expr::Variable(Variable::indexed(
        NonZeroU32::new(idx).expect("nonzero param index"),
    ))
}
fn strlit(s: &str) -> Expr {
    // `Literal::String` stores the already-quoted, escaped form (the parser
    // itself builds e.g. `Literal::String("'error'".to_owned())`).
    let quoted = format!("'{}'", s.replace('\'', "''"));
    Expr::Literal(Literal::String(quoted))
}
fn numlit(s: &'static str) -> Expr {
    Expr::Literal(Literal::Numeric(s.to_string()))
}
fn ty(s: &'static str) -> Type {
    Type {
        name: s.to_string(),
        size: None,
        array_dimensions: 0,
    }
}
fn fcall(name_str: &'static str, args: Vec<Expr>) -> Expr {
    Expr::FunctionCall {
        name: nm(name_str),
        distinctness: None,
        args: args.into_iter().map(Box::new).collect(),
        order_by: vec![],
        within_group: vec![],
        filter_over: FunctionTail {
            filter_clause: None,
            over_clause: None,
        },
    }
}
fn pk_constraint() -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::PrimaryKey {
            order: None,
            conflict_clause: None,
            auto_increment: false,
        },
    }
}
fn not_null_constraint() -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::NotNull {
            nullable: false,
            conflict_clause: None,
        },
    }
}
fn unique_constraint() -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::Unique(None),
    }
}
fn check_constraint(expr: Expr) -> NamedColumnConstraint {
    NamedColumnConstraint {
        name: None,
        constraint: ColumnConstraint::Check(Box::new(expr)),
    }
}
fn column(
    name: &'static str,
    type_str: &'static str,
    cons: Vec<NamedColumnConstraint>,
) -> ColumnDefinition {
    ColumnDefinition {
        col_name: nm(name),
        col_type: Some(ty(type_str)),
        constraints: cons,
    }
}

fn strict_options() -> TableOptions {
    TableOptions {
        without_rowid_text: None,
        strict_text: Some("STRICT".to_string()),
    }
}

// ---------- transactions ----------

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

// ---------- static internal catalog DDL ----------

/// `CREATE TABLE __fastdb_meta (...) STRICT`.
pub fn catalog_meta_ddl() -> Stmt {
    let columns = vec![
        column(
            "singleton",
            "INTEGER",
            vec![
                pk_constraint(),
                check_constraint(Expr::binary(id("singleton"), Operator::Equals, numlit("1"))),
            ],
        ),
        column("format_version", "INTEGER", vec![not_null_constraint()]),
        column("dialect_version", "INTEGER", vec![not_null_constraint()]),
        column("database_id", "TEXT", vec![not_null_constraint()]),
    ];
    Stmt::CreateTable {
        temporary: false,
        if_not_exists: false,
        tbl_name: qnm(crate::catalog::META_TABLE),
        body: CreateTableBody::ColumnsAndConstraints {
            columns,
            constraints: vec![],
            options: strict_options(),
        },
    }
}

/// `CREATE TABLE __fastdb_tables (...) STRICT`.
pub fn catalog_tables_ddl() -> Stmt {
    let mode_check = Expr::InList {
        lhs: Box::new(id("mode")),
        not: false,
        rhs: vec![
            Box::new(strlit("SCHEMALESS")),
            Box::new(strlit("SCHEMAFULL")),
        ],
    };
    let columns = vec![
        column("table_id", "TEXT", vec![pk_constraint()]),
        column(
            "logical_name",
            "TEXT",
            vec![not_null_constraint(), unique_constraint()],
        ),
        column(
            "physical_name",
            "TEXT",
            vec![not_null_constraint(), unique_constraint()],
        ),
        column(
            "mode",
            "TEXT",
            vec![not_null_constraint(), check_constraint(mode_check)],
        ),
        column("definition", "TEXT", vec![not_null_constraint()]),
    ];
    Stmt::CreateTable {
        temporary: false,
        if_not_exists: false,
        tbl_name: qnm(crate::catalog::TABLES_TABLE),
        body: CreateTableBody::ColumnsAndConstraints {
            columns,
            constraints: vec![],
            options: strict_options(),
        },
    }
}

/// `CREATE TABLE <opaque> (rid TEXT PRIMARY KEY, doc BLOB NOT NULL) STRICT`.
///
/// The logical `doc` invariant is JSONB content: every stored `doc` is
/// produced by `jsonb(...)` and read back only through `json`/`json_extract`.
/// STRICT mode rejects the literal type name `JSONB` (only INT/INTEGER/REAL/
/// TEXT/BLOB/ANY are STRICT-valid on the pinned engine), so the physical
/// column is declared `BLOB`, which is exactly how JSONB is represented
/// internally (a BLOB subtype). This is the documented, plan-sanctioned
/// "smallest valid physical declaration without weakening the JSONB
/// invariant." See docs/phase0-engine-audit.md.
pub fn physical_table_ddl(opaque_name: &str) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_name, TABLE_NAME_PREFIX)?;
    let columns = vec![
        column("rid", "TEXT", vec![pk_constraint()]),
        column("doc", "BLOB", vec![not_null_constraint()]),
    ];
    Ok(Stmt::CreateTable {
        temporary: false,
        if_not_exists: false,
        tbl_name: qnm(opaque_name),
        body: CreateTableBody::ColumnsAndConstraints {
            columns,
            constraints: vec![],
            options: strict_options(),
        },
    })
}

/// Test-only non-unique expression index on the canonical `name` field.
/// `CREATE INDEX <opaque_index> ON <opaque_table> (json_extract(doc,'$.name'))`.
pub fn physical_name_index_ddl(
    opaque_index: &str,
    opaque_table: &str,
    path: &str,
) -> Result<Stmt, FastDbError> {
    validate_physical_name(opaque_index, INDEX_NAME_PREFIX)?;
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let idx = Stmt::CreateIndex {
        unique: false,
        if_not_exists: false,
        idx_name: qnm(opaque_index),
        tbl_name: nm(opaque_table),
        using: None,
        columns: vec![SortedColumn {
            expr: Box::new(json_extract_doc(path)),
            order: None,
            nulls: None,
        }],
        with_clause: vec![],
        where_clause: None,
    };
    Ok(idx)
}

// ---------- canonical JSON expression (shared by index + filter) ----------

/// Build the canonical top-level JSON path `$.<field>` for a single Phase 0
/// identifier field. This is the sole owner of path construction. The lexer
/// already restricts identifiers; here we additionally reject any character
/// that could break out of a JSON path.
pub fn canonical_field_path(field: &str) -> Result<String, FastDbError> {
    if field.is_empty() {
        return Err(FastDbError::format("JSON path field is empty"));
    }
    let bad = |c: char| matches!(c, '.' | '[' | ']' | '\'' | '"' | '\\' | ' ') || c.is_control();
    if field.chars().any(bad) {
        return Err(FastDbError::format(
            "field contains characters not allowed in a Phase 0 JSON path",
        ));
    }
    Ok(format!("$.{field}"))
}

/// `json_extract(doc, '<path>')` as an AST expression. Identical structure is
/// used by the expression-index definition and the equality filter so the
/// optimizer can match them.
pub fn json_extract_doc(path: &str) -> Expr {
    fcall("json_extract", vec![id("doc"), strlit(path)])
}

// ---------- catalog data statements (bound logical names) ----------

/// `SELECT 1 FROM sqlite_schema WHERE type='table' AND name = ?1`.
/// Caller binds the object name at `?1`.
pub fn catalog_exists_stmt() -> Stmt {
    Stmt::Select(Select {
        with: None,
        body: SelectBody {
            select: OneSelect::Select {
                distinctness: None,
                columns: vec![ResultColumn::Expr(Box::new(numlit("1")), None)],
                from: Some(FromClause {
                    select: Box::new(SelectTable::Table(qnm("sqlite_schema"), None, None)),
                    joins: vec![],
                }),
                where_clause: Some(Box::new(Expr::binary(
                    Expr::binary(strlit("table"), Operator::Equals, id("type")),
                    Operator::And,
                    Expr::binary(id("name"), Operator::Equals, var(1)),
                ))),
                group_by: None,
                window_clause: vec![],
            },
            compounds: vec![],
        },
        order_by: vec![],
        limit: None,
    })
}

/// `SELECT format_version, dialect_version FROM __fastdb_meta WHERE singleton = 1`.
pub fn catalog_versions_stmt() -> Stmt {
    one_select(
        vec![
            ResultColumn::Expr(Box::new(id("format_version")), None),
            ResultColumn::Expr(Box::new(id("dialect_version")), None),
        ],
        crate::catalog::META_TABLE,
        Some(Expr::binary(id("singleton"), Operator::Equals, numlit("1"))),
    )
}

/// `INSERT INTO __fastdb_meta VALUES (1, 0, 0, ?1)` (database_id bound).
pub fn catalog_meta_insert(database_id: &str) -> (Stmt, Bindings) {
    let stmt = Stmt::Insert {
        with: None,
        or_conflict: None,
        tbl_name: qnm(crate::catalog::META_TABLE),
        columns: vec![
            nm("singleton"),
            nm("format_version"),
            nm("dialect_version"),
            nm("database_id"),
        ],
        body: InsertBody::Select(
            Select {
                with: None,
                body: SelectBody {
                    select: OneSelect::Values(vec![vec![
                        Box::new(numlit("1")),
                        Box::new(numlit("0")),
                        Box::new(numlit("0")),
                        Box::new(var(1)),
                    ]]),
                    compounds: vec![],
                },
                order_by: vec![],
                limit: None,
            },
            None,
        ),
        returning: vec![],
    };
    let bindings = vec![Value::build_text(database_id.to_string())];
    (stmt, bindings)
}

/// `SELECT table_id, physical_name FROM __fastdb_tables WHERE logical_name = ?1`.
pub fn catalog_lookup_stmt(logical_name: &str) -> (Stmt, Bindings) {
    let stmt = one_select(
        vec![
            ResultColumn::Expr(Box::new(id("table_id")), None),
            ResultColumn::Expr(Box::new(id("physical_name")), None),
        ],
        crate::catalog::TABLES_TABLE,
        Some(Expr::binary(id("logical_name"), Operator::Equals, var(1))),
    );
    (stmt, bindings_text(vec![logical_name]))
}

/// `INSERT INTO __fastdb_tables VALUES (?1, ?2, ?3, 'SCHEMALESS', ?4)`.
pub fn catalog_register_stmt(
    table_id_hex: &str,
    logical_name: &str,
    physical_name: &str,
    definition: &str,
) -> (Stmt, Bindings) {
    let stmt = Stmt::Insert {
        with: None,
        or_conflict: None,
        tbl_name: qnm(crate::catalog::TABLES_TABLE),
        columns: vec![
            nm("table_id"),
            nm("logical_name"),
            nm("physical_name"),
            nm("mode"),
            nm("definition"),
        ],
        body: InsertBody::Select(
            Select {
                with: None,
                body: SelectBody {
                    select: OneSelect::Values(vec![vec![
                        Box::new(var(1)),
                        Box::new(var(2)),
                        Box::new(var(3)),
                        Box::new(strlit("SCHEMALESS")),
                        Box::new(var(4)),
                    ]]),
                    compounds: vec![],
                },
                order_by: vec![],
                limit: None,
            },
            None,
        ),
        returning: vec![],
    };
    let bindings = bindings_text(vec![table_id_hex, logical_name, physical_name, definition]);
    (stmt, bindings)
}

// ---------- physical data statements ----------

/// User `CREATE`: `INSERT INTO <opaque> (rid, doc) VALUES (?1, jsonb(json_object(?2, ?3)))`.
/// `?1`=encoded rid, `?2`=field name, `?3`=field value. The engine builds the
/// JSON object so no FastDB-side JSON construction or escaping is needed.
pub fn physical_insert_stmt(
    opaque_table: &str,
    encoded_rid: &str,
    field_name: &str,
    field_value: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let doc_expr = fcall("jsonb", vec![fcall("json_object", vec![var(2), var(3)])]);
    let stmt = Stmt::Insert {
        with: None,
        or_conflict: None,
        tbl_name: qnm(opaque_table),
        columns: vec![nm("rid"), nm("doc")],
        body: InsertBody::Select(
            Select {
                with: None,
                body: SelectBody {
                    select: OneSelect::Values(vec![vec![Box::new(var(1)), Box::new(doc_expr)]]),
                    compounds: vec![],
                },
                order_by: vec![],
                limit: None,
            },
            None,
        ),
        returning: vec![],
    };
    let bindings = bindings_text(vec![encoded_rid, field_name, field_value]);
    Ok((stmt, bindings))
}

/// Record read: `SELECT rid, json(doc) FROM <opaque> WHERE rid = ?1`.
pub fn physical_select_by_rid_stmt(
    opaque_table: &str,
    encoded_rid: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let doc_json = fcall("json", vec![id("doc")]);
    let stmt = one_select(
        vec![
            ResultColumn::Expr(Box::new(id("rid")), None),
            ResultColumn::Expr(Box::new(doc_json), None),
        ],
        opaque_table,
        Some(Expr::binary(id("rid"), Operator::Equals, var(1))),
    );
    Ok((stmt, bindings_text(vec![encoded_rid])))
}

/// Equality filter: `SELECT rid, json(doc) FROM <opaque> WHERE json_extract(doc,'$.<field>') = ?1`.
pub fn physical_select_by_field_stmt(
    opaque_table: &str,
    path: &str,
    field_value: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let doc_json = fcall("json", vec![id("doc")]);
    let where_expr = Expr::binary(json_extract_doc(path), Operator::Equals, var(1));
    let stmt = one_select(
        vec![
            ResultColumn::Expr(Box::new(id("rid")), None),
            ResultColumn::Expr(Box::new(doc_json), None),
        ],
        opaque_table,
        Some(where_expr),
    );
    Ok((stmt, bindings_text(vec![field_value])))
}

/// `DELETE FROM <opaque> WHERE rid = ?1`.
pub fn physical_delete_by_rid_stmt(
    opaque_table: &str,
    encoded_rid: &str,
) -> Result<(Stmt, Bindings), FastDbError> {
    validate_physical_name(opaque_table, TABLE_NAME_PREFIX)?;
    let stmt = Stmt::Delete {
        with: None,
        tbl_name: qnm(opaque_table),
        indexed: None,
        where_clause: Some(Box::new(Expr::binary(id("rid"), Operator::Equals, var(1)))),
        returning: vec![],
        order_by: vec![],
        limit: None,
    };
    Ok((stmt, bindings_text(vec![encoded_rid])))
}

// ---------- helpers ----------

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

fn bindings_text(values: Vec<&str>) -> Bindings {
    values
        .into_iter()
        .map(|v| Value::build_text(v.to_string()))
        .collect()
}
