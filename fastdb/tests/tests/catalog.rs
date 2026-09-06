use fastdb::{Database, Key, Parameters, Record, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
fn setup() -> (Database, fastdb::Connection) {
    let db = Database::open(":memory:").expect("open");
    let c = db.connect().expect("connect");
    q(&c, "CREATE TABLE users");
    q(&c, "DEFINE FIELD name ON users TYPE string REQUIRED");
    q(&c, "CREATE UNIQUE INDEX users_name ON users (name)");
    (db, c)
}
#[test]
fn upsert_inserts_or_shallow_patches_only_its_id() {
    let (_db, c) = setup();
    q(
        &c,
        "UPSERT users:u1 {name: 'Alice', profile: {city: 'Bangkok', zip: 1}} RETURNING *",
    );
    q(
        &c,
        "UPSERT users {id: users:u1, profile: {city: 'Berlin'}} RETURNING *",
    );
    let row = q(&c, "SELECT * FROM users").exactly_one().expect("one");
    let Value::Object(doc) = &row[0] else {
        panic!("document");
    };
    assert_eq!(doc["name"], Value::String("Alice".into()));
    let Value::Object(profile) = &doc["profile"] else {
        panic!("profile");
    };
    assert!(!profile.contains_key("zip"));
    q(&c, "UPSERT users:u2 {name: 'Bob'}");
    for sql in [
        "UPSERT users {name: 'Missing id'}",
        "UPSERT users:u1 {id: users:u1, name:'wrong source'}",
        "UPSERT users:u3 {profile:{}}",
        "UPSERT users:u2 {name:'Alice'}",
        "UPSERT users {id: other:u1,name:'wrong table'}",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(
        q(&c, "SELECT name FROM users ORDER BY name").rows,
        vec![
            vec![Value::String("Alice".into())],
            vec![Value::String("Bob".into())]
        ]
    );
    q(&c, "BEGIN");
    q(&c, "UPSERT users:u1 {name:'Changed'}");
    q(&c, "UPSERT users:u4 {name:'New'}");
    q(&c, "ROLLBACK");
    assert_eq!(
        c.lookup_index("users", "users_name", &Value::String("Alice".into()))
            .expect("restored index")
            .len(),
        1
    );
    assert!(c
        .get(&Record {
            table: "users".into(),
            key: Key::String("u4".into())
        })
        .expect("rolled back id")
        .is_none());
}
#[test]
fn index_drop_removes_metadata_and_rollback_restores_uniqueness() {
    let (_db, c) = setup();
    q(&c, "INSERT INTO users {id:users:u1,name:'Alice'}");
    q(&c, "BEGIN");
    q(&c, "DROP INDEX users_name");
    assert!(c
        .lookup_index("users", "users_name", &Value::String("Alice".into()))
        .is_err());
    q(&c, "INSERT INTO users {id:users:u2,name:'Alice'}");
    q(&c, "ROLLBACK");
    assert!(c
        .execute(
            "INSERT INTO users {id:users:u2,name:'Alice'}",
            &Parameters::new()
        )
        .is_err());
    assert_eq!(
        c.lookup_index("users", "users_name", &Value::String("Alice".into()))
            .expect("restored metadata")
            .len(),
        1
    );
    q(&c, "DROP INDEX users_name");
    q(&c, "DROP INDEX IF EXISTS users_name");
    q(&c, "CREATE INDEX users_name ON users (name)");
    q(&c, "INSERT INTO users {id:users:u2,name:'Alice'}");
    assert_eq!(
        c.lookup_index("users", "users_name", &Value::String("Alice".into()))
            .expect("rebuilt index")
            .len(),
        2
    );
}
#[test]
fn field_removal_is_metadata_only_and_incompatible_definitions_fail() {
    let (_db, c) = setup();
    assert!(c
        .execute(
            "DEFINE FIELD OVERWRITE name ON users TYPE object",
            &Parameters::new()
        )
        .is_err());
    q(&c, "INSERT INTO users {id:users:u1,name:'Alice'}");
    q(&c, "REMOVE FIELD name ON users");
    assert_eq!(
        q(&c, "SELECT name FROM users").rows,
        vec![vec![Value::String("Alice".into())]]
    );
    assert!(c
        .execute(
            "INSERT INTO users {id:users:u2,name:'Alice'}",
            &Parameters::new()
        )
        .is_err());
    q(&c, "DEFINE FIELD name ON users TYPE string");
    q(&c, "DEFINE FIELD profile ON users TYPE object");
    assert!(c
        .execute(
            "CREATE INDEX object_idx ON users (profile)",
            &Parameters::new()
        )
        .is_err());
    q(&c, "CREATE INDEX city_idx ON users (profile.city)");
    assert!(c
        .execute(
            "DEFINE FIELD OVERWRITE profile ON users TYPE string",
            &Parameters::new()
        )
        .is_err());
    q(&c, "DROP INDEX city_idx");
    q(&c, "DEFINE FIELD OVERWRITE profile ON users TYPE string");
}
#[test]
fn collection_drop_is_atomic_and_links_remain_weak() {
    let (_db, c) = setup();
    q(&c, "UPSERT users:u1 {name:'Alice'}");
    q(&c, "CREATE TABLE posts");
    q(&c, "INSERT INTO posts {id:posts:p1,author:users:u1}");
    q(&c, "BEGIN");
    q(&c, "DROP TABLE users");
    q(&c, "ROLLBACK");
    assert_eq!(q(&c, "SELECT * FROM users").rows.len(), 1);
    assert_eq!(
        c.lookup_index("users", "users_name", &Value::String("Alice".into()))
            .expect("restored index")
            .len(),
        1
    );
    q(&c, "DROP TABLE users");
    q(&c, "DROP TABLE IF EXISTS users");
    assert_eq!(q(&c, "SELECT * FROM posts").rows.len(), 1);
    q(&c, "CREATE TABLE users");
    q(&c, "CREATE UNIQUE INDEX users_name ON users (name)");
    q(&c, "INSERT INTO users {id:users:u2}");
    assert_eq!(q(&c, "SELECT * FROM users").rows.len(), 1);
}
#[test]
fn info_is_logical_and_if_not_exists_never_converts_models() {
    let (_db, c) = setup();
    q(
        &c,
        "CREATE TABLE accounts (id INTEGER PRIMARY KEY, name TEXT)",
    );
    q(&c, "CREATE INDEX accounts_name ON accounts (name)");
    q(
        &c,
        "CREATE TABLE IF NOT EXISTS users (id INTEGER PRIMARY KEY)",
    );
    q(&c, "CREATE TABLE IF NOT EXISTS accounts");
    q(
        &c,
        "CREATE INDEX IF NOT EXISTS users_name ON users (different)",
    );
    let info = q(&c, "INFO FOR TABLE users").exactly_one().expect("info");
    assert!(!format!("{info:?}").contains("__fastdb_"));
    let Value::Object(info) = &info[0] else {
        panic!("info object");
    };
    assert_eq!(info["model"], Value::String("document".into()));
    let index = q(&c, "INFO FOR INDEX users_name")
        .exactly_one()
        .expect("index");
    let Value::Object(index) = &index[0] else {
        panic!("index object");
    };
    assert_eq!(
        index["path"],
        Value::Array(vec![Value::String("name".into())])
    );
    for sql in [
        "INFO FOR DB",
        "INFO FOR TABLE accounts",
        "INFO FOR INDEX accounts_name",
    ] {
        assert!(!format!("{:?}", q(&c, sql)).contains("__fastdb_"));
    }
    q(&c, "DROP INDEX accounts_name");
    q(&c, "DROP TABLE accounts");
    assert!(c
        .execute("INFO FOR TABLE accounts", &Parameters::new())
        .is_err());
    assert!(c
        .execute("CREATE INDEX users ON users (name)", &Parameters::new())
        .is_err());
}

#[test]
fn lifecycle_persists_across_reopen_and_rebuild() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("catalog.db");
    let path = path.to_str().expect("path");
    {
        let db = Database::open(path).expect("open");
        let c = db.connect().expect("connect");
        q(&c, "CREATE TABLE users");
        q(&c, "DEFINE FIELD name ON users TYPE string REQUIRED");
        q(&c, "CREATE UNIQUE INDEX users_name ON users (name)");
        q(&c, "UPSERT users:u1 {name:'Alice'}");
        q(&c, "REMOVE FIELD name ON users");
        q(&c, "DROP INDEX users_name");
    }
    {
        let db = Database::open(path).expect("reopen");
        let c = db.connect().expect("connect");
        q(&c, "UPSERT users:u2 {name:'Alice'}");
        q(&c, "UPSERT users:u3 {}");
        assert!(c
            .execute(
                "CREATE UNIQUE INDEX users_name ON users (name)",
                &Parameters::new()
            )
            .is_err());
        q(&c, "CREATE INDEX users_name ON users (name)");
        assert_eq!(
            c.lookup_index("users", "users_name", &Value::String("Alice".into()))
                .expect("rebuilt index")
                .len(),
            2
        );
        q(&c, "DROP TABLE users");
    }
    let db = Database::open(path).expect("reopen drop");
    let c = db.connect().expect("connect");
    assert!(c
        .execute("INFO FOR TABLE users", &Parameters::new())
        .is_err());
    q(&c, "CREATE TABLE users");
    q(&c, "CREATE UNIQUE INDEX users_name ON users (name)");
}
#[test]
fn unsupported_catalog_version_is_rejected() {
    let temp = tempfile::tempdir().expect("temporary directory");
    let path = temp.path().join("version.db");
    let path = path.to_str().expect("path");
    let db = Database::open(path).expect("open");
    let c = db.connect().expect("connect");
    q(&c, "CREATE TABLE users");
    let engine =
        turso_core::Database::open_file(turso_core::Database::io_for_path(path).expect("io"), path)
            .expect("raw engine");
    let raw = engine.connect().expect("raw connection");
    raw.execute("UPDATE __fastdb_catalog SET metadata=json_set(metadata,'$.version',99)")
        .expect("future version fixture");
    assert!(c
        .execute("INFO FOR TABLE users", &Parameters::new())
        .expect_err("future version")
        .to_string()
        .contains("metadata version 99"));
    assert!(c.execute("INFO FOR DB", &Parameters::new()).is_err());
    assert!(c.execute("DROP TABLE users", &Parameters::new()).is_err());
    raw.execute("UPDATE __fastdb_catalog SET metadata=json_remove(metadata,'$.version')")
        .expect("legacy prototype fixture");
    q(&c, "UPSERT users:u1 {name:'legacy'}");
    assert_eq!(
        q(&c, "SELECT name FROM users").rows,
        vec![vec![Value::String("legacy".into())]]
    );
}
