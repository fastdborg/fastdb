#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb_parser::{parse_one, ExprKind, IndexKindSyntax, ProjectionList, Statement};

#[test]
fn p6_ast_001_namespaced_calls_have_spanned_segments_and_arguments() {
    let Statement::Select(select) =
        parse_one("SELECT search::score(1, vector::distance::cosine(a, $q)) AS rank FROM person")
            .unwrap()
    else {
        panic!("expected SELECT")
    };
    let ProjectionList::Fields(projections) = select.projections else {
        panic!("expected expression projection")
    };
    assert_eq!(projections.len(), 1);
    assert_eq!(projections[0].alias.as_ref().unwrap().value, "rank");
    let ExprKind::FunctionCall { name, arguments } = &projections[0].expression.kind else {
        panic!("expected function call")
    };
    assert_eq!(
        name.iter()
            .map(|segment| segment.value.as_str())
            .collect::<Vec<_>>(),
        ["search", "score"]
    );
    assert_eq!(arguments.len(), 2);
    let ExprKind::FunctionCall {
        name: nested_name,
        arguments: nested_arguments,
    } = &arguments[1].kind
    else {
        panic!("expected nested function call")
    };
    assert_eq!(
        nested_name
            .iter()
            .map(|segment| segment.value.as_str())
            .collect::<Vec<_>>(),
        ["vector", "distance", "cosine"]
    );
    assert_eq!(nested_arguments.len(), 2);
    assert!(projections[0].expression.span.is_within(select.span.end()));
}

#[test]
fn p6_ast_002_expression_projections_preserve_aliases() {
    let Statement::Select(select) =
        parse_one("SELECT score + 1 AS next_score, address.city FROM person").unwrap()
    else {
        panic!("expected SELECT")
    };
    let ProjectionList::Fields(projections) = select.projections else {
        panic!("expected projections")
    };
    assert!(matches!(
        projections[0].expression.kind,
        ExprKind::Binary { .. }
    ));
    assert_eq!(projections[0].alias.as_ref().unwrap().value, "next_score");
    assert!(matches!(
        projections[1].expression.kind,
        ExprKind::FieldPath(_)
    ));
    assert!(projections[1].alias.is_none());
}

#[test]
fn p6_ast_003_explain_remove_and_rebuild_are_structured_statements() {
    let Statement::Explain(explain) =
        parse_one("EXPLAIN SELECT * FROM person WHERE score = 1").unwrap()
    else {
        panic!("expected EXPLAIN")
    };
    assert!(explain.span.end() >= explain.select.span.end());

    for (source, rebuild) in [
        ("REMOVE INDEX by_score ON person", false),
        ("REBUILD INDEX by_score ON TABLE person", true),
    ] {
        let statement = parse_one(source).unwrap();
        let maintenance = match statement {
            Statement::RemoveIndex(statement) if !rebuild => statement,
            Statement::RebuildIndex(statement) if rebuild => statement,
            other => panic!("unexpected statement: {other:?}"),
        };
        assert_eq!(maintenance.name.value, "by_score");
        assert_eq!(maintenance.table.value, "person");
        assert_eq!(maintenance.table_keyword.is_some(), rebuild);
    }
}

#[test]
fn p6_ast_004_provider_options_and_fulltext_analyzers_are_structured() {
    let Statement::DefineIndex(provider) = parse_one(
        "DEFINE INDEX body ON article FIELDS body USING fts \
         WITH (tokenizer='simple', weights=[1, 2])",
    )
    .unwrap() else {
        panic!("expected DEFINE INDEX")
    };
    let IndexKindSyntax::Provider { name, options, .. } = provider.kind else {
        panic!("expected provider index")
    };
    assert_eq!(name.value, "fts");
    assert_eq!(options.len(), 2);
    assert_eq!(options[0].key.value, "tokenizer");
    assert!(matches!(options[0].value.kind, ExprKind::String(_)));
    assert!(matches!(options[1].value.kind, ExprKind::Array(_)));

    let Statement::DefineIndex(fulltext) =
        parse_one("DEFINE INDEX body ON article FIELDS body FULLTEXT ANALYZER blank").unwrap()
    else {
        panic!("expected DEFINE INDEX")
    };
    assert!(matches!(
        fulltext.kind,
        IndexKindSyntax::Fulltext { analyzer, .. } if analyzer.value == "blank"
    ));
}
