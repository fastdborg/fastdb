use fastdb::{Database, Key, Parameters, Record, Value};
fn query(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
fn setup() -> (Database, fastdb::Connection) {
    let db = Database::open(":memory:").expect("open");
    let c = db.connect().expect("connect");
    query(&c, "CREATE TABLE users");
    query(&c, "CREATE INDEX users_city ON users (profile.city)");
    query(&c,"INSERT INTO users {id: users:2, name: 'Bob', active: true, profile: {city: 'Bangkok'}, tags: ['rust'], nickname: null}");
    query(
        &c,
        "INSERT INTO users {id: users:10, name: 'Alice', active: false, profile: {city: 'Berlin'}}",
    );
    query(
        &c,
        "INSERT INTO users {id: users:-1, name: 'Carol', profile: {city: 'Bangkok'}}",
    );
    (db, c)
}
#[test]
fn projections_filters_sort_and_pagination() {
    let (_db, c) = setup();
    let result=query(&c,"SELECT u.id, u.name, u.active, u.profile.city AS city, u.tags, u.unknown FROM users AS u WHERE u.profile.city = 'Bangkok' ORDER BY u.name LIMIT 1 OFFSET 0");
    assert_eq!(
        result.columns,
        vec!["id", "name", "active", "city", "tags", "unknown"]
    );
    assert_eq!(
        result.rows[0],
        vec![
            Value::Record(Record {
                table: "users".into(),
                key: Key::Integer(2)
            }),
            Value::String("Bob".into()),
            Value::Boolean(true),
            Value::String("Bangkok".into()),
            Value::Array(vec![Value::String("rust".into())]),
            Value::Null
        ]
    );
    let result = query(
        &c,
        "SELECT name, length(name) AS size FROM users WHERE active = true ORDER BY name",
    );
    assert_eq!(
        result.rows,
        vec![vec![Value::String("Bob".into()), Value::Integer(3)]]
    );
    assert_eq!(
        query(&c, "SELECT name FROM users WHERE id = users:10").rows,
        vec![vec![Value::String("Alice".into())]]
    );
    let values = query(&c, "SELECT id FROM users ORDER BY id").rows;
    assert_eq!(
        values.into_iter().map(|r| r[0].clone()).collect::<Vec<_>>(),
        [-1, 2, 10].map(|key| Value::Record(Record {
            table: "users".into(),
            key: Key::Integer(key)
        }))
    );
}
#[test]
fn full_documents_preserve_absence_and_literal_keys() {
    let (_db, c) = setup();
    query(&c,"INSERT INTO users {id: users:literal, \"profile.city\": 'literal', profile: {city: 'nested'}}");
    assert_eq!(query(&c,"SELECT u.\"profile.city\" AS literal, u.profile.city AS nested FROM users u WHERE id = users:literal").rows,vec![vec![Value::String("literal".into()),Value::String("nested".into())]]);
    let row = query(&c, "SELECT u.* FROM users u WHERE id = users:2")
        .exactly_one()
        .expect("one");
    let Value::Object(doc) = &row[0] else {
        panic!("document");
    };
    assert_eq!(doc["nickname"], Value::Null);
    assert!(!doc.contains_key("unknown"));
    assert!(c
        .execute("SELECT u.profile = u.tags FROM users u", &Parameters::new())
        .is_err());
    assert!(c
        .execute("SELECT name, name FROM users", &Parameters::new())
        .is_err());
}
#[test]
fn joins_and_typed_parameter_filters() {
    let (_db, c) = setup();
    query(&c, "CREATE TABLE posts");
    query(
        &c,
        "INSERT INTO posts {id: posts:p1, title: 'Hello', author: users:2}",
    );
    query(
        &c,
        "INSERT INTO posts {id: posts:p2, title: 'Orphan', author: users:404}",
    );
    let result = query(
        &c,
        "SELECT p.title, u.name FROM posts p LEFT JOIN users u ON p.author = u.id ORDER BY p.title",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![Value::String("Hello".into()), Value::String("Bob".into())],
            vec![Value::String("Orphan".into()), Value::Null]
        ]
    );
    query(
        &c,
        "CREATE TABLE cities(name TEXT PRIMARY KEY, country TEXT)",
    );
    query(&c, "INSERT INTO cities VALUES ('Bangkok', 'Thailand')");
    assert_eq!(query(&c,"SELECT u.name, c.country FROM users u JOIN cities c ON u.profile.city = c.name ORDER BY u.name").rows.len(),2);
    let params = Parameters::from([
        ("$city".into(), Value::String("Bangkok".into())),
        (
            "$id".into(),
            Value::Record(Record {
                table: "users".into(),
                key: Key::Integer(2),
            }),
        ),
    ]);
    assert_eq!(
        c.execute(
            "SELECT name FROM users WHERE users.profile.city = $city AND id = $id",
            &params
        )
        .expect("bound filters")
        .rows
        .len(),
        1
    );
    assert_eq!(
        query(&c, "SELECT name FROM users WHERE id = 'users:2'")
            .rows
            .len(),
        0
    );
}
#[test]
fn planner_uses_managed_index_and_sees_transaction_changes() {
    let (_db, c) = setup();
    let plan = query(
        &c,
        "EXPLAIN QUERY PLAN SELECT u.name FROM users u WHERE u.profile.city = 'Bangkok'",
    );
    let details = format!("{:?}", plan.rows);
    assert!(details.contains("users_city"), "{details}");
    assert!(details.to_ascii_uppercase().contains("SEARCH"), "{details}");
    query(&c, "BEGIN");
    query(&c, "UPDATE users:2 {profile: {city: 'Paris'}}");
    assert_eq!(
        query(&c, "SELECT * FROM users u WHERE u.profile.city = 'Bangkok'")
            .rows
            .len(),
        1
    );
    query(&c, "ROLLBACK");
    assert_eq!(
        query(&c, "SELECT * FROM users u WHERE u.profile.city = 'Bangkok'")
            .rows
            .len(),
        2
    );
    assert!(c
        .execute("SELECT u.doc FROM __fastdb_catalog u", &Parameters::new())
        .is_err());
    assert!(c
        .execute(
            "SELECT __fastdb_value(u.doc, '[]') FROM users u",
            &Parameters::new()
        )
        .is_err());
}

#[test]
fn positional_ordering_and_indexed_null_semantics() {
    let (_db, c) = setup();
    assert_eq!(
        query(&c, "SELECT id FROM users ORDER BY 1").rows,
        query(&c, "SELECT id FROM users ORDER BY id").rows
    );
    query(
        &c,
        "INSERT INTO users {id: users:nullcity, profile: {city: null}}",
    );
    assert_eq!(
        query(&c, "SELECT id FROM users u WHERE u.profile.city IS NULL")
            .rows
            .len(),
        1
    );
    assert_eq!(
        query(&c, "SELECT id FROM users u WHERE u.profile.city = NULL")
            .rows
            .len(),
        0
    );
    let plan = query(
        &c,
        "EXPLAIN QUERY PLAN SELECT name FROM users WHERE id = users:2",
    );
    assert!(format!("{:?}", plan.rows).contains("SEARCH"));
    assert!(c
        .execute("SELECT DISTINCT profile FROM users", &Parameters::new())
        .is_err());
}
