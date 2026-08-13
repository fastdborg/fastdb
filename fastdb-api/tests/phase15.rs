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
                "REMOVE FUNCTION fn::dependent; REMOVE FUNCTION fn::base",
                params! {},
            )
            .await
            .unwrap();
        connection.close().await.unwrap();
    });
}
