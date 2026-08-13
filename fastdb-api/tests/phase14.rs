#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{json, params, Builder, RecordId, RecordIdValue, StatementResult, Value};
use futures::executor::block_on;
use std::collections::BTreeMap;

#[test]
fn p14_api_001_complex_record_ids_round_trip_without_collisions() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:['eu', 5] SET name = 'array'; \
                 CREATE person:{ region: 'eu', n: 5 } SET name = 'object'; \
                 CREATE person:`['eu', 5]` SET name = 'string'",
                params! {},
            )
            .await
            .unwrap();
        let response = connection
            .query(
                "SELECT * FROM person:['eu', 5]; \
                 SELECT * FROM person:{ n: 5, region: 'eu' }; \
                 SELECT * FROM person:`['eu', 5]`",
                params! {},
            )
            .await
            .unwrap();
        for (statement, expected) in response
            .statements
            .iter()
            .zip(["array", "object", "string"])
        {
            let StatementResult::Rows(rows) = statement else {
                panic!("expected rows")
            };
            let Value::Object(row) = &rows[0] else {
                panic!("expected object")
            };
            assert_eq!(row.get("name"), Some(&Value::Str(expected.into())));
        }

        let mut object = BTreeMap::new();
        object.insert("n".into(), Value::Integer(5));
        object.insert("region".into(), Value::Str("eu".into()));
        let value = Value::RecordId(RecordId::new("person", RecordIdValue::Object(object)));
        assert_eq!(
            json::value_from_json(json::value_to_json(&value).unwrap()).unwrap(),
            value
        );
        connection.close().await.unwrap();
    });
}
