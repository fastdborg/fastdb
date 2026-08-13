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

#[test]
fn p13_fn_026_misc_pure_helpers_match_reference_results() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE misc_fn:one SET \
             fuzzy_equal = string::similarity::fuzzy('abc','abc'), \
             fuzzy_subsequence = string::similarity::fuzzy('hello','hlo'), \
             fuzzy_missing = string::similarity::fuzzy('abc','xyz'), \
             sanitized = string::html::sanitize('<script>x</script><b onclick=\"x\">ok</b>'), \
             ulid_time = time::from_ulid('01KZY55N06VR4FHTSDTRRFA1SZ'), \
             count_empty = count(), count_true = count(true), count_false = count(false), \
             not_value = not(false), not_none = not(NONE)",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM misc_fn:one").unwrap();
    let row = row(&selected.statements[0]);
    assert_eq!(row.get("fuzzy_equal"), Some(&Value::Integer(71)));
    assert_eq!(row.get("fuzzy_subsequence"), Some(&Value::Integer(62)));
    assert_eq!(row.get("fuzzy_missing"), Some(&Value::Integer(0)));
    assert_eq!(row.get("sanitized"), Some(&Value::Str("<b>ok</b>".into())));
    assert_eq!(
        row.get("ulid_time"),
        Some(&Value::Datetime(
            DatetimeValue::parse("2026-08-13T18:11:54.502Z").unwrap()
        ))
    );
    assert_eq!(row.get("count_empty"), Some(&Value::Integer(1)));
    assert_eq!(row.get("count_true"), Some(&Value::Integer(1)));
    assert_eq!(row.get("count_false"), Some(&Value::Integer(0)));
    assert_eq!(row.get("not_value"), Some(&Value::Bool(true)));
    assert_eq!(row.get("not_none"), Some(&Value::Bool(true)));
}

#[test]
fn p13_fn_027_misc_pure_errors_leave_no_record() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    assert!(connection
        .execute("CREATE bad:one SET value = time::from_ulid('bad')")
        .is_err());
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
