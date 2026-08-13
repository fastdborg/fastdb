#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::decode::SetValue;
use turso_fastdb::{Database, Params, StatementResult, Value};

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
fn p13_fn_024_array_and_set_closures_match_reference_semantics() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute_with_params(
            "CREATE closure_fn:one SET \
             array_all = array::all([1,2,3], |$v| $v > 0), \
             array_any = array::any([1,2,3], |$v| $v > 2), \
             array_filter = array::filter([1,2,3], |$v| $v > 1), \
             array_filter_index = array::filter_index([1,2,3], |$v| $v > 1), \
             array_find = array::find([1,2,3], |$v| $v > 1), \
             array_find_index = array::find_index([1,2,3], |$v| $v > 1), \
             array_fold = array::fold([1,2,3], 0, |$a,$b| $a + $b), \
             array_map = array::map([1,2,3], |$v,$i| $v * $factor + $i), \
             array_reduce = array::reduce([1,2,3], |$a,$b| $a + $b), \
             set_all = set::all(<set>[1,2,3], |$v| $v > 0), \
             set_any = set::any(<set>[1,2,3], |$v| $v > 2), \
             set_filter = set::filter(<set>[1,2,3], |$v| $v > 1), \
             set_find = set::find(<set>[1,2,3], |$v| $v > 1), \
             set_fold = set::fold(<set>[1,2,3], 0, |$a,$b| $a + $b), \
             set_map = set::map(<set>[1,2,3], |$v| $v * 2), \
             set_reduce = set::reduce(<set>[1,2,3], |$a,$b| $a + $b)",
            &Params::from([("factor".into(), Value::Integer(2))]),
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM closure_fn:one").unwrap();
    let row = row(&selected.statements[0]);
    for key in ["array_all", "array_any", "set_all", "set_any"] {
        assert_eq!(row.get(key), Some(&Value::Bool(true)), "{key}");
    }
    assert_eq!(row.get("array_filter"), Some(&array([2, 3])));
    assert_eq!(row.get("array_filter_index"), Some(&array([1, 2])));
    assert_eq!(row.get("array_find"), Some(&Value::Integer(2)));
    assert_eq!(row.get("array_find_index"), Some(&Value::Integer(1)));
    assert_eq!(row.get("array_fold"), Some(&Value::Integer(6)));
    assert_eq!(row.get("array_map"), Some(&array([2, 5, 8])));
    assert_eq!(row.get("array_reduce"), Some(&Value::Integer(6)));
    assert_eq!(
        row.get("set_filter"),
        Some(&Value::Set(
            SetValue::new(vec![Value::Integer(2), Value::Integer(3)]).unwrap()
        ))
    );
    assert_eq!(row.get("set_find"), Some(&Value::Integer(2)));
    assert_eq!(row.get("set_fold"), Some(&Value::Integer(6)));
    assert_eq!(
        row.get("set_map"),
        Some(&Value::Set(
            SetValue::new(vec![
                Value::Integer(2),
                Value::Integer(4),
                Value::Integer(6)
            ])
            .unwrap()
        ))
    );
    assert_eq!(row.get("set_reduce"), Some(&Value::Integer(6)));
}

#[test]
fn p13_fn_025_closure_failures_poison_transactions_and_leave_no_record() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    assert!(connection
        .execute("BEGIN; CREATE bad:one SET value = array::map([1,0], |$v| 1 % $v); COMMIT")
        .is_err());
    connection.execute("CANCEL").unwrap();
    assert!(connection
        .execute("CREATE bad:one SET value = array::map([1], |$a,$b,$c| $a)")
        .is_err());
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
