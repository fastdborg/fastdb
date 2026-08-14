#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{
    params, Builder, ErrorCategory, QueryOptions, ResourceLimits, StatementResult, Value,
};
use futures::executor::block_on;
use std::time::{Duration, Instant};
use tempfile::tempdir;

#[test]
fn p15_api_001_let_return_and_if_use_lexical_scope() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "LET $x = $seed; RETURN $x; \
                 IF false { RETURN 0; } ELSE IF true { LET $x = 2; RETURN $x; } \
                 ELSE { RETURN 3; }; \
                 RETURN $x",
                params! { "seed" => 4_i64 },
            )
            .await
            .unwrap();
        assert_eq!(
            response.statements,
            vec![
                StatementResult::None,
                StatementResult::Value(Value::Integer(4)),
                StatementResult::Value(Value::Integer(2)),
                StatementResult::Value(Value::Integer(4)),
            ]
        );

        let next = connection
            .query("RETURN $seed", params! { "seed" => 9_i64 })
            .await
            .unwrap();
        assert_eq!(
            next.statements,
            vec![StatementResult::Value(Value::Integer(9))]
        );
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_002_for_supports_control_flow_results_and_bounded_iterables() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "FOR $x IN [1, 2, 3] { \
                    IF $x = 2 { CONTINUE; }; \
                    CREATE item CONTENT { n: $x }; \
                    IF $x = 3 { BREAK; }; \
                 }; \
                 SELECT VALUE n FROM item ORDER BY n; \
                 FOR $x IN 1..=3 { RETURN $x; }; \
                 FOR $x IN <set>[4, 5] { LET $ignored = $x; }",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 2);
        assert!(matches!(response.statements[0], StatementResult::None));
        assert!(matches!(
            &response.statements[1],
            StatementResult::Rows(rows)
                if rows == &vec![Value::Integer(1), Value::Integer(3)]
        ));
        assert_eq!(
            response.statements[2],
            StatementResult::Value(Value::Integer(1))
        );
        assert!(matches!(response.statements[3], StatementResult::None));
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_003_throw_stops_the_script_redacts_and_poisons_a_guard() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let mut connection = database.connect().unwrap();
        let error = connection
            .execute(
                "CREATE item:kept SET n = 1; THROW 'do-not-leak-this-secret'",
                params! {},
            )
            .await
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Schema);
        assert!(!error.detail().contains("do-not-leak-this-secret"));
        let response = connection
            .query("SELECT * FROM item:kept", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &response.statements[0],
            StatementResult::Rows(rows) if rows.len() == 1
        ));

        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("CREATE item:rolled_back SET n = 2", params! {})
            .await
            .unwrap();
        let error = transaction
            .execute("THROW 'guard-secret'", params! {})
            .await
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Schema);
        assert!(!error.detail().contains("guard-secret"));
        drop(transaction);
        let response = connection
            .query("SELECT * FROM item:rolled_back", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &response.statements[0],
            StatementResult::Rows(rows) if rows.is_empty()
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_004_script_limits_apply_recursively_without_waiting() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let vector_limits = QueryOptions::default()
            .with_resource_limits(ResourceLimits::default().with_vector_dimensions(2));
        let error = connection
            .query_with_options(
                "IF true { RETURN vector::dot($vector, [1, 2, 3]); }",
                params! { "vector" => vec![Value::Integer(1), Value::Integer(2), Value::Integer(3)] },
                vector_limits,
            )
            .await
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Constraint);

        let timeout = QueryOptions::default()
            .with_resource_limits(ResourceLimits::default().with_timeout(Duration::from_millis(5)));
        let error = connection
            .query_with_options("SLEEP 10ms", params! {}, timeout)
            .await
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceLimit);
        assert_eq!(
            connection
                .query("SLEEP 'not a duration'", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        assert_eq!(
            connection
                .query("SLEEP 6s", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::ResourceLimit
        );

        let interrupt = connection.interrupt_handle();
        let interrupter = std::thread::spawn(move || {
            for _ in 0..20 {
                std::thread::sleep(Duration::from_millis(10));
                interrupt.interrupt();
            }
        });
        let started = Instant::now();
        let error = connection.query("SLEEP 1s", params! {}).await.unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Engine);
        assert!(started.elapsed() < Duration::from_millis(750));
        interrupter.join().unwrap();
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_005_nested_transaction_control_poison_rolls_back_the_guard() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let mut connection = database.connect().unwrap();
        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("CREATE item:rolled_back", params! {})
            .await
            .unwrap();
        let error = transaction
            .query("IF true { BEGIN; }", params! {})
            .await
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Transaction);
        drop(transaction);

        let response = connection
            .query("SELECT * FROM item:rolled_back", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &response.statements[0],
            StatementResult::Rows(rows) if rows.is_empty()
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_006_database_parameters_persist_overwrite_remove_and_shadow() {
    block_on(async {
        let directory = tempdir().unwrap();
        let path = directory.path().join("parameters.fastdb");
        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "DEFINE PARAM $answer VALUE { n: 42 }; RETURN $answer; INFO FOR DB",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            &response.statements[1],
            StatementResult::Value(Value::Object(value))
                if value.get("n") == Some(&Value::Integer(42))
        ));
        let StatementResult::Value(Value::Object(info)) = &response.statements[2] else {
            panic!("expected database information")
        };
        assert!(matches!(
            info.get("params"),
            Some(Value::Object(values))
                if values.get("answer") == Some(&Value::Str(
                    "DEFINE PARAM $answer VALUE { n: 42 } PERMISSIONS FULL".into()
                ))
        ));
        connection.close().await.unwrap();
        drop(database);

        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        let reopened = connection
            .query(
                "RETURN $answer; LET $answer = 7; RETURN $answer; \
                 DEFINE PARAM OVERWRITE $answer VALUE 43; RETURN $answer",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            &reopened.statements[0],
            StatementResult::Value(Value::Object(value))
                if value.get("n") == Some(&Value::Integer(42))
        ));
        assert_eq!(
            reopened.statements[2],
            StatementResult::Value(Value::Integer(7))
        );
        assert_eq!(
            reopened.statements[4],
            StatementResult::Value(Value::Integer(7)),
            "a top-level LET binding remains request-local"
        );

        let persisted = connection
            .query("RETURN $answer", params! {})
            .await
            .unwrap();
        assert_eq!(
            persisted.statements,
            vec![StatementResult::Value(Value::Integer(43))]
        );
        let altered = connection
            .query(
                "ALTER PARAM $answer PERMISSIONS NONE; INFO FOR DB; \
                 ALTER PARAM $answer VALUE 44; RETURN $answer",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            &altered.statements[1],
            StatementResult::Value(Value::Object(info))
                if matches!(info.get("params"), Some(Value::Object(values))
                    if values.get("answer") == Some(&Value::Str(
                        "DEFINE PARAM $answer VALUE 43 PERMISSIONS NONE".into()
                    )))
        ));
        assert_eq!(
            altered.statements[3],
            StatementResult::Value(Value::Integer(44))
        );
        let overridden = connection
            .query("RETURN $answer", params! { "answer" => 99_i64 })
            .await
            .unwrap();
        assert_eq!(
            overridden.statements,
            vec![StatementResult::Value(Value::Integer(99))]
        );

        connection
            .execute("REMOVE PARAM $answer", params! {})
            .await
            .unwrap();
        assert_eq!(
            connection
                .query("RETURN $answer", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_007_parameter_catalog_changes_roll_back_with_explicit_transactions() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let mut connection = database.connect().unwrap();
        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("DEFINE PARAM $temporary VALUE 1", params! {})
            .await
            .unwrap();
        assert_eq!(
            transaction
                .execute("THROW 'rollback'", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        drop(transaction);
        assert_eq!(
            connection
                .query("RETURN $temporary", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );

        connection
            .execute("DEFINE PARAM $stable VALUE 2", params! {})
            .await
            .unwrap();
        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("REMOVE PARAM $stable", params! {})
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        let response = connection
            .query("RETURN $stable", params! {})
            .await
            .unwrap();
        assert_eq!(
            response.statements,
            vec![StatementResult::Value(Value::Integer(2))]
        );
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_008_custom_functions_persist_mutate_and_have_a_complete_lifecycle() {
    block_on(async {
        let directory = tempdir().unwrap();
        let path = directory.path().join("functions.fastdb");
        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "DEFINE PARAM $offset VALUE 1; \
                 DEFINE FUNCTION fn::write($x: int) { \
                    CREATE item CONTENT { n: $x }; RETURN $x + $offset; \
                 }; \
                 LET $offset = 3; RETURN fn::write(4); \
                 SELECT VALUE n FROM item; INFO FOR DB",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 1);
        assert_eq!(
            response.statements[3],
            StatementResult::Value(Value::Integer(7))
        );
        assert!(matches!(
            &response.statements[4],
            StatementResult::Rows(rows) if rows == &vec![Value::Integer(4)]
        ));
        assert!(matches!(
            &response.statements[5],
            StatementResult::Value(Value::Object(info))
                if matches!(info.get("functions"), Some(Value::Object(functions))
                    if matches!(functions.get("write"), Some(Value::Str(definition))
                        if definition.contains("DEFINE FUNCTION fn::write($x: int)")
                            && definition.ends_with("PERMISSIONS FULL")))
        ));
        connection.close().await.unwrap();
        drop(database);

        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "RETURN fn::write(5); SELECT VALUE n FROM item ORDER BY n",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            response.statements[0],
            StatementResult::Value(Value::Integer(6))
        );
        assert!(matches!(
            &response.statements[1],
            StatementResult::Rows(rows)
                if rows == &vec![Value::Integer(4), Value::Integer(5)]
        ));
        let row_side_effect = connection
            .query(
                "CREATE source:a SET n = 2; \
                 SELECT fn::write(n) AS copied FROM source; \
                 DELETE source; DELETE item WHERE n = 2",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(row_side_effect.mutation_count, 4);
        assert!(
            matches!(
                &row_side_effect.statements[1],
                StatementResult::Rows(rows)
                    if matches!(&rows[0], Value::Object(value)
                        if value.get("copied") == Some(&Value::Integer(3)))
            ),
            "{:?}",
            row_side_effect.statements
        );

        let predicate_side_effect = connection
            .query(
                "DEFINE FUNCTION fn::predicate($x: int) { \
                    CREATE audit CONTENT { n: $x }; RETURN true; \
                 }; \
                 CREATE source:a SET n = 9; \
                 SELECT VALUE n FROM source WHERE fn::predicate(n); \
                 DELETE source; DELETE audit; REMOVE FUNCTION fn::predicate",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(predicate_side_effect.mutation_count, 4);
        assert!(matches!(
            &predicate_side_effect.statements[2],
            StatementResult::Rows(rows) if rows == &vec![Value::Integer(9)]
        ));

        assert_eq!(
            connection
                .query("RETURN fn::write('bad')", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        assert_eq!(
            connection
                .query("RETURN fn::write()", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        connection
            .execute("ALTER FUNCTION fn::write PERMISSIONS NONE", params! {})
            .await
            .unwrap();
        connection
            .execute(
                "DEFINE FUNCTION IF NOT EXISTS fn::write($x: int) { RETURN 0; }",
                params! {},
            )
            .await
            .unwrap();
        connection
            .execute(
                "DEFINE FUNCTION OVERWRITE fn::write($x: int) { RETURN $x * 2; }",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .query("RETURN fn::write(6) + fn::write(1)", params! {})
                .await
                .unwrap()
                .statements,
            vec![StatementResult::Value(Value::Integer(14))]
        );
        assert_eq!(
            connection
                .query("RETURN array::map([1, 2], |$x| fn::write($x))", params! {})
                .await
                .unwrap()
                .statements,
            vec![StatementResult::Value(Value::Array(vec![
                Value::Integer(2),
                Value::Integer(4),
            ]))]
        );
        let projected = connection
            .query(
                "SELECT fn::write(n) AS doubled FROM item ORDER BY n",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            &projected.statements[0],
            StatementResult::Rows(rows)
                if matches!(&rows[0], Value::Object(value)
                    if value.get("doubled") == Some(&Value::Integer(8)))
                && matches!(&rows[1], Value::Object(value)
                    if value.get("doubled") == Some(&Value::Integer(10)))
        ));
        assert!(matches!(
            &connection
                .query(
                    "SELECT VALUE n FROM item WHERE fn::write(n) = 8",
                    params! {},
                )
                .await
                .unwrap()
                .statements[0],
            StatementResult::Rows(rows) if rows == &vec![Value::Integer(4)]
        ));
        connection
            .execute(
                "REMOVE FUNCTION fn::write; REMOVE FUNCTION IF EXISTS fn::write",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .query("RETURN fn::write(1)", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        connection
            .execute(
                "DEFINE FUNCTION fn::loop($x: int) { RETURN fn::loop($x); }",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .query("RETURN fn::loop(1)", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::ResourceLimit
        );
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_009_function_catalog_changes_and_side_effects_share_transactions() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let mut connection = database.connect().unwrap();
        connection
            .execute(
                "DEFINE FUNCTION fn::write($x: int) { CREATE item CONTENT { n: $x }; RETURN $x; }",
                params! {},
            )
            .await
            .unwrap();
        connection
            .execute(
                "DEFINE FUNCTION fn::partial() { \
                    CREATE atomic:a SET n = 1; CREATE atomic:a SET n = 2; RETURN 1; \
                 }",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .query("RETURN fn::partial()", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        assert!(matches!(
            &connection
                .query("SELECT * FROM atomic:a", params! {})
                .await
                .unwrap()
                .statements[0],
            StatementResult::Rows(rows) if rows.is_empty()
        ));
        connection
            .execute(
                "DEFINE FUNCTION fn::content($x: int) { \
                    CREATE audit CONTENT { n: $x }; RETURN { n: $x }; \
                 }; \
                 DEFINE FUNCTION fn::bad_content() { \
                    CREATE audit CONTENT { n: 99 }; RETURN 'not-an-object'; \
                 }",
                params! {},
            )
            .await
            .unwrap();
        let content = connection
            .execute("CREATE made CONTENT fn::content(7)", params! {})
            .await
            .unwrap();
        assert_eq!(content.mutation_count, 2);
        assert_eq!(
            connection
                .execute("CREATE made CONTENT fn::bad_content()", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        let audit = connection
            .query("SELECT VALUE n FROM audit ORDER BY n", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &audit.statements[0],
            StatementResult::Rows(rows) if rows == &vec![Value::Integer(7)]
        ));
        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .query("RETURN fn::write(1)", params! {})
            .await
            .unwrap();
        transaction
            .execute("THROW 'rollback'", params! {})
            .await
            .unwrap_err();
        drop(transaction);
        assert!(matches!(
            &connection
                .query("SELECT * FROM item", params! {})
                .await
                .unwrap()
                .statements[0],
            StatementResult::Rows(rows) if rows.is_empty()
        ));

        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("DEFINE FUNCTION fn::temporary() { RETURN 1; }", params! {})
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        assert_eq!(
            connection
                .query("RETURN fn::temporary()", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_010_function_dependencies_block_dangling_catalog_state() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        assert_eq!(
            connection
                .execute(
                    "DEFINE FUNCTION fn::broken($x: int) { RETURN fn::missing($x); }",
                    params! {},
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        connection
            .execute(
                "DEFINE FUNCTION fn::base($x: int) { RETURN $x + 1; }; \
                 DEFINE FUNCTION fn::dependent($x: int) { RETURN fn::base($x); }",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute("REMOVE FUNCTION fn::base", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        assert_eq!(
            connection
                .execute(
                    "DEFINE FUNCTION OVERWRITE fn::base($x: int, $y: int) { RETURN $x + $y; }",
                    params! {},
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        let result = connection
            .query("RETURN fn::dependent(4)", params! {})
            .await
            .unwrap();
        assert_eq!(
            result.statements,
            vec![StatementResult::Value(Value::Integer(5))]
        );
        connection
            .execute(
                "REMOVE FUNCTION fn::dependent; DEFINE TABLE item",
                params! {},
            )
            .await
            .unwrap();
        connection
            .execute(
                "DEFINE EVENT dependency ON item THEN RETURN fn::base(1)",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute("REMOVE FUNCTION fn::base", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        assert_eq!(
            connection
                .execute(
                    "DEFINE FUNCTION OVERWRITE fn::base($x: int, $y: int) { RETURN $x + $y; }",
                    params! {},
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        connection
            .execute(
                "REMOVE EVENT dependency ON item; REMOVE FUNCTION fn::base",
                params! {},
            )
            .await
            .unwrap();
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_011_table_metadata_lifecycle_is_atomic_and_reopens() {
    block_on(async {
        let directory = tempdir().unwrap();
        let path = directory.path().join("tables.fastdb");
        let database = Builder::new_local(&path).build().await.unwrap();
        let mut connection = database.connect().unwrap();
        connection
            .execute(
                "DEFINE TABLE item DROP SCHEMALESS TYPE NORMAL \
                 PERMISSIONS FULL COMMENT 'initial'",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute("CREATE item:a CONTENT {}", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        let info = connection
            .query("INFO FOR DB; INFO FOR TABLE item", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &info.statements[0],
            StatementResult::Value(Value::Object(root))
                if matches!(root.get("tables"), Some(Value::Object(tables))
                    if matches!(tables.get("item"), Some(Value::Str(definition))
                        if definition.contains("DROP") && definition.contains("COMMENT 'initial'")))
        ));
        assert!(matches!(
            &info.statements[1],
            StatementResult::Value(Value::Object(root))
                if matches!(root.get("fields"), Some(Value::Object(fields)) if fields.is_empty())
                    && matches!(root.get("indexes"), Some(Value::Object(indexes)) if indexes.is_empty())
        ));

        connection
            .execute(
                "DEFINE TABLE IF NOT EXISTS item SCHEMAFULL TYPE NORMAL; \
                 DEFINE TABLE OVERWRITE item SCHEMALESS TYPE NORMAL \
                   PERMISSIONS NONE COMMENT 'open'; \
                 CREATE item:a CONTENT {}; \
                 ALTER TABLE item SCHEMAFULL PERMISSIONS FULL COMMENT 'locked'",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute("CREATE item:b CONTENT { extra: true }", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );

        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("REMOVE TABLE item", params! {})
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        assert!(matches!(
            &connection
                .query("SELECT * FROM item:a", params! {})
                .await
                .unwrap()
                .statements[0],
            StatementResult::Rows(rows) if rows.len() == 1
        ));
        connection.close().await.unwrap();
        drop(database);

        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        let reopened = connection.query("INFO FOR DB", params! {}).await.unwrap();
        assert!(matches!(
            &reopened.statements[0],
            StatementResult::Value(Value::Object(root))
                if matches!(root.get("tables"), Some(Value::Object(tables))
                    if matches!(tables.get("item"), Some(Value::Str(definition))
                        if definition == "DEFINE TABLE item TYPE NORMAL SCHEMAFULL PERMISSIONS FULL COMMENT 'locked'"))
        ));
        connection
            .execute(
                "ALTER TABLE item SCHEMALESS; \
                 DEFINE TABLE other SCHEMALESS TYPE NORMAL; \
                 DEFINE TABLE link TYPE RELATION FROM item TO other ENFORCED",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute("REMOVE TABLE item", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        connection
            .execute(
                "DEFINE TABLE left_node SCHEMALESS TYPE NORMAL; \
                 DEFINE TABLE right_node SCHEMALESS TYPE NORMAL; \
                 DEFINE TABLE wrong_node SCHEMALESS TYPE NORMAL; \
                 CREATE left_node:a; CREATE right_node:a; CREATE wrong_node:a; \
                 RELATE wrong_node:a->redefined->right_node:a; \
                 RELATE left_node:a->valid_link->right_node:a; \
                 DEFINE TABLE OVERWRITE valid_link TYPE RELATION \
                   FROM left_node TO right_node ENFORCED",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute(
                    "DEFINE TABLE OVERWRITE redefined TYPE RELATION \
                       FROM left_node TO right_node ENFORCED",
                    params! {},
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        assert!(matches!(
            &connection
                .query("SELECT * FROM redefined", params! {})
                .await
                .unwrap()
                .statements[0],
            StatementResult::Rows(rows) if rows.len() == 1
        ));
        connection
            .execute(
                "DEFINE TABLE dangling_node SCHEMALESS TYPE NORMAL; \
                 DEFINE TABLE dangling_other SCHEMALESS TYPE NORMAL; \
                 RELATE dangling_node:absent->loose_link->dangling_other:absent; \
                 REMOVE TABLE dangling_node",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            &connection
                .query("SELECT * FROM loose_link", params! {})
                .await
                .unwrap()
                .statements[0],
            StatementResult::Rows(rows) if rows.is_empty()
        ));
        connection
            .execute(
                "REMOVE TABLE link; REMOVE TABLE item; \
                 REMOVE TABLE IF EXISTS item; \
                 DEFINE TABLE item SCHEMALESS TYPE NORMAL; \
                 CREATE item:replacement SET n = 1",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            &connection
                .query("SELECT VALUE n FROM item", params! {})
                .await
                .unwrap()
                .statements[0],
            StatementResult::Rows(rows) if rows == &vec![Value::Integer(1)]
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_012_field_rules_normalize_validate_and_persist() {
    block_on(async {
        let directory = tempdir().unwrap();
        let path = directory.path().join("fields.fastdb");
        let database = Builder::new_local(&path).build().await.unwrap();
        let mut connection = database.connect().unwrap();
        connection
            .execute(
                "DEFINE TABLE item SCHEMAFULL TYPE NORMAL; \
                 DEFINE FIELD count ON item TYPE int DEFAULT ALWAYS 1 \
                   ASSERT $value >= 0 COMMENT 'counter'; \
                 DEFINE FIELD stamp ON item TYPE number VALUE count + 1; \
                 DEFINE FIELD code ON item TYPE string READONLY; \
                 DEFINE FIELD owner ON item TYPE option<record> REFERENCE; \
                 DEFINE FIELD metadata ON item COMMENT 'any value'; \
                 CREATE item:a CONTENT { code: 'fixed', owner: person:a, metadata: { ok: true } }",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute(
                    "DEFINE FIELD random_value ON item TYPE int DEFAULT rand::int()",
                    params! {},
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        let created = connection
            .query("SELECT count, stamp, code FROM item:a", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &created.statements[0],
            StatementResult::Rows(rows)
                if matches!(&rows[0], Value::Object(value)
                    if value.get("count") == Some(&Value::Integer(1))
                        && value.get("stamp") == Some(&Value::Integer(2))
                        && value.get("code") == Some(&Value::Str("fixed".into())))
        ));
        connection
            .execute("UPDATE item:a SET count = 4", params! {})
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute("UPDATE item:a SET code = 'changed'", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        assert_eq!(
            connection
                .execute("UPDATE item:a SET count = -1", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        connection
            .execute("UPDATE item:a SET count = NONE", params! {})
            .await
            .unwrap();
        let defaulted = connection
            .query("SELECT count, stamp FROM item:a", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &defaulted.statements[0],
            StatementResult::Rows(rows)
                if matches!(&rows[0], Value::Object(value)
                    if value.get("count") == Some(&Value::Integer(1))
                        && value.get("stamp") == Some(&Value::Integer(2)))
        ));

        connection
            .execute(
                "DEFINE FIELD IF NOT EXISTS count ON item TYPE string; \
                 DEFINE FIELD OVERWRITE count ON item TYPE int DEFAULT ALWAYS 2 \
                   ASSERT $value >= 0 COMMENT 'overwritten'; \
                 ALTER FIELD count ON item ASSERT $value <= 10; \
                 ALTER FIELD count ON item TYPE float; \
                 ALTER FIELD code ON item DROP READONLY; \
                 ALTER FIELD metadata ON item PERMISSIONS NONE",
                params! {},
            )
            .await
            .unwrap();
        connection
            .execute("UPDATE item:a SET code = 'changed', count = 3", params! {})
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute(
                    "CREATE item:bad SET code = 'x', count = 11, metadata = {}",
                    params! {},
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        let info = connection
            .query("INFO FOR TABLE item", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &info.statements[0],
            StatementResult::Value(Value::Object(root))
                if matches!(root.get("fields"), Some(Value::Object(fields))
                    if matches!(fields.get("count"), Some(Value::Str(definition))
                        if definition.contains("TYPE float")
                            && definition.contains("ASSERT $value <= 10")))
        ));

        connection
            .execute(
                "DEFINE FIELD embedding ON item TYPE option<array<float, 2>>; \
                 CREATE item:vector SET code = 'v', metadata = {}, embedding = [1, 2]; \
                 DEFINE INDEX code_idx ON item FIELDS code",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(
            connection
                .execute("REMOVE FIELD code ON item", params! {})
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        connection
            .execute(
                "REMOVE INDEX code_idx ON item; REMOVE FIELD embedding ON item; \
                 REMOVE FIELD IF EXISTS embedding ON item",
                params! {},
            )
            .await
            .unwrap();

        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("REMOVE FIELD code ON item", params! {})
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        let info = connection
            .query("INFO FOR TABLE item", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &info.statements[0],
            StatementResult::Value(Value::Object(root))
                if matches!(root.get("fields"), Some(Value::Object(fields)) if fields.contains_key("code"))
        ));
        connection.close().await.unwrap();
        assert_eq!(database.check().await.unwrap().vector_fields, 0);
        drop(database);

        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        let reopened = connection
            .query("SELECT count, stamp, code FROM item:a", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &reopened.statements[0],
            StatementResult::Rows(rows)
                if matches!(&rows[0], Value::Object(value)
                    if value.get("count") == Some(&Value::Float(3.0))
                        && value.get("stamp") == Some(&Value::Integer(4))
                        && value.get("code") == Some(&Value::Str("changed".into())))
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_013_event_definitions_persist_alter_remove_and_roll_back() {
    block_on(async {
        let directory = tempdir().unwrap();
        let path = directory.path().join("events.fastdb");
        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "DEFINE TABLE item SCHEMALESS TYPE NORMAL; \
                 DEFINE EVENT audit ON item WHEN $event = 'CREATE' \
                   THEN { RETURN $after; } COMMENT 'audit'; \
                 DEFINE EVENT IF NOT EXISTS audit ON item THEN { RETURN NONE; }",
                params! {},
            )
            .await
            .unwrap();
        let info = connection
            .query("INFO FOR TABLE item", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &info.statements[0],
            StatementResult::Value(Value::Object(root))
                if matches!(root.get("events"), Some(Value::Object(events))
                    if matches!(events.get("audit"), Some(Value::Str(definition))
                        if definition == "DEFINE EVENT audit ON TABLE item WHEN $event = 'CREATE' THEN { RETURN $after; } COMMENT 'audit'"))
        ));
        connection.close().await.unwrap();
        drop(database);

        let database = Builder::new_local(&path).build().await.unwrap();
        let mut connection = database.connect().unwrap();
        connection
            .execute(
                "ALTER EVENT audit ON item DROP WHEN \
                   THEN (RETURN $before) COMMENT 'changed'; \
                 ALTER EVENT IF EXISTS missing ON item DROP COMMENT",
                params! {},
            )
            .await
            .unwrap();
        let info = connection
            .query("INFO FOR TABLE item", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &info.statements[0],
            StatementResult::Value(Value::Object(root))
                if matches!(root.get("events"), Some(Value::Object(events))
                    if matches!(events.get("audit"), Some(Value::Str(definition))
                        if definition == "DEFINE EVENT audit ON TABLE item WHEN true THEN (RETURN $before) COMMENT 'changed'"))
        ));

        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("REMOVE EVENT audit ON item", params! {})
            .await
            .unwrap();
        transaction.rollback().await.unwrap();
        assert!(matches!(
            &connection
                .query("INFO FOR TABLE item", params! {})
                .await
                .unwrap()
                .statements[0],
            StatementResult::Value(Value::Object(root))
                if matches!(root.get("events"), Some(Value::Object(events)) if events.contains_key("audit"))
        ));
        connection
            .execute(
                "REMOVE EVENT audit ON item; REMOVE EVENT IF EXISTS audit ON item",
                params! {},
            )
            .await
            .unwrap();
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_014_events_execute_in_name_order_with_characterized_context() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "DEFINE TABLE item SCHEMALESS TYPE NORMAL; \
                 DEFINE EVENT capture ON item THEN { \
                   CREATE log CONTENT { \
                     kind: $event, before: $before, after: $after, \
                     value: $value, input: $input \
                   }; \
                 }; \
                 DEFINE EVENT z_order ON item WHEN $event = 'CREATE' \
                   THEN { UPSERT ordering:state SET markers += ['z']; }; \
                 DEFINE EVENT a_order ON item WHEN $event = 'CREATE' \
                   THEN { UPSERT ordering:state SET markers += ['a']; }; \
                 CREATE item:a SET n = 1; \
                 UPDATE item:a SET n = 2; \
                 DELETE item:a; \
                 SELECT kind, before, after, value, input FROM log ORDER BY kind; \
                 SELECT VALUE markers FROM ordering:state",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 8);
        let StatementResult::Rows(events) = &response.statements[7] else {
            panic!("expected captured event rows")
        };
        assert_eq!(events.len(), 3);
        let Value::Object(created) = &events[0] else {
            panic!("expected CREATE event")
        };
        assert_eq!(created.get("kind"), Some(&Value::Str("CREATE".into())));
        assert_eq!(created.get("before"), Some(&Value::Null));
        assert!(
            matches!(created.get("after"), Some(Value::Object(value)) if value.get("n") == Some(&Value::Integer(1)))
        );
        assert!(
            matches!(created.get("input"), Some(Value::Object(value)) if value.get("n") == Some(&Value::Integer(1)))
        );
        let Value::Object(deleted) = &events[1] else {
            panic!("expected DELETE event")
        };
        assert_eq!(deleted.get("kind"), Some(&Value::Str("DELETE".into())));
        assert_eq!(deleted.get("after"), Some(&Value::Null));
        assert_eq!(deleted.get("value"), deleted.get("before"));
        assert_eq!(deleted.get("input"), Some(&Value::Null));
        let Value::Object(updated) = &events[2] else {
            panic!("expected UPDATE event")
        };
        assert_eq!(updated.get("kind"), Some(&Value::Str("UPDATE".into())));
        assert!(
            matches!(updated.get("before"), Some(Value::Object(value)) if value.get("n") == Some(&Value::Integer(1)))
        );
        assert!(
            matches!(updated.get("after"), Some(Value::Object(value)) if value.get("n") == Some(&Value::Integer(2)))
        );
        assert!(matches!(
            &response.statements[8],
            StatementResult::Rows(rows)
                if rows == &vec![Value::Array(vec![
                    Value::Str("a".into()), Value::Str("z".into())
                ])]
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_015_event_failure_and_recursion_roll_back_atomically() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "DEFINE TABLE item SCHEMALESS TYPE NORMAL; \
                 DEFINE EVENT fail ON item THEN { \
                   CREATE log SET source = $after.id; THROW 'event-secret'; \
                 }",
                params! {},
            )
            .await
            .unwrap();
        let error = connection
            .execute("CREATE item:rolled_back SET n = 1", params! {})
            .await
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Schema);
        assert!(!error.detail().contains("event-secret"));
        for table in ["item", "log"] {
            assert!(matches!(
                &connection
                    .query(&format!("SELECT * FROM {table}"), params! {})
                    .await
                    .unwrap()
                    .statements[0],
                StatementResult::Rows(rows) if rows.is_empty()
            ));
        }

        connection
            .execute(
                "REMOVE EVENT fail ON item; \
                 DEFINE EVENT recursive ON item THEN { UPDATE item:loop SET n += 1; }",
                params! {},
            )
            .await
            .unwrap();
        let error = connection
            .execute("CREATE item:loop SET n = 0", params! {})
            .await
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::ResourceLimit);
        assert!(matches!(
            &connection
                .query("SELECT * FROM item:loop", params! {})
                .await
                .unwrap()
                .statements[0],
            StatementResult::Rows(rows) if rows.is_empty()
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_016_relation_events_cover_create_and_node_cascade_delete() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "DEFINE TABLE person SCHEMALESS TYPE NORMAL; \
                 DEFINE TABLE follows SCHEMALESS TYPE RELATION; \
                 DEFINE EVENT edge_audit ON follows THEN { \
                   CREATE edge_log SET kind = $event, edge = $value.id; \
                 }; \
                 CREATE person:a; CREATE person:b; \
                 RELATE person:a->follows->person:b; \
                 DELETE person:a; \
                 SELECT VALUE kind FROM edge_log ORDER BY kind; \
                 SELECT * FROM follows",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 7);
        assert!(matches!(
            &response.statements[7],
            StatementResult::Rows(rows)
                if rows == &vec![Value::Str("CREATE".into()), Value::Str("DELETE".into())]
        ));
        assert!(matches!(
            &response.statements[8],
            StatementResult::Rows(rows) if rows.is_empty()
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p15_api_017_insert_and_upsert_emit_create_or_update_events() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "DEFINE TABLE item SCHEMALESS TYPE NORMAL; \
                 DEFINE EVENT audit ON item THEN { \
                   CREATE changes SET kind = $event, source = $value.id; \
                 }; \
                 INSERT INTO item { id: 'a', n: 1 }; \
                 INSERT INTO item { id: 'a', n: 2 } \
                   ON DUPLICATE KEY UPDATE n = $input.n; \
                 UPSERT item:b SET n = 1; \
                 UPSERT item:b SET n = 2; \
                 SELECT VALUE kind FROM changes ORDER BY kind",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 8);
        assert!(matches!(
            &response.statements[6],
            StatementResult::Rows(rows)
                if rows == &vec![
                    Value::Str("CREATE".into()),
                    Value::Str("CREATE".into()),
                    Value::Str("UPDATE".into()),
                    Value::Str("UPDATE".into()),
                ]
        ));
        connection.close().await.unwrap();
    });
}
