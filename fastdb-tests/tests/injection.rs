//! P0-INJECT-001 — A user value containing a quote and a semicolon is stored
//! as data and cannot alter the schema or be executed as a separate statement.
//!
//! `'a;''b'` decodes to the literal value `a;'b`. The value round-trips
//! through CREATE -> reopen -> SELECT exactly, the stored `doc` contains it
//! verbatim, and the schema is unchanged (no extra/dropped tables).

#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::tempdir;
use turso_fastdb::{Database, Value};

#[test]
fn quoted_semicolon_value_stored_literally_and_schema_unchanged() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("inj.fastdb");
    let s = path.to_str().unwrap();

    {
        let db = Database::open(s).unwrap();
        let conn = db.connect().unwrap();
        // The value contains a semicolon and an escaped quote; it is data.
        let r = conn.execute("CREATE person:x SET name = 'a;''b';").unwrap();
        assert_eq!(r.records.len(), 1);
        assert_eq!(
            r.records[0].fields,
            vec![("name".to_string(), Value::Str("a;'b".to_string()))]
        );
    }

    // Reopen and read back: the value is byte-for-byte literal.
    let db = Database::open(s).unwrap();
    let conn = db.connect().unwrap();
    let r = conn.execute("SELECT * FROM person:x;").unwrap();
    assert_eq!(r.records.len(), 1);
    assert_eq!(r.records[0].id.id, "x");
    assert_eq!(
        r.records[0].fields,
        vec![("name".to_string(), Value::Str("a;'b".to_string()))]
    );

    // The stored doc contains the literal value and nothing injected.
    let native = conn.native();
    let physical = common::physical_name_for(native, "person").unwrap();
    let rows = common::native_rows(native, &format!("SELECT json(doc) FROM {physical}"));
    assert_eq!(rows.len(), 1);
    assert!(
        rows[0][0].contains("a;'b"),
        "doc stored literally: {}",
        rows[0][0]
    );

    // Schema unchanged: four stable catalogs and one opaque physical table —
    // no injected table and nothing dropped.
    let tables = common::native_rows(native, "SELECT name FROM sqlite_schema WHERE type='table'");
    assert_eq!(
        tables.len(),
        5,
        "no injected/dropped tables, got: {tables:?}"
    );
    assert_eq!(common::integrity_check(native), "ok");
}
