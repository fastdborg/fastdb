#![forbid(unsafe_code)]
#![deny(warnings)]

use std::collections::BTreeMap;
use turso_fastdb::{Database, ErrorCategory, RecordIdValue, Value};

#[test]
fn p2_bridge_001_exact_create_select_define_and_generated_uuid_slice() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();

    conn.execute("DEFINE TABLE person SCHEMAFULL").unwrap();
    conn.execute("DEFINE FIELD name ON person TYPE string")
        .unwrap();
    conn.execute("DEFINE FIELD profile ON TABLE person TYPE object")
        .unwrap();
    conn.execute("DEFINE FIELD profile.age ON person TYPE float")
        .unwrap();
    conn.execute("DEFINE FIELD nickname ON person TYPE option<string>")
        .unwrap();

    let created = conn
        .execute(
            "CREATE person:u'550e8400-e29b-41d4-a716-446655440000' \
             CONTENT { name: 'Tracy', profile: { age: 42 } }",
        )
        .unwrap();
    let created_records = created.legacy_records();
    assert_eq!(created_records.len(), 1);
    assert!(matches!(
        created_records[0].id.id,
        RecordIdValue::Uuid(uuid) if uuid.get_version_num() == 4
    ));
    let profile = created_records[0]
        .fields
        .iter()
        .find(|(key, _)| key == "profile")
        .unwrap();
    let Value::Object(profile) = &profile.1 else {
        panic!("profile must be object");
    };
    assert_eq!(profile.get("age"), Some(&Value::Float(42.0)));

    let selected = conn
        .execute(
            "SELECT * FROM person \
             WHERE name = 'Tracy' AND profile.age = 42.0",
        )
        .unwrap();
    assert_eq!(selected.legacy_records(), created_records);

    let generated = conn
        .execute("CREATE note CONTENT { title: 'generated' }")
        .unwrap();
    let generated_records = generated.legacy_records();
    let RecordIdValue::Uuid(uuid) = generated_records[0].id.id else {
        panic!("omitted CREATE ID must generate UUID");
    };
    assert_eq!(uuid.get_version_num(), 7);
    let source = generated_records[0].id.to_string();
    assert!(source.starts_with("note:u'"));
    assert_eq!(
        conn.execute(&format!("SELECT * FROM {source}")).unwrap(),
        generated
    );
}

#[test]
fn p2_bridge_002_values_duplicate_keys_nested_set_and_typed_record_tags() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    let created = conn
        .execute(
            "CREATE item:1 CONTENT { \
                z: null, a: false, n: -7, f: +1.5, \
                list: [1, 'two', true], \
                owner: person:`quoted id`, \
                dup: 'first', dup: 'last' \
            }",
        )
        .unwrap();
    let created_records = created.legacy_records();
    assert_eq!(created_records[0].id.id, RecordIdValue::Integer(1));
    assert_eq!(
        created_records[0]
            .fields
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        vec!["a", "dup", "f", "list", "n", "owner", "z"]
    );
    assert_eq!(
        created_records[0]
            .fields
            .iter()
            .find(|(key, _)| key == "dup")
            .map(|(_, value)| value),
        Some(&Value::Str("last".into()))
    );
    assert_eq!(
        conn.execute("SELECT * FROM item:1")
            .unwrap()
            .legacy_records(),
        created_records
    );

    let nested = conn
        .execute("CREATE metric:-1 SET profile.age = 42")
        .unwrap();
    let mut profile = BTreeMap::new();
    profile.insert("age".into(), Value::Integer(42));
    assert_eq!(
        nested.legacy_records()[0].fields,
        vec![("profile".into(), Value::Object(profile))]
    );
    assert_eq!(
        conn.execute("SELECT * FROM metric WHERE profile.age = 42")
            .unwrap()
            .legacy_records(),
        nested.legacy_records()
    );
}

#[test]
fn p2_bridge_003_explicit_mvp_exclusions_stay_gated() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    for source in [
        "SELECT * FROM ONLY person",
        "UPDATE ONLY person:tracy SET name = 'Trace'",
        "DELETE FROM person",
        "SELECT * FROM person WHERE lower(name) = 'tracy'",
    ] {
        let error = conn.execute(source).unwrap_err();
        assert_eq!(
            error.category(),
            ErrorCategory::UnsupportedSyntax,
            "{source}: {error}"
        );
    }
}

#[test]
fn p2_bridge_004_top_level_id_is_never_stored() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    for source in [
        "CREATE person:tracy CONTENT { id: 'forged', name: 'Tracy' }",
        "CREATE person:tracy SET id = 'forged'",
        "DEFINE TABLE person SCHEMAFULL",
    ] {
        if source.starts_with("DEFINE") {
            conn.execute(source).unwrap();
            continue;
        }
        assert_eq!(
            conn.execute(source).unwrap_err().category(),
            ErrorCategory::Schema
        );
    }
    assert_eq!(
        conn.execute("DEFINE FIELD id ON person TYPE string")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
}
