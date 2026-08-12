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
    assert_eq!(created.records.len(), 1);
    assert_eq!(created.records[0].id.id, "tracy");
    assert_eq!(
        created.records[0].fields,
        vec![("name".to_string(), Value::Str("Tracy".to_string()))]
    );
    assert_eq!(
        connection
            .execute("SELECT * FROM person:tracy")
            .unwrap()
            .records
            .len(),
        1
    );
    assert_eq!(
        connection
            .execute("SELECT * FROM person WHERE name = 'Tracy'")
            .unwrap()
            .records
            .len(),
        1
    );
    connection.execute("DELETE person:tracy").unwrap();
    assert!(connection
        .execute("SELECT * FROM person:tracy")
        .unwrap()
        .records
        .is_empty());
}

#[test]
fn p1_bridge_002_every_still_deferred_shape_is_frontend_unsupported() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    let inputs = [
        "CREATE ONLY person:tracy SET name = 'Tracy'",
        "CREATE person:tracy SET name = 'Tracy', active = true",
        "CREATE person:tracy SET age = 1 + 2",
        "CREATE person:tracy SET age = other",
        "CREATE person:tracy CONTENT $document",
        "CREATE person:tracy SET name = 'Tracy' RETURN AFTER",
        "SELECT name FROM person",
        "SELECT * FROM person WHERE age >= 18",
        "SELECT * FROM person WHERE age = 18 OR age = 21",
        "SELECT * FROM person ORDER BY name LIMIT 1 START 0",
        "UPDATE person:tracy SET name = 'Trace'",
        "DELETE person WHERE active = false",
        "DELETE person:tracy RETURN BEFORE",
        "BEGIN",
        "COMMIT",
        "CANCEL",
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

    for input in ["CREATE", "BEGIN; COMMIT", "SELECT * person"] {
        let error = connection.execute(input).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Parse, "{input}: {error}");
    }

    for input in ["INSERT INTO person {}", "SELECT * FROM person FETCH friend"] {
        let error = connection.execute(input).unwrap_err();
        assert_eq!(
            error.category(),
            ErrorCategory::UnsupportedSyntax,
            "{input}: {error}"
        );
    }
}
