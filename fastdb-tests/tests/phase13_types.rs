#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::{Database, RecordId, RecordIdValue, StatementResult, Value};

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
fn p13_fn_007_type_casts_predicates_and_record_helpers_execute() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE typed_fn:one SET \
             array_value = type::array(<set>[2,1]), bool_value = type::bool('true'), \
             bytes_value = type::bytes('abc'), datetime_value = type::datetime('2026-01-01T00:00:00Z'), \
             decimal_value = type::decimal('1.20'), duration_value = type::duration('1s'), \
             file_value = type::file('assets:/one'), float_value = type::float('1.5'), \
             int_value = type::int(1.0), number_value = type::number('2'), \
             range_value = type::range([1,3]), record_value = type::record('person','one'), \
             thing_value = type::thing('person','two'), string_value = type::string(person:one), \
             string_lossy = type::string_lossy(<bytes>'abc'), \
             table_value = type::table(person:one), uuid_value = type::uuid('018f1f12-7b42-7cc7-98ad-dbdc0d501234'), \
             type_none = type::of(NONE), type_record = type::of(person:one), \
             record_id = record::id(person:one), record_table = record::table(person:one), \
             record_tb = record::tb(person:one), meta_id = meta::id(person:one), \
             meta_table = meta::table(person:one), meta_tb = meta::tb(person:one), \
             p_array = type::is::array([]) AND type::is_array([]), \
             p_bool = type::is::bool(true) AND type::is_bool(true), \
             p_bytes = type::is::bytes(<bytes>'x') AND type::is_bytes(<bytes>'x'), \
             p_collection = type::is::collection(<set>[1]) AND type::is_collection([1]), \
             p_datetime = type::is::datetime(type::datetime('2026-01-01T00:00:00Z')) AND type::is_datetime(type::datetime('2026-01-01T00:00:00Z')), \
             p_decimal = type::is::decimal(type::decimal('1')) AND type::is_decimal(type::decimal('1')), \
             p_duration = type::is::duration(type::duration('1s')) AND type::is_duration(type::duration('1s')), \
             p_float = type::is::float(1.0) AND type::is_float(1.0), \
             p_none = type::is::none(NONE) AND type::is_none(NONE), \
             p_null = type::is::null(NULL) AND type::is_null(NULL), \
             p_number = type::is::number(1) AND type::is_number(1), \
             p_object = type::is::object({}) AND type::is_object({}), \
             p_range = type::is::range(1..3) AND type::is_range(1..3), \
             p_record = type::is::record(person:one) AND type::is_record(person:one), \
             p_string = type::is::string('x') AND type::is_string('x'), \
             p_uuid = type::is::uuid(type::uuid('018f1f12-7b42-7cc7-98ad-dbdc0d501234')) AND type::is_uuid(type::uuid('018f1f12-7b42-7cc7-98ad-dbdc0d501234'))",
        )
        .unwrap();
    let result = connection.execute("SELECT * FROM typed_fn:one").unwrap();
    let row = row(&result.statements[0]);

    assert_eq!(row.get("bool_value"), Some(&Value::Bool(true)));
    assert_eq!(row.get("int_value"), Some(&Value::Integer(1)));
    assert_eq!(row.get("number_value"), Some(&Value::Integer(2)));
    assert_eq!(
        row.get("record_value"),
        Some(&Value::RecordId(RecordId::new("person", "one")))
    );
    assert_eq!(
        row.get("thing_value"),
        Some(&Value::RecordId(RecordId::new("person", "two")))
    );
    assert_eq!(
        row.get("string_value"),
        Some(&Value::Str("person:one".into()))
    );
    assert_eq!(row.get("string_lossy"), Some(&Value::Str("abc".into())));
    assert_eq!(row.get("type_none"), Some(&Value::Str("none".into())));
    assert_eq!(row.get("type_record"), Some(&Value::Str("record".into())));
    assert_eq!(row.get("record_id"), Some(&Value::Str("one".into())));
    assert_eq!(row.get("record_table"), Some(&Value::Str("person".into())));
    for key in [
        "p_array",
        "p_bool",
        "p_bytes",
        "p_collection",
        "p_datetime",
        "p_decimal",
        "p_duration",
        "p_float",
        "p_none",
        "p_null",
        "p_number",
        "p_object",
        "p_range",
        "p_record",
        "p_string",
        "p_uuid",
    ] {
        assert_eq!(row.get(key), Some(&Value::Bool(true)), "{key}");
    }
}

#[test]
fn p13_fn_008_type_casts_reject_lossy_or_wrong_domain_inputs() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE bad:one SET value = type::array(1)",
        "CREATE bad:one SET value = type::bool(0)",
        "CREATE bad:one SET value = type::int(1.9)",
        "CREATE bad:one SET value = type::record('invalid')",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());

    let typed = RecordId::new("person", RecordIdValue::Integer(7));
    assert_eq!(typed.id, RecordIdValue::Integer(7));
}
