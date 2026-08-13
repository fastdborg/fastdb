#![forbid(unsafe_code)]
#![deny(warnings)]

use std::collections::BTreeMap;
use tempfile::tempdir;
use turso_fastdb::decode::{
    DatetimeValue, DecimalValue, DurationValue, FileValue, RangeBound, RangeValue, RegexValue,
    SetValue, TableValue,
};
use turso_fastdb::{Database, Params, RecordId, StatementResult, Value};

fn phase12_document() -> BTreeMap<String, Value> {
    BTreeMap::from([
        ("absent".into(), Value::None),
        ("blob".into(), Value::Bytes(vec![0, 1, 127, 255])),
        (
            "created".into(),
            Value::Datetime(DatetimeValue::parse("2026-08-13T23:15:00.123456789+07:00").unwrap()),
        ),
        (
            "amount".into(),
            Value::Decimal(DecimalValue::parse("123456789.1200").unwrap()),
        ),
        (
            "elapsed".into(),
            Value::Duration(DurationValue::new(86_401, 250_000_000).unwrap()),
        ),
        (
            "file_ref".into(),
            Value::File(FileValue::new("assets:/one.bin").unwrap()),
        ),
        (
            "pattern".into(),
            Value::Regex(RegexValue::new("^[a-z]+$").unwrap()),
        ),
        (
            "span".into(),
            Value::Range(RangeValue::new(
                RangeBound::Included(Box::new(Value::Integer(1))),
                RangeBound::Excluded(Box::new(Value::Integer(4))),
            )),
        ),
        (
            "tags".into(),
            Value::Set(
                SetValue::new(vec![Value::Str("two".into()), Value::Str("one".into())]).unwrap(),
            ),
        ),
        (
            "table_value".into(),
            Value::Table(TableValue::new("person").unwrap()),
        ),
        (
            "typed".into(),
            Value::Array(vec![Value::Integer(1), Value::Integer(2)]),
        ),
        (
            "uuid_value".into(),
            Value::Uuid(uuid::Uuid::parse_str("018f1f12-7b42-7cc7-98ad-dbdc0d501234").unwrap()),
        ),
    ])
}

fn assert_round_trip(database: &Database) {
    let connection = database.connect().unwrap();
    let fields = phase12_document();
    let params = Params::from([("doc".into(), Value::Object(fields.clone()))]);
    connection
        .execute_with_params("CREATE sample:one CONTENT $doc", &params)
        .unwrap();
    let selected = connection.execute("SELECT * FROM sample:one").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    let Value::Object(stored) = &rows[0] else {
        panic!("expected object")
    };
    let mut expected = fields;
    expected.insert("id".into(), Value::RecordId(RecordId::new("sample", "one")));
    assert_eq!(stored, &expected);
}

#[test]
fn p12_value_004_bound_values_round_trip_in_memory_and_on_disk_after_reopen() {
    let memory = Database::open_memory().unwrap();
    assert_round_trip(&memory);

    let directory = tempdir().unwrap();
    let path = directory.path().join("values.fastdb");
    {
        let database = Database::open(path.to_str().unwrap()).unwrap();
        assert_round_trip(&database);
    }
    let database = Database::open(path.to_str().unwrap()).unwrap();
    let connection = database.connect().unwrap();
    let selected = connection.execute("SELECT * FROM sample:one").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    let Value::Object(stored) = &rows[0] else {
        panic!("expected object")
    };
    assert_eq!(
        stored.get("blob"),
        Some(&Value::Bytes(vec![0, 1, 127, 255]))
    );
    assert_eq!(stored.get("amount"), phase12_document().get("amount"));
    connection.close().unwrap();

    let backup = directory.path().join("values-backup.fastdb");
    database.backup_to(&backup).unwrap();
    let restored = Database::open(backup.to_str().unwrap()).unwrap();
    let restored_rows = restored
        .connect()
        .unwrap()
        .execute("SELECT * FROM sample:one")
        .unwrap();
    assert_eq!(restored_rows.statements, selected.statements);
}

#[test]
fn p12_value_005_schema_types_enforce_typed_values_and_collection_lengths() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "DEFINE TABLE typed SCHEMAFULL; \
             DEFINE FIELD blob ON typed TYPE bytes; \
             DEFINE FIELD created ON typed TYPE datetime; \
             DEFINE FIELD amount ON typed TYPE decimal; \
             DEFINE FIELD elapsed ON typed TYPE duration; \
             DEFINE FIELD file_ref ON typed TYPE file; \
             DEFINE FIELD pattern ON typed TYPE regex; \
             DEFINE FIELD span ON typed TYPE range; \
             DEFINE FIELD tags ON typed TYPE set<string, 2>; \
             DEFINE FIELD table_value ON typed TYPE table; \
             DEFINE FIELD typed ON typed TYPE array<int, 2>; \
             DEFINE FIELD uuid_value ON typed TYPE uuid; \
             DEFINE FIELD absent ON typed TYPE option<string>",
        )
        .unwrap();
    let params = Params::from([("doc".into(), Value::Object(phase12_document()))]);
    connection
        .execute_with_params("CREATE typed:one CONTENT $doc", &params)
        .unwrap();

    let mut invalid = phase12_document();
    invalid.insert("typed".into(), Value::Array(vec![Value::Integer(1)]));
    let params = Params::from([("doc".into(), Value::Object(invalid))]);
    assert!(connection
        .execute_with_params("CREATE typed:bad CONTENT $doc", &params)
        .is_err());
    let selected = connection.execute("SELECT * FROM typed").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert_eq!(rows.len(), 1);
}
