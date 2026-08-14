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

#[test]
fn p15_parse_006_table_lifecycle_metadata_is_structured() {
    let script = parse(
        "DEFINE TABLE IF NOT EXISTS item DROP SCHEMAFULL TYPE NORMAL \
         PERMISSIONS FULL COMMENT 'items'; \
         DEFINE TABLE OVERWRITE item TYPE NORMAL SCHEMALESS PERMISSIONS NONE; \
         ALTER TABLE IF EXISTS item SCHEMAFULL PERMISSIONS FULL COMMENT 'changed'; \
         ALTER TABLE item DROP COMMENT; INFO FOR TABLE item; \
         REMOVE TABLE IF EXISTS item",
    )
    .unwrap();
    let Statement::DefineTable(define) = &script.statements[0] else {
        panic!("expected DEFINE TABLE")
    };
    assert!(define.if_not_exists.is_some());
    assert!(define.drop.is_some());
    assert_eq!(
        define.mode.value,
        turso_fastdb_parser::TableMode::Schemafull
    );
    assert_eq!(define.comment.as_ref().unwrap().value, "items");
    assert_eq!(
        define.permissions,
        turso_fastdb_parser::SchemaPermissions::Full
    );
    let Statement::AlterTable(alter) = &script.statements[2] else {
        panic!("expected ALTER TABLE")
    };
    assert!(alter.if_exists.is_some());
    assert!(alter.mode.is_some());
    assert!(matches!(
        alter.comment,
        turso_fastdb_parser::TableCommentChange::Set(ref value) if value == "changed"
    ));
    assert!(matches!(script.statements[4], Statement::InfoTable(_)));
    assert!(matches!(script.statements[5], Statement::RemoveTable(_)));

    for source in [
        "DEFINE TABLE IF NOT EXISTS OVERWRITE item",
        "DEFINE TABLE item TYPE ANY",
        "DEFINE TABLE item CHANGEFEED 1h",
        "ALTER TABLE item COMPACT",
        "ALTER TABLE item DROP CHANGEFEED",
        "ALTER TABLE item COMMENT 1",
        "REMOVE TABLE IF item",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}

#[test]
fn p15_parse_007_field_clauses_and_lifecycle_are_structured() {
    let script = parse(
        "DEFINE FIELD IF NOT EXISTS score ON TABLE item TYPE int \
           DEFAULT ALWAYS 1 READONLY VALUE $value ASSERT $value >= 0 \
           PERMISSIONS FULL COMMENT 'score'; \
         DEFINE FIELD OVERWRITE anything ON item COMMENT 'untyped'; \
         ALTER FIELD IF EXISTS score ON TABLE item DROP READONLY; \
         ALTER FIELD score ON item DEFAULT 2; \
         REMOVE FIELD IF EXISTS score ON TABLE item",
    )
    .unwrap();
    let Statement::DefineField(field) = &script.statements[0] else {
        panic!("expected DEFINE FIELD")
    };
    assert!(field.if_not_exists.is_some());
    assert!(field.default.as_ref().unwrap().always.is_some());
    assert!(field.readonly.is_some());
    assert!(field.value.is_some());
    assert!(field.assert.is_some());
    assert!(field.comment.is_some());
    let Statement::DefineField(anything) = &script.statements[1] else {
        panic!("expected untyped DEFINE FIELD")
    };
    assert!(matches!(
        anything.ty.kind,
        turso_fastdb_parser::SchemaTypeKind::Any
    ));
    assert!(matches!(script.statements[2], Statement::AlterField(_)));
    assert!(matches!(script.statements[4], Statement::RemoveField(_)));

    for source in [
        "DEFINE FIELD IF NOT EXISTS OVERWRITE score ON item TYPE int",
        "DEFINE FIELD score ON item DEFAULT",
        "DEFINE FIELD score ON item REFERENCE ON DELETE CASCADE",
        "ALTER FIELD score ON item",
        "ALTER FIELD score ON item DROP",
        "REMOVE FIELD score item",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}

#[test]
fn p15_parse_011_union_literal_typed_record_and_flexible_types_are_structured() {
    let script = parse(
        "DEFINE FIELD state ON item TYPE 'open' | 'closed' | none; \
         DEFINE FIELD payload ON item TYPE object FLEXIBLE; \
         DEFINE FIELD owner ON item TYPE record<person | company>; \
         DEFINE FIELD values ON item TYPE array<int | string>; \
         ALTER FIELD payload ON item FLEXIBLE; \
         ALTER FIELD payload ON item DROP FLEXIBLE",
    )
    .unwrap();

    let Statement::DefineField(state) = &script.statements[0] else {
        panic!("expected literal union field")
    };
    assert!(matches!(
        &state.ty.kind,
        turso_fastdb_parser::SchemaTypeKind::Union(variants) if variants.len() == 3
    ));
    let Statement::DefineField(payload) = &script.statements[1] else {
        panic!("expected flexible object field")
    };
    assert!(payload.flexible.is_some());
    let Statement::DefineField(owner) = &script.statements[2] else {
        panic!("expected typed record field")
    };
    assert!(matches!(
        &owner.ty.kind,
        turso_fastdb_parser::SchemaTypeKind::Record { tables }
            if tables.iter().map(|table| table.value.as_str()).eq(["person", "company"])
    ));
    let Statement::DefineField(values) = &script.statements[3] else {
        panic!("expected typed array field")
    };
    assert!(matches!(
        &values.ty.kind,
        turso_fastdb_parser::SchemaTypeKind::TypedArray { element, .. }
            if matches!(&element.kind, turso_fastdb_parser::SchemaTypeKind::Union(variants) if variants.len() == 2)
    ));
    assert!(matches!(
        script.statements[4],
        Statement::AlterField(turso_fastdb_parser::AlterFieldStatement {
            change: turso_fastdb_parser::AlterFieldChange::Flexible,
            ..
        })
    ));
    assert!(matches!(
        script.statements[5],
        Statement::AlterField(turso_fastdb_parser::AlterFieldStatement {
            change: turso_fastdb_parser::AlterFieldChange::DropFlexible,
            ..
        })
    ));
}

#[test]
fn p15_parse_012_conditional_schema_permissions_are_structured() {
    let script = parse(
        "DEFINE TABLE item TYPE NORMAL PERMISSIONS \
           FOR select WHERE $auth != NONE, \
           FOR create, update WHERE $value != NONE, FOR delete NONE; \
         DEFINE FIELD score ON item TYPE int PERMISSIONS \
           FOR select WHERE $value > 0, FOR create, update WHERE $value < 10; \
         ALTER TABLE item PERMISSIONS FOR select FULL, FOR create, update NONE; \
         ALTER FIELD score ON item PERMISSIONS FOR select NONE, FOR create, update FULL",
    )
    .unwrap();

    let Statement::DefineTable(table) = &script.statements[0] else {
        panic!("expected table definition")
    };
    assert_eq!(
        table.permissions.to_source(),
        "FOR select WHERE $auth != NONE, FOR create, update WHERE $value != NONE, FOR delete NONE"
    );
    let Statement::DefineField(field) = &script.statements[1] else {
        panic!("expected field definition")
    };
    assert_eq!(
        field.permissions.to_source(),
        "FOR select WHERE $value > 0, FOR create, update WHERE $value < 10"
    );
    assert!(matches!(script.statements[2], Statement::AlterTable(_)));
    assert!(matches!(script.statements[3], Statement::AlterField(_)));

    for source in [
        "DEFINE TABLE item PERMISSIONS FOR select FULL, FOR select NONE",
        "DEFINE FIELD x ON item PERMISSIONS FOR delete NONE",
        "DEFINE FIELD x ON item PERMISSIONS FOR select, delete NONE",
        "DEFINE PARAM $x VALUE 1 PERMISSIONS FOR select FULL",
        "DEFINE FUNCTION fn::x() { RETURN 1; } PERMISSIONS FOR select FULL",
    ] {
        assert!(parse(source).is_err(), "{source}");
    }
}

#[test]
fn p15_parse_008_stopped_sequence_module_and_server_api_fail_explicitly() {
    for source in [
        "DEFINE SEQUENCE ids BATCH 1 START 0 TIMEOUT 1s",
        "ALTER SEQUENCE ids TIMEOUT 2s",
        "REMOVE SEQUENCE IF EXISTS ids",
        "INFO FOR DB.sequences",
        "INFO FOR SEQUENCE ids",
        "DEFINE MODULE mod::demo AS f\"files:/demo.surli\"",
        "DEFINE API /health FOR get THEN RETURN 'ok'",
        "ALTER API /health DROP ACTIONS",
        "REMOVE API IF EXISTS /health",
        "INFO FOR DB.apis",
        "INFO FOR API /health",
    ] {
        let error = parse_one(source).unwrap_err();
        assert!(
            matches!(
                error.kind,
                turso_fastdb_parser::ParseErrorKind::UnsupportedSyntax { .. }
            ),
            "{source}: {error:?}"
        );
    }
}

#[test]
fn p15_parse_009_synchronous_event_lifecycle_is_structured() {
    let script = parse(
        "DEFINE EVENT IF NOT EXISTS audit ON TABLE item \
           WHEN $event = 'CREATE' THEN { CREATE log CONTENT $after; } \
           COMMENT 'audit'; \
         DEFINE EVENT OVERWRITE compact ON item THEN (CREATE log SET id = $value.id); \
         DEFINE EVENT multi ON item THEN (CREATE log, CREATE audit); \
         DEFINE EVENT bare ON item THEN RETURN $value COMMENT 'bare'; \
         ALTER EVENT IF EXISTS audit ON TABLE item DROP WHEN \
           THEN { RETURN $before; } DROP COMMENT; \
         REMOVE EVENT IF EXISTS audit ON TABLE item",
    )
    .unwrap();
    let Statement::DefineEvent(first) = &script.statements[0] else {
        panic!("expected DEFINE EVENT")
    };
    assert!(first.if_not_exists.is_some());
    assert!(first.condition.is_some());
    assert_eq!(first.action.block.statements.len(), 1);
    assert_eq!(first.comment.as_ref().unwrap().value, "audit");
    let Statement::DefineEvent(second) = &script.statements[1] else {
        panic!("expected parenthesized DEFINE EVENT")
    };
    assert_eq!(
        second.action.style,
        turso_fastdb_parser::EventActionStyle::Parenthesized
    );
    let Statement::DefineEvent(multi) = &script.statements[2] else {
        panic!("expected multi-action DEFINE EVENT")
    };
    assert_eq!(multi.action.block.statements.len(), 2);
    let Statement::DefineEvent(bare) = &script.statements[3] else {
        panic!("expected bare DEFINE EVENT")
    };
    assert_eq!(
        bare.action.style,
        turso_fastdb_parser::EventActionStyle::Bare
    );
    assert_eq!(bare.comment.as_ref().unwrap().value, "bare");
    let Statement::AlterEvent(alter) = &script.statements[4] else {
        panic!("expected ALTER EVENT")
    };
    assert!(matches!(alter.changes.condition, Some(None)));
    assert!(matches!(alter.changes.action, Some(Some(_))));
    assert!(matches!(alter.changes.comment, Some(None)));
    assert!(matches!(script.statements[5], Statement::RemoveEvent(_)));

    for source in [
        "DEFINE EVENT empty ON item",
        "DEFINE EVENT IF NOT EXISTS OVERWRITE e ON item THEN RETURN NONE",
        "DEFINE EVENT e ON item ASYNC THEN RETURN NONE",
        "DEFINE EVENT e ON item THEN ()",
        "ALTER EVENT e ON item",
        "ALTER EVENT e ON item ASYNC",
        "ALTER EVENT e ON item WHEN true WHEN false",
        "REMOVE EVENT e item",
    ] {
        assert!(parse_one(source).is_err(), "{source}");
    }
}

#[test]
fn p15_parse_010_materialized_table_view_is_structured() {
    let statement = parse_one(
        "DEFINE TABLE IF NOT EXISTS totals AS SELECT category, count() AS total \
         FROM source WHERE active = true GROUP BY category PERMISSIONS NONE COMMENT 'derived'",
    )
    .unwrap();
    let Statement::DefineTable(table) = statement else {
        panic!("expected DEFINE TABLE")
    };
    let view = table.view.expect("view SELECT");
    assert!(table.if_not_exists.is_some());
    assert!(view.condition.is_some());
    assert!(view.group.is_some());
    assert_eq!(table.comment.unwrap().value, "derived");

    for source in [
        "DEFINE TABLE edge TYPE RELATION AS SELECT * FROM source",
        "DEFINE TABLE strict SCHEMAFULL AS SELECT * FROM source",
        "DEFINE TABLE dropped DROP AS SELECT * FROM source",
        "DEFINE TABLE old VIEW SELECT * FROM source",
    ] {
        assert!(parse_one(source).is_err(), "{source}");
    }
}
