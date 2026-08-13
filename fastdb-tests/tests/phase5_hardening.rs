#![forbid(unsafe_code)]
#![deny(warnings)]

use std::collections::BTreeMap;
use turso_fastdb::{Database, ErrorCategory, Params, RecordId, StatementResult, Value};
use turso_fastdb_parser::{parse_with_limits, LimitKind, ParseErrorKind, ParserLimits};

#[test]
fn p5_resource_001_every_parser_ceiling_and_malformed_class_is_bounded() {
    let cases = [
        (
            "x".repeat(9),
            ParserLimits {
                max_input_bytes: 8,
                ..ParserLimits::default()
            },
            LimitKind::InputBytes,
        ),
        (
            "SELECT * FROM item WHERE a=1 AND b=2".into(),
            ParserLimits {
                max_tokens: 4,
                ..ParserLimits::default()
            },
            LimitKind::Tokens,
        ),
        (
            "SELECT * FROM item; SELECT * FROM item".into(),
            ParserLimits {
                max_statements: 1,
                ..ParserLimits::default()
            },
            LimitKind::Statements,
        ),
        (
            "SELECT * FROM longname".into(),
            ParserLimits {
                max_identifier_bytes: 4,
                ..ParserLimits::default()
            },
            LimitKind::IdentifierBytes,
        ),
        (
            "CREATE item:a CONTENT [1,2]".into(),
            ParserLimits {
                max_collection_elements: 1,
                ..ParserLimits::default()
            },
            LimitKind::CollectionElements,
        ),
        (
            "CREATE item:a CONTENT [[1]]".into(),
            ParserLimits {
                max_nesting_depth: 1,
                ..ParserLimits::default()
            },
            LimitKind::NestingDepth,
        ),
    ];
    for (source, limits, expected) in cases {
        let error = parse_with_limits(&source, &limits).unwrap_err();
        assert!(
            matches!(error.kind, ParseErrorKind::LimitExceeded { kind, .. } if kind == expected)
        );
        assert!(error.span.end() <= source.len());
    }

    for source in [
        "\0",
        "'unterminated",
        "/* unterminated",
        "SELECT (((((((",
        "CREATE :",
        "UPDATE x SET =",
        "DELETE FROM",
        "💥",
        "SELECT * FROM `bad",
    ] {
        let result = std::panic::catch_unwind(|| turso_fastdb_parser::parse(source));
        assert!(result.is_ok(), "parser panicked for {source:?}");
        if let Err(error) = result.unwrap() {
            assert!(error.span.end() <= source.len(), "{error:?}");
        }
    }
}

#[test]
fn p5_inject_001_recursive_params_ids_paths_and_source_payloads_stay_data() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    let payload = "x'; DELETE item; DEFINE TABLE owned SCHEMALESS; --";
    let mut nested = Value::Str(payload.into());
    for _ in 0..16 {
        nested = Value::Array(vec![Value::Object(BTreeMap::from([(
            "$fastdb".into(),
            nested,
        )]))]);
    }
    let mut params = Params::new();
    params.insert("payload".into(), Value::Str(payload.into()));
    params.insert("nested".into(), nested.clone());
    params.insert(
        "record".into(),
        Value::RecordId(RecordId::new("external", "quoted:component")),
    );
    connection
        .execute_with_params(
            "CREATE item:`odd.id[]` CONTENT { text:$payload, nested:$nested, link:$record }",
            &params,
        )
        .unwrap();
    let result = connection
        .execute("SELECT text, nested, link FROM item:`odd.id[]`")
        .unwrap();
    let StatementResult::Rows(rows) = &result.statements[0] else {
        panic!("expected rows")
    };
    let Value::Object(row) = &rows[0] else {
        panic!("expected object")
    };
    assert_eq!(row["text"], Value::Str(payload.into()));
    assert_eq!(row["nested"], nested);
    assert_eq!(row["link"], params["record"]);
    assert_eq!(
        connection
            .execute("SELECT * FROM owned")
            .unwrap()
            .statements[0],
        StatementResult::Rows(vec![])
    );

    let mut too_deep = Value::Null;
    for _ in 0..66 {
        too_deep = Value::Array(vec![too_deep]);
    }
    let mut invalid = Params::new();
    invalid.insert("deep".into(), too_deep);
    assert_eq!(
        connection
            .execute_with_params("SELECT * FROM item WHERE nested=$deep", &invalid)
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
}
