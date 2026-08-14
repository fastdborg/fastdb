#![forbid(unsafe_code)]
#![deny(warnings)]

use tempfile::tempdir;
use turso_fastdb::decode::{RangeBound, RangeValue};
use turso_fastdb::{Database, ErrorCategory, Params, RecordId, StatementResult, Value};

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

fn create_expression_record(database: &Database) {
    database
        .connect()
        .unwrap()
        .execute(
            "CREATE calc:one SET \
             coalesced = 0 ?? 1 + 2, truthy = 0 ?: 1 + 2, \
             power = -2 ** 3 ** 2, modulo = 5 % 2, \
             first = [1,2,3][0], last = [1,2,3][$], \
             missing_index = [1][-1], missing_slice = [1][1..=4], \
             middle = [1,2,3][1..=2], open = [1,2,3][..2], \
             casted = <array<int, 2>> ['1','2'], span = 1..=3, absent = NONE, \
             contain_result = [1,2] CONTAINS 2, contains_any = [1,2] CONTAINSANY [2,3], \
             contains_all = [1,2] CONTAINSALL [1,2], \
             contains_none = [1,2] CONTAINSNONE [3], \
             contains_not = [1,2] CONTAINSNOT 3, inside_result = 2 IN [1,2], \
             not_inside = 3 NOT IN [1,2], all_inside = [1,2] ALLINSIDE [1,2,3], \
             any_inside = [2,3] ANYINSIDE [1,2], none_inside = [3] NONEINSIDE [1,2], \
             any_equal = [1,2] ?= 2, all_equal = [2,2] *= 2, \
             exact = 1 == 1.0, truth = !!1",
        )
        .unwrap();
}

fn assert_expression_record(database: &Database) {
    let selected = database
        .connect()
        .unwrap()
        .execute("SELECT * FROM calc:one")
        .unwrap();
    let row = object(&selected.statements[0]);
    assert_eq!(
        row.get("id"),
        Some(&Value::RecordId(RecordId::new("calc", "one")))
    );
    assert_eq!(row.get("coalesced"), Some(&Value::Integer(0)));
    assert_eq!(row.get("truthy"), Some(&Value::Integer(3)));
    assert_eq!(row.get("power"), Some(&Value::Integer(64)));
    assert_eq!(row.get("modulo"), Some(&Value::Integer(1)));
    assert_eq!(row.get("first"), Some(&Value::Integer(1)));
    assert_eq!(row.get("last"), Some(&Value::Integer(3)));
    assert_eq!(row.get("missing_index"), Some(&Value::None));
    assert_eq!(row.get("missing_slice"), Some(&Value::None));
    assert_eq!(
        row.get("middle"),
        Some(&Value::Array(vec![Value::Integer(2), Value::Integer(3)]))
    );
    assert_eq!(
        row.get("open"),
        Some(&Value::Array(vec![Value::Integer(1), Value::Integer(2)]))
    );
    assert_eq!(
        row.get("casted"),
        Some(&Value::Array(vec![Value::Integer(1), Value::Integer(2)]))
    );
    assert_eq!(
        row.get("span"),
        Some(&Value::Range(RangeValue::new(
            RangeBound::Included(Box::new(Value::Integer(1))),
            RangeBound::Included(Box::new(Value::Integer(3))),
        )))
    );
    assert_eq!(row.get("absent"), Some(&Value::None));
    for key in [
        "contain_result",
        "contains_any",
        "contains_all",
        "contains_none",
        "contains_not",
        "inside_result",
        "not_inside",
        "all_inside",
        "any_inside",
        "none_inside",
        "any_equal",
        "all_equal",
        "exact",
        "truth",
    ] {
        assert_eq!(row.get(key), Some(&Value::Bool(true)), "{key}");
    }
}

#[test]
fn p13_expr_001_operators_access_casts_and_ranges_survive_reopen() {
    let memory = Database::open_memory().unwrap();
    create_expression_record(&memory);
    assert_expression_record(&memory);

    let directory = tempdir().unwrap();
    let path = directory.path().join("phase13-expressions.fastdb");
    {
        let database = Database::open(path.to_str().unwrap()).unwrap();
        create_expression_record(&database);
    }
    let reopened = Database::open(path.to_str().unwrap()).unwrap();
    assert_expression_record(&reopened);
}

#[test]
fn p13_expr_002_postfilter_and_transaction_failure_are_authoritative() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE item:one SET tags = ['a','b'], score = 2; \
             CREATE item:two SET tags = ['c'], score = 3",
        )
        .unwrap();
    let selected = connection
        .execute("SELECT id FROM item WHERE tags CONTAINS 'b' AND score ** 2 = 4")
        .unwrap();
    assert_eq!(rows(&selected.statements[0]).len(), 1);

    connection.execute("BEGIN").unwrap();
    connection
        .execute("UPDATE item:one SET score = 5 % 0")
        .unwrap_err();
    let poisoned = connection.execute("SELECT * FROM item").unwrap_err();
    assert_eq!(poisoned.category(), ErrorCategory::Transaction);
    connection.execute("CANCEL").unwrap();

    let selected = connection.execute("SELECT score FROM item:one").unwrap();
    assert_eq!(
        object(&selected.statements[0]).get("score"),
        Some(&Value::Integer(2))
    );
}

#[test]
fn p13_expr_003_bounded_collection_results_poison_explicit_transactions() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection.execute("CREATE item:one SET ok = true").unwrap();
    let values = vec![Value::Integer(1); 40_000];
    let params = Params::from([
        ("left".into(), Value::Array(values.clone())),
        ("right".into(), Value::Array(values)),
    ]);

    connection.execute("BEGIN").unwrap();
    let error = connection
        .execute_with_params("UPDATE item:one SET too_large = $left + $right", &params)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceLimit);
    assert_eq!(
        connection
            .execute("SELECT * FROM item")
            .unwrap_err()
            .category(),
        ErrorCategory::Transaction
    );
    connection.execute("CANCEL").unwrap();
}
