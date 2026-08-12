#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::{Database, ErrorCategory, Value};

#[test]
fn p2_schema_003_schemafull_all_types_required_optional_and_unknown_paths() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("DEFINE TABLE typed SCHEMAFULL").unwrap();
    for definition in [
        "DEFINE FIELD b ON typed TYPE bool",
        "DEFINE FIELD i ON typed TYPE int",
        "DEFINE FIELD f ON typed TYPE float",
        "DEFINE FIELD n ON typed TYPE number",
        "DEFINE FIELD s ON typed TYPE string",
        "DEFINE FIELD o ON typed TYPE object",
        "DEFINE FIELD o.child ON typed TYPE string",
        "DEFINE FIELD a ON typed TYPE array",
        "DEFINE FIELD r ON typed TYPE record",
        "DEFINE FIELD optional ON typed TYPE option<string>",
    ] {
        conn.execute(definition).unwrap();
    }
    conn.execute(
        "CREATE typed:ok CONTENT { \
            b: true, i: -1, f: 2, n: 3.5, s: 'text', \
            o: { child: 'declared' }, a: [1, false], r: person:tracy \
        }",
    )
    .unwrap();

    for source in [
        "CREATE typed:missing CONTENT { b:true, i:1, f:1, n:1, s:'x', o:{child:'x'}, a:[] }",
        "CREATE typed:badnull CONTENT { b:null, i:1, f:1, n:1, s:'x', o:{child:'x'}, a:[], r:person:x }",
        "CREATE typed:badoptional CONTENT { b:true, i:1, f:1, n:1, s:'x', o:{child:'x'}, a:[], r:person:x, optional:null }",
        "CREATE typed:wrong CONTENT { b:1, i:1, f:1, n:1, s:'x', o:{child:'x'}, a:[], r:person:x }",
        "CREATE typed:extra CONTENT { b:true, i:1, f:1, n:1, s:'x', o:{child:'x', sibling:'no'}, a:[], r:person:x }",
    ] {
        assert_eq!(conn.execute(source).unwrap_err().category(), ErrorCategory::Schema, "{source}");
    }
}

#[test]
fn p2_schema_004_schemaless_enforces_declared_fields_but_allows_extras() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("DEFINE TABLE flexible SCHEMALESS").unwrap();
    conn.execute("DEFINE FIELD age ON flexible TYPE int")
        .unwrap();
    let result = conn
        .execute("CREATE flexible:one CONTENT { age: 42, extra: { any: true } }")
        .unwrap();
    assert_eq!(result.records.len(), 1);
    assert_eq!(
        conn.execute("CREATE flexible:two CONTENT { age: 'wrong', extra: 1 }")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    assert_eq!(
        conn.execute("CREATE flexible:three CONTENT { extra: 1 }")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
}

#[test]
fn p2_schema_005_new_field_validates_and_normalizes_existing_rows_atomically() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("CREATE metric:one SET score = 42").unwrap();
    conn.execute("CREATE metric:two CONTENT { score:43, label:'text' }")
        .unwrap();
    conn.execute("DEFINE FIELD score ON metric TYPE float")
        .unwrap();
    let selected = conn.execute("SELECT * FROM metric:one").unwrap();
    assert_eq!(
        selected.records[0]
            .fields
            .iter()
            .find(|(key, _)| key == "score")
            .map(|(_, value)| value),
        Some(&Value::Float(42.0))
    );

    let error = conn
        .execute("DEFINE FIELD label ON metric TYPE int")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Schema);
    assert!(
        conn.catalog_state().unwrap().snapshot().unwrap().tables["metric"]
            .fields
            .values()
            .all(|field| field.path_key != r#"$."label""#)
    );
}

#[test]
fn p2_schema_006_duplicate_definitions_missing_owners_and_path_conflicts() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    assert_eq!(
        conn.execute("DEFINE FIELD age ON absent TYPE int")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    assert_eq!(
        conn.execute("DEFINE INDEX by_age ON absent FIELDS age")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    conn.execute("DEFINE TABLE person SCHEMAFULL").unwrap();
    assert_eq!(
        conn.execute("DEFINE TABLE person SCHEMALESS")
            .unwrap_err()
            .category(),
        ErrorCategory::Constraint
    );
    conn.execute("DEFINE FIELD profile ON person TYPE string")
        .unwrap();
    assert_eq!(
        conn.execute("DEFINE FIELD profile ON person TYPE string")
            .unwrap_err()
            .category(),
        ErrorCategory::Constraint
    );
    assert_eq!(
        conn.execute("DEFINE FIELD profile.age ON person TYPE int")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
}
