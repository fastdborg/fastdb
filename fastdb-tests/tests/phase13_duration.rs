#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::decode::DurationValue;
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
fn p13_fn_009_duration_literals_extractors_constructors_and_max_execute() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE duration_fn:one SET \
             years = duration::years(730d), weeks = duration::weeks(15d), \
             days = duration::days(1d2h), hours = duration::hours(1d2h), \
             minutes = duration::mins(1h30m), seconds = duration::secs(1m2s), \
             millis = duration::millis(1s500ms), micros = duration::micros(1ms500us), \
             nanos = duration::nanos(1us5ns), from_weeks = duration::from_weeks(2), \
             from_days = duration::from_days(2), from_hours = duration::from_hours(2), \
             from_mins = duration::from_mins(2), from_secs = duration::from_secs(2), \
             from_millis = duration::from_millis(1500), \
             from_micros = duration::from_micros(1500), \
             from_nanos = duration::from_nanos(1005), maximum = duration::max",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM duration_fn:one").unwrap();
    let row = row(&selected.statements[0]);
    for (key, expected) in [
        ("years", 2),
        ("weeks", 2),
        ("days", 1),
        ("hours", 26),
        ("minutes", 90),
        ("seconds", 62),
        ("millis", 1500),
        ("micros", 1500),
        ("nanos", 1005),
    ] {
        assert_eq!(row.get(key), Some(&Value::Integer(expected)), "{key}");
    }
    assert_eq!(
        row.get("from_days"),
        Some(&Value::Duration(DurationValue::new(172_800, 0).unwrap()))
    );
    assert_eq!(
        row.get("from_millis"),
        Some(&Value::Duration(
            DurationValue::new(1, 500_000_000).unwrap()
        ))
    );
    assert_eq!(
        row.get("from_micros"),
        Some(&Value::Duration(DurationValue::new(0, 1_500_000).unwrap()))
    );
    assert_eq!(
        row.get("from_nanos"),
        Some(&Value::Duration(DurationValue::new(0, 1_005).unwrap()))
    );
    assert_eq!(
        row.get("maximum"),
        Some(&Value::Duration(
            DurationValue::new(u64::MAX, 999_999_999).unwrap()
        ))
    );
}

#[test]
fn p13_fn_010_duration_errors_do_not_publish_partial_records() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE bad:one SET value = duration::from_days(-1)",
        "CREATE bad:one SET value = duration::days(1)",
        "CREATE bad:one SET value = duration::max()",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
