#![forbid(unsafe_code)]
#![deny(warnings)]

use std::collections::BTreeMap;
use turso_fastdb::{Database, ErrorCategory, StatementResult, Value};

fn row(result: &StatementResult) -> &BTreeMap<String, Value> {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    let Value::Object(row) = &rows[0] else {
        panic!("expected object row")
    };
    row
}

#[test]
fn p13_fn_029_value_diff_matches_characterized_structural_operations() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE diff:one SET object_diff = value::diff({a:1,b:2},{a:1,b:3,c:4}), \
             array_diff = value::diff([1,2],[1,3,4]), scalar_diff = value::diff(1,2)",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM diff:one").unwrap();
    let row = row(&selected.statements[0]);

    assert_eq!(
        row.get("object_diff"),
        Some(&Value::Array(vec![
            Value::Object(BTreeMap::from([
                ("op".into(), Value::Str("replace".into())),
                ("path".into(), Value::Str("/b".into())),
                ("value".into(), Value::Integer(3)),
            ])),
            Value::Object(BTreeMap::from([
                ("op".into(), Value::Str("add".into())),
                ("path".into(), Value::Str("/c".into())),
                ("value".into(), Value::Integer(4)),
            ])),
        ]))
    );
    assert_eq!(
        row.get("array_diff"),
        Some(&Value::Array(vec![
            Value::Object(BTreeMap::from([
                ("op".into(), Value::Str("replace".into())),
                ("path".into(), Value::Str("/1".into())),
                ("value".into(), Value::Integer(3)),
            ])),
            Value::Object(BTreeMap::from([
                ("op".into(), Value::Str("add".into())),
                ("path".into(), Value::Str("/2".into())),
                ("value".into(), Value::Integer(4)),
            ])),
        ]))
    );
    assert_eq!(
        row.get("scalar_diff"),
        Some(&Value::Array(vec![Value::Object(BTreeMap::from([
            ("op".into(), Value::Str("replace".into())),
            ("path".into(), Value::Str(String::new())),
            ("value".into(), Value::Integer(2)),
        ]))]))
    );
}

#[test]
fn p13_fn_030_value_patch_round_trips_structures_and_strings() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE patched:one SET \
             object_patch = value::patch({a:1,b:2}, value::diff({a:1,b:2},{a:1,b:3,c:4})), \
             array_patch = value::patch([1,2], value::diff([1,2],[1,3,4])), \
             scalar_patch = value::patch(1, value::diff(1,2)), \
             string_patch = value::patch('tobie', value::diff('tobie','tobias')), \
             manual = value::patch({a:1,b:[2]}, [ \
               {'op':'test','path':'/a','value':1}, \
               {'op':'copy','from':'/a','path':'/c'}, \
               {'op':'move','from':'/b/0','path':'/d'} \
             ])",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM patched:one").unwrap();
    let row = row(&selected.statements[0]);
    assert_eq!(
        row.get("object_patch"),
        Some(&Value::Object(BTreeMap::from([
            ("a".into(), Value::Integer(1)),
            ("b".into(), Value::Integer(3)),
            ("c".into(), Value::Integer(4)),
        ])))
    );
    assert_eq!(
        row.get("array_patch"),
        Some(&Value::Array(vec![
            Value::Integer(1),
            Value::Integer(3),
            Value::Integer(4),
        ]))
    );
    assert_eq!(row.get("scalar_patch"), Some(&Value::Integer(2)));
    assert_eq!(row.get("string_patch"), Some(&Value::Str("tobias".into())));
    assert_eq!(
        row.get("manual"),
        Some(&Value::Object(BTreeMap::from([
            ("a".into(), Value::Integer(1)),
            ("b".into(), Value::Array(Vec::new())),
            ("c".into(), Value::Integer(1)),
            ("d".into(), Value::Integer(2)),
        ])))
    );

    let error = connection
        .execute(
            "CREATE bad:one SET patched_value = value::patch({a:1}, [ \
             {'op':'replace','path':'/a','value':2}, \
             {'op':'test','path':'/a','value':1} ])",
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Constraint);
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
