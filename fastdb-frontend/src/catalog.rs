//! Disposable Phase 0 catalog.
//!
//! Two static internal tables store format metadata and the logical→physical
//! table map. Physical table/index names are opaque (P0.4). All catalog data
//! statements bind the logical name as a parameter; user identifiers are
//! never interpolated into catalog SQL. Format and dialect versions are `0`
//! and explicitly disposable.

use crate::connection::{value_to_i64, value_to_string, Connection};
use crate::error::{FastDbError, Result};
use crate::lower;
use crate::names::{physical_table_name, TableId, TABLE_NAME_PREFIX};
use turso_core::Value;

/// Reserved metadata table name.
pub const META_TABLE: &str = "__fastdb_meta";
/// Reserved logical→physical map table name.
pub const TABLES_TABLE: &str = "__fastdb_tables";

/// Phase 0 disposable format version.
pub const FORMAT_VERSION: i64 = 0;
pub const DIALECT_VERSION: i64 = 0;

/// Stored definition text for Phase 0 schemaless tables.
const DEFINITION_PHASE0: &str = "SCHEMALESS (phase0)";

/// Logical names with this prefix are reserved and may not be used through
/// the Phase 0 frontend. (The Phase 0 grammar would otherwise accept them as
/// ordinary identifiers; this guard prevents collision with catalog tables.)
pub fn is_reserved_logical_name(name: &str) -> bool {
    name.starts_with("__fastdb_")
}

/// A logical table resolved through the catalog.
#[derive(Debug, Clone)]
pub struct ResolvedTable {
    pub logical: String,
    pub table_id_hex: String,
    pub physical_name: String,
}

/// Does `name` exist as a table in the engine schema?
pub fn catalog_exists(conn: &Connection, name: &str) -> Result<bool> {
    let stmt = lower::catalog_exists_stmt();
    let bindings = vec![Value::build_text(name.to_string())];
    let rows = conn.collect_rows(stmt, bindings)?;
    Ok(!rows.is_empty())
}

/// Read the persisted `(format_version, dialect_version)`. Caller must
/// ensure `__fastdb_meta` exists.
pub fn read_catalog_versions(conn: &Connection) -> Result<(i64, i64)> {
    let rows = conn.collect_rows(lower::catalog_versions_stmt(), vec![])?;
    let row = rows
        .into_iter()
        .next()
        .ok_or_else(|| FastDbError::Format("__fastdb_meta row missing".into()))?;
    Ok((value_to_i64(&row[0])?, value_to_i64(&row[1])?))
}

/// Refuse an existing catalog whose format or dialect version is not the
/// Phase 0 value (`0`). Called before resolving or interpreting the catalog
/// on every operation, so a future-format/dialect database is never mutated.
pub fn ensure_catalog_compatible(conn: &Connection) -> Result<()> {
    let (format_version, dialect_version) = read_catalog_versions(conn)?;
    if format_version != FORMAT_VERSION {
        return Err(FastDbError::Format(format!(
            "unknown Phase 0 format version {format_version}; only {FORMAT_VERSION} is supported"
        )));
    }
    if dialect_version != DIALECT_VERSION {
        return Err(FastDbError::Format(format!(
            "unknown Phase 0 dialect version {dialect_version}; only {DIALECT_VERSION} is supported"
        )));
    }
    Ok(())
}

/// Create the two catalog tables and the singleton metadata row. Run inside
/// the first-mutation transaction.
pub fn bootstrap_catalog(conn: &Connection, database_id: &str) -> Result<()> {
    conn.exec_bound(lower::catalog_meta_ddl(), vec![])?;
    conn.exec_bound(lower::catalog_tables_ddl(), vec![])?;
    let (ins, bindings) = lower::catalog_meta_insert(database_id);
    conn.exec_bound(ins, bindings)?;
    Ok(())
}

/// Resolve a logical name to its physical table, or `None` if unregistered.
pub fn resolve_table(conn: &Connection, logical: &str) -> Result<Option<ResolvedTable>> {
    let (stmt, bindings) = lower::catalog_lookup_stmt(logical);
    let rows = conn.collect_rows(stmt, bindings)?;
    match rows.into_iter().next() {
        None => Ok(None),
        Some(row) => {
            let table_id_hex = value_to_string(&row[0])?;
            let physical_name = value_to_string(&row[1])?;
            // Re-validate the persisted opaque name before trusting it.
            crate::names::validate_physical_name(&physical_name, TABLE_NAME_PREFIX)?;
            Ok(Some(ResolvedTable {
                logical: logical.to_string(),
                table_id_hex,
                physical_name,
            }))
        }
    }
}

/// Allocate a fresh catalog id and opaque physical name for a new logical
/// table. Does not touch the engine.
pub fn allocate_table(logical: &str) -> Result<ResolvedTable> {
    if is_reserved_logical_name(logical) {
        return Err(FastDbError::Constraint(format!(
            "logical name {logical:?} uses a reserved prefix"
        )));
    }
    let id = TableId::new_random();
    let table_id_hex = id.to_hex();
    let physical_name = physical_table_name(id);
    Ok(ResolvedTable {
        logical: logical.to_string(),
        table_id_hex,
        physical_name,
    })
}

/// Insert the catalog row for a new table (logical name bound).
pub fn insert_catalog_row(conn: &Connection, resolved: &ResolvedTable) -> Result<()> {
    let (ins, bindings) = lower::catalog_register_stmt(
        &resolved.table_id_hex,
        &resolved.logical,
        &resolved.physical_name,
        DEFINITION_PHASE0,
    );
    conn.exec_bound(ins, bindings)
}

/// Create the hidden physical table for a resolved table.
pub fn create_physical_table(conn: &Connection, resolved: &ResolvedTable) -> Result<()> {
    let ddl = lower::physical_table_ddl(&resolved.physical_name)?;
    conn.exec_bound(ddl, vec![])
}
