use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn selected_documents_keep_types_and_self_inserts_are_finite() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE posts");
    q(
        &c,
        "INSERT INTO posts {id:posts:p1,n:1,tags:[true,posts:p1]}",
    );
    let rows = q(
        &c,
        "INSERT INTO posts (n,tags) SELECT n+1,tags FROM posts RETURNING *",
    )
    .rows;
    assert_eq!(rows.len(), 1);
    assert_eq!(
        q(&c, "SELECT n FROM posts ORDER BY n").rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    q(&c, "CREATE TABLE archive");
    q(
        &c,
        "INSERT INTO archive (a,b,tags) SELECT n,n,tags FROM posts ORDER BY n LIMIT 1",
    );
    let rows = q(&c, "SELECT a,b,tags FROM archive").rows;
    assert_eq!(rows[0][0], rows[0][1]);
    assert!(
        matches!(&rows[0][2],Value::Array(a) if a[0]==Value::Boolean(true) && matches!(&a[1],Value::Record(_)))
    );
}
#[test]
fn later_conflicts_rollback_copies_indexes_and_keep_outer_transaction() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE src");
    q(&c, "CREATE TABLE dst");
    q(&c, "INSERT INTO src {n:1}");
    q(&c, "INSERT INTO src {n:1}");
    q(&c, "CREATE UNIQUE INDEX dst_n ON dst (n)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO dst {n:9}");
    assert!(c
        .execute("INSERT INTO dst (n) SELECT n FROM src", &Parameters::new())
        .is_err());
    assert_eq!(
        q(&c, "SELECT n FROM dst").rows,
        vec![vec![Value::Integer(9)]]
    );
    assert!(c
        .lookup_index("dst", "dst_n", &Value::Integer(1))
        .unwrap()
        .is_empty());
    q(&c, "COMMIT");
}
#[test]
fn relational_sources_parameters_and_empty_width_validation() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE src (n INTEGER)");
    q(&c, "INSERT INTO src VALUES (1),(2)");
    q(&c, "CREATE TABLE dst");
    let params = Parameters::from([
        ("$min".into(), Value::Integer(2)),
        ("$flag".into(), Value::Boolean(true)),
    ]);
    c.execute(
        "INSERT INTO dst (n,flag) SELECT n,$flag FROM src WHERE n >= $min",
        &params,
    )
    .unwrap();
    assert_eq!(
        q(&c, "SELECT n,flag FROM dst").rows,
        vec![vec![Value::Integer(2), Value::Boolean(true)]]
    );
    assert!(c
        .execute(
            "INSERT INTO dst (a,b) SELECT n FROM src WHERE 0",
            &Parameters::new()
        )
        .is_err());
    assert!(q(
        &c,
        "INSERT INTO dst (a) SELECT n FROM src WHERE 0 RETURNING *"
    )
    .rows
    .is_empty());
}

#[test]
fn unsupported_source_clauses_never_silently_insert_a_prefix() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE dst");
    assert!(c
        .execute(
            "INSERT INTO dst (n) VALUES (1) UNION ALL SELECT 2",
            &Parameters::new()
        )
        .is_err());
    assert!(q(&c, "SELECT * FROM dst").rows.is_empty());
}
