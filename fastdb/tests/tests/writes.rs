use fastdb::{Database, Document, Key, Parameters, Record, Value};
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
fn column_list_insert_and_multirow_updates_use_pre_update_values() {
    let (_db, c) = setup();
    let rows=q(&c,"INSERT INTO users (id, name, a, b) VALUES (users:u1, 'Alice', 1, 2), (users:u2, 'Bob', 3, 4) RETURNING *");
    assert_eq!(rows.affected, 2);
    let params = Parameters::from([("$key".into(), Value::String("u3".into()))]);
    c.execute(
        "INSERT INTO users (id, name, a, b) VALUES (type::record('users', $key), 'Carol', 5, 6)",
        &params,
    )
    .expect("dynamic id");
    q(&c, "UPDATE users SET a=b, b=a WHERE a < 5 RETURNING *");
    assert_eq!(
        q(&c, "SELECT a, b FROM users WHERE id = users:u1").rows,
        vec![vec![Value::Integer(2), Value::Integer(1)]]
    );
    assert_eq!(
        q(&c, "SELECT a, b FROM users WHERE id = users:u2").rows,
        vec![vec![Value::Integer(4), Value::Integer(3)]]
    );
    assert_eq!(
        q(&c, "SELECT a, b FROM users WHERE id = users:u3").rows,
        vec![vec![Value::Integer(5), Value::Integer(6)]]
    );
    q(&c, "UPDATE users SET name=upper(name)");
    assert_eq!(
        c.lookup_index("users", "users_name", &Value::String("ALICE".into()))
            .expect("updated index")
            .len(),
        1
    );
    let deleted = q(&c, "DELETE FROM users WHERE a < 5 RETURNING *");
    assert_eq!(deleted.affected, 2);
    assert!(c
        .lookup_index("users", "users_name", &Value::String("ALICE".into()))
        .expect("deleted index")
        .is_empty());
}
#[test]
fn failed_statements_rollback_all_rows_and_keep_outer_transaction() {
    let (_db, c) = setup();
    assert!(c
        .execute(
            "INSERT INTO users (id,name) VALUES (users:u1,'same'),(users:u2,'same')",
            &Parameters::new()
        )
        .is_err());
    assert!(q(&c, "SELECT * FROM users").rows.is_empty());
    q(
        &c,
        "INSERT INTO users (id,name) VALUES (users:u1,'Alice'),(users:u2,'Bob')",
    );
    q(&c, "CREATE TABLE accounts (id INTEGER)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO accounts VALUES (42)");
    assert!(c
        .execute("UPDATE users SET name='duplicate'", &Parameters::new())
        .is_err());
    assert_eq!(
        q(&c, "SELECT name FROM users ORDER BY name").rows,
        vec![
            vec![Value::String("Alice".into())],
            vec![Value::String("Bob".into())]
        ]
    );
    assert_eq!(
        q(&c, "SELECT id FROM accounts").rows,
        vec![vec![Value::Integer(42)]]
    );
    q(&c, "DELETE FROM users");
    q(&c, "ROLLBACK");
    assert_eq!(q(&c, "SELECT * FROM users").rows.len(), 2);
    assert!(q(&c, "SELECT * FROM accounts").rows.is_empty());
}
#[test]
fn typed_parameters_and_validation_cannot_be_bypassed() {
    let (_db, c) = setup();
    q(&c, "DEFINE FIELD active ON users TYPE boolean");
    let params = Parameters::from([
        (
            "$id".into(),
            Value::Record(Record {
                table: "users".into(),
                key: Key::String("typed".into()),
            }),
        ),
        ("$active".into(), Value::Boolean(true)),
    ]);
    c.execute(
        "INSERT INTO users (id,name,active) VALUES ($id,'Typed',$active)",
        &params,
    )
    .expect("typed input");
    assert_eq!(
        q(&c, "SELECT active FROM users").rows,
        vec![vec![Value::Boolean(true)]]
    );
    let profile = Value::Object(Document::from([(
        "city".into(),
        Value::String("Bangkok".into()),
    )]));
    let params = Parameters::from([("$profile".into(), profile.clone())]);
    c.execute("UPDATE users SET profile=$profile", &params)
        .expect("typed object assignment");
    assert_eq!(q(&c, "SELECT profile FROM users").rows, vec![vec![profile]]);
    for sql in [
        "UPDATE users SET id = users:other",
        "UPDATE users SET name = 42",
        "UPDATE users SET active = 1",
        "INSERT INTO users (id,name) VALUES (1,'bad')",
        "INSERT INTO users (id,name) VALUES (users:u2,'x') ON CONFLICT DO NOTHING",
        "UPDATE users SET name='x', name='y'",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(
        q(&c, "SELECT name FROM users").rows,
        vec![vec![Value::String("Typed".into())]]
    );
}
#[test]
fn record_expression_projection_stays_typed_and_ordinary_sql_stays_relational() {
    let (_db, c) = setup();
    q(&c, "INSERT INTO users (id,name) VALUES (users:u1,'Alice')");
    let record = Value::Record(Record {
        table: "users".into(),
        key: Key::Integer(123),
    });
    assert_eq!(
        q(&c, "SELECT users:123 AS reference FROM users").rows,
        vec![vec![record]]
    );
    q(
        &c,
        "CREATE TABLE ordinary (id INTEGER PRIMARY KEY, name TEXT)",
    );
    q(&c, "INSERT INTO ordinary (id,name) VALUES (1,'x'),(2,'y')");
    q(&c, "UPDATE ordinary SET name=upper(name)");
    assert_eq!(
        q(&c, "DELETE FROM ordinary WHERE id=1 RETURNING id").rows,
        vec![vec![Value::Integer(1)]]
    );
}

#[test]
fn nested_set_unset_and_overlap_rules() {
    let (_db, c) = setup();
    q(&c,"INSERT INTO users {id: users:u1, name: 'Alice', profile: {city: 'Bangkok', zip: 10110}, a: 1, b: 2}");
    q(&c, "CREATE INDEX users_city ON users (profile.city)");
    q(
        &c,
        "UPDATE users:u1 SET profile.city='Berlin', a=b, b=a RETURNING *",
    );
    assert_eq!(
        q(
            &c,
            "SELECT u.profile.city AS city, u.profile.zip AS zip, a, b FROM users u"
        )
        .rows,
        vec![vec![
            Value::String("Berlin".into()),
            Value::Integer(10110),
            Value::Integer(2),
            Value::Integer(1)
        ]]
    );
    assert_eq!(
        c.lookup_index("users", "users_city", &Value::String("Berlin".into()))
            .expect("nested index")
            .len(),
        1
    );
    q(&c, "UPDATE users SET extra.deep.leaf=42 WHERE id=users:u1");
    q(
        &c,
        "UPDATE users SET \"profile.city\"='literal', profile.city='Paris'",
    );
    q(
        &c,
        "UPDATE users:u1 UNSET profile.city, missing.deep RETURNING *",
    );
    let row = q(&c, "SELECT * FROM users").exactly_one().expect("one");
    let Value::Object(doc) = &row[0] else {
        panic!("document");
    };
    let Value::Object(profile) = &doc["profile"] else {
        panic!("profile");
    };
    assert!(!profile.contains_key("city"));
    assert!(profile.contains_key("zip"));
    assert_eq!(doc["profile.city"], Value::String("literal".into()));
    assert!(c
        .lookup_index("users", "users_city", &Value::String("Paris".into()))
        .expect("unset index")
        .is_empty());
    for sql in [
        "UPDATE users SET profile=1, profile.city='x'",
        "UPDATE users SET name.first='x'",
        "UPDATE users UNSET name",
        "UPDATE users UNSET id",
        "UPDATE users SET a=1, a=2",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(
        q(&c, "SELECT name FROM users").rows,
        vec![vec![Value::String("Alice".into())]]
    );
}

#[test]
fn standalone_record_constructors_and_parameter_names() {
    let (_db, c) = setup();
    let params = Parameters::from([("$key".into(), Value::Integer(i64::MIN))]);
    let value = Value::Record(Record {
        table: "users".into(),
        key: Key::Integer(i64::MIN),
    });
    assert_eq!(
        c.execute("SELECT type::record('users', $key) AS id", &params)
            .expect("constructor")
            .rows,
        vec![vec![value]]
    );
    assert_eq!(
        q(&c, "SELECT users:123 AS id").rows,
        vec![vec![Value::Record(Record {
            table: "users".into(),
            key: Key::Integer(123)
        })]]
    );
    for sql in [
        "SELECT type::record('', 1)",
        "SELECT type::record('users', '')",
        "SELECT type::record('users', 1.5)",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    let params = Parameters::from([("$name::suffix".into(), Value::Integer(7))]);
    let baseline = turso_core::Database::open_file(
        turso_core::Database::io_for_path(":memory:").expect("io"),
        ":memory:",
    )
    .expect("baseline");
    let raw = baseline.connect().expect("baseline connection");
    let expected = match raw.prepare("SELECT $name::suffix") {
        Ok(_) => {
            panic!("update compatibility expectation: baseline now supports namespace parameters")
        }
        Err(e) => e.to_string(),
    };
    let error = c
        .execute("SELECT $name::suffix", &params)
        .expect_err("same baseline rejection");
    let fastdb::Error::Engine(actual) = error else {
        panic!("expected baseline error");
    };
    assert_eq!(actual.to_string(), expected);
    q(
        &c,
        "INSERT INTO users (id,name,n) VALUES (users:u1,'Alice',10),(users:u2,'Bob',2)",
    );
    let rows = q(
        &c,
        "SELECT type::record('users', n) AS r FROM users ORDER BY r",
    )
    .rows;
    assert_eq!(
        rows,
        vec![
            vec![Value::Record(Record {
                table: "users".into(),
                key: Key::Integer(2)
            })],
            vec![Value::Record(Record {
                table: "users".into(),
                key: Key::Integer(10)
            })]
        ]
    );
}

#[test]
fn indexed_and_anonymous_parameters_keep_statement_numbering() {
    let (_db, c) = setup();
    let params = Parameters::from([
        (
            "?1".into(),
            Value::Record(Record {
                table: "users".into(),
                key: Key::String("numbered".into()),
            }),
        ),
        ("?2".into(), Value::String("Alice".into())),
    ]);
    c.execute("INSERT INTO users (id,name) VALUES (?,?)", &params)
        .expect("anonymous slots");
    let params = Parameters::from([
        ("?1".into(), Value::String("Bob".into())),
        ("?2".into(), Value::String("Alice".into())),
    ]);
    c.execute("UPDATE users SET name=?1 WHERE name=?2", &params)
        .expect("numbered update");
    let params = Parameters::from([
        ("?1".into(), Value::Integer(1)),
        ("?2".into(), Value::Integer(2)),
    ]);
    assert_eq!(
        c.execute("SELECT ?1 + ?2", &params)
            .expect("ordinary positional SQL")
            .rows,
        vec![vec![Value::Integer(3)]]
    );
    assert_eq!(
        q(&c, "SELECT name FROM users").rows,
        vec![vec![Value::String("Bob".into())]]
    );
}
