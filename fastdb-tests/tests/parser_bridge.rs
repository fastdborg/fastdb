//! Parser-to-frontend capability boundary.

#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::{Database, ErrorCategory, FastDbError, Value};

#[test]
fn p1_bridge_001_phase0_shapes_still_execute() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();

    let created = connection
        .execute("CREATE person:tracy SET name = 'Tracy'")
        .unwrap();
    let records = created.legacy_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].id.id, "tracy");
    assert_eq!(
        records[0].fields,
        vec![("name".to_string(), Value::Str("Tracy".to_string()))]
    );
    assert_eq!(
        connection
            .execute("SELECT * FROM person:tracy")
            .unwrap()
            .legacy_records()
            .len(),
        1
    );
    assert_eq!(
        connection
            .execute("SELECT * FROM person WHERE name = 'Tracy'")
            .unwrap()
            .legacy_records()
            .len(),
        1
    );
    connection.execute("DELETE person:tracy").unwrap();
    assert!(connection
        .execute("SELECT * FROM person:tracy")
        .unwrap()
        .legacy_records()
        .is_empty());
}

#[test]
fn p1_bridge_002_excluded_shapes_remain_frontend_unsupported() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    let inputs = [
        "SELECT * FROM ONLY person",
        "SELECT * FROM person WHERE lower(name) = 'tracy'",
        "RELATE person:a->likes->person:b OR UPDATE",
        "LIVE SELECT * FROM person",
    ];

    for input in inputs {
        let error = connection.execute(input).unwrap_err();
        assert_eq!(
            error.category(),
            ErrorCategory::UnsupportedSyntax,
            "{input}: {error}"
        );
        let FastDbError::UnsupportedSyntax(parse_error) = error else {
            panic!("expected spanned unsupported error for {input}")
        };
        assert!(parse_error.span.is_within(input.len()), "{input}");
    }
}

#[test]
fn p1_bridge_003_parse_and_capability_errors_remain_distinct() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();

    for input in ["CREATE", "BEGIN COMMIT", "SELECT * person"] {
        let error = connection.execute(input).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Parse, "{input}: {error}");
    }

    for input in [
        "LET $value = 1",
        "RELATE person:a->likes->person:b OR UPDATE",
    ] {
        let error = connection.execute(input).unwrap_err();
        assert_eq!(
            error.category(),
            ErrorCategory::UnsupportedSyntax,
            "{input}: {error}"
        );
    }
}
