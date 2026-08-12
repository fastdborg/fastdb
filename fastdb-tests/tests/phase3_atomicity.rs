#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::{Database, ErrorCategory, Failpoint, StatementResult, Value};

fn values(connection: &turso_fastdb::Connection) -> Vec<Value> {
    let response = connection
        .execute("SELECT n FROM item ORDER BY id")
        .unwrap();
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    rows.clone()
}

#[test]
fn p3_atomic_001_update_failures_before_and_after_mutation_roll_back() {
    for failpoint in [
        Failpoint::BeforeUpdateMutations,
        Failpoint::AfterUpdateMutation,
    ] {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE item:a SET n=1; CREATE item:b SET n=2")
            .unwrap();
        let before = values(&conn);
        conn.arm_failpoint(failpoint);
        assert_eq!(
            conn.execute("UPDATE item SET n=n+10")
                .unwrap_err()
                .category(),
            ErrorCategory::Transaction
        );
        conn.disarm_all_failpoints();
        assert_eq!(values(&conn), before, "{failpoint:?}");
    }
}

#[test]
fn p3_atomic_002_delete_failures_before_and_after_mutation_roll_back() {
    for failpoint in [
        Failpoint::BeforeDeleteMutations,
        Failpoint::AfterDeleteMutation,
    ] {
        let db = Database::open_memory().unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE item:a SET n=1; CREATE item:b SET n=2")
            .unwrap();
        let before = values(&conn);
        conn.arm_failpoint(failpoint);
        assert_eq!(
            conn.execute("DELETE item WHERE n>=1")
                .unwrap_err()
                .category(),
            ErrorCategory::Transaction
        );
        conn.disarm_all_failpoints();
        assert_eq!(values(&conn), before, "{failpoint:?}");
    }
}

#[test]
fn p3_atomic_003_explicit_failure_rolls_back_catalog_schema_and_data() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute(
        "BEGIN; DEFINE TABLE person SCHEMAFULL; \
         DEFINE FIELD age ON person TYPE int; CREATE person:a SET age=1",
    )
    .unwrap();
    assert_eq!(
        conn.execute("UPDATE person:a SET age='bad'")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    conn.execute("CANCEL").unwrap();
    assert_eq!(
        conn.catalog_state().unwrap(),
        turso_fastdb::catalog::CatalogState::Empty
    );
    conn.execute("CREATE person:b SET anything='schemaless'")
        .unwrap();
}
