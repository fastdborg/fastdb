//! Unknown future format/dialect versions are refused during open, before any
//! mutation or interpretation of the remaining catalogs.

#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory};

fn assert_future_column_refused(column: &'static str) {
    let directory = tempdir().unwrap();
    let file = directory.path().join("future.fastdb");
    let path = file.to_str().unwrap();
    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE person:tracy SET name='Tracy'")
        .unwrap();
    common::native_exec(
        conn.native(),
        &format!("UPDATE __fastdb_meta SET {column}=7"),
    );

    let error = Database::open(path).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Format);

    // The already-open diagnostic connection proves refusal did not mutate
    // the logical catalog or record.
    assert_eq!(
        common::native_rows(conn.native(), "SELECT logical_name FROM __fastdb_tables").len(),
        1
    );
    let physical = common::physical_name_for(conn.native(), "person").unwrap();
    assert_eq!(
        common::native_rows(conn.native(), &format!("SELECT rid FROM {physical}")).len(),
        1
    );
    assert_eq!(common::integrity_check(conn.native()), "ok");
}

#[test]
fn future_format_version_refused_before_mutation() {
    assert_future_column_refused("format_version");
}

#[test]
fn future_dialect_version_refused_before_mutation() {
    assert_future_column_refused("dialect_version");
}
