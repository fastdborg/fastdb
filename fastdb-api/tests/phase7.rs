#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{params, Builder, RecordId, StatementResult, Value};
use futures::executor::block_on;

#[test]
fn p7_api_001_graph_records_and_bound_endpoints_cross_async_worker() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:one CONTENT {}; CREATE post:one CONTENT {}",
                params! {},
            )
            .await
            .unwrap();
        let response = connection
            .query(
                "RELATE ONLY $from->likes->$to SET weight=2; \
                 SELECT ->likes->post AS ids FROM person:one",
                params! {
                    "from" => Value::RecordId(RecordId::new("person", "one")),
                    "to" => Value::RecordId(RecordId::new("post", "one")),
                },
            )
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 1);
        let StatementResult::Value(Value::Object(edge)) = &response.statements[0] else {
            panic!("expected edge")
        };
        assert_eq!(
            edge.get("in"),
            Some(&Value::RecordId(RecordId::new("person", "one")))
        );
        let StatementResult::Rows(rows) = &response.statements[1] else {
            panic!("expected traversal rows")
        };
        assert_eq!(rows.len(), 1);
        connection.close().await.unwrap();
    });
}
