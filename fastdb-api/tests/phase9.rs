#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{params, Builder, RecordId, StatementResult, Value};
use futures::executor::block_on;

#[test]
fn p9_api_001_bound_vector_query_crosses_async_worker() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE point:a SET embedding = [1,0]; \
                 CREATE point:b SET embedding = [0,1]; \
                 DEFINE FIELD embedding ON point TYPE array<float, 2>",
                params! {},
            )
            .await
            .unwrap();
        let response = connection
            .query(
                "SELECT id, vector::distance::knn() AS distance FROM point \
                 WHERE embedding <|1,EUCLIDEAN|> $query",
                params! { "query" => vec![Value::Float(1.0), Value::Float(0.0)] },
            )
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected rows")
        };
        let Value::Object(row) = &rows[0] else {
            panic!("expected projected object")
        };
        assert_eq!(
            row.get("id"),
            Some(&Value::RecordId(RecordId::new("point", "a")))
        );
        assert!(matches!(row.get("distance"), Some(Value::Float(value)) if value.abs() < 1e-12));
        connection.close().await.unwrap();
    });
}
