//! P0.8 — Atomicity and failure injection.
//!
//! For each deterministic failpoint: start from a new empty file, force the
//! `CREATE` to fail, drop/reopen, and prove the whole transaction (catalog
//! bootstrap, table registration, physical DDL, and the record) rolled
//! back together — leaving an empty schema and `integrity_check == ok` —
//! and that a subsequent `CREATE` then succeeds. Also: a duplicate explicit
//! id is a constraint error and preserves the original record.

#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::{tempdir, TempDir};
use turso_fastdb::{Database, ErrorCategory, Failpoint, Value};

fn fresh_db() -> (TempDir, String) {
    let dir = tempdir().unwrap();
    let path = dir.path().join("a.fastdb");
    let s = path.to_str().unwrap().to_string();
    (dir, s)
}

/// After a rolled-back first CREATE the schema must be empty (no catalog
/// tables, no physical table, no record) and the database must be consistent.
fn assert_empty_after_rollback(path: &str) {
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    let native = conn.native();
    let tables =
        common::native_rows(native, "SELECT name FROM sqlite_schema WHERE type='table'");
    assert!(
        tables.is_empty(),
        "rolled-back transaction left tables behind: {tables:?}"
    );
    assert_eq!(
        common::integrity_check(native),
        "ok",
        "integrity_check after rollback"
    );
    let r = conn.execute("SELECT * FROM person:tobie;").unwrap();
    assert!(r.records.is_empty(), "no record after rollback");
}

fn then_create_succeeds(path: &str) {
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    let r = conn.execute("CREATE person:tobie SET name = 'Tobie';").unwrap();
    assert_eq!(r.records.len(), 1, "subsequent non-failing CREATE succeeds");
}

/// Arm `fp`, run a CREATE that must fail, reopen, prove clean rollback, then
/// prove a fresh CREATE succeeds.
fn injected_fail_round(fp: Failpoint) {
    let (_dir, path) = fresh_db();
    {
        let db = Database::open(&path).unwrap();
        let conn = db.connect().unwrap();
        conn.arm_failpoint(fp);
        let err = conn
            .execute("CREATE person:tobie SET name = 'Tobie';")
            .unwrap_err();
        assert_eq!(
            err.category(),
            ErrorCategory::Transaction,
            "failpoint {fp:?}: expected Transaction error, got: {err}"
        );
    }
    assert_empty_after_rollback(&path);
    then_create_succeeds(&path);
}

#[test]
fn atomic_001_fail_after_bootstrap() {
    injected_fail_round(Failpoint::AfterBootstrap);
}
#[test]
fn atomic_002_fail_after_catalog_row() {
    injected_fail_round(Failpoint::AfterCatalogRow);
}
#[test]
fn atomic_003_fail_after_physical_ddl() {
    injected_fail_round(Failpoint::AfterPhysicalDdl);
}
#[test]
fn atomic_004_fail_after_record_prepare() {
    injected_fail_round(Failpoint::AfterRecordPrepare);
}
#[test]
fn atomic_005_fail_after_record_insert() {
    injected_fail_round(Failpoint::AfterRecordInsert);
}

#[test]
fn duplicate_explicit_id_is_constraint_and_preserves_original() {
    let (_dir, path) = fresh_db();
    {
        let db = Database::open(&path).unwrap();
        let conn = db.connect().unwrap();
        let r = conn.execute("CREATE person:tobie SET name = 'Tobie';").unwrap();
        assert_eq!(
            r.records[0].fields,
            vec![("name".to_string(), Value::Str("Tobie".to_string()))]
        );
        let err = conn
            .execute("CREATE person:tobie SET name = 'Other';")
            .unwrap_err();
        assert_eq!(
            err.category(),
            ErrorCategory::Constraint,
            "duplicate id: {err}"
        );
    }
    // Reopen: exactly one unchanged record; the failed create did not persist.
    let db = Database::open(&path).unwrap();
    let conn = db.connect().unwrap();
    let r = conn
        .execute("SELECT * FROM person WHERE name = 'Tobie';")
        .unwrap();
    assert_eq!(r.records.len(), 1);
    let r = conn
        .execute("SELECT * FROM person WHERE name = 'Other';")
        .unwrap();
    assert!(r.records.is_empty(), "failed create must not persist");
    assert_eq!(common::integrity_check(conn.native()), "ok");
}
