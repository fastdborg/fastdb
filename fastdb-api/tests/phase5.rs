#![forbid(unsafe_code)]
#![deny(warnings)]

use fastdb::{Builder, ErrorCategory, Object, RecordId, RecordIdValue, Value};
use futures::executor::block_on;
use tempfile::tempdir;

#[test]
fn p5_json_001_nested_reserved_envelopes_round_trip_and_reject_spoofing() {
    let value = Value::Array(vec![Value::Object(Object::from([
        (
            "$fastdb".into(),
            Value::Object(Object::from([(
                "$fastdb".into(),
                Value::Str("user envelope".into()),
            )])),
        ),
        (
            "rid".into(),
            Value::RecordId(RecordId::new(
                "таблица",
                RecordIdValue::Uuid(
                    uuid::Uuid::parse_str("018f1f12-7b42-7cc7-98ad-dbdc0d501234").unwrap(),
                ),
            )),
        ),
    ]))]);
    let encoded = fastdb::json::value_to_json(&value).unwrap();
    assert_eq!(fastdb::json::value_from_json(encoded).unwrap(), value);

    for invalid in [
        serde_json::json!({"$fastdb":{"v":3,"t":"object","value":{}}}),
        serde_json::json!({"$fastdb":{"v":1,"t":"rid","table":"t","id_type":"integer","id":"1"}}),
        serde_json::json!({"$fastdb":{"v":1,"t":"rid","table":"t","id_type":"uuid","id":"00000000-0000-1000-8000-000000000000"}}),
        serde_json::json!({"$fastdb":{"v":1,"t":"object","value":{},"extra":true}}),
        serde_json::json!({"$fastdb":{"v":2,"t":"object","value":{},"extra":true}}),
    ] {
        assert_eq!(
            fastdb::json::value_from_json(invalid)
                .unwrap_err()
                .category(),
            ErrorCategory::Schema
        );
    }
}

#[test]
fn p5_fs_001_unicode_spaces_relative_absolute_reopen_sidecars_and_close() {
    block_on(async {
        let directory = tempdir().unwrap();
        let absolute = directory.path().join("space 数据.fastdb");
        let database = Builder::new_local(&absolute).build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("CREATE item:one SET n=1", fastdb::params! {})
            .await
            .unwrap();
        connection.close().await.unwrap();
        drop(database);
        assert!(absolute.is_file());

        let database = Builder::new_local(&absolute).build().await.unwrap();
        let connection = database.connect().unwrap();
        let response = connection
            .query("SELECT * FROM item:one", fastdb::params! {})
            .await
            .unwrap();
        assert_eq!(response.mutation_count, 0);
        connection.close().await.unwrap();
        drop(database);

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(directory.path()).unwrap();
        let database = Builder::new_local("relative.fastdb").build().await.unwrap();
        let connection = database.connect().unwrap();
        connection.close().await.unwrap();
        drop(database);
        std::env::set_current_dir(original).unwrap();
        assert!(directory.path().join("relative.fastdb").is_file());

        for suffix in ["-wal", "-shm"] {
            let sidecar = absolute.with_file_name(format!(
                "{}{}",
                absolute.file_name().unwrap().to_string_lossy(),
                suffix
            ));
            if sidecar.exists() {
                assert_eq!(
                    std::fs::metadata(&sidecar).unwrap().len(),
                    0,
                    "clean close left durable sidecar content in {sidecar:?}"
                );
            }
        }
    });
}
