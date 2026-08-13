#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb_parser::{
    parse, AssignmentOperator, ExprKind, InsertData, RecordIdPartKind, ReturnKind, SelectTarget,
    Statement, Target, UpdateData,
};

#[test]
fn p14_parse_001_complex_record_ids_are_structured_literals() {
    let script = parse("SELECT * FROM person:{ region: 'eu', key: [5, true, NULL] }").unwrap();
    let Statement::Select(select) = &script.statements[0] else {
        panic!("expected SELECT")
    };
    let SelectTarget::Target(Target::Record(record)) = &select.target else {
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
fn p14_parse_005_complete_select_pipeline_is_structured() {
    let script = parse(
        "SELECT VALUE n FROM person ORDER BY n NUMERIC LIMIT BY $limit START AT $start; \
         SELECT *, n * 2 AS double OMIT nested.secret FROM person \
           SPLIT ON tags GROUP BY grp ORDER BY grp COLLATE FETCH friend; \
         SELECT { grp, nested.{ keep } } AS picked FROM (SELECT * FROM person), [42]",
    )
    .unwrap();
    let Statement::Select(value) = &script.statements[0] else {
        panic!("expected SELECT")
    };
    assert!(value.value.is_some());
    assert!(value.limit_expression.is_some() && value.start_expression.is_some());
    assert!(value.order_by[0].numeric.is_some());
    let Statement::Select(pipeline) = &script.statements[1] else {
        panic!("expected SELECT")
    };
    assert_eq!(pipeline.omit.len(), 1);
    assert_eq!(pipeline.split.len(), 1);
    assert!(matches!(
        pipeline.group,
        Some(turso_fastdb_parser::GroupClause::By(_))
    ));
    assert_eq!(pipeline.fetch.len(), 1);
    let Statement::Select(targets) = &script.statements[2] else {
        panic!("expected SELECT")
    };
    assert!(matches!(targets.target, SelectTarget::Subquery(_)));
    assert_eq!(targets.additional_targets.len(), 1);
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
            let SelectTarget::Target(Target::RecordRange(range)) = &select.target else {
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

#[test]
fn p14_parse_006_mutation_target_expressions_and_timeouts_are_structured() {
    let script = parse(
        "CREATE [person:a, animal:b] SET n = 1 RETURN AFTER TIMEOUT 2s; \
         UPDATE $targets SET n += 1 TIMEOUT $deadline; \
         DELETE [person:a, animal:b] RETURN BEFORE TIMEOUT 1s; \
         INSERT INTO person { id: 'c' } TIMEOUT 3s",
    )
    .unwrap();
    let Statement::Create(create) = &script.statements[0] else {
        panic!("expected CREATE")
    };
    assert!(matches!(create.target, Target::Expression(_)) && create.timeout.is_some());
    let Statement::Update(update) = &script.statements[1] else {
        panic!("expected UPDATE")
    };
    assert!(matches!(update.target, Target::Expression(_)) && update.timeout.is_some());
    let Statement::Delete(delete) = &script.statements[2] else {
        panic!("expected DELETE")
    };
    assert!(matches!(delete.target, Target::Expression(_)) && delete.timeout.is_some());
    let Statement::Insert(insert) = &script.statements[3] else {
        panic!("expected INSERT")
    };
    assert!(insert.timeout.is_some());

    assert!(parse("UPDATE person SET n = 1 TIMEOUT 4").is_err());
    assert!(parse("DELETE person TIMEOUT 1s TIMEOUT 2s").is_err());
    assert!(parse("CREATE person:a VERSION d'2024-01-01T00:00:00Z'").is_err());
}
