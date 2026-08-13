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
