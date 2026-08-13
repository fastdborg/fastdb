#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::{Database, StatementResult, Value};

fn row(result: &StatementResult) -> &std::collections::BTreeMap<String, Value> {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    let Value::Object(row) = &rows[0] else {
        panic!("expected object row")
    };
    row
}

#[test]
fn p13_fn_013_encoding_and_digest_functions_match_reference_vectors() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE encoding_fn:one SET \
             base64_encoded = encoding::base64::encode(<bytes>'hello'), \
             base64_decoded = encoding::base64::decode('aGVsbG8='), \
             json_encoded = encoding::json::encode({a:1,b:[2]}), \
             json_decoded = encoding::json::decode('{\"a\":1}'), \
             cbor_value = encoding::cbor::decode(encoding::cbor::encode({a:1})), \
             md5 = crypto::md5('hello'), sha1 = crypto::sha1('hello'), \
             sha256 = crypto::sha256('hello'), sha512 = crypto::sha512('hello'), \
             blake3 = crypto::blake3('hello'), joaat = crypto::joaat('hello')",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM encoding_fn:one").unwrap();
    let row = row(&selected.statements[0]);
    assert_eq!(
        row.get("base64_encoded"),
        Some(&Value::Str("aGVsbG8".into()))
    );
    assert_eq!(
        row.get("base64_decoded"),
        Some(&Value::Bytes(b"hello".to_vec()))
    );
    assert_eq!(
        row.get("json_encoded"),
        Some(&Value::Str("{\"a\":1,\"b\":[2]}".into()))
    );
    let expected_object = Value::Object(std::collections::BTreeMap::from([(
        "a".into(),
        Value::Integer(1),
    )]));
    assert_eq!(row.get("json_decoded"), Some(&expected_object));
    assert_eq!(row.get("cbor_value"), Some(&expected_object));
    for (key, expected) in [
        ("md5", "5d41402abc4b2a76b9719d911017c592"),
        ("sha1", "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d"),
        (
            "sha256",
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
        ),
        (
            "sha512",
            "9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043",
        ),
        (
            "blake3",
            "ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f",
        ),
    ] {
        assert_eq!(row.get(key), Some(&Value::Str(expected.into())), "{key}");
    }
    assert_eq!(row.get("joaat"), Some(&Value::Integer(3_372_029_979)));
}

#[test]
fn p13_fn_014_invalid_encoding_inputs_fail_without_mutation() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE bad:one SET value = encoding::base64::decode('!')",
        "CREATE bad:one SET value = encoding::json::decode('{')",
        "CREATE bad:one SET value = encoding::cbor::decode(encoding::base64::decode('/w=='))",
        "CREATE bad:one SET value = crypto::sha256({})",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
