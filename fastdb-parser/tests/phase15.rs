use turso_fastdb_parser::{parse, parse_one, Statement, StatementCursor};

#[test]
fn p15_parse_001_script_control_flow_is_structured() {
    let script = parse(
        "LET $seed = 1; \
         IF $seed = 1 { LET $value = 2; RETURN $value; } \
         ELSE IF false { THROW 'unreachable'; } \
         ELSE { SLEEP 1ms; RETURN 0; }; \
         FOR $item IN [1, 2, 3] { \
           IF $item = 2 { CONTINUE; }; \
           IF $item = 3 { BREAK; }; \
           CREATE item SET n = $item RETURN NONE; \
         }",
    )
    .unwrap();

    assert!(matches!(script.statements[0], Statement::Let(_)));
    let Statement::If(statement) = &script.statements[1] else {
        panic!("expected IF")
    };
    assert_eq!(statement.branches.len(), 2);
    assert!(statement.otherwise.is_some());
    let Statement::For(statement) = &script.statements[2] else {
        panic!("expected FOR")
    };
    assert_eq!(statement.binding.value, "item");
    assert_eq!(statement.body.statements.len(), 3);
}

#[test]
fn p15_parse_002_statement_cursor_keeps_block_semicolons_nested() {
    let source = "IF true { LET $x = { semi: ';' }; RETURN $x; }; SELECT * FROM item";
    let mut cursor = StatementCursor::new(source);
    assert!(matches!(
        cursor.next_statement().unwrap(),
        Some(Statement::If(_))
    ));
    assert!(matches!(
        cursor.next_statement().unwrap(),
        Some(Statement::Select(_))
    ));
    assert!(cursor.next_statement().unwrap().is_none());
}

#[test]
fn p15_parse_003_invalid_control_flow_and_blocks_fail_explicitly() {
    for source in [
        "LET value = 1",
        "LET $value 1",
        "IF true RETURN 1",
        "IF true { RETURN 1; ELSE { RETURN 2; }",
        "FOR item IN [1] { RETURN item; }",
        "FOR $item [1] { RETURN $item; }",
        "RETURN",
        "THROW",
        "SLEEP",
    ] {
        assert!(parse_one(source).is_err(), "{source}");
    }
}
