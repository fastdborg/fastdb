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

#[test]
fn p14_api_003_complete_update_data_and_return_modes_are_atomic() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:one SET n = 1, tags = ['old', 'keep'], nested = { x: 1 }",
                params! {},
            )
            .await
            .unwrap();
        let response = connection
            .query(
                "UPDATE person:one SET n += 2, tags -= 'old' RETURN BEFORE; \
                 UPDATE person:one MERGE { nested: { y: 2 } } RETURN DIFF; \
                 UPDATE person:one PATCH [{ op: 'add', path: '/patched', 'value': true }]; \
                 UPDATE person:one UNSET nested.x RETURN VALUE nested; \
                 UPDATE ONLY person:one REPLACE { final: true } RETURN AFTER",
                params! {},
            )
            .await
            .unwrap();
        let StatementResult::Rows(before) = &response.statements[0] else {
            panic!("expected rows")
        };
        assert!(matches!(
            before[0],
            Value::Object(ref value) if value.get("n") == Some(&Value::Integer(1))
        ));
        let StatementResult::Rows(diff) = &response.statements[1] else {
            panic!("expected diff")
        };
        assert!(matches!(&diff[0], Value::Array(operations) if !operations.is_empty()));
        let StatementResult::Rows(nested) = &response.statements[3] else {
            panic!("expected value return")
        };
        assert!(matches!(
            &nested[0],
            Value::Object(value) if value.get("y") == Some(&Value::Integer(2))
        ));
        let StatementResult::Value(Value::Object(final_record)) = &response.statements[4] else {
            panic!("expected ONLY value")
        };
        assert_eq!(final_record.get("final"), Some(&Value::Bool(true)));
        assert!(!final_record.contains_key("n"));
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_004_insert_and_upsert_cover_rows_duplicates_and_missing_records() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let inserted = connection
            .query(
                "INSERT INTO person [{ id: 'a', name: 'A' }, { id: 'b', name: 'B' }]; \
                 INSERT INTO person (id, name) VALUES ('c', 'C'), ('d', 'D')",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(inserted.mutation_count, 4);

        let duplicate = connection
            .query(
                "INSERT INTO person { id: 'a', name: 'new' } \
                 ON DUPLICATE KEY UPDATE name = $input.name RETURN BEFORE; \
                 INSERT IGNORE INTO person { id: 'a', name: 'ignored' }; \
                 UPSERT ONLY person:z SET name = 'Z' RETURN BEFORE; \
                 UPSERT ONLY person:z SET count += 1 RETURN DIFF",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(duplicate.mutation_count, 3);
        let StatementResult::Rows(before) = &duplicate.statements[0] else {
            panic!("expected rows")
        };
        assert!(matches!(
            before[0],
            Value::Object(ref value) if value.get("name") == Some(&Value::Str("A".into()))
        ));
        let StatementResult::Rows(ignored) = &duplicate.statements[1] else {
            panic!("expected rows")
        };
        assert!(ignored.is_empty());
        assert!(matches!(
            duplicate.statements[2],
            StatementResult::Value(Value::Null)
        ));
        assert!(matches!(
            duplicate.statements[3],
            StatementResult::Value(Value::Array(ref operations)) if !operations.is_empty()
        ));

        let result = connection
            .query("SELECT name, count FROM person:z", params! {})
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &result.statements[0] else {
            panic!("expected rows")
        };
        assert!(matches!(
            &rows[0],
            Value::Object(value)
                if value.get("name") == Some(&Value::Str("Z".into()))
                    && value.get("count") == Some(&Value::Integer(1))
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_005_create_and_delete_complete_return_modes() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "CREATE ONLY item:empty RETURN VALUE id; \
                 CREATE item:gone SET n = 1 RETURN DIFF; \
                 DELETE ONLY item:gone RETURN AFTER",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            response.statements[0],
            StatementResult::Value(Value::RecordId(ref id)) if id == &RecordId::new("item", "empty")
        ));
        assert!(matches!(
            response.statements[1],
            StatementResult::Rows(ref rows)
                if matches!(&rows[0], Value::Array(operations) if !operations.is_empty())
        ));
        assert!(matches!(
            response.statements[2],
            StatementResult::Value(Value::Null)
        ));
        connection.close().await.unwrap();
    });
}
