#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb_parser::{parse, ExprKind, RecordIdPartKind, Statement, Target};

#[test]
fn p14_parse_001_complex_record_ids_are_structured_literals() {
    let script = parse("SELECT * FROM person:{ region: 'eu', key: [5, true, NULL] }").unwrap();
    let Statement::Select(select) = &script.statements[0] else {
        panic!("expected SELECT")
    };
    let Target::Record(record) = &select.target else {
        panic!("expected record target")
    };
    let RecordIdPartKind::Complex(expression) = &record.id.kind else {
        panic!("expected complex record ID")
    };
    assert!(matches!(expression.kind, ExprKind::Object(_)));
    assert_eq!(
        record.id.to_source(),
        "{region: 'eu', key: [5, true, NULL]}"
    );
}

#[test]
fn p14_parse_003_record_ranges_preserve_open_and_inclusive_bounds() {
    let script =
        parse("SELECT * FROM person:1..3; SELECT * FROM person:..=3; SELECT * FROM person:2..")
            .unwrap();
    let ranges = script
        .statements
        .iter()
        .map(|statement| {
            let Statement::Select(select) = statement else {
                panic!("expected SELECT")
            };
            let Target::RecordRange(range) = &select.target else {
                panic!("expected record range")
            };
            range
        })
        .collect::<Vec<_>>();
    assert!(ranges[0].start.is_some() && ranges[0].end.is_some() && !ranges[0].inclusive);
    assert!(ranges[1].start.is_none() && ranges[1].end.is_some() && ranges[1].inclusive);
    assert!(ranges[2].start.is_some() && ranges[2].end.is_none() && !ranges[2].inclusive);
}

#[test]
fn p14_parse_002_complex_record_ids_reject_dynamic_expressions() {
    for source in [
        "SELECT * FROM person:[name]",
        "SELECT * FROM person:{ key: $value }",
        "SELECT * FROM person:[1 + 2]",
    ] {
        assert!(parse(source).is_err(), "accepted {source}");
    }
}
