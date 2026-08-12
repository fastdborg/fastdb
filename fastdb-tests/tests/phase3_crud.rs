#![forbid(unsafe_code)]
#![deny(warnings)]

use std::collections::BTreeMap;
use tempfile::tempdir;
use turso_fastdb::{
    Database, ErrorCategory, Params, QueryResponse, RecordId, StatementResult, Value,
};

fn one_result(response: QueryResponse) -> StatementResult {
    assert_eq!(response.statements.len(), 1);
    response.statements.into_iter().next().unwrap()
}

fn rows(response: QueryResponse) -> Vec<Value> {
    let StatementResult::Rows(rows) = one_result(response) else {
        panic!("expected rows")
    };
    rows
}

fn object(value: &Value) -> &BTreeMap<String, Value> {
    let Value::Object(value) = value else {
        panic!("expected object, got {value:?}")
    };
    value
}

#[test]
fn p3_expr_001_truthiness_short_circuit_arithmetic_equality_and_missing() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    let created = rows(
        conn.execute(
            "CREATE calc:a SET \
             or_value = 0 OR 7, and_value = 2 AND 3, not_value = NOT [], \
             quotient = 7 / 2, zero_division = 7 / 0, \
             short = false AND (9223372036854775807 + 1), \
             exact = 9007199254740993 = 9007199254740992.0, \
             exact_max = 9223372036854775807 = 9223372036854775808.0, \
             recursive = [1,{a:true}] = [1,{a:true}], \
             array_order = [1] < [1,0], object_order = {a:1} < {a:2}, \
             type_order = missing < null AND null < false AND false < 0 \
                 AND 0 < '' AND '' < [] AND [] < {} AND {} < person:a, \
             rid_order = person:1 < person:`1` \
                 AND person:`1` < person:u'550e8400-e29b-41d4-a716-446655440000', \
             absent = unknown",
        )
        .unwrap(),
    );
    let value = object(&created[0]);
    assert_eq!(value.get("or_value"), Some(&Value::Integer(7)));
    assert_eq!(value.get("and_value"), Some(&Value::Integer(3)));
    assert_eq!(value.get("not_value"), Some(&Value::Bool(true)));
    assert_eq!(value.get("quotient"), Some(&Value::Integer(3)));
    assert_eq!(value.get("zero_division"), Some(&Value::Null));
    assert_eq!(value.get("short"), Some(&Value::Bool(false)));
    assert_eq!(value.get("exact"), Some(&Value::Bool(false)));
    assert_eq!(value.get("exact_max"), Some(&Value::Bool(false)));
    assert_eq!(value.get("recursive"), Some(&Value::Bool(true)));
    assert_eq!(value.get("array_order"), Some(&Value::Bool(true)));
    assert_eq!(value.get("object_order"), Some(&Value::Bool(true)));
    assert_eq!(value.get("type_order"), Some(&Value::Bool(true)));
    assert_eq!(value.get("rid_order"), Some(&Value::Bool(true)));
    assert!(!value.contains_key("absent"));

    let error = conn
        .execute("CREATE calc:overflow SET n = 9223372036854775807 + 1")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Schema);
    assert!(rows(conn.execute("SELECT * FROM calc:overflow").unwrap()).is_empty());
    for source in [
        "CREATE calc:float_overflow SET n=1e308*1e308",
        "CREATE calc:type_error SET n='x'+1",
    ] {
        assert_eq!(
            conn.execute(source).unwrap_err().category(),
            ErrorCategory::Schema
        );
    }
}

#[test]
fn p3_param_001_bindings_are_values_case_sensitive_recursive_and_injection_safe() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    let mut params = Params::new();
    params.insert("Name".into(), Value::Str("x'; DELETE person:a; --".into()));
    params.insert("age".into(), Value::Integer(42));
    params.insert(
        "friend".into(),
        Value::RecordId(RecordId::new("person", "friend")),
    );
    conn.execute_with_params(
        "CREATE person:a CONTENT { name:$Name, age:$age, again:$age, friend:$friend }",
        &params,
    )
    .unwrap();
    let selected = rows(
        conn.execute_with_params(
            "SELECT * FROM person WHERE name=$Name AND age=$age",
            &params,
        )
        .unwrap(),
    );
    assert_eq!(selected.len(), 1);
    let selected = object(&selected[0]);
    assert_eq!(selected.get("name"), params.get("Name"));
    assert_eq!(selected.get("again"), Some(&Value::Integer(42)));
    assert_eq!(selected.get("friend"), params.get("friend"));

    assert_eq!(
        conn.execute_with_params("SELECT * FROM person WHERE age=$Age", &params)
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    params.insert("unused".into(), Value::Array(vec![Value::Integer(1)]));
    assert_eq!(
        rows(
            conn.execute_with_params("SELECT * FROM person:a", &params)
                .unwrap()
        )
        .len(),
        1
    );

    params.insert("年龄".into(), Value::Integer(3));
    params.insert("inc".into(), Value::Integer(2));
    let updated = rows(
        conn.execute_with_params(
            "UPDATE person SET age=age+$inc, unicode=$年龄 \
             WHERE name=$Name RETURN AFTER",
            &params,
        )
        .unwrap(),
    );
    assert_eq!(object(&updated[0]).get("age"), Some(&Value::Integer(44)));
    assert_eq!(object(&updated[0]).get("unicode"), Some(&Value::Integer(3)));
    params.insert("target".into(), Value::Integer(44));
    assert_eq!(
        rows(
            conn.execute_with_params("DELETE person WHERE age=$target RETURN BEFORE", &params,)
                .unwrap()
        )
        .len(),
        1
    );

    let mut invalid = Params::new();
    invalid.insert("$bad".into(), Value::Integer(1));
    assert_eq!(
        conn.execute_with_params("SELECT * FROM person", &invalid)
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    let mut nonfinite = Params::new();
    nonfinite.insert("bad".into(), Value::Float(f64::INFINITY));
    assert_eq!(
        conn.execute_with_params("SELECT * FROM person", &nonfinite)
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );

    let mut too_deep = Value::Null;
    for _ in 0..=turso_fastdb_parser::ParserLimits::default().max_nesting_depth {
        too_deep = Value::Array(vec![too_deep]);
    }
    let mut recursive = Params::new();
    recursive.insert("deep".into(), too_deep);
    assert_eq!(
        conn.execute_with_params("SELECT * FROM person", &recursive)
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );

    let mut bad_record = Params::new();
    bad_record.insert(
        "bad".into(),
        Value::RecordId(RecordId::new(
            "person",
            uuid::Uuid::parse_str("6ba7b810-9dad-11d1-80b4-00c04fd430c8").unwrap(),
        )),
    );
    assert_eq!(
        conn.execute_with_params("SELECT * FROM person", &bad_record)
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
}

#[test]
fn p3_crud_001_update_snapshot_assignments_delete_and_missing_records() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE person:a CONTENT { score:1, gone:'x', shape:0 }")
        .unwrap();
    conn.execute("CREATE person:b CONTENT { score:2, gone:'y', shape:0 }")
        .unwrap();
    let updated = rows(
        conn.execute(
            "UPDATE person SET prior=score, score=score+10, gone=missing, \
             shape=1, shape.child=score WHERE score >= 1 RETURN AFTER",
        )
        .unwrap(),
    );
    assert_eq!(updated.len(), 2);
    for value in &updated {
        let value = object(value);
        let Value::Integer(before) = value["prior"] else {
            panic!("prior must be integer")
        };
        assert_eq!(value.get("score"), Some(&Value::Integer(before + 10)));
        assert!(!value.contains_key("gone"));
        assert_eq!(
            value.get("shape"),
            Some(&Value::Object(
                [("child".into(), Value::Integer(before))]
                    .into_iter()
                    .collect()
            ))
        );
    }
    assert!(rows(
        conn.execute("UPDATE person:missing SET score=1 RETURN AFTER")
            .unwrap()
    )
    .is_empty());

    let deleted = rows(
        conn.execute("DELETE person WHERE score > 11 RETURN BEFORE")
            .unwrap(),
    );
    assert_eq!(deleted.len(), 1);
    assert!(rows(conn.execute("DELETE person:a").unwrap()).is_empty());
    assert!(rows(conn.execute("SELECT * FROM person").unwrap()).is_empty());
}

#[test]
fn p3_result_001_only_projection_and_return_shapes_are_exact() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    assert!(matches!(
        one_result(conn.execute("DEFINE TABLE person SCHEMALESS").unwrap()),
        StatementResult::None
    ));
    let created = one_result(
        conn.execute("CREATE ONLY person:a SET profile.age=42, name='A'")
            .unwrap(),
    );
    assert!(matches!(created, StatementResult::Value(Value::Object(_))));
    assert_eq!(
        one_result(
            conn.execute("CREATE person:b SET name='B' RETURN BEFORE")
                .unwrap()
        ),
        StatementResult::Rows(vec![Value::Null])
    );
    assert_eq!(
        one_result(
            conn.execute("CREATE person:c SET name='C' RETURN NONE")
                .unwrap()
        ),
        StatementResult::Rows(vec![])
    );
    assert_eq!(
        one_result(
            conn.execute("CREATE ONLY person:d SET name='D' RETURN NONE")
                .unwrap()
        ),
        StatementResult::Value(Value::Null)
    );
    assert_eq!(
        one_result(
            conn.execute("UPDATE person:c SET name='CC' RETURN NONE")
                .unwrap()
        ),
        StatementResult::Rows(vec![])
    );
    assert_eq!(
        one_result(conn.execute("DELETE person:b").unwrap()),
        StatementResult::Rows(vec![])
    );

    let projected = one_result(
        conn.execute("SELECT profile.age, name AS id FROM ONLY person:a")
            .unwrap(),
    );
    let StatementResult::Value(projected) = projected else {
        panic!("expected scalar ONLY result")
    };
    let projected = object(&projected);
    assert_eq!(projected.get("id"), Some(&Value::Str("A".into())));
    assert_eq!(
        projected.get("profile"),
        Some(&Value::Object(
            [("age".into(), Value::Integer(42))].into_iter().collect()
        ))
    );
    assert_eq!(
        one_result(conn.execute("SELECT * FROM ONLY person:missing").unwrap()),
        StatementResult::Value(Value::Null)
    );
    assert_eq!(
        conn.execute("SELECT * FROM ONLY person")
            .unwrap_err()
            .category(),
        ErrorCategory::UnsupportedSyntax
    );
}

#[test]
fn p3_crud_002_order_start_limit_type_order_and_virtual_id() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    for source in [
        "CREATE item:n SET key=null",
        "CREATE item:b SET key=false",
        "CREATE item:i SET key=2",
        "CREATE item:s SET key='a'",
        "CREATE item:a SET key=[]",
        "CREATE item:o SET key={}",
    ] {
        conn.execute(source).unwrap();
    }
    let ordered = rows(
        conn.execute("SELECT id FROM item ORDER BY key ASC, id ASC LIMIT 3 START 1")
            .unwrap(),
    );
    let ids = ordered
        .iter()
        .map(|value| object(value).get("id").cloned().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec![
            Value::RecordId(RecordId::new("item", "b")),
            Value::RecordId(RecordId::new("item", "i")),
            Value::RecordId(RecordId::new("item", "s")),
        ]
    );
    assert_eq!(
        rows(conn.execute("SELECT * FROM item WHERE id=item:i").unwrap()).len(),
        1
    );
}

#[test]
fn p3_crud_003_schema_unique_maintenance_and_file_reopen() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("phase3.fastdb");
    let path = path.to_str().unwrap();
    {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        conn.execute("DEFINE TABLE user SCHEMALESS").unwrap();
        conn.execute("DEFINE FIELD age ON user TYPE int").unwrap();
        conn.execute("DEFINE INDEX unique_email ON user FIELDS email UNIQUE")
            .unwrap();
        conn.execute("CREATE user:a CONTENT { age:1, email:'a@test' }")
            .unwrap();
        conn.execute("CREATE user:b CONTENT { age:2, email:'b@test' }")
            .unwrap();
        assert_eq!(
            conn.execute("UPDATE user SET email='same@test'")
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        assert_eq!(
            rows(
                conn.execute("SELECT * FROM user WHERE email='a@test'")
                    .unwrap()
            )
            .len(),
            1
        );
    }
    {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        assert_eq!(
            rows(
                conn.execute("SELECT * FROM user WHERE age>=1 ORDER BY age")
                    .unwrap()
            )
            .len(),
            2
        );
        assert_eq!(
            conn.execute("UPDATE user:a SET age='wrong'")
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
    }
}
