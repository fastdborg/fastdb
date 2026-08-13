#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{
    params, Builder, ErrorCategory, QueryEvent, QueryOptions, ResourceLimits, StatementResult,
    Value,
};
use futures::executor::block_on;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;

#[test]
fn p10_api_001_limits_are_bounded_transactional_and_request_local() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let mut connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE item:a SET label = 'one'; CREATE item:b SET label = 'two'",
                params! {},
            )
            .await
            .unwrap();

        let one_row = QueryOptions::default().with_resource_limits(
            ResourceLimits::default()
                .with_output_rows(1)
                .with_output_bytes(1024),
        );
        assert_eq!(
            connection
                .query_with_options("SELECT * FROM item", params! {}, one_row.clone())
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        let tiny_bytes = QueryOptions::default().with_resource_limits(
            ResourceLimits::default()
                .with_output_rows(10)
                .with_output_bytes(1),
        );
        assert_eq!(
            connection
                .query_with_options("SELECT * FROM item:a", params! {}, tiny_bytes)
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        assert_eq!(
            connection
                .query("SELECT * FROM item LIMIT 1", params! {})
                .await
                .unwrap()
                .statements
                .len(),
            1
        );

        let graph_limited = QueryOptions::default()
            .with_resource_limits(ResourceLimits::default().with_graph_hops(1));
        assert_eq!(
            connection
                .query_with_options(
                    "SELECT ->likes->person->likes->person AS ids FROM item:a",
                    params! {},
                    graph_limited,
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );

        connection
            .execute(
                "CREATE article:a SET text = 'Rust search'; \
                 DEFINE ANALYZER blankish TOKENIZERS blank; \
                 DEFINE INDEX text_idx ON article FIELDS text FULLTEXT ANALYZER blankish",
                params! {},
            )
            .await
            .unwrap();
        let fts_limited = QueryOptions::default()
            .with_resource_limits(ResourceLimits::default().with_fts_query_bytes(4));
        assert_eq!(
            connection
                .query_with_options(
                    "SELECT id FROM article WHERE text @@ $query",
                    params! { "query" => "search" },
                    fts_limited,
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );

        connection
            .execute(
                "CREATE point:a SET embedding = [1,0]; \
                 DEFINE FIELD embedding ON point TYPE array<float, 2>",
                params! {},
            )
            .await
            .unwrap();
        let vector_limited = QueryOptions::default()
            .with_resource_limits(ResourceLimits::default().with_vector_dimensions(1));
        assert_eq!(
            connection
                .query_with_options(
                    "SELECT id FROM point WHERE embedding <|1,EUCLIDEAN|> $query",
                    params! { "query" => Value::Array(vec![Value::Float(1.0), Value::Float(0.0)]) },
                    vector_limited,
                )
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );

        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("CREATE item:rolled_back SET label = 'x'", params! {})
            .await
            .unwrap();
        assert_eq!(
            transaction
                .query_with_options("SELECT * FROM item", params! {}, one_row)
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Constraint
        );
        drop(transaction);
        let missing = connection
            .query("SELECT * FROM item:rolled_back", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &missing.statements[0],
            StatementResult::Rows(rows) if rows.is_empty()
        ));

        let invalid = QueryOptions::default()
            .with_resource_limits(ResourceLimits::default().with_timeout(Duration::ZERO));
        assert_eq!(
            connection
                .query_with_options("SELECT * FROM item:a", params! {}, invalid)
                .await
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
        connection.close().await.unwrap();
        database.close().await.unwrap();
    });
}

#[test]
fn p10_api_002_check_backup_rebuild_and_database_close_are_deterministic() {
    block_on(async {
        let directory = tempdir().unwrap();
        let source = directory.path().join("source.fastdb");
        let backup = directory.path().join("backup.fastdb");
        let database = Builder::new_local(&source).build().await.unwrap();
        let mut connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:a CONTENT {}; CREATE person:b CONTENT {}; \
                 DEFINE TABLE links TYPE RELATION FROM person TO person ENFORCED; \
                 RELATE person:a->links->person:b \
                   SET note = 'Rust backup', embedding = [1,0]; \
                 DEFINE FIELD embedding ON links TYPE array<float, 2>; \
                 DEFINE ANALYZER blankish TOKENIZERS blank; \
                 DEFINE INDEX note_idx ON links FIELDS note FULLTEXT ANALYZER blankish",
                params! {},
            )
            .await
            .unwrap();

        assert_eq!(
            database.close().await.unwrap_err().category(),
            ErrorCategory::Transaction
        );
        let transaction = connection.transaction().await.unwrap();
        assert_eq!(
            database.backup_to(&backup).await.unwrap_err().category(),
            ErrorCategory::Transaction,
            "maintenance must reject an active explicit transaction"
        );
        transaction.rollback().await.unwrap();
        connection.close().await.unwrap();

        let report = database.check().await.unwrap();
        assert_eq!(report.tables, 2);
        assert_eq!(report.fts_indexes, 1);
        assert_eq!(report.vector_fields, 1);
        assert!(report.pinned_fts_exception);
        database.rebuild_index("links", "note_idx").await.unwrap();
        let backup_report = database.backup_to(&backup).await.unwrap();
        assert_eq!(backup_report.tables, report.tables);
        assert!(backup.exists());

        let restored = Builder::new_local(&backup).build().await.unwrap();
        assert_eq!(restored.check().await.unwrap().fts_indexes, 1);
        let restored_connection = restored.connect().unwrap();
        let rows = restored_connection
            .query(
                "SELECT in, out FROM links WHERE note @@ 'Rust backup'",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            &rows.statements[0],
            StatementResult::Rows(rows) if rows.len() == 1
        ));
        restored_connection.close().await.unwrap();
        restored.close().await.unwrap();
        database.close().await.unwrap();
        assert_eq!(
            database.connect().unwrap_err().category(),
            ErrorCategory::Engine
        );
        database.close().await.unwrap();
    });
}

#[test]
fn p10_api_003_event_hooks_are_metadata_only_and_panic_is_contained() {
    block_on(async {
        let events = Arc::new(Mutex::new(Vec::<QueryEvent>::new()));
        let captured = events.clone();
        let database = Builder::new_memory()
            .event_hook(Arc::new(move |event| {
                captured.lock().unwrap().push(event.clone());
                if event.mutation_count == 1 {
                    panic!("consumer panic must not stop the worker");
                }
            }))
            .build()
            .await
            .unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("CREATE secret:a SET token = 'not-in-event'", params! {})
            .await
            .unwrap();
        connection
            .query("SELECT * FROM secret:a", params! {})
            .await
            .unwrap();
        {
            let events = events.lock().unwrap();
            assert_eq!(events.len(), 2);
            assert_eq!(events[0].operation, "execute");
            assert_eq!(events[0].mutation_count, 1);
            assert_eq!(events[1].operation, "query");
            assert_eq!(events[1].output_rows, 1);
        }
        connection.close().await.unwrap();
        database.close().await.unwrap();
    });
}
