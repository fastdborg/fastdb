#![forbid(unsafe_code)]
#![deny(warnings)]

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

#[test]
fn p13_fn_017_statistics_functions_match_reference_results() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE stats:one SET \
             bottom = math::bottom([4,1,3,2],2), top = math::top([4,1,3,2],2), \
             fixed = math::fixed(1.2345,2), iqr = math::interquartile([1,2,3,4]), \
             angle = math::lerpangle(350,10,0.5), median = math::median([1,2,3,4]), \
             midhinge = math::midhinge([1,2,3,4]), mode = math::mode([1,2,2,3]), \
             nearest = math::nearestrank([1,2,3,4],75), \
             percentile = math::percentile([1,2,3,4],75), \
             stddev = math::stddev([1,2,3,4]), trimean = math::trimean([1,2,3,4]), \
             variance = math::variance([1,2,3,4])",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM stats:one").unwrap();
    let row = row(&selected.statements[0]);
    assert_eq!(
        row.get("bottom"),
        Some(&Value::Array(vec![Value::Integer(2), Value::Integer(1)]))
    );
    assert_eq!(
        row.get("top"),
        Some(&Value::Array(vec![Value::Integer(3), Value::Integer(4)]))
    );
    assert_eq!(row.get("mode"), Some(&Value::Integer(2)));
    assert_eq!(row.get("nearest"), Some(&Value::Integer(4)));
    for (key, expected) in [
        ("fixed", 1.23),
        ("iqr", 1.5),
        ("angle", 360.0),
        ("median", 2.5),
        ("midhinge", 2.5),
        ("percentile", 3.25),
        ("stddev", 1.290_994_448_735_805_6),
        ("trimean", 2.5),
        ("variance", 1.666_666_666_666_666_7),
    ] {
        let Some(Value::Float(value)) = row.get(key) else {
            panic!("expected float for {key}")
        };
        assert!((value - expected).abs() < 1e-12, "{key}: {value}");
    }
}

#[test]
fn p13_fn_018_statistics_limits_fail_before_mutation() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE bad:one SET value = math::percentile([1,2],101)",
        "CREATE bad:one SET value = math::fixed(1.2,0)",
        "CREATE bad:one SET value = math::bottom([1,'x'],1)",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
