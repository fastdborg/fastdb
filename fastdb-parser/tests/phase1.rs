#![forbid(unsafe_code)]
#![deny(warnings)]

use miette::{Diagnostic, GraphicalReportHandler, GraphicalTheme};
use turso_fastdb_parser::{
    parse, parse_one, parse_with_limits, tokenize_with_limits, BinaryOperator, CreateData, Expr,
    ExprKind, LimitKind, ParseErrorKind, ParserLimits, ProjectionList, RecordIdPartKind,
    SchemaTypeKind, Statement, Target, UnaryOperator,
};

fn create_content(input: &str) -> Expr {
    let Statement::Create(statement) = parse_one(input).unwrap() else {
        panic!("expected CREATE")
    };
    let Some(CreateData::Content(expression)) = statement.data else {
        panic!("expected CONTENT")
    };
    expression
}

fn binary(expression: &Expr) -> (&Expr, BinaryOperator, &Expr) {
    let ExprKind::Binary {
        left,
        operator,
        right,
    } = &expression.kind
    else {
        panic!("expected binary expression, got {expression:?}")
    };
    (left, operator.value, right)
}

#[test]
fn p1_lex_001_comments_and_statement_boundaries() {
    let script = parse(
        "# first\nCREATE p:x SET n = 'a;/*data*/'; // line\n\
         /* block */ SELECT * FROM p:x; -- tail\nDELETE p:x",
    )
    .unwrap();
    assert_eq!(script.statements.len(), 3);
    assert!(matches!(script.statements[0], Statement::Create(_)));
    assert!(matches!(script.statements[1], Statement::Select(_)));
    assert!(matches!(script.statements[2], Statement::Delete(_)));
}

#[test]
fn p1_lex_002_strings_escapes_unicode_and_byte_spans() {
    let input = "CREATE café:tracy SET naïve = 'O\\'Brien', emoji = \"😺\\n\"";
    let Statement::Create(statement) = parse_one(input).unwrap() else {
        panic!("expected CREATE")
    };
    let Target::Record(record) = statement.target else {
        panic!("expected record")
    };
    assert_eq!(record.table.value, "café");
    assert_eq!(
        &input[record.table.span.offset..record.table.span.end()],
        "café"
    );
    let Some(CreateData::Set(assignments)) = statement.data else {
        panic!("expected SET")
    };
    assert_eq!(assignments.len(), 2);
    assert!(matches!(
        &assignments[0].value.kind,
        ExprKind::String(value) if value == "O'Brien"
    ));
    assert!(matches!(
        &assignments[1].value.kind,
        ExprKind::String(value) if value == "😺\n"
    ));
    assert!(statement.span.is_within(input.len()));
}

#[test]
fn p1_lex_003_keywords_are_case_insensitive_and_identifiers_preserve_case() {
    let Statement::Create(statement) =
        parse_one("cReAtE Person:Tracy sEt DisplayName = 'Tracy'").unwrap()
    else {
        panic!("expected CREATE")
    };
    let Target::Record(record) = statement.target else {
        panic!("expected record")
    };
    assert_eq!(record.table.value, "Person");
    assert!(matches!(record.id.kind, RecordIdPartKind::Bare(ref value) if value == "Tracy"));
    let Some(CreateData::Set(assignments)) = statement.data else {
        panic!("expected SET")
    };
    assert_eq!(assignments[0].path.segments[0].value, "DisplayName");
}

#[test]
fn p1_lex_004_typed_lexical_failures_have_small_spans() {
    let cases = [
        ("CREATE p CONTENT 'x", "fastdb::parse::unterminated_string"),
        (
            "CREATE p CONTENT /* x",
            "fastdb::parse::unterminated_comment",
        ),
        (
            "CREATE p:`x CONTENT null",
            "fastdb::parse::unterminated_quoted_identifier",
        ),
        ("CREATE p CONTENT '\\q'", "fastdb::parse::invalid_escape"),
        ("CREATE p CONTENT '\\/'", "fastdb::parse::invalid_escape"),
        ("CREATE p CONTENT 1e+", "fastdb::parse::invalid_number"),
        ("CREATE p CONTENT @", "fastdb::parse::unexpected_character"),
    ];
    for (input, code) in cases {
        let error = parse(input).unwrap_err();
        assert_eq!(error.code().unwrap().to_string(), code, "{input}");
        assert!(error.span.is_within(input.len()), "{input}: {error:?}");
    }
}

#[test]
fn p1_lex_005_nested_block_comments_are_not_silently_accepted() {
    let error = parse("BEGIN /* outer /* inner */ tail */").unwrap_err();
    assert!(matches!(error.kind, ParseErrorKind::UnexpectedToken { .. }));
    assert_eq!(error.span.offset, 27);
}

#[test]
fn p1_expr_001_all_value_forms_and_source_order() {
    let expression = create_content(
        "CREATE sample CONTENT [null, true, false, 1, 1.5, 'x', \"y\", [2,], \
         {a: 1, a: 2, 'quoted': $item,}, person:bare, profile.path]",
    );
    let ExprKind::Array(values) = expression.kind else {
        panic!("expected array")
    };
    assert_eq!(values.len(), 11);
    assert!(matches!(values[0].kind, ExprKind::Null));
    assert!(matches!(values[1].kind, ExprKind::Bool(true)));
    assert!(matches!(values[3].kind, ExprKind::Integer(1)));
    assert!(matches!(values[4].kind, ExprKind::Float(value) if value == 1.5));
    let ExprKind::Object(fields) = &values[8].kind else {
        panic!("expected object")
    };
    assert_eq!(fields.len(), 3, "duplicate keys remain in source order");
    assert!(matches!(values[9].kind, ExprKind::RecordId(_)));
    assert!(matches!(values[10].kind, ExprKind::FieldPath(_)));
}

#[test]
fn p1_expr_002_record_id_component_types() {
    let script = parse(
        "CREATE person:bare CONTENT null;\
         CREATE person:`quoted id 😺` CONTENT null;\
         CREATE person:-9223372036854775808 CONTENT null;\
         CREATE person:+9223372036854775807 CONTENT null",
    )
    .unwrap();
    let ids: Vec<_> = script
        .statements
        .into_iter()
        .map(|statement| {
            let Statement::Create(statement) = statement else {
                panic!("expected CREATE")
            };
            let Target::Record(record) = statement.target else {
                panic!("expected record")
            };
            record.id.kind
        })
        .collect();
    assert!(matches!(&ids[0], RecordIdPartKind::Bare(value) if value == "bare"));
    assert!(matches!(&ids[1], RecordIdPartKind::Quoted(value) if value == "quoted id 😺"));
    assert!(matches!(ids[2], RecordIdPartKind::Integer(i64::MIN)));
    assert!(matches!(ids[3], RecordIdPartKind::Integer(i64::MAX)));

    let quoted_string = parse("CREATE person:'not an id' CONTENT null").unwrap_err();
    assert!(matches!(
        quoted_string.kind,
        ParseErrorKind::InvalidCombination { .. }
    ));
}

#[test]
fn p2_uuid_001_typed_uuid_components_validate_spans_and_source() {
    let source = "SELECT * FROM person:u'018f22e2-79b0-7cc3-98c4-dc0c0c07398f';";
    let statement = parse_one(source).unwrap();
    let Statement::Select(select) = statement else {
        panic!("expected SELECT");
    };
    let Target::Record(record) = select.target else {
        panic!("expected record target");
    };
    assert_eq!(record.id.span.offset, source.find("u'").unwrap());
    assert_eq!(record.id.span.len, 39);
    assert_eq!(
        record.id.to_source(),
        "u'018f22e2-79b0-7cc3-98c4-dc0c0c07398f'"
    );
    assert!(matches!(record.id.kind, RecordIdPartKind::Uuid(uuid) if uuid.get_version_num() == 7));

    let v4 = parse_one("DELETE person:u\"550e8400-e29b-41d4-a716-446655440000\"").unwrap();
    let Statement::Delete(delete) = v4 else {
        panic!("expected DELETE");
    };
    let Target::Record(record) = delete.target else {
        panic!("expected record target");
    };
    assert!(matches!(record.id.kind, RecordIdPartKind::Uuid(uuid) if uuid.get_version_num() == 4));
}

#[test]
fn p2_uuid_002_rejects_noncanonical_wrong_version_and_separated_prefix() {
    for source in [
        "SELECT * FROM person:u'550E8400-E29B-41D4-A716-446655440000'",
        "SELECT * FROM person:u'550e8400e29b41d4a716446655440000'",
        "SELECT * FROM person:u'6ba7b810-9dad-11d1-80b4-00c04fd430c8'",
        "SELECT * FROM person:u'not-a-uuid'",
    ] {
        let error = parse_one(source).unwrap_err();
        assert!(
            matches!(error.kind, ParseErrorKind::InvalidUuid { .. }),
            "{source}: {error:?}"
        );
        assert_eq!(
            &source[error.span.offset..error.span.end()],
            &source[source.find("u'").unwrap()..]
        );
    }

    let error =
        parse_one("SELECT * FROM person:u '550e8400-e29b-41d4-a716-446655440000'").unwrap_err();
    assert!(matches!(error.kind, ParseErrorKind::UnexpectedToken { .. }));
}

#[test]
fn p1_expr_003_precedence_and_left_associativity() {
    let expression = create_content("CREATE p CONTENT a OR b AND c = d + e * f");
    let (_, root, right) = binary(&expression);
    assert_eq!(root, BinaryOperator::Or);
    let (_, and, equality) = binary(right);
    assert_eq!(and, BinaryOperator::And);
    let (_, equal, addition) = binary(equality);
    assert_eq!(equal, BinaryOperator::Equal);
    let (_, add, multiplication) = binary(addition);
    assert_eq!(add, BinaryOperator::Add);
    assert_eq!(binary(multiplication).1, BinaryOperator::Multiply);

    let expression = create_content("CREATE p CONTENT 10 - 3 - 2");
    let (left, root, _) = binary(&expression);
    assert_eq!(root, BinaryOperator::Subtract);
    assert_eq!(binary(left).1, BinaryOperator::Subtract);
}

#[test]
fn p1_expr_004_unary_and_parentheses_are_preserved() {
    let expression = create_content("CREATE p CONTENT NOT -(+item)");
    let ExprKind::Unary { operator, operand } = expression.kind else {
        panic!("expected NOT")
    };
    assert_eq!(operator.value, UnaryOperator::Not);
    let ExprKind::Unary { operator, operand } = operand.kind else {
        panic!("expected minus")
    };
    assert_eq!(operator.value, UnaryOperator::Minus);
    assert!(matches!(operand.kind, ExprKind::Parenthesized(_)));
}

#[test]
fn p1_expr_005_invalid_and_excluded_expressions_fail_explicitly() {
    let input = "CREATE p CONTENT person->friend";
    let error = parse(input).unwrap_err();
    assert!(
        matches!(error.kind, ParseErrorKind::UnsupportedSyntax { .. }),
        "{input}: {error:?}"
    );
    for input in [
        "CREATE p CONTENT 9223372036854775808",
        "CREATE p CONTENT 1e309",
        "CREATE p CONTENT .5",
    ] {
        assert!(parse(input).is_err(), "{input}");
    }
}

#[test]
fn p1_stmt_001_all_exact_statement_shapes() {
    let input = "\
        CREATE ONLY person CONTENT {name: 'Tracy'} RETURN AFTER;\
        CREATE person:tracy SET name = 'Tracy', active = true RETURN BEFORE;\
        SELECT name AS display, address.city FROM ONLY person WHERE active = true \
          ORDER BY name DESC, address.city ASC LIMIT 10 START 2;\
        UPDATE person:tracy SET name = 'Trace' WHERE active = true RETURN NONE;\
        DELETE person WHERE active = false RETURN BEFORE;\
        DEFINE TABLE person SCHEMAFULL;\
        DEFINE FIELD address.city ON TABLE person TYPE option<option<string>>;\
        DEFINE INDEX by_name ON TABLE person FIELDS name, address.city UNIQUE;\
        BEGIN; COMMIT; CANCEL";
    let script = parse(input).unwrap();
    assert_eq!(script.statements.len(), 11);
    assert!(matches!(script.statements[0], Statement::Create(_)));
    assert!(matches!(script.statements[2], Statement::Select(_)));
    assert!(matches!(script.statements[3], Statement::Update(_)));
    assert!(matches!(script.statements[4], Statement::Delete(_)));
    assert!(matches!(script.statements[5], Statement::DefineTable(_)));
    let Statement::DefineField(field) = &script.statements[6] else {
        panic!("expected DEFINE FIELD")
    };
    assert!(matches!(field.ty.kind, SchemaTypeKind::Option(_)));
    assert!(matches!(script.statements[7], Statement::DefineIndex(_)));
    assert!(matches!(script.statements[8], Statement::Begin(_)));
    assert!(matches!(script.statements[9], Statement::Commit(_)));
    assert!(matches!(script.statements[10], Statement::Cancel(_)));
    assert!(script.span.is_within(input.len()));
}

#[test]
fn p1_stmt_002_projection_and_order_defaults_are_structural() {
    let Statement::Select(statement) = parse_one("SELECT name FROM person ORDER BY name").unwrap()
    else {
        panic!("expected SELECT")
    };
    assert!(matches!(statement.projections, ProjectionList::Fields(_)));
    assert_eq!(statement.order_by.len(), 1);
    assert_eq!(statement.order_by[0].direction.span.len, 0);
}

#[test]
fn p1_stmt_003_script_separator_rules() {
    assert_eq!(parse("BEGIN;COMMIT;").unwrap().statements.len(), 2);
    assert!(matches!(
        parse_one("BEGIN; COMMIT").unwrap_err().kind,
        ParseErrorKind::MultipleStatements { count: 2 }
    ));
    assert!(matches!(
        parse("BEGIN COMMIT").unwrap_err().kind,
        ParseErrorKind::MissingStatementSeparator
    ));
    assert!(matches!(
        parse(";BEGIN").unwrap_err().kind,
        ParseErrorKind::EmptyStatement
    ));
    assert!(matches!(
        parse("BEGIN;;COMMIT").unwrap_err().kind,
        ParseErrorKind::EmptyStatement
    ));
    assert!(matches!(
        parse("/* comment */").unwrap_err().kind,
        ParseErrorKind::EmptyInput
    ));
}

#[test]
fn p1_stmt_004_clause_order_duplicates_and_combinations() {
    let cases = [
        "SELECT * FROM person LIMIT 1 WHERE active = true",
        "SELECT * FROM person WHERE a = 1 WHERE b = 2",
        "SELECT * FROM person LIMIT 1 LIMIT 2",
        "SELECT *, name FROM person",
        "SELECT name, * FROM person",
        "CREATE person CONTENT {} SET name = 'x'",
        "CREATE person SET name = 'x' CONTENT {}",
        "UPDATE person SET a = 1 RETURN NONE WHERE b = 2",
        "DELETE person RETURN BEFORE WHERE b = 2",
    ];
    for input in cases {
        let error = parse(input).unwrap_err();
        assert!(
            matches!(
                error.kind,
                ParseErrorKind::DuplicateClause { .. }
                    | ParseErrorKind::ClauseOrder { .. }
                    | ParseErrorKind::InvalidCombination { .. }
            ),
            "{input}: {error:?}"
        );
    }
}

#[test]
fn p1_stmt_005_unsupported_families_and_clauses_are_precise() {
    for input in [
        "LET $x = 1",
        "SELECT * FROM person FETCH friend",
        "SELECT * FROM person GROUP BY name",
        "CREATE person CONTENT {} TIMEOUT 1s",
        "CREATE person CONTENT {} PARALLEL",
        "DEFINE INDEX x ON person FIELDS name FULLTEXT",
        "BEGIN TRANSACTION",
    ] {
        let error = parse(input).unwrap_err();
        assert!(
            matches!(error.kind, ParseErrorKind::UnsupportedSyntax { .. }),
            "{input}: {error:?}"
        );
    }
}

#[test]
fn p1_stmt_006_limit_start_and_return_ranges() {
    assert!(parse("SELECT * FROM p LIMIT 0 START 9223372036854775807").is_ok());
    let Statement::Select(select) = parse_one("SELECT * FROM p LIMIT +1").unwrap() else {
        panic!("expected SELECT")
    };
    let limit = select.limit.unwrap();
    assert_eq!(limit.value, 1);
    assert_eq!(limit.span.len, 2);
    for input in [
        "SELECT * FROM p LIMIT -1",
        "SELECT * FROM p START 1.5",
        "SELECT * FROM p LIMIT 9223372036854775808",
        "CREATE p CONTENT {} RETURN VALUE",
    ] {
        assert!(parse(input).is_err(), "{input}");
    }
}

#[test]
fn p1_limit_001_input_bytes_below_at_and_above_default() {
    let limits = ParserLimits::default();
    for length in [limits.max_input_bytes - 1, limits.max_input_bytes] {
        let input = format!("#{}", "x".repeat(length - 1));
        assert!(tokenize_with_limits(&input, &limits).is_ok());
    }
    let input = format!("#{}", "x".repeat(limits.max_input_bytes));
    let error = tokenize_with_limits(&input, &limits).unwrap_err();
    assert!(matches!(
        error.kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::InputBytes,
            ..
        }
    ));
}

#[test]
fn p1_limit_002_tokens_below_at_and_above_default() {
    let limits = ParserLimits::default();
    for count in [limits.max_tokens - 1, limits.max_tokens] {
        let input = "x ".repeat(count);
        assert_eq!(
            tokenize_with_limits(&input, &limits).unwrap().len(),
            count + 1
        );
    }
    let input = "x ".repeat(limits.max_tokens + 1);
    let error = tokenize_with_limits(&input, &limits).unwrap_err();
    assert!(matches!(
        error.kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::Tokens,
            ..
        }
    ));
}

#[test]
fn p1_limit_003_nesting_below_at_and_above_default() {
    let limits = ParserLimits::default();
    for depth in [limits.max_nesting_depth - 1, limits.max_nesting_depth] {
        let input = format!(
            "CREATE p CONTENT {}true{}",
            "(".repeat(depth),
            ")".repeat(depth)
        );
        assert!(parse_with_limits(&input, &limits).is_ok(), "depth {depth}");
    }
    let depth = limits.max_nesting_depth + 1;
    let input = format!(
        "CREATE p CONTENT {}true{}",
        "(".repeat(depth),
        ")".repeat(depth)
    );
    let error = parse_with_limits(&input, &limits).unwrap_err();
    assert!(matches!(
        error.kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::NestingDepth,
            ..
        }
    ));
}

#[test]
fn p1_limit_004_collection_below_at_and_above_default() {
    let limits = ParserLimits::default();
    for count in [
        limits.max_collection_elements - 1,
        limits.max_collection_elements,
    ] {
        let input = format!("CREATE p CONTENT [{}]", vec!["0"; count].join(","));
        assert!(parse_with_limits(&input, &limits).is_ok(), "count {count}");
    }
    let count = limits.max_collection_elements + 1;
    let input = format!("CREATE p CONTENT [{}]", vec!["0"; count].join(","));
    let error = parse_with_limits(&input, &limits).unwrap_err();
    assert!(matches!(
        error.kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::CollectionElements,
            ..
        }
    ));
}

#[test]
fn p1_limit_005_statements_below_at_and_above_default() {
    let limits = ParserLimits::default();
    for count in [limits.max_statements - 1, limits.max_statements] {
        assert_eq!(
            parse_with_limits(&"BEGIN;".repeat(count), &limits)
                .unwrap()
                .statements
                .len(),
            count
        );
    }
    let error =
        parse_with_limits(&"BEGIN;".repeat(limits.max_statements + 1), &limits).unwrap_err();
    assert!(matches!(
        error.kind,
        ParseErrorKind::LimitExceeded {
            kind: LimitKind::Statements,
            ..
        }
    ));
}

#[test]
fn p1_limit_006_identifier_and_parameter_below_at_and_above_default() {
    let limits = ParserLimits::default();
    for length in [limits.max_identifier_bytes - 1, limits.max_identifier_bytes] {
        let name = "a".repeat(length);
        assert!(parse_with_limits(&format!("CREATE {name} CONTENT null"), &limits).is_ok());
        assert!(parse_with_limits(&format!("CREATE p CONTENT ${name}"), &limits).is_ok());
    }
    let name = "a".repeat(limits.max_identifier_bytes + 1);
    for input in [
        format!("CREATE {name} CONTENT null"),
        format!("CREATE p CONTENT ${name}"),
    ] {
        let error = parse_with_limits(&input, &limits).unwrap_err();
        assert!(matches!(
            error.kind,
            ParseErrorKind::LimitExceeded {
                kind: LimitKind::IdentifierBytes,
                ..
            }
        ));
    }
}

#[test]
fn p1_diag_001_code_message_label_span_and_rendering_are_stable() {
    let input = "SELECT * FROM person LIMIT -1";
    let error = parse(input).unwrap_err();
    assert_eq!(
        error.code().unwrap().to_string(),
        "fastdb::parse::invalid_number"
    );
    assert_eq!(
        error.to_string(),
        "invalid number \"negative integer\": LIMIT and START require a nonnegative integer"
    );
    assert_eq!(&input[error.span.offset..error.span.end()], "-");
    let labels: Vec<_> = error.labels().unwrap().collect();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].label(), Some("invalid number"));

    let report = miette::Report::new(error).with_source_code(input.to_string());
    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
        .with_width(60)
        .with_links(false)
        .with_urls(false);
    let mut rendered = String::new();
    handler
        .render_report(&mut rendered, report.as_ref())
        .unwrap();
    assert_eq!(
        rendered,
        "fastdb::parse::invalid_number\n\n  × invalid number \"negative integer\": LIMIT and START\n  │ require a nonnegative integer\n   ╭────\n 1 │ SELECT * FROM person LIMIT -1\n   ·                            ┬\n   ·                            ╰── invalid number\n   ╰────\n"
    );
}

#[test]
fn p1_fuzz_001_arbitrary_utf8_smoke_never_panics() {
    let samples = [
        String::new(),
        "\0".to_string(),
        "😺".repeat(200),
        "((((((((".to_string(),
        "/*".to_string(),
        "'\\uD800'".to_string(),
        "CREATE p CONTENT [{a:[{b:null}]}]".to_string(),
    ];
    for sample in samples {
        let result = std::panic::catch_unwind(|| parse(&sample));
        assert!(result.is_ok(), "parser panicked for {sample:?}");
        if let Ok(script) = result.unwrap() {
            assert!(script.span.is_within(sample.len()));
            let debug = format!("{script:?}");
            assert!(!debug.is_empty());
        }
    }
}

#[test]
fn p1_bridge_004_phase0_parser_shapes_and_spans_regress() {
    let create_input = "CREATE person:tracy SET name = 'Tracy';";
    let Statement::Create(create) = parse_one(create_input).unwrap() else {
        panic!("expected CREATE")
    };
    let Target::Record(record) = create.target else {
        panic!("expected record target")
    };
    assert_eq!(record.table.value, "person");
    assert_eq!(record.table.span, turso_fastdb_parser::Span::new(7, 6));
    assert!(matches!(record.id.kind, RecordIdPartKind::Bare(ref id) if id == "tracy"));
    let Some(CreateData::Set(assignments)) = create.data else {
        panic!("expected SET")
    };
    assert_eq!(assignments[0].path.segments[0].value, "name");
    assert!(matches!(&assignments[0].value.kind, ExprKind::String(value) if value == "Tracy"));

    for input in [
        "SELECT * FROM person:tracy",
        "SELECT * FROM person WHERE name = 'Tracy'",
        "DELETE person:tracy",
    ] {
        assert!(parse_one(input).is_ok(), "{input}");
        assert!(parse_one(&format!("{input};")).is_ok(), "{input};");
    }
}

#[test]
fn p1_bridge_005_phase0_legacy_string_escape_remains_parseable() {
    let expression = create_content(r"CREATE p:x CONTENT 'O''Brien\\n'");
    assert!(matches!(expression.kind, ExprKind::String(ref value) if value == "O'Brien\\n"));

    let expression = create_content("CREATE p:x CONTENT 'a;''b'");
    assert!(matches!(expression.kind, ExprKind::String(ref value) if value == "a;'b"));
}

#[test]
fn p1_diag_002_phase0_malformed_inputs_still_fail_without_panics() {
    for input in [
        "CREATE SET n = 'v'",
        "CREATE p:x SET = 'v'",
        "CREATE p:x SET n =",
        "SELECT * person",
        "DELETE p:x extra",
        "CREATE p:'quoted' SET n = 'v'",
        "\0",
        "\x01\x02\x03",
        ":::::",
        "'''",
        "SELECT * * FROM",
        "🦀🦀🦀",
        "CREATE p:x SET n = '",
        "\n\n\n;;;",
    ] {
        let result = std::panic::catch_unwind(|| parse(input));
        assert!(result.is_ok(), "parser panicked for {input:?}");
        assert!(result.unwrap().is_err(), "unexpectedly accepted {input:?}");
    }
}
