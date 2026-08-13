#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{params, Builder, ErrorCategory, RecordId, RecordIdValue, StatementResult, Value};
use futures::executor::block_on;
use std::collections::BTreeMap;
use std::sync::{mpsc, Arc};
use tempfile::tempdir;

#[test]
fn p4_api_001_lifecycle_reopen_results_and_exact_mutation_counts() {
    block_on(async {
        let directory = tempdir().unwrap();
        let path = directory.path().join("api.fastdb");
        {
            let database = Builder::new_local(&path).build().await.unwrap();
            let connection = database.connect().unwrap();
            let summary = connection
                .execute(
                    "CREATE item:one SET n=1 RETURN NONE;\
                     CREATE item:two SET n=2;\
                     SELECT * FROM item;\
                     UPDATE item SET n=n+1 RETURN NONE;\
                     DELETE item:two",
                    params! {},
                )
                .await
                .unwrap();
            assert_eq!(summary.statement_count, 5);
            assert_eq!(summary.mutation_count, 5);
            connection.close().await.unwrap();
        }
        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query("SELECT n FROM item:one", params! {})
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 0);
        assert_eq!(
            response.statements,
            vec![StatementResult::Rows(vec![Value::Object(BTreeMap::from(
                [("n".into(), Value::Integer(2)),]
            ))])]
        );
        connection.close().await.unwrap();
    });
}

#[test]
fn p4_api_002_transaction_commit_rollback_drop_and_guard_rejection() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let mut connection = database.connect().unwrap();
        {
            let mut transaction = connection.transaction().await.unwrap();
            transaction
                .execute("CREATE item:drop SET n=1", params! {})
                .await
                .unwrap();
        }
        let dropped = connection
            .query("SELECT * FROM item:drop", params! {})
            .await
            .unwrap();
        assert_eq!(dropped.statements, vec![StatementResult::Rows(vec![])]);

        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("CREATE item:saved SET n=2", params! {})
            .await
            .unwrap();
        transaction.commit().await.unwrap();

        let mut transaction = connection.transaction().await.unwrap();
        let error = transaction
            .query("COMMIT; SELECT FROM item", params! {})
            .await
            .expect_err("guard must reject source transaction control");
        assert_eq!(error.category(), ErrorCategory::Transaction);
        drop(transaction);

        let response = connection
            .query("SELECT * FROM item:saved", params! {})
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 1);
        connection.close().await.unwrap();
    });
}

#[test]
fn p4_api_003_guard_error_returns_original_and_connection_recovers() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let mut connection = database.connect().unwrap();
        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("CREATE item:one SET n=1", params! {})
            .await
            .unwrap();
        let error = transaction
            .query("SELECT FROM item", params! {})
            .await
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Parse);
        assert!(error.span().is_some());
        drop(transaction);
        let response = connection
            .query("SELECT * FROM item:one", params! {})
            .await
            .unwrap();
        assert_eq!(response.statements, vec![StatementResult::Rows(vec![])]);
        connection.close().await.unwrap();
    });
}

#[test]
fn p4_api_004_strict_json_typed_ids_and_reserved_key_are_collision_safe() {
    let mut object = BTreeMap::new();
    object.insert("$fastdb".into(), Value::Str("user data".into()));
    object.insert(
        "record".into(),
        Value::RecordId(RecordId::new("person", RecordIdValue::Integer(7))),
    );
    let value = Value::Object(object);
    let json = fastdb::json::value_to_json(&value).unwrap();
    assert_eq!(json["$fastdb"]["v"], 1);
    assert_eq!(json["$fastdb"]["t"], "object");
    assert_eq!(
        json["$fastdb"]["value"]["record"]["$fastdb"]["id_type"],
        "integer"
    );
    assert_eq!(fastdb::json::value_from_json(json).unwrap(), value);
}

#[cfg(unix)]
#[test]
fn p4_api_005_non_utf8_paths_are_io_errors() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let path = OsString::from_vec(vec![b'f', 0x80]);
    let error = block_on(Builder::new_local(path).build()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Io);
}

#[test]
fn p4_api_006_concurrent_callers_are_serialized_by_one_worker() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = Arc::new(database.connect().unwrap());
        let threads = (0..12)
            .map(|id| {
                let connection = connection.clone();
                std::thread::spawn(move || {
                    block_on(connection.execute(
                        &format!("CREATE item:r{id} SET n={id} RETURN NONE"),
                        params! {},
                    ))
                    .unwrap()
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            assert_eq!(thread.join().unwrap().mutation_count, 1);
        }
        let response = connection
            .query("SELECT * FROM item ORDER BY id", params! {})
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 12);
        Arc::try_unwrap(connection)
            .expect("all caller handles were joined")
            .close()
            .await
            .unwrap();
    });
}

#[test]
fn p4_api_007_interrupt_is_engine_error_and_connection_remains_usable() {
    let database = block_on(Builder::new_memory().build()).unwrap();
    let connection = database.connect().unwrap();
    block_on(async {
        let mut transaction = connection;
        {
            let mut guard = transaction.transaction().await.unwrap();
            for batch in 0..24 {
                let source = (0..240)
                    .map(|offset| {
                        let id = batch * 240 + offset;
                        format!("CREATE item:r{id} SET n={id} RETURN NONE")
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                guard.execute(&source, params! {}).await.unwrap();
            }
            guard.commit().await.unwrap();
        }

        let interrupt = transaction.interrupt_handle();
        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::scope(|scope| {
            let connection = &transaction;
            scope.spawn(move || {
                let result = block_on(
                    connection.query("SELECT * FROM item WHERE n >= 0 ORDER BY id", params! {}),
                );
                sender.send(result).unwrap();
            });
            loop {
                match receiver.try_recv() {
                    Ok(result) => {
                        let error = result.expect_err("scan should be interrupted");
                        assert_eq!(error.category(), ErrorCategory::Engine);
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        interrupt.interrupt();
                        std::thread::yield_now();
                    }
                    Err(mpsc::TryRecvError::Disconnected) => panic!("query thread stopped"),
                }
            }
        });
        let response = transaction
            .query("SELECT * FROM item:r1", params! {})
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 1);
        transaction.close().await.unwrap();
    });
}
