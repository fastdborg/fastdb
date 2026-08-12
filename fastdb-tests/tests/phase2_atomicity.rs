#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use turso_fastdb::{Database, Failpoint};

#[test]
fn p2_atomic_009_field_boundaries_preserve_persisted_and_cached_catalog() {
    for failpoint in [
        Failpoint::AfterFieldValidation,
        Failpoint::AfterFieldCatalogRow,
    ] {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE person:tracy SET age=42").unwrap();
        let before = conn.catalog_state().unwrap();
        conn.arm_failpoint(failpoint);
        conn.execute("DEFINE FIELD age ON person TYPE int")
            .unwrap_err();
        assert_eq!(conn.catalog_state().unwrap(), before);
        assert!(
            common::native_rows(conn.native(), "SELECT path_key FROM __fastdb_fields").is_empty()
        );
        assert_eq!(common::integrity_check(conn.native()), "ok");
    }
}

#[test]
fn p2_atomic_010_index_boundaries_leave_no_physical_or_catalog_index() {
    for failpoint in [
        Failpoint::AfterIndexValidation,
        Failpoint::AfterIndexPhysicalDdl,
        Failpoint::AfterIndexCatalogRow,
    ] {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE person:tracy SET name='Tracy'")
            .unwrap();
        let before = conn.catalog_state().unwrap();
        conn.arm_failpoint(failpoint);
        conn.execute("DEFINE INDEX by_name ON person FIELDS name")
            .unwrap_err();
        assert_eq!(conn.catalog_state().unwrap(), before);
        assert!(
            common::native_rows(conn.native(), "SELECT logical_name FROM __fastdb_indexes")
                .is_empty()
        );
        assert!(common::native_rows(
            conn.native(),
            "SELECT name FROM sqlite_schema WHERE type='index' AND name LIKE '__fastdb_i_%'"
        )
        .is_empty());
        assert_eq!(common::integrity_check(conn.native()), "ok");
    }
}
