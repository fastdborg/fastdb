#![forbid(unsafe_code)]
#![deny(warnings)]

use tempfile::tempdir;
use turso_fastdb::{
    Connection, Database, ErrorCategory, FastDbError, Params, RecordId, StatementResult, Value,
};

fn rows(connection: &Connection, source: &str) -> Vec<Value> {
    let response = connection.execute(source).unwrap();
    let StatementResult::Rows(rows) = response.statements.into_iter().next().unwrap() else {
        panic!("expected rows")
    };
    rows
}

fn exercise_promoted_surface(connection: &Connection) {
    let schema = connection
        .execute(
            "DEFINE TABLE item SCHEMALESS; \
             DEFINE FIELD score ON item TYPE number; \
             DEFINE INDEX by_score ON item FIELDS score",
        )
        .unwrap();
    assert!(schema
        .statements
        .iter()
        .all(|result| matches!(result, StatementResult::None)));

    let mut params = Params::new();
    params.insert("Name".into(), Value::Str("alpha".into()));
    params.insert("base".into(), Value::Integer(2));
    params.insert(
        "owner".into(),
        Value::RecordId(RecordId::new("person", "owner")),
    );
    let created = connection
        .execute_with_params(
            "CREATE ONLY item:a CONTENT { \
                 name:$Name, score:$base, active:true, tags:['x'], \
                 meta:{rank:1}, owner:$owner \
             }",
            &params,
        )
        .unwrap();
    assert!(matches!(
        created.statements.as_slice(),
        [StatementResult::Value(Value::Object(_))]
    ));
    connection
        .execute("CREATE item:b SET name='beta', score=4, active=false")
        .unwrap();

    let updated = connection
        .execute(
            "UPDATE item SET prior=score, score=score+1 \
             WHERE NOT active OR score < 3 RETURN AFTER",
        )
        .unwrap();
    assert!(matches!(
        updated.statements.as_slice(),
        [StatementResult::Rows(values)] if values.len() == 2
    ));

    let selected = rows(
        connection,
        "SELECT name, score AS measured FROM item \
         WHERE score >= 3 ORDER BY score DESC, id ASC LIMIT 1 START 0",
    );
    assert_eq!(selected.len(), 1);
    let Value::Object(projected) = &selected[0] else {
        panic!("projection must be an object")
    };
    assert_eq!(projected.get("name"), Some(&Value::Str("beta".into())));
    assert_eq!(projected.get("measured"), Some(&Value::Integer(5)));

    assert!(matches!(
        connection
            .execute("SELECT name FROM ONLY item:missing")
            .unwrap()
            .statements
            .as_slice(),
        [StatementResult::Value(Value::Null)]
    ));
    assert_eq!(
        rows(connection, "DELETE item WHERE id=item:b RETURN BEFORE").len(),
        1
    );

    connection
        .execute("BEGIN; CREATE audit:cancelled SET ok=true; CANCEL")
        .unwrap();
    assert!(rows(connection, "SELECT * FROM audit:cancelled").is_empty());
    connection
        .execute("BEGIN; CREATE audit:kept SET ok=true; COMMIT")
        .unwrap();
    assert_eq!(rows(connection, "SELECT * FROM audit:kept").len(), 1);
}

#[test]
fn p3_bridge_001_promoted_surface_executes_in_memory() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    exercise_promoted_surface(&connection);
}

#[test]
fn p3_bridge_002_promoted_surface_executes_on_disk_and_reopens() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("bridge.fastdb");
    {
        let database = Database::open(path.to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        exercise_promoted_surface(&connection);
    }
    {
        let database = Database::open(path.to_str().unwrap()).unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(rows(&connection, "SELECT * FROM item:a").len(), 1);
        assert_eq!(rows(&connection, "SELECT * FROM item:b").len(), 0);
        assert_eq!(rows(&connection, "SELECT * FROM audit:kept").len(), 1);
    }
}

#[test]
fn p3_bridge_003_excluded_syntax_stays_spanned_and_unsupported() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "SELECT * FROM ONLY person",
        "SELECT * FROM person WHERE lower(name)='a'",
        "RELATE person:a->likes->person:b OR UPDATE",
        "LIVE SELECT * FROM person",
        "CREATE person:a VERSION d'2024-01-01T00:00:00Z'",
    ] {
        let error = connection.execute(source).unwrap_err();
        assert_eq!(
            error.category(),
            ErrorCategory::UnsupportedSyntax,
            "{source}"
        );
        let FastDbError::UnsupportedSyntax(error) = error else {
            panic!("expected unsupported syntax")
        };
        assert!(error.span.is_within(source.len()), "{source}");
    }
}
