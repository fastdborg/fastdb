use turso_fastdb_parser::{
    parse_one, AnalyzerTokenizerSyntax, BinaryOperator, ExprKind, IndexDefinitionSurface,
    IndexKindSyntax, ProjectionList, Statement,
};

#[test]
fn p8_parse_001_surreal_analyzer_and_fulltext_index_are_structural() {
    let Statement::DefineAnalyzer(analyzer) =
        parse_one("DEFINE ANALYZER blankish TOKENIZERS blank").unwrap()
    else {
        panic!("expected analyzer definition");
    };
    assert_eq!(analyzer.name.value, "blankish");
    assert_eq!(analyzer.tokenizer.value, AnalyzerTokenizerSyntax::Blank);

    let Statement::DefineIndex(index) = parse_one(
        "DEFINE INDEX text_idx ON TABLE doc FIELDS text FULLTEXT ANALYZER blankish HIGHLIGHTS",
    )
    .unwrap() else {
        panic!("expected index definition");
    };
    assert_eq!(index.surface, IndexDefinitionSurface::SurrealDefine);
    assert_eq!(index.fields.len(), 1);
    assert!(matches!(
        index.kind,
        IndexKindSyntax::Fulltext {
            analyzer,
            highlights: Some(_),
            ..
        } if analyzer.value == "blankish"
    ));
}

#[test]
fn p8_parse_002_match_operators_and_search_functions_preserve_references() {
    let Statement::Select(select) = parse_one(
        "SELECT search::score(7) AS score, search::highlight('<b>', '</b>', 7) AS marked FROM doc WHERE text @7@ $query",
    )
    .unwrap()
    else {
        panic!("expected select");
    };
    let condition = select.condition.unwrap();
    let ExprKind::Binary { operator, .. } = condition.kind else {
        panic!("expected binary match");
    };
    assert_eq!(operator.value, BinaryOperator::FtsMatch(Some(7)));
    let ProjectionList::Fields(projections) = select.projections else {
        panic!("expected projections");
    };
    assert!(matches!(
        &projections[0].expression.kind,
        ExprKind::FunctionCall { name, arguments }
            if name.iter().map(|part| part.value.as_str()).collect::<Vec<_>>() == ["search", "score"]
                && matches!(arguments[0].kind, ExprKind::Integer(7))
    ));

    let Statement::Select(select) =
        parse_one("SELECT * FROM doc WHERE text @@ 'Rust web'").unwrap()
    else {
        panic!("expected select");
    };
    let ExprKind::Binary { operator, .. } = select.condition.unwrap().kind else {
        panic!("expected binary match");
    };
    assert_eq!(operator.value, BinaryOperator::FtsMatch(None));
}

#[test]
fn p8_parse_003_native_fts_index_is_a_labeled_create_surface() {
    let Statement::DefineIndex(index) = parse_one(
        "CREATE INDEX search_idx ON TABLE doc USING fts (title, body) WITH (tokenizer = 'simple', weights = [2, 1])",
    )
    .unwrap()
    else {
        panic!("expected native index definition");
    };
    assert_eq!(index.surface, IndexDefinitionSurface::FastDbCreate);
    assert_eq!(index.fields.len(), 2);
    let IndexKindSyntax::Provider { name, options, .. } = index.kind else {
        panic!("expected provider index");
    };
    assert_eq!(name.value, "fts");
    assert_eq!(options.len(), 2);
}

#[test]
fn p8_parse_004_deferred_analyzers_and_bad_match_references_fail_explicitly() {
    for source in [
        "DEFINE ANALYZER a TOKENIZERS class",
        "DEFINE ANALYZER a TOKENIZERS blank FILTERS lowercase",
        "DEFINE ANALYZER a TOKENIZERS blank FUNCTIONS fn::custom",
        "SELECT * FROM doc WHERE text @OR@ 'term'",
        "SELECT * FROM doc WHERE text @4294967296@ 'term'",
    ] {
        let error = parse_one(source).unwrap_err();
        assert!(
            error.to_string().contains("unsupported"),
            "{source}: {error}"
        );
        assert!(error.span.is_within(source.len()));
    }
}
