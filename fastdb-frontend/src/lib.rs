//! # turso_fastdb
//!
//! FastDB Phase 0 frontend: parses SurrealQL-subset input with the
//! independent parser crate, lowers it directly into Turso AST, executes
//! via `Connection::prepare_translated_stmt_with_options`, and decodes
//! results into FastDB value/record types.
//!
//! The request path is:
//! ```text
//! FastDB source -> parser AST -> frontend plan
//!   -> Turso AST + bound values -> prepare_translated_stmt_with_options
//!   -> Turso execution -> FastDB result decoding
//! ```
//! FastDB user input is never parsed by Turso's SQLite parser and never
//! rendered into SQLite text. Logical identifiers resolve through the
//! catalog; physical names are opaque.

// Deny warnings in-crate so that FastDB code stays lint-clean. This is
// scoped to FastDB crates only: upstream workspace-member dependencies
// (e.g. turso_core) carry their own, pre-existing lint state and are not
// fixed by Phase 0 (per plan-phase0.md: "do not fix upstream failures").
// The verified clippy command is therefore `cargo clippy -p <fastdb crate>
// --all-targets` WITHOUT a global `-D warnings`, which would otherwise
// fatalize pre-existing upstream warnings. See docs/phase0-engine-audit.md.
#![forbid(unsafe_code)]
#![deny(warnings)]

pub mod catalog;
pub mod connection;
pub mod decode;
pub mod error;
pub mod execute;
pub mod lower;
pub mod names;
pub mod test_failpoints;

pub use connection::{Connection, Database};
pub use decode::{parse_doc, Record, RecordId, Value};
pub use error::{ErrorCategory, FastDbError};
pub use test_failpoints::Failpoint;

/// Result of executing one FastDB statement. `records` is empty for `DELETE`
/// and for selects that match nothing; `CREATE` returns the created record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    pub records: Vec<Record>,
}

impl ExecutionResult {
    pub fn len(&self) -> usize {
        self.records.len()
    }
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_create_select_filter_delete_memory() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();

        let r = conn
            .execute("CREATE person:tracy SET name = 'Tracy';")
            .unwrap();
        assert_eq!(r.records.len(), 1);
        let rec = &r.records[0];
        assert_eq!(rec.id.table, "person");
        assert_eq!(rec.id.id, "tracy");
        assert_eq!(
            rec.fields,
            vec![("name".to_string(), Value::Str("Tracy".to_string()))]
        );

        let r = conn.execute("SELECT * FROM person:tracy;").unwrap();
        assert_eq!(r.records.len(), 1);
        assert_eq!(r.records[0].id.id, "tracy");

        let r = conn
            .execute("SELECT * FROM person WHERE name = 'Tracy';")
            .unwrap();
        assert_eq!(r.records.len(), 1);

        let r = conn.execute("DELETE person:tracy;").unwrap();
        assert!(r.records.is_empty());

        let r = conn.execute("SELECT * FROM person:tracy;").unwrap();
        assert!(r.records.is_empty());
    }

    #[test]
    fn smoke_read_of_empty_db_does_not_mutate() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        let r = conn.execute("SELECT * FROM person:tracy;").unwrap();
        assert!(r.records.is_empty());
        // No catalog should have been created by the read.
        let exists = crate::catalog::catalog_exists(&conn, crate::catalog::META_TABLE).unwrap();
        assert!(!exists, "read of empty db must not create catalog");
    }

    #[test]
    fn smoke_duplicate_create_is_constraint_error() {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE person:tracy SET name = 'Tracy';")
            .unwrap();
        let err = conn
            .execute("CREATE person:tracy SET name = 'Other';")
            .unwrap_err();
        assert_eq!(err.category(), ErrorCategory::Constraint);
    }
}
