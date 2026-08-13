#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::decode::SetValue;
use turso_fastdb::{Database, StatementResult, Value};

fn row(result: &StatementResult) -> &std::collections::BTreeMap<String, Value> {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    let Value::Object(row) = &rows[0] else {
        panic!("expected object row")
    };
    row
}

fn array(values: impl IntoIterator<Item = i64>) -> Value {
    Value::Array(values.into_iter().map(Value::Integer).collect())
}

#[test]
fn p13_fn_022_advanced_collection_helpers_match_reference_results() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE collection_fn:one SET \
             bool_and = array::boolean_and([true,false],[true,true]), \
             bool_not = array::boolean_not([true,false]), \
             bool_or = array::boolean_or([true,false],[false,false]), \
             bool_xor = array::boolean_xor([true,false],[true,true]), \
             clumped = array::clump([1,2,3,4,5],2), \
             combined = array::combine([1,2],['a','b']), \
             filled = array::fill([1,2,3],9,1,2), \
             flattened = array::flatten([1,[2,[3]]]), \
             grouped = array::group([1,[2,[1]],2]), \
             inserted = array::insert([1,3],2,1), \
             logical_and_value = array::logical_and([1,NONE,3],[2,2,NONE]), \
             logical_or_value = array::logical_or([1,NONE,3],[2,2,NONE]), \
             logical_xor_value = array::logical_xor([1,NONE,3],[2,2,NONE]), \
             matched = array::matches([1,2,1],1), \
             sequence_value = array::sequence(2,5), \
             lexical = array::sort_lexical(['10','2','a']), \
             natural = array::sort_natural(['a10','a2','a1']), \
             natural_lexical = array::sort_natural_lexical(['A10','a2','a1']), \
             swapped = array::swap([1,2,3],0,2), \
             transposed = array::transpose([[1,2],[3,4]]), \
             windows_value = array::windows([1,2,3,4],3), \
             set_flattened = set::flatten(<set>[1,[2,[3]]]), \
             shuffled = array::shuffle([1,2,3,4])",
        )
        .unwrap();
    let selected = connection
        .execute("SELECT * FROM collection_fn:one")
        .unwrap();
    let row = row(&selected.statements[0]);
    assert_eq!(
        row.get("bool_and"),
        Some(&Value::Array(vec![Value::Bool(true), Value::Bool(false)]))
    );
    assert_eq!(
        row.get("bool_not"),
        Some(&Value::Array(vec![Value::Bool(false), Value::Bool(true)]))
    );
    assert_eq!(row.get("bool_or"), row.get("bool_and"));
    assert_eq!(row.get("bool_xor"), row.get("bool_not"));
    assert_eq!(
        row.get("clumped"),
        Some(&Value::Array(vec![
            array([1, 2]),
            array([3, 4]),
            array([5])
        ]))
    );
    assert_eq!(row.get("filled"), Some(&array([1, 9, 3])));
    assert_eq!(
        row.get("flattened"),
        Some(&Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            array([3])
        ]))
    );
    assert_eq!(
        row.get("grouped"),
        Some(&Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            array([1])
        ]))
    );
    assert_eq!(row.get("inserted"), Some(&array([1, 2, 3])));
    assert_eq!(
        row.get("logical_and_value"),
        Some(&Value::Array(vec![
            Value::Integer(2),
            Value::None,
            Value::None
        ]))
    );
    assert_eq!(row.get("logical_or_value"), Some(&array([1, 2, 3])));
    assert_eq!(
        row.get("logical_xor_value"),
        Some(&Value::Array(vec![
            Value::Bool(false),
            Value::Integer(2),
            Value::Integer(3)
        ]))
    );
    assert_eq!(
        row.get("matched"),
        Some(&Value::Array(vec![
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true)
        ]))
    );
    assert_eq!(row.get("sequence_value"), Some(&array([2, 3, 4, 5, 6])));
    for (key, expected) in [
        ("lexical", vec!["10", "2", "a"]),
        ("natural", vec!["a1", "a2", "a10"]),
        ("natural_lexical", vec!["a1", "a2", "A10"]),
    ] {
        assert_eq!(
            row.get(key),
            Some(&Value::Array(
                expected.into_iter().map(Value::from).collect()
            )),
            "{key}"
        );
    }
    assert_eq!(row.get("swapped"), Some(&array([3, 2, 1])));
    assert_eq!(
        row.get("transposed"),
        Some(&Value::Array(vec![array([1, 3]), array([2, 4])]))
    );
    assert_eq!(
        row.get("windows_value"),
        Some(&Value::Array(vec![array([1, 2, 3]), array([2, 3, 4])]))
    );
    assert_eq!(
        row.get("set_flattened"),
        Some(&Value::Set(
            SetValue::new(vec![Value::Integer(1), Value::Integer(2), array([3])]).unwrap()
        ))
    );
    let Some(Value::Array(shuffled)) = row.get("shuffled") else {
        panic!("expected shuffled array")
    };
    let mut shuffled = shuffled.clone();
    shuffled.sort_by_key(|value| match value {
        Value::Integer(value) => *value,
        _ => panic!("expected integer"),
    });
    assert_eq!(Value::Array(shuffled), array([1, 2, 3, 4]));
}

#[test]
fn p13_fn_023_advanced_collection_errors_are_atomic() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE bad:one SET value = array::clump([1],0)",
        "CREATE bad:one SET value = array::transpose([[1],[2,3]])",
        "CREATE bad:one SET value = array::boolean_not([1])",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
