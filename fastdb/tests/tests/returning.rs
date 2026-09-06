use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
fn setup() -> (Database, fastdb::Connection) {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE posts");
    (db, c)
}
#[test]
fn projections_see_inserted_updated_and_deleted_typed_values() {
    let (_db, c) = setup();
    let inserted=q(&c,"INSERT INTO posts (id,n,tags) VALUES (posts:p1,2,array::new(posts:p2)) RETURNING id,n+1 AS next,tags");
    assert_eq!(inserted.affected, 1);
    assert_eq!(inserted.columns, vec!["id", "next", "tags"]);
    assert!(matches!(&inserted.rows[0][0], Value::Record(_)));
    assert_eq!(inserted.rows[0][1], Value::Integer(3));
    assert!(matches!(&inserted.rows[0][2], Value::Array(_)));
    assert_eq!(
        q(
            &c,
            "UPDATE posts SET n=n+2 RETURNING record::id(id) AS key,n"
        )
        .rows,
        vec![vec![Value::String("p1".into()), Value::Integer(4)]]
    );
    assert_eq!(
        q(
            &c,
            "DELETE FROM posts RETURNING n,doc::get(tags,'$[0]') AS ref"
        )
        .rows[0][0],
        Value::Integer(4)
    );
    assert!(q(&c, "SELECT * FROM posts").rows.is_empty());
}
#[test]
fn projection_failure_rolls_back_data_and_indexes_for_all_write_kinds() {
    let (_db, c) = setup();
    q(&c, "CREATE UNIQUE INDEX post_n ON posts (n)");
    q(&c, "INSERT INTO posts (n) VALUES (1),(2)");
    q(&c, "BEGIN");
    for sql in [
        "INSERT INTO posts (n) VALUES (3),(4) RETURNING CASE WHEN n=3 THEN n ELSE array::append(1,2) END AS v",
        "UPDATE posts SET n=n+10 RETURNING CASE WHEN n=11 THEN n ELSE array::append(1,2) END AS v",
        "DELETE FROM posts RETURNING CASE WHEN n=1 THEN n ELSE array::append(1,2) END AS v"
    ] {
        assert!(c.execute(sql,&Parameters::new()).is_err());
        assert_eq!(q(&c,"SELECT n FROM posts ORDER BY n").rows,vec![vec![Value::Integer(1)],vec![Value::Integer(2)]]);
        assert_eq!(c.lookup_index("posts","post_n",&Value::Integer(1)).unwrap().len(),1);
    }
    let error = c
        .execute("COMMIT", &Parameters::new())
        .expect_err("pinned engine aborts outer transaction on UDF errors");
    assert!(error.to_string().contains("no transaction is active"));
    q(&c, "BEGIN");
    q(&c, "INSERT INTO posts (n) VALUES (9) RETURNING n");
    q(&c, "COMMIT");
}
#[test]
fn empty_results_keep_metadata_and_bound_results_keep_types() {
    let (_db, c) = setup();
    let params = Parameters::from([("$flag".into(), Value::Boolean(true))]);
    let empty = c
        .execute("UPDATE posts SET n=1 RETURNING n,$flag AS flag", &params)
        .unwrap();
    assert_eq!(empty.columns, vec!["n", "flag"]);
    assert!(empty.rows.is_empty());
    assert_eq!(empty.affected, 0);
    let row = c
        .execute(
            "INSERT INTO posts (n) VALUES (1) RETURNING $flag AS flag",
            &params,
        )
        .unwrap();
    assert_eq!(row.rows, vec![vec![Value::Boolean(true)]]);
    for projection in [
        "count(*)",
        "sum(n)",
        "min(n)",
        "(SELECT 1)",
        "row_number() OVER ()",
    ] {
        assert!(c
            .execute(
                &format!("DELETE FROM posts RETURNING {projection}"),
                &Parameters::new()
            )
            .is_err());
    }
    assert_eq!(q(&c, "SELECT n FROM posts").rows.len(), 1);
}
#[test]
fn insert_select_returning_and_qualified_snapshot_fields() {
    let (_db, c) = setup();
    q(&c, "INSERT INTO posts (n) VALUES (1),(2)");
    let result = q(
        &c,
        "INSERT INTO posts (n) SELECT n+10 FROM posts RETURNING posts.n AS n, *",
    );
    assert_eq!(result.affected, 2);
    assert!(result
        .rows
        .iter()
        .all(|r| matches!(&r[1], Value::Object(_))));
}
