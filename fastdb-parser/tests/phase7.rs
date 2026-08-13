#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb_parser::{
    parse_one, parse_one_with_limits, CreateData, ExprKind, LimitKind, ParseErrorKind,
    ParserLimits, ProjectionList, ReturnKind, Statement, TableKindSyntax, TableMode,
    TraversalDirection,
};

#[test]
fn p7_ast_001_relation_table_type_normalizes_endpoint_synonyms() {
    for source in [
        "DEFINE TABLE wrote SCHEMAFULL TYPE RELATION IN person OUT post ENFORCED",
        "DEFINE TABLE wrote SCHEMAFULL TYPE RELATION FROM person TO post ENFORCED",
    ] {
        let Statement::DefineTable(statement) = parse_one(source).unwrap() else {
            panic!("expected DEFINE TABLE")
        };
        assert_eq!(statement.mode.value, TableMode::Schemafull);
        let TableKindSyntax::Relation(relation) = statement.kind else {
            panic!("expected relation table")
        };
        assert_eq!(relation.input.unwrap().value, "person");
        assert_eq!(relation.output.unwrap().value, "post");
        assert!(relation.enforced.is_some());
    }

    let Statement::DefineTable(statement) = parse_one("DEFINE TABLE likes TYPE RELATION").unwrap()
    else {
        panic!("expected DEFINE TABLE")
    };
    assert_eq!(statement.mode.value, TableMode::Schemaless);
}

#[test]
fn p7_ast_002_relate_accepts_literal_or_bound_endpoints_and_data() {
    let Statement::Relate(statement) =
        parse_one("RELATE ONLY person:one->wrote->post:two SET role = 'author' RETURN AFTER")
            .unwrap()
    else {
        panic!("expected RELATE")
    };
    assert!(statement.only.is_some());
    assert!(matches!(statement.from.kind, ExprKind::RecordId(_)));
    assert!(matches!(statement.to.kind, ExprKind::RecordId(_)));
    assert!(matches!(statement.data, Some(CreateData::Set(_))));
    assert_eq!(
        statement.return_clause.unwrap().kind.value,
        ReturnKind::After
    );

    let Statement::Relate(statement) =
        parse_one("RELATE $from->likes->$to CONTENT { weight: 2 }").unwrap()
    else {
        panic!("expected RELATE")
    };
    assert!(matches!(statement.from.kind, ExprKind::Parameter(_)));
    assert!(matches!(statement.to.kind, ExprKind::Parameter(_)));
    assert!(matches!(statement.data, Some(CreateData::Content(_))));
}

#[test]
fn p7_ast_003_traversal_supports_fixed_depth_directions_and_materialization() {
    let Statement::Select(statement) = parse_one(
        "SELECT ->likes->person<-follows<-person<->friends<->person.* AS network FROM person:one",
    )
    .unwrap() else {
        panic!("expected SELECT")
    };
    let ProjectionList::Fields(projections) = statement.projections else {
        panic!("expected expression projections")
    };
    let ExprKind::Traversal(traversal) = &projections[0].expression.kind else {
        panic!("expected traversal")
    };
    assert_eq!(traversal.hops.len(), 3);
    assert_eq!(
        traversal.hops[0].direction.value,
        TraversalDirection::Forward
    );
    assert_eq!(
        traversal.hops[1].direction.value,
        TraversalDirection::Reverse
    );
    assert_eq!(
        traversal.hops[2].direction.value,
        TraversalDirection::Bidirectional
    );
    assert!(traversal.materialize);
    assert_eq!(projections[0].alias.as_ref().unwrap().value, "network");
}

#[test]
fn p7_ast_004_deferred_relate_shapes_and_unbounded_traversal_fail_explicitly() {
    for source in [
        "RELATE [person:one]->likes->post:two",
        "RELATE person:one->likes->[post:two]",
        "RELATE person:one->likes->post:two OR UPDATE",
    ] {
        assert!(matches!(
            parse_one(source).unwrap_err().kind,
            ParseErrorKind::UnsupportedSyntax { .. }
        ));
    }

    let limits = ParserLimits {
        max_graph_hops: 2,
        ..ParserLimits::default()
    };
    let error = parse_one_with_limits("SELECT ->a->t->b->t->c->t AS path FROM person:one", &limits)
        .unwrap_err();
    assert!(matches!(
        error.kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::GraphHops,
            limit: 2
        }
    ));
}
