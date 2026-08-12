//! P0-CAT-003 — Unknown future format and dialect versions are refused
//! before any mutation (and before interpreting the catalog on reads).
//!
//! Each test bootstraps a real database with one record, rewrites the stored
//! version to a future value through the native connection, reopens, and
//! asserts every FastDB operation returns a `Format` error and leaves the
//! schema/data logically unchanged.

#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::{tempdir, TempDir};
use turso_fastdb::{Database, ErrorCategory};

/// Open, create one record, close. Returns the retained temp dir and path.
fn fresh_with_record() -> (TempDir, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("v.fastdb");
    let s = path.to_str().unwrap().to_string();
    {
        let db = Database::open(&s).unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE person:tobie SET name = 'Tobie';")
            .unwrap();
    }
    (dir, s)
}

/// Rewrite a `__fastdb_meta` numeric column via the native connection.
/// `col` is a static column name; `val` is a static integer literal.
fn set_meta_column(path: &str, col: &'static str, val: &'static str) {
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    // Internal catalog column name and integer literal are static.
    common::native_exec(
        conn.native(),
        &format!("UPDATE __fastdb_meta SET {col} = {val}"),
    );
}

/// Every FastDB operation on a future-version database must refuse with
/// `Format` before mutating or interpreting the catalog.
fn assert_refuses_all_ops(path: &str) {
    for sql in [
        "CREATE person:jaime SET name = 'Jaime';",
        "SELECT * FROM person:tobie;",
        "SELECT * FROM person WHERE name = 'Tobie';",
        "DELETE person:tobie;",
    ] {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        let err = conn.execute(sql).unwrap_err();
        assert_eq!(
            err.category(),
            ErrorCategory::Format,
            "op {sql:?} should refuse on future-version db, got: {err}"
        );
    }
}

/// After refusal, the original record and catalog are logically unchanged.
fn assert_unchanged(path: &str) {
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    let native = conn.native();
    assert_eq!(
        common::native_rows(native, "SELECT logical_name FROM __fastdb_tables").len(),
        1,
        "catalog row preserved"
    );
    let physical = common::physical_name_for(native, "person").unwrap();
    assert_eq!(
        common::native_rows(native, &format!("SELECT rid FROM {physical}")).len(),
        1,
        "original record preserved"
    );
    assert_eq!(common::integrity_check(native), "ok");
}

#[test]
fn future_format_version_refused_before_mutation() {
    let (_dir, path) = fresh_with_record();
    set_meta_column(&path, "format_version", "1");
    assert_refuses_all_ops(&path);
    assert_unchanged(&path);
}

#[test]
fn future_dialect_version_refused_before_mutation() {
    let (_dir, path) = fresh_with_record();
    set_meta_column(&path, "dialect_version", "1");
    assert_refuses_all_ops(&path);
    assert_unchanged(&path);
}
