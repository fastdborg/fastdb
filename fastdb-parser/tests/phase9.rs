use turso_fastdb_parser::{
    parse_one, ExprKind, KnnMetric, ProjectionList, SchemaTypeKind, Statement,
};

#[test]
fn p9_parse_001_fixed_float_array_preserves_dimension() {
    let Statement::DefineField(field) =
        parse_one("DEFINE FIELD embedding ON item TYPE array<float, 1536>").unwrap()
    else {
        panic!("expected field definition")
    };
    assert!(matches!(
        field.ty.kind,
        SchemaTypeKind::FixedFloatArray(dimension) if dimension.value == 1536
    ));

    let Statement::DefineField(field) =
        parse_one("DEFINE FIELD embedding ON item TYPE option<array<float, 2>>").unwrap()
    else {
        panic!("expected optional field definition")
    };
    assert!(matches!(
        field.ty.kind,
        SchemaTypeKind::Option(inner)
            if matches!(&inner.kind, SchemaTypeKind::FixedFloatArray(dimension) if dimension.value == 2)
    ));
}

#[test]
fn p9_parse_002_knn_metrics_and_bound_query_are_structural() {
    let Statement::Select(select) = parse_one(
        "SELECT id, vector::distance::knn() AS distance FROM item \
         WHERE active = true AND embedding <|10,COSINE|> $query",
    )
    .unwrap() else {
        panic!("expected SELECT")
    };
    let ExprKind::Binary { right, .. } = select.condition.unwrap().kind else {
        panic!("expected ordinary predicate AND KNN")
    };
    assert!(matches!(
        right.kind,
        ExprKind::Knn(knn)
            if knn.k.value == 10
                && knn.metric.value == KnnMetric::Cosine
                && matches!(knn.query.kind, ExprKind::Parameter(ref name) if name == "query")
    ));
    let ProjectionList::Fields(projections) = select.projections else {
        panic!("expected projections")
    };
    assert!(matches!(
        &projections[1].expression.kind,
        ExprKind::FunctionCall { name, arguments }
            if name.iter().map(|part| part.value.as_str()).collect::<Vec<_>>()
                == ["vector", "distance", "knn"]
                && arguments.is_empty()
    ));

    let Statement::Select(select) =
        parse_one("SELECT * FROM item WHERE embedding <|2,EUCLIDEAN|> [1, 0]").unwrap()
    else {
        panic!("expected SELECT")
    };
    assert!(matches!(
        select.condition.unwrap().kind,
        ExprKind::Knn(knn) if knn.metric.value == KnnMetric::Euclidean
    ));
}

#[test]
fn p9_parse_003_vector_function_paths_remain_independent_calls() {
    let Statement::Select(select) = parse_one(
        "SELECT vector::distance::euclidean(embedding, $query) AS l2, \
         vector::similarity::cosine(embedding, [1, 0]) AS similarity FROM item",
    )
    .unwrap() else {
        panic!("expected SELECT")
    };
    let ProjectionList::Fields(projections) = select.projections else {
        panic!("expected projections")
    };
    assert_eq!(projections.len(), 2);
    for projection in projections {
        assert!(matches!(
            projection.expression.kind,
            ExprKind::FunctionCall { name, arguments }
                if name.len() == 3 && arguments.len() == 2
        ));
    }
}

#[test]
fn p9_parse_004_invalid_dimensions_k_metrics_and_delimiters_fail() {
    for source in [
        "DEFINE FIELD v ON item TYPE array<float, 0>",
        "DEFINE FIELD v ON item TYPE array<float, 65537>",
        "DEFINE FIELD v ON item TYPE array<string, 2>",
        "SELECT * FROM item WHERE v <|0,COSINE|> [1]",
        "SELECT * FROM item WHERE v <|10001,COSINE|> [1]",
        "SELECT * FROM item WHERE v <|2,DOT|> [1]",
        "SELECT * FROM item WHERE v <|2,COSINE> [1]",
    ] {
        let error = parse_one(source).unwrap_err();
        assert!(error.span.is_within(source.len()), "{source}: {error}");
    }
}
