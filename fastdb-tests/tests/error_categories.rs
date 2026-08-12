//! Error-boundary category coverage: the public `execute`/`open` paths must
//! distinguish Parse, UnsupportedSyntax, Constraint, Format, Transaction,
//! Engine, and Io categories (plan-phase0.md §6.3).

#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use std::fs;
use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Failpoint};

#[test]
fn category_parse_malformed() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    let err = conn.execute("CREATE").unwrap_err(); // incomplete
    assert_eq!(err.category(), ErrorCategory::Parse, "{err}");
}

#[test]
fn category_unsupported_syntax() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    // Multiple SET assignments are recognized-but-unsupported.
    let err = conn.execute("CREATE p:x SET a = '1', b = '2'").unwrap_err();
    assert_eq!(err.category(), ErrorCategory::UnsupportedSyntax, "{err}");
}

#[test]
fn category_constraint_duplicate_id() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE p:x SET n = '1'").unwrap();
    let err = conn.execute("CREATE p:x SET n = '2'").unwrap_err();
    assert_eq!(err.category(), ErrorCategory::Constraint, "{err}");
}

#[test]
fn category_format_future_version() {
    let dir = tempdir().unwrap();
    let s = dir.path().join("e.fastdb");
    let s = s.to_str().unwrap();
    {
        let db = Database::open(s).unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE p:x SET n = '1'").unwrap();
    }
    {
        let db = Database::open(s).unwrap();
        let conn = db.connect().unwrap();
        common::native_exec(conn.native(), "UPDATE __fastdb_meta SET format_version = 7");
    }
    let db = Database::open(s).unwrap();
    let conn = db.connect().unwrap();
    let err = conn.execute("CREATE p:y SET n = '2'").unwrap_err();
    assert_eq!(err.category(), ErrorCategory::Format, "{err}");
}

#[test]
fn category_transaction_injected() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.arm_failpoint(Failpoint::CommitFailure);
    let err = conn.execute("CREATE p:x SET n = '1'").unwrap_err();
    assert_eq!(err.category(), ErrorCategory::Transaction, "{err}");
}

#[test]
fn category_engine_not_a_database() {
    let dir = tempdir().unwrap();
    let p = dir.path().join("bad.fastdb");
    // A page-sized file with a wrong magic header: a full page is read, then
    // the header check fails with NotADB (an Engine-category error, not I/O).
    fs::write(&p, vec![0xAAu8; 512]).unwrap();
    let err = Database::open(p.to_str().unwrap()).unwrap_err();
    assert_eq!(err.category(), ErrorCategory::Engine, "{err}");
}

#[test]
fn category_io_missing_directory() {
    // Opening a file in a nonexistent directory surfaces an I/O error.
    let err = Database::open("/tmp/fastdb_no_such_dir_xyz_987/sub.fastdb").unwrap_err();
    assert_eq!(err.category(), ErrorCategory::Io, "expected Io, got: {err}");
}
