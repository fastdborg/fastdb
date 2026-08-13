#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{json, params, Builder, RecordId, RecordIdValue, StatementResult, Value};
use futures::executor::block_on;
use std::collections::BTreeMap;
use tempfile::tempdir;

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

#[test]
fn p14_api_006_select_value_omit_split_group_fetch_and_destructure() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:a SET grp = 'x', n = 1, tags = ['b','a'], \
                    nested = { keep: 2, secret: 9 }, friend = person:b; \
                 CREATE person:b SET grp = 'x', n = 2, tags = ['c'], \
                    nested = { keep: 3, secret: 8 }; \
                 CREATE person:c SET grp = 'y', n = 3, tags = []",
                params! {},
            )
            .await
            .unwrap();

        let value = connection
            .query(
                "SELECT VALUE n FROM person ORDER BY n LIMIT BY $limit START AT $start",
                params! { "limit" => 2_i64, "start" => 1_i64 },
            )
            .await
            .unwrap();
        assert!(matches!(
            &value.statements[0],
            StatementResult::Rows(rows)
                if rows == &vec![Value::Integer(2), Value::Integer(3)]
        ));

        let omitted = connection
            .query(
                "SELECT *, n * 2 AS double OMIT nested.secret FROM person:a FETCH friend",
                params! {},
            )
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &omitted.statements[0] else {
            panic!("expected rows")
        };
        let Value::Object(row) = &rows[0] else {
            panic!("expected object")
        };
        assert_eq!(row.get("double"), Some(&Value::Integer(2)));
        assert!(
            matches!(row.get("friend"), Some(Value::Object(friend)) if friend.get("n") == Some(&Value::Integer(2)))
        );
        assert!(
            matches!(row.get("nested"), Some(Value::Object(nested)) if !nested.contains_key("secret"))
        );

        let split = connection
            .query(
                "SELECT tags FROM person SPLIT ON tags ORDER BY tags",
                params! {},
            )
            .await
            .unwrap();
        let StatementResult::Rows(split_rows) = &split.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(split_rows.len(), 4);

        let grouped = connection
            .query(
                "SELECT grp, count() AS count, math::sum(n) AS total, id FROM person GROUP BY grp ORDER BY grp",
                params! {},
            )
            .await
            .unwrap();
        let StatementResult::Rows(groups) = &grouped.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(groups.len(), 2);
        assert!(matches!(
            &groups[0],
            Value::Object(group)
                if group.get("grp") == Some(&Value::Str("x".into()))
                    && group.get("count") == Some(&Value::Integer(2))
                    && group.get("total") == Some(&Value::Integer(3))
                    && matches!(group.get("id"), Some(Value::Array(ids)) if ids.len() == 2)
        ));

        let destructured = connection
            .query(
                "SELECT { grp, nested.{ keep } } AS picked FROM person:a",
                params! {},
            )
            .await
            .unwrap();
        assert!(matches!(
            &destructured.statements[0],
            StatementResult::Rows(rows)
                if matches!(&rows[0], Value::Object(row)
                    if matches!(row.get("picked"), Some(Value::Array(values))
                        if values == &vec![
                            Value::Str("x".into()),
                            Value::Object(BTreeMap::from([("keep".into(), Value::Integer(2))])),
                        ]))
        ));
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_007_multi_targets_subqueries_and_numeric_order_preserve_pipeline_order() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE item:a SET label = 'item10'; CREATE item:b SET label = 'item2'",
                params! {},
            )
            .await
            .unwrap();
        let ordered = connection
            .query("SELECT label FROM item ORDER BY label NUMERIC", params! {})
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &ordered.statements[0] else {
            panic!("expected rows")
        };
        assert!(
            matches!(&rows[0], Value::Object(row) if row.get("label") == Some(&Value::Str("item2".into())))
        );

        let targets = connection
            .query(
                "SELECT * FROM (SELECT * FROM item:a), [42, { x: 1 }], item:b",
                params! {},
            )
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &targets.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 4);
        assert!(matches!(rows[1], Value::Integer(42)));
        assert!(
            matches!(rows[2], Value::Object(ref value) if value.get("x") == Some(&Value::Integer(1)))
        );
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_008_explain_full_analyze_json_executes_and_reports_structured_metrics() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("CREATE item:a SET n = 1", params! {})
            .await
            .unwrap();
        let response = connection
            .query(
                "EXPLAIN ANALYZE FULL FORMAT JSON SELECT * FROM item WHERE n = 1",
                params! {},
            )
            .await
            .unwrap();
        let StatementResult::Value(Value::Object(plan)) = &response.statements[0] else {
            panic!("expected structured JSON plan")
        };
        assert_eq!(plan.get("actual_rows"), Some(&Value::Integer(1)));
        assert!(matches!(plan.get("details"), Some(Value::Array(details)) if !details.is_empty()));
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_013_multi_target_mutations_are_atomic_and_statement_timeouts_are_bounded() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let created = connection
            .query(
                "CREATE [person:a, animal:b] SET n = 1 RETURN AFTER TIMEOUT 2s",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(created.mutation_count, 2);

        let targets = Value::Array(vec![
            Value::RecordId(RecordId::new("person", "a")),
            Value::RecordId(RecordId::new("animal", "b")),
        ]);
        let updated = connection
            .query(
                "UPDATE $targets SET n += 1 RETURN AFTER TIMEOUT 2s",
                params! { "targets" => targets.clone() },
            )
            .await
            .unwrap();
        assert_eq!(updated.mutation_count, 2);

        let error = connection
            .query(
                "UPDATE $targets SET n += 1 TIMEOUT 0s",
                params! { "targets" => targets },
            )
            .await
            .unwrap_err();
        assert!(error.to_string().contains("greater than zero"));
        let unchanged = connection
            .query("SELECT n FROM person:a; SELECT n FROM animal:b", params! {})
            .await
            .unwrap();
        assert!(unchanged.statements.iter().all(|statement| {
            matches!(statement, StatementResult::Rows(rows)
                if matches!(&rows[0], Value::Object(row)
                    if row.get("n") == Some(&Value::Integer(2))))
        }));

        let deleted = connection
            .query(
                "DELETE [person:a, animal:b] RETURN BEFORE TIMEOUT 2s",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(deleted.mutation_count, 2);
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_009_batch_create_count_and_integer_ranges_are_one_atomic_statement() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query(
                "CREATE |batch:3| SET kind = 'generated'; \
                 CREATE |numbered:1..=3| SET kind = 'range'",
                params! {},
            )
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 6);
        assert!(matches!(&response.statements[0], StatementResult::Rows(rows) if rows.len() == 3));
        let numbered = connection
            .query("SELECT id FROM numbered ORDER BY id", params! {})
            .await
            .unwrap();
        let StatementResult::Rows(rows) = &numbered.statements[0] else {
            panic!("expected rows")
        };
        assert_eq!(rows.len(), 3);
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_010_insert_relation_maintains_both_adjacency_directions() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute(
                "CREATE person:a; CREATE person:b; \
                 INSERT RELATION INTO likes { id: 'edge', in: person:a, out: person:b, weight: 1 }",
                params! {},
            )
            .await
            .unwrap();
        let response = connection
            .query(
                "SELECT ->likes->person AS outgoing FROM person:a; \
                 SELECT <-likes<-person AS incoming FROM person:b",
                params! {},
            )
            .await
            .unwrap();
        for statement in response.statements {
            let StatementResult::Rows(rows) = statement else {
                panic!("expected rows")
            };
            assert!(
                matches!(&rows[0], Value::Object(row) if matches!(row.values().next(), Some(Value::Array(ids)) if ids.len() == 1))
            );
        }
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_011_disk_reopen_and_explicit_transaction_poison_preserve_atomicity() {
    block_on(async {
        let directory = tempdir().unwrap();
        let path = directory.path().join("phase14.fastdb");
        let database = Builder::new_local(&path).build().await.unwrap();
        let mut connection = database.connect().unwrap();
        connection
            .execute(
                "INSERT INTO item [{ id: 'a', n: 1 }, { id: 'b', n: 2 }]",
                params! {},
            )
            .await
            .unwrap();
        let mut transaction = connection.transaction().await.unwrap();
        transaction
            .execute("UPDATE item:a SET n = 9", params! {})
            .await
            .unwrap();
        assert!(transaction
            .execute("INSERT INTO item [{ id: 'c' }, { id: 'c' }]", params! {},)
            .await
            .is_err());
        drop(transaction);
        connection.close().await.unwrap();
        drop(database);

        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query("SELECT * FROM ONLY item LIMIT 1", params! {})
            .await
            .unwrap();
        assert!(matches!(
            response.statements[0],
            StatementResult::Value(Value::Object(_))
        ));
        let unchanged = connection
            .query("SELECT n FROM item:a; SELECT * FROM item:c", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &unchanged.statements[0],
            StatementResult::Rows(rows)
                if matches!(&rows[0], Value::Object(row) if row.get("n") == Some(&Value::Integer(1)))
        ));
        assert!(matches!(&unchanged.statements[1], StatementResult::Rows(rows) if rows.is_empty()));
        connection.close().await.unwrap();
    });
}

#[test]
fn p14_api_012_group_all_empty_input_returns_zero_count() {
    block_on(async {
        let database = Builder::new_memory().build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("DEFINE TABLE empty SCHEMALESS", params! {})
            .await
            .unwrap();
        let response = connection
            .query("SELECT count() AS count FROM empty GROUP ALL", params! {})
            .await
            .unwrap();
        assert!(matches!(
            &response.statements[0],
            StatementResult::Rows(rows)
                if matches!(&rows[0], Value::Object(row) if row.get("count") == Some(&Value::Integer(0)))
        ));
        connection.close().await.unwrap();
    });
}
