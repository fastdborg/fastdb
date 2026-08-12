#![forbid(unsafe_code)]
#![deny(warnings)]

mod common;

use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Value};

fn assert_uses_index(plans: &[String], physical_name: &str) {
    assert!(
        plans.iter().any(|plan| plan.contains(physical_name)),
        "expected {physical_name} in {plans:?}"
    );
    assert!(
        !plans
            .iter()
            .any(|plan| plan.contains("SCAN") && !plan.contains(physical_name)),
        "unexpected full scan: {plans:?}"
    );
}

#[test]
fn p2_idx_001_field_composite_and_named_plan_survive_reopen() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("indexes.fastdb");
    let path = path.to_str().unwrap();
    let (name_index, composite_index) = {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE person:tracy CONTENT { name:'Tracy', age:42, nickname:'T' }")
            .unwrap();
        conn.execute("CREATE person:jaime CONTENT { name:'Jaime', age:42, nickname:'J' }")
            .unwrap();
        conn.execute("DEFINE INDEX by_nickname ON person FIELDS nickname")
            .unwrap();
        conn.execute("DEFINE INDEX by_name_age ON person FIELDS name, age")
            .unwrap();
        let state = conn.catalog_state().unwrap();
        let table = &state.snapshot().unwrap().tables["person"];
        let name = table.indexes["by_nickname"].physical_name.clone();
        let composite = table.indexes["by_name_age"].physical_name.clone();
        assert_uses_index(
            &conn
                .explain_filters("person", &[("nickname", Value::Str("T".into()))])
                .unwrap(),
            &name,
        );
        assert_uses_index(
            &conn
                .explain_filters(
                    "person",
                    &[
                        ("name", Value::Str("Tracy".into())),
                        ("age", Value::Integer(42)),
                    ],
                )
                .unwrap(),
            &composite,
        );
        (name, composite)
    };

    let db = Database::open(path).unwrap();
    let conn = db.connect().unwrap();
    assert_eq!(
        conn.execute("SELECT * FROM person WHERE name='Tracy' AND age=42")
            .unwrap()
            .records
            .len(),
        1
    );
    assert_uses_index(
        &conn
            .explain_filters("person", &[("nickname", Value::Str("J".into()))])
            .unwrap(),
        &name_index,
    );
    assert_uses_index(
        &conn
            .explain_filters(
                "person",
                &[
                    ("name", Value::Str("Jaime".into())),
                    ("age", Value::Integer(42)),
                ],
            )
            .unwrap(),
        &composite_index,
    );
}

#[test]
fn p2_idx_002_unique_missing_null_existing_duplicates_and_write_maintenance() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE user:a CONTENT { email:null }")
        .unwrap();
    conn.execute("CREATE user:b CONTENT { other:1 }").unwrap();
    conn.execute("DEFINE INDEX unique_email ON user FIELDS email UNIQUE")
        .unwrap();
    conn.execute("CREATE user:c CONTENT { email:null }")
        .unwrap();
    conn.execute("CREATE user:d CONTENT { other:2 }").unwrap();
    conn.execute("CREATE user:e CONTENT { email:'same@example.test' }")
        .unwrap();
    let duplicate = conn
        .execute("CREATE user:f CONTENT { email:'same@example.test' }")
        .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Constraint);
    assert!(!duplicate.to_string().contains("__fastdb_"));
    assert_eq!(
        conn.execute("SELECT * FROM user WHERE email='same@example.test'")
            .unwrap()
            .records
            .len(),
        1
    );

    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE duplicate:a SET score = 1").unwrap();
    conn.execute("CREATE duplicate:b SET score = 1").unwrap();
    assert_eq!(
        conn.execute("DEFINE INDEX unique_score ON duplicate FIELDS score UNIQUE")
            .unwrap_err()
            .category(),
        ErrorCategory::Constraint
    );
    assert!(
        conn.catalog_state().unwrap().snapshot().unwrap().tables["duplicate"]
            .indexes
            .is_empty()
    );
    assert!(common::native_rows(
        conn.native(),
        "SELECT name FROM sqlite_schema WHERE type='index' AND name LIKE '__fastdb_i_%'"
    )
    .is_empty());
}

#[test]
fn p2_idx_003_non_scalar_existing_and_future_values_are_rejected() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE item:a CONTENT { key:{ nested:1 } }")
        .unwrap();
    assert_eq!(
        conn.execute("DEFINE INDEX by_key ON item FIELDS key")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );

    conn.execute("DEFINE INDEX by_scalar ON item FIELDS scalar")
        .unwrap();
    assert_eq!(
        conn.execute("CREATE item:b CONTENT { scalar:[1,2] }")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    assert_eq!(
        conn.execute("SELECT * FROM item:b").unwrap().records.len(),
        0
    );
}

#[test]
fn p2_idx_004_duplicate_index_definition_is_logical_constraint() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("DEFINE TABLE person SCHEMALESS").unwrap();
    conn.execute("DEFINE INDEX by_name ON person FIELDS name")
        .unwrap();
    let error = conn
        .execute("DEFINE INDEX by_name ON person FIELDS age")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Constraint);
    assert!(!error.to_string().contains("__fastdb_"));
}
