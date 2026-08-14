#![forbid(unsafe_code)]
#![deny(warnings)]

use tempfile::tempdir;
use turso_fastdb::decode::SetValue;
use turso_fastdb::{Database, ErrorCategory, StatementResult, Value};

fn rows(result: &StatementResult) -> &[Value] {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    rows
}

fn object(result: &StatementResult) -> &std::collections::BTreeMap<String, Value> {
    let Value::Object(value) = &rows(result)[0] else {
        panic!("expected object row")
    };
    value
}

fn create_function_record(database: &Database) {
    database
        .connect()
        .unwrap()
        .execute(
            "CREATE funcs:one SET \
             added = array::add([1,2],2), appended = array::append([1],2), \
             pushed = array::push([1],2), prepended = array::prepend([2],1), \
             at = array::at([1,2],-1), concatenated = array::concat([1],[2,3]), \
             distinct = array::distinct([2,1,2]), sliced = array::slice([1,2,3],1,2), \
             complemented = array::complement([2,1],[2,3]), \
             intersected = array::intersect([2,1],[2,3]), \
             included = array::includes([1,2],2), index_of = array::index_of([1,2],2), \
             a_empty = array::is_empty([]), a_first = array::first([1,2]), \
             a_last = array::last([1,2]), a_len = array::len([1,2]), \
             a_max = array::max([1,2]), a_min = array::min([1,2]), \
             popped = array::pop([1,2]), removed_index = array::remove([1,2,3],1), \
             reversed = array::reverse([1,2]), sorted = array::sort([2,1]), \
             ascending = array::sort::asc([2,1]), unioned = array::union([2,1],[2,3]), \
             difference = array::difference([2,1],[2,3]), \
             repeated = array::repeat([1,2],2), ranged = array::range(1,4), \
             descending = array::sort::desc([2,1]), joined = array::join([1,2],'-'), \
             entries = object::entries({b:2,a:1}), \
             extended = object::extend({a:1},{b:2,a:3}), \
             from_entries = object::from_entries([['a',1],['b',2]]), \
             o_empty = object::is_empty({}), keys = object::keys({b:2,a:1}), \
             o_len = object::len({a:1}), removed = object::remove({a:1,b:2},'a'), \
             o_values = object::values({b:2,a:1}), \
             s_added = set::add(<set>[1,2],2), s_at = set::at(<set>[1,2],-1), \
             s_complement = set::complement(<set>[2,1],<set>[2,3]), \
             s_contains = set::contains(<set>[1,2],2), \
             s_first = set::first(<set>[1,2]), s_intersect = set::intersect(<set>[2,1],<set>[2,3]), \
             s_empty = set::is_empty(<set>[]), s_join = set::join(<set>[1,2],'-'), \
             s_last = set::last(<set>[1,2]), s_len = set::len(<set>[1,2]), \
             s_max = set::max(<set>[1,2]), s_min = set::min(<set>[1,2]), \
             s_remove = set::remove(<set>[1,2],1), s_slice = set::slice(<set>[1,2,3],1,2), \
             set_union = set::union(<set>[2,1],<set>[2,3]), \
             set_difference = set::difference(<set>[2,1],<set>[2,3]), \
             byte_len = bytes::len(<bytes>'abc')",
        )
        .unwrap();
}

fn assert_function_record(database: &Database) {
    let selected = database
        .connect()
        .unwrap()
        .execute("SELECT * FROM funcs:one")
        .unwrap();
    let row = object(&selected.statements[0]);
    assert_eq!(
        row.get("added"),
        Some(&Value::Array(vec![Value::Integer(1), Value::Integer(2)]))
    );
    let one_two = Value::Array(vec![Value::Integer(1), Value::Integer(2)]);
    for key in ["appended", "pushed", "prepended", "sorted", "ascending"] {
        assert_eq!(row.get(key), Some(&one_two), "{key}");
    }
    let two_one = Value::Array(vec![Value::Integer(2), Value::Integer(1)]);
    for key in ["distinct", "reversed", "descending"] {
        assert_eq!(row.get(key), Some(&two_one), "{key}");
    }
    assert_eq!(row.get("at"), Some(&Value::Integer(2)));
    assert_eq!(row.get("a_first"), Some(&Value::Integer(1)));
    assert_eq!(row.get("a_last"), Some(&Value::Integer(2)));
    assert_eq!(row.get("a_len"), Some(&Value::Integer(2)));
    assert_eq!(row.get("a_max"), Some(&Value::Integer(2)));
    assert_eq!(row.get("a_min"), Some(&Value::Integer(1)));
    assert_eq!(row.get("popped"), Some(&Value::Integer(2)));
    assert_eq!(row.get("index_of"), Some(&Value::Integer(1)));
    assert_eq!(row.get("included"), Some(&Value::Bool(true)));
    assert_eq!(row.get("a_empty"), Some(&Value::Bool(true)));
    assert_eq!(
        row.get("complemented"),
        Some(&Value::Array(vec![Value::Integer(1)]))
    );
    assert_eq!(
        row.get("intersected"),
        Some(&Value::Array(vec![Value::Integer(2)]))
    );
    assert_eq!(
        row.get("removed_index"),
        Some(&Value::Array(vec![Value::Integer(1), Value::Integer(3)]))
    );
    assert_eq!(
        row.get("difference"),
        Some(&Value::Array(vec![Value::Integer(1), Value::Integer(3)]))
    );
    assert_eq!(
        row.get("concatenated"),
        Some(&Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]))
    );
    assert_eq!(
        row.get("sliced"),
        Some(&Value::Array(vec![Value::Integer(2)]))
    );
    assert_eq!(
        row.get("unioned"),
        Some(&Value::Array(vec![
            Value::Integer(2),
            Value::Integer(1),
            Value::Integer(3),
        ]))
    );
    assert_eq!(
        row.get("ranged"),
        Some(&Value::Array(vec![
            Value::Integer(1),
            Value::Integer(2),
            Value::Integer(3),
        ]))
    );
    assert_eq!(
        row.get("repeated"),
        Some(&Value::Array(vec![
            Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
            Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
        ]))
    );
    assert_eq!(row.get("joined"), Some(&Value::Str("1-2".into())));
    assert_eq!(row.get("byte_len"), Some(&Value::Integer(3)));
    assert_eq!(
        row.get("entries"),
        Some(&Value::Array(vec![
            Value::Array(vec![Value::Str("a".into()), Value::Integer(1)]),
            Value::Array(vec![Value::Str("b".into()), Value::Integer(2)]),
        ]))
    );
    assert_eq!(row.get("o_empty"), Some(&Value::Bool(true)));
    assert_eq!(row.get("o_len"), Some(&Value::Integer(1)));
    assert_eq!(
        row.get("keys"),
        Some(&Value::Array(vec![
            Value::Str("a".into()),
            Value::Str("b".into()),
        ]))
    );
    assert_eq!(
        row.get("o_values"),
        Some(&Value::Array(vec![Value::Integer(1), Value::Integer(2)]))
    );
    for key in ["extended", "from_entries"] {
        let Value::Object(value) = row.get(key).unwrap() else {
            panic!("expected {key} object")
        };
        assert_eq!(
            value.get("a"),
            Some(&Value::Integer(if key == "extended" { 3 } else { 1 }))
        );
        assert_eq!(value.get("b"), Some(&Value::Integer(2)));
    }
    let Value::Object(removed) = row.get("removed").unwrap() else {
        panic!("expected removed object")
    };
    assert_eq!(removed.len(), 1);
    assert_eq!(removed.get("b"), Some(&Value::Integer(2)));
    assert_eq!(row.get("s_at"), Some(&Value::Integer(2)));
    assert_eq!(row.get("s_contains"), Some(&Value::Bool(true)));
    assert_eq!(row.get("s_first"), Some(&Value::Integer(1)));
    assert_eq!(row.get("s_empty"), Some(&Value::Bool(true)));
    assert_eq!(row.get("s_join"), Some(&Value::Str("1-2".into())));
    assert_eq!(row.get("s_last"), Some(&Value::Integer(2)));
    assert_eq!(row.get("s_len"), Some(&Value::Integer(2)));
    assert_eq!(row.get("s_max"), Some(&Value::Integer(2)));
    assert_eq!(row.get("s_min"), Some(&Value::Integer(1)));
    let set_one = Value::Set(SetValue::new(vec![Value::Integer(1)]).unwrap());
    let set_two = Value::Set(SetValue::new(vec![Value::Integer(2)]).unwrap());
    let set_one_two = one_two_set();
    assert_eq!(row.get("s_added"), Some(&set_one_two));
    assert_eq!(row.get("s_complement"), Some(&set_one));
    assert_eq!(row.get("s_intersect"), Some(&set_two));
    assert_eq!(row.get("s_remove"), Some(&set_two));
    assert_eq!(row.get("s_slice"), Some(&set_two));
    let set_one_three =
        Value::Set(SetValue::new(vec![Value::Integer(1), Value::Integer(3)]).unwrap());
    assert_eq!(row.get("set_difference"), Some(&set_one_three));
    assert_eq!(
        row.get("set_union"),
        Some(&Value::Set(
            SetValue::new(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])
            .unwrap()
        ))
    );
}

fn one_two_set() -> Value {
    Value::Set(SetValue::new(vec![Value::Integer(1), Value::Integer(2)]).unwrap())
}

#[test]
fn p13_fn_002_collection_and_object_functions_execute_and_reopen() {
    let memory = Database::open_memory().unwrap();
    create_function_record(&memory);
    assert_function_record(&memory);

    let directory = tempdir().unwrap();
    let path = directory.path().join("phase13-functions.fastdb");
    {
        let database = Database::open(path.to_str().unwrap()).unwrap();
        create_function_record(&database);
    }
    assert_function_record(&Database::open(path.to_str().unwrap()).unwrap());
}

#[test]
fn p13_fn_003_functions_execute_in_projection_and_postfilter() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE item:one SET values = [1,2]; \
             CREATE item:two SET values = [3]",
        )
        .unwrap();
    let selected = connection
        .execute(
            "SELECT array::len(values) AS length FROM item \
             WHERE array::includes(values, 2)",
        )
        .unwrap();
    assert_eq!(rows(&selected.statements[0]).len(), 1);
    let Value::Object(row) = &rows(&selected.statements[0])[0] else {
        panic!("expected row")
    };
    assert_eq!(row.get("length"), Some(&Value::Integer(2)));
}

#[test]
fn p13_fn_004_unknown_names_and_wrong_arity_fail_before_mutation() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE item:one SET value = array::unknown([])",
        "CREATE item:one SET value = array::len([], [])",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT * FROM item").unwrap();
    assert!(rows(&selected.statements[0]).is_empty());

    connection.execute("BEGIN").unwrap();
    connection
        .execute("CREATE item:one SET value = array::len([], [])")
        .unwrap_err();
    assert_eq!(
        connection
            .execute("SELECT * FROM item")
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.execute("CANCEL").unwrap();
}
