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
fn p14_parse_002_complex_record_ids_reject_dynamic_expressions() {
    for source in [
        "SELECT * FROM person:[name]",
        "SELECT * FROM person:{ key: $value }",
        "SELECT * FROM person:[1 + 2]",
    ] {
        assert!(parse(source).is_err(), "accepted {source}");
    }
}
