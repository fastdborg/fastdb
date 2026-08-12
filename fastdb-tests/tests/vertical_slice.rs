//! P0.7 — File-backed vertical slice and reopen.
//!
//! Exact sequence from `plan-phase0.md` work package P0.7: create, assert,
//! drop, reopen, select, inspect internal state, delete, reopen, and
//! `PRAGMA integrity_check`.

#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::collections::HashMap;
use tempfile::tempdir;
use turso_fastdb::{Database, Value};

/// Assert the canonical "create → reopen → select → inspect → delete →
/// reopen → integrity" lifecycle.
#[test]
fn file_backed_vertical_slice() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("vertical.fastdb");
    let path_str = path.to_str().unwrap();

    // 1-4. Open, CREATE, assert typed record.
    let created = {
        let db = Database::open(path_str).unwrap();
        let conn = db.connect().unwrap();
        let r = conn.execute("CREATE person:tobie SET name = 'Tobie';").unwrap();
        assert_eq!(r.records.len(), 1);
        let rec = &r.records[0];
        assert_eq!(rec.id.table, "person");
        assert_eq!(rec.id.id, "tobie"); // typed, not the string "person:tobie"
        assert_eq!(
            rec.fields,
            vec![("name".to_string(), Value::Str("Tobie".to_string()))]
        );
        rec.clone()
    }; // 5. drop statements/connections/database handles

    let _ = created;

    // 6-7. Reopen and SELECT by record id; exactly one typed record.
    {
        let db = Database::open(path_str).unwrap();
        let conn = db.connect().unwrap();
        let r = conn.execute("SELECT * FROM person:tobie;").unwrap();
        assert_eq!(r.records.len(), 1);
        assert_eq!(r.records[0].id.table, "person");
        assert_eq!(r.records[0].id.id, "tobie");
        assert_eq!(
            r.records[0].fields,
            vec![("name".to_string(), Value::Str("Tobie".to_string()))]
        );

        // 8. Inspect internal state through the test-only native connection.
        let native = conn.native();
        // 8a. One version-0 metadata row.
        let meta = common::native_rows(native, "SELECT singleton, format_version, dialect_version FROM __fastdb_meta");
        assert_eq!(meta.len(), 1, "exactly one metadata row");
        assert_eq!(meta[0][0], "1");
        assert_eq!(meta[0][1], "0", "format_version == 0");
        assert_eq!(meta[0][2], "0", "dialect_version == 0");

        // 8b. One person catalog row.
        let tables = common::native_rows(
            native,
            "SELECT logical_name, physical_name FROM __fastdb_tables",
        );
        assert_eq!(tables.len(), 1, "exactly one table catalog row");
        let row: HashMap<String, String> = HashMap::from([
            ("logical".to_string(), tables[0][0].clone()),
            ("physical".to_string(), tables[0][1].clone()),
        ]);
        assert_eq!(row["logical"], "person");
        let physical = row["physical"].clone();

        // 8c. Physical name is opaque and contains no logical identifier text.
        assert!(
            physical.starts_with("__fastdb_t_"),
            "physical name has table prefix: {physical}"
        );
        assert_eq!(physical.len(), "__fastdb_t_".len() + 32);
        let suffix = &physical["__fastdb_t_".len()..];
        assert!(suffix.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        assert!(!physical.contains("person"), "no logical name leakage");
        assert!(!physical.contains("tobie"), "no record id leakage");

        // 8d. The physical row stores the canonical rid and a doc with no id.
        let prows = common::native_rows(
            native,
            &format!("SELECT rid, json(doc) FROM {physical}"),
        );
        assert_eq!(prows.len(), 1);
        let rid = &prows[0][0];
        let doc = &prows[0][1];
        assert!(rid.starts_with("s:5:"), "rid is canonical: {rid}");
        assert!(rid.ends_with("tobie"));
        assert!(doc.contains("\"name\""), "doc has name field: {doc}");
        assert!(doc.contains("Tobie"));
        assert!(
            !doc.contains("\"id\""),
            "doc must not store an id member: {doc}"
        );
    }

    // 9-10. DELETE then SELECT again -> empty.
    {
        let db = Database::open(path_str).unwrap();
        let conn = db.connect().unwrap();
        let r = conn.execute("DELETE person:tobie;").unwrap();
        assert!(r.records.is_empty(), "DELETE returns the empty default result");
        let r = conn.execute("SELECT * FROM person:tobie;").unwrap();
        assert!(r.records.is_empty());
    }

    // 11. Drop/reopen again: record absent, catalog/table definitions remain.
    let physical = {
        let db = Database::open(path_str).unwrap();
        let conn = db.connect().unwrap();
        let r = conn.execute("SELECT * FROM person:tobie;").unwrap();
        assert!(r.records.is_empty(), "record stays absent after reopen");
        let native = conn.native();
        // Catalog row remains.
        assert_eq!(
            common::native_rows(native, "SELECT logical_name FROM __fastdb_tables").len(),
            1
        );
        // Physical table definition remains present in the schema.
        let physical = common::physical_name_for(native, "person").unwrap();
        // The physical table is queryable (exists).
        let _ = common::native_rows(native, &format!("SELECT rid FROM {physical}"));
        physical
    };

    // 12. PRAGMA integrity_check == ok (from a fresh native connection).
    {
        let db = Database::open(path_str).unwrap();
        let conn = db.connect().unwrap();
        let chk = common::integrity_check(conn.native());
        assert_eq!(chk, "ok", "PRAGMA integrity_check must be ok, got: {chk}");
    }

    let _ = physical;
}
