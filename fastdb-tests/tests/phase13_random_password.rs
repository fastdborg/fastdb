#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::decode::{DatetimeValue, DurationValue};
use turso_fastdb::{Database, Params, StatementResult, Value};

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
fn p13_fn_019_csprng_functions_have_characterized_types_and_bounds() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE random_fn:one SET \
             boolean = rand::bool(), integer = rand::int(10,10), float_value = rand::float(1,1), \
             choice = rand::enum(['x']), id_value = rand::id(8), string_value = rand::string(8), \
             duration_value = rand::duration(1s,1s), \
             time_value = rand::time(type::datetime('2024-01-01T00:00:00Z'),type::datetime('2024-01-01T00:00:00Z')), \
             ulid_value = rand::ulid(), uuid_value = rand::uuid(), \
             uuid4_value = rand::uuid::v4(), uuid7_value = rand::uuid::v7()",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM random_fn:one").unwrap();
    let row = row(&selected.statements[0]);
    assert!(matches!(row.get("boolean"), Some(Value::Bool(_))));
    assert_eq!(row.get("integer"), Some(&Value::Integer(10)));
    assert_eq!(row.get("float_value"), Some(&Value::Float(1.0)));
    assert_eq!(row.get("choice"), Some(&Value::Str("x".into())));
    for (key, alphabet) in [
        ("id_value", "abcdefghijklmnopqrstuvwxyz0123456789"),
        (
            "string_value",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
        ),
    ] {
        let Some(Value::Str(value)) = row.get(key) else {
            panic!("expected string for {key}")
        };
        assert_eq!(value.len(), 8);
        assert!(value.chars().all(|character| alphabet.contains(character)));
    }
    assert_eq!(
        row.get("duration_value"),
        Some(&Value::Duration(DurationValue::parse("1s").unwrap()))
    );
    assert_eq!(
        row.get("time_value"),
        Some(&Value::Datetime(
            DatetimeValue::parse("2024-01-01T00:00:00Z").unwrap()
        ))
    );
    let Some(Value::Str(ulid)) = row.get("ulid_value") else {
        panic!("expected ULID string")
    };
    assert_eq!(ulid.len(), 26);
    assert!(ulid
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()));
    for (key, version) in [("uuid_value", 7), ("uuid4_value", 4), ("uuid7_value", 7)] {
        let Some(Value::Uuid(value)) = row.get(key) else {
            panic!("expected UUID for {key}")
        };
        assert_eq!(value.get_version_num(), version, "{key}");
    }
}

#[test]
fn p13_fn_020_password_hashes_use_bounded_reference_work_factors() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE password_fn:one SET \
             argon = crypto::argon2::generate('password'), \
             bcrypt = crypto::bcrypt::generate('password'), \
             pbkdf2 = crypto::pbkdf2::generate('password'), \
             scrypt = crypto::scrypt::generate('password')",
        )
        .unwrap();
    let selected = connection.execute("SELECT * FROM password_fn:one").unwrap();
    let hashes = row(&selected.statements[0]);
    let mut params = Params::new();
    for (key, prefix) in [
        ("argon", "$argon2id$v=19$m=19456,t=2,p=1$"),
        ("bcrypt", "$2b$12$"),
        ("pbkdf2", "$pbkdf2-sha256$i=600000,l=32$"),
        ("scrypt", "$scrypt$ln=17,r=8,p=1$"),
    ] {
        let Some(Value::Str(hash)) = hashes.get(key) else {
            panic!("expected hash for {key}")
        };
        assert!(hash.starts_with(prefix), "unexpected {key} work factors");
        params.insert(key.into(), Value::Str(hash.clone()));
    }
    connection
        .execute_with_params(
            "CREATE password_check:one SET \
             argon = crypto::argon2::compare($argon,'password'), \
             bcrypt = crypto::bcrypt::compare($bcrypt,'password'), \
             pbkdf2 = crypto::pbkdf2::compare($pbkdf2,'password'), \
             scrypt = crypto::scrypt::compare($scrypt,'password'), \
             wrong = crypto::argon2::compare($argon,'wrong')",
            &params,
        )
        .unwrap();
    let selected = connection
        .execute("SELECT * FROM password_check:one")
        .unwrap();
    let row = row(&selected.statements[0]);
    for key in ["argon", "bcrypt", "pbkdf2", "scrypt"] {
        assert_eq!(row.get(key), Some(&Value::Bool(true)), "{key}");
    }
    assert_eq!(row.get("wrong"), Some(&Value::Bool(false)));
}

#[test]
fn p13_fn_021_random_and_password_limit_failures_do_not_mutate() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE bad:one SET value = rand::int(2,1)",
        "CREATE bad:one SET value = rand::string(65537)",
        "CREATE bad:one SET value = rand::float(1)",
    ] {
        assert!(connection.execute(source).is_err(), "accepted {source}");
    }
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
