#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::decode::DatetimeValue;
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

fn datetime(value: &str) -> Value {
    Value::Datetime(DatetimeValue::parse(value).unwrap())
}

#[test]
fn p13_fn_011_time_extractors_constructors_setters_and_rounding_execute() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE time_fn:one SET \
             epoch = time::epoch, now_value = time::now(), timezone = time::timezone(), \
             year = time::year(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             month = time::month(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             day = time::day(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             hour = time::hour(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             minute = time::minute(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             second = time::second(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             nano = time::nano(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             unix = time::unix(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             millis = time::millis(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             micros = time::micros(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             weekday = time::wday(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             week = time::week(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             yearday = time::yday(type::datetime('2024-02-29T12:34:56.123456789Z')), \
             from_secs = time::from_secs(1), from_unix = time::from_unix(1), \
             from_millis = time::from_millis(1500), from_micros = time::from_micros(1500000), \
             from_nanos = time::from_nanos(1500000000), \
             from_uuid = time::from_uuid(type::uuid('018f1f12-7b42-7cc7-98ad-dbdc0d501234')), \
             leap = time::is_leap_year(type::datetime('2024-01-01T00:00:00Z')), \
             minimum = time::min([type::datetime('2024-01-01T00:00:00Z'),type::datetime('2023-01-01T00:00:00Z')]), \
             maximum = time::max([type::datetime('2024-01-01T00:00:00Z'),type::datetime('2023-01-01T00:00:00Z')]), \
             formatted = time::format(type::datetime('2024-02-29T12:34:56Z'),'%Y-%m-%d'), \
             set_year = time::set_year(type::datetime('2024-02-28T12:34:56Z'),2023), \
             set_month = time::set_month(type::datetime('2024-02-29T12:34:56Z'),1), \
             set_day = time::set_day(type::datetime('2024-02-29T12:34:56Z'),1), \
             set_hour = time::set_hour(type::datetime('2024-02-29T12:34:56Z'),1), \
             set_minute = time::set_minute(type::datetime('2024-02-29T12:34:56Z'),1), \
             set_second = time::set_second(type::datetime('2024-02-29T12:34:56Z'),1), \
             set_nano = time::set_nanosecond(type::datetime('2024-02-29T12:34:56Z'),1), \
             floored = time::floor(type::datetime('2024-02-29T12:34:56.789Z'),1s), \
             ceiled = time::ceil(type::datetime('2024-02-29T12:34:56.789Z'),1s), \
             rounded = time::round(type::datetime('2024-02-29T12:34:56.789Z'),1s), \
             grouped = time::group(type::datetime('2024-02-29T12:34:56.789Z'),1s)",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM time_fn:one").unwrap();
    let row = row(&selected.statements[0]);
    for (key, expected) in [
        ("year", 2024),
        ("month", 2),
        ("day", 29),
        ("hour", 12),
        ("minute", 34),
        ("second", 56),
        ("weekday", 4),
        ("week", 9),
        ("yearday", 60),
        ("unix", 1_709_210_096),
        ("millis", 1_709_210_096_123),
        ("micros", 1_709_210_096_123_456),
        ("nano", 1_709_210_096_123_456_789),
    ] {
        assert_eq!(row.get(key), Some(&Value::Integer(expected)), "{key}");
    }
    assert_eq!(row.get("epoch"), Some(&datetime("1970-01-01T00:00:00Z")));
    assert!(matches!(row.get("now_value"), Some(Value::Datetime(_))));
    assert!(matches!(row.get("timezone"), Some(Value::Str(value)) if !value.is_empty()));
    assert_eq!(row.get("leap"), Some(&Value::Bool(true)));
    assert_eq!(row.get("formatted"), Some(&Value::Str("2024-02-29".into())));
    assert_eq!(row.get("minimum"), Some(&datetime("2023-01-01T00:00:00Z")));
    assert_eq!(row.get("maximum"), Some(&datetime("2024-01-01T00:00:00Z")));
    assert_eq!(row.get("floored"), Some(&datetime("2024-02-29T12:34:56Z")));
    assert_eq!(row.get("grouped"), Some(&datetime("2024-02-29T12:34:56Z")));
    assert_eq!(row.get("ceiled"), Some(&datetime("2024-02-29T12:34:57Z")));
    assert_eq!(row.get("rounded"), Some(&datetime("2024-02-29T12:34:57Z")));
}

#[test]
fn p13_fn_012_invalid_time_mutations_and_domains_fail_atomically() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE bad:one SET value = time::set_year(type::datetime('2024-02-29T00:00:00Z'),2023)",
        "CREATE bad:one SET value = time::floor(type::datetime('2024-01-01T00:00:00Z'),0s)",
        "CREATE bad:one SET value = time::epoch()",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
