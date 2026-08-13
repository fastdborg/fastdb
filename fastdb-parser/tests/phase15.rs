use turso_fastdb_parser::{parse, parse_one, ExprKind, Statement, StatementCursor};

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

#[test]
fn p15_parse_004_parameter_lifecycle_and_database_info_are_structured() {
    let script = parse(
        "DEFINE PARAM IF NOT EXISTS $answer VALUE 42 PERMISSIONS FULL; \
         DEFINE PARAM OVERWRITE $answer VALUE 43 PERMISSIONS NONE; \
         ALTER PARAM $answer VALUE 44 PERMISSIONS FULL; \
         REMOVE PARAM IF EXISTS $answer; INFO FOR DB",
    )
    .unwrap();
    let Statement::DefineParam(first) = &script.statements[0] else {
        panic!("expected DEFINE PARAM")
    };
    assert!(first.if_not_exists.is_some());
    assert!(first.overwrite.is_none());
    assert_eq!(first.name.value, "answer");
    assert!(matches!(first.value.kind, ExprKind::Integer(42)));
    assert_eq!(
        first.permissions,
        turso_fastdb_parser::SchemaPermissions::Full
    );

    let Statement::DefineParam(second) = &script.statements[1] else {
        panic!("expected DEFINE PARAM OVERWRITE")
    };
    assert!(second.if_not_exists.is_none());
    assert!(second.overwrite.is_some());
    assert_eq!(
        second.permissions,
        turso_fastdb_parser::SchemaPermissions::None
    );
    let Statement::AlterParam(alter) = &script.statements[2] else {
        panic!("expected ALTER PARAM")
    };
    assert!(alter.value.is_some());
    assert_eq!(
        alter.permissions,
        Some(turso_fastdb_parser::SchemaPermissions::Full)
    );
    assert!(matches!(script.statements[3], Statement::RemoveParam(_)));
    assert!(matches!(script.statements[4], Statement::InfoDatabase(_)));

    for source in [
        "DEFINE PARAM IF NOT EXISTS OVERWRITE $x VALUE 1",
        "DEFINE PARAM $x 1",
        "DEFINE PARAM $x VALUE 1 PERMISSIONS WHERE",
        "REMOVE PARAM IF $x",
        "ALTER PARAM $x",
        "ALTER TABLE thing",
        "INFO FOR TABLE",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}

#[test]
fn p15_parse_005_function_lifecycle_is_structured_and_typed() {
    let script = parse(
        "DEFINE FUNCTION IF NOT EXISTS fn::math::double($x: int) { RETURN $x * 2; } \
         PERMISSIONS FULL; ALTER FUNCTION fn::math::double PERMISSIONS NONE; \
         REMOVE FUNCTION IF EXISTS fn::math::double",
    )
    .unwrap();
    let Statement::DefineFunction(define) = &script.statements[0] else {
        panic!("expected DEFINE FUNCTION")
    };
    assert!(define.if_not_exists.is_some());
    assert_eq!(define.name[1].value, "math");
    assert_eq!(define.name[2].value, "double");
    assert_eq!(define.arguments.len(), 1);
    assert_eq!(define.arguments[0].name.value, "x");
    assert_eq!(define.body.statements.len(), 1);
    assert!(matches!(script.statements[1], Statement::AlterFunction(_)));
    assert!(matches!(script.statements[2], Statement::RemoveFunction(_)));

    for source in [
        "DEFINE FUNCTION double($x: int) { RETURN $x; }",
        "DEFINE FUNCTION fn::double($x) { RETURN $x; }",
        "DEFINE FUNCTION fn::double($x: int, $x: int) { RETURN $x; }",
        "DEFINE FUNCTION IF NOT EXISTS OVERWRITE fn::f() { RETURN 1; }",
        "ALTER FUNCTION fn::f",
        "REMOVE FUNCTION IF fn::f",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}
