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

#[test]
fn p14_api_002_record_ranges_select_update_and_delete_by_typed_id_order() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:1 SET n = 1; CREATE person:2 SET n = 2; \
                 CREATE person:3 SET n = 3; CREATE person:4 SET n = 4",
                params! {},
            )
            .await
            .unwrap();

        let response = connection
            .query("SELECT id FROM person:1..=3 ORDER BY id", params! {})
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &response.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter()
                .map(|row| match row {
                    Value::Object(row) => match row.get("id") {
                        Some(Value::RecordId(record)) => record.id.clone(),
                        _ => panic!("expected id"),
                    },
                    _ => panic!("expected object"),
                })
                .collect::<Vec<_>>(),
            vec![
                RecordIdValue::Integer(1),
                RecordIdValue::Integer(2),
                RecordIdValue::Integer(3),
            ]
        );

        let updated = connection
            .execute("UPDATE person:2..4 SET ranged = true", params! {})
            .await
            .unwrap();
        assert_eq!(updated.mutation_count, 2);
        let deleted = connection
            .execute("DELETE person:..=2", params! {})
            .await
            .unwrap();
        assert_eq!(deleted.mutation_count, 2);
        let remaining = connection
            .query("SELECT * FROM person ORDER BY id", params! {})
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &remaining.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 2);
        connection.close().await.unwrap();
    });
}
