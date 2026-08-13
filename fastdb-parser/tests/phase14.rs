#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb_parser::{
    parse, AssignmentOperator, ExprKind, InsertData, RecordIdPartKind, ReturnKind, Statement,
    Target, UpdateData,
};

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
fn p14_parse_004_complete_mutation_forms_are_structured() {
    let script = parse(
        "CREATE ONLY person:empty RETURN DIFF; \
         UPDATE ONLY person:one MERGE { nested: { y: 2 } } RETURN VALUE nested; \
         UPSERT person:two SET n += 1, tags -= 'old' RETURN BEFORE; \
         INSERT IGNORE INTO person (id, name) VALUES ('a', 'A'), ('b', 'B') \
           ON DUPLICATE KEY UPDATE name = $input.name RETURN AFTER; \
         DELETE FROM ONLY person:one RETURN AFTER",
    )
    .unwrap();
    let Statement::Create(create) = &script.statements[0] else {
        panic!("expected CREATE")
    };
    assert!(create.data.is_none());
    assert!(matches!(
        create
            .return_clause
            .as_ref()
            .map(|clause| &clause.kind.value),
        Some(ReturnKind::Diff)
    ));
    let Statement::Update(update) = &script.statements[1] else {
        panic!("expected UPDATE")
    };
    assert!(matches!(update.data, UpdateData::Merge(_)) && update.only.is_some());
    let Statement::Upsert(upsert) = &script.statements[2] else {
        panic!("expected UPSERT")
    };
    let UpdateData::Set(assignments) = &upsert.data else {
        panic!("expected SET")
    };
    assert_eq!(assignments[0].operator.value, AssignmentOperator::Add);
    assert_eq!(assignments[1].operator.value, AssignmentOperator::Subtract);
    let Statement::Insert(insert) = &script.statements[3] else {
        panic!("expected INSERT")
    };
    assert!(matches!(&insert.data, InsertData::Values { rows, .. } if rows.len() == 2));
    assert_eq!(insert.on_duplicate.len(), 1);
    let Statement::Delete(delete) = &script.statements[4] else {
        panic!("expected DELETE")
    };
    assert!(delete.only.is_some());
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
