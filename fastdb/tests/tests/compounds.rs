use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn union_all_preserves_duplicates_types_order_and_nested_sources() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {id:docs:a,n:10,flag:true}");
    q(&c, "INSERT INTO docs {id:docs:b,n:2,flag:false}");
    let baseline = q(&c, "SELECT id,n,flag FROM docs ORDER BY n");
    let rows = q(
        &c,
        "SELECT id,n,flag FROM docs UNION ALL SELECT id,n,flag FROM docs ORDER BY n",
    );
    assert_eq!(rows.columns, baseline.columns);
    assert_eq!(
        rows.rows,
        vec![
            baseline.rows[0].clone(),
            baseline.rows[0].clone(),
            baseline.rows[1].clone(),
            baseline.rows[1].clone()
        ]
    );
    assert_eq!(
        q(
            &c,
            "SELECT n FROM docs UNION ALL SELECT 3 ORDER BY 1 LIMIT 2 OFFSET 1"
        )
        .rows,
        vec![vec![Value::Integer(3)], vec![Value::Integer(10)]]
    );
    assert_eq!(
        q(
            &c,
            "WITH v(x) AS (SELECT n FROM docs UNION ALL SELECT 3) SELECT x FROM v ORDER BY x"
        )
        .rows,
        vec![
            vec![Value::Integer(2)],
            vec![Value::Integer(3)],
            vec![Value::Integer(10)]
        ]
    );
    assert_eq!(
        q(
            &c,
            "SELECT v.x FROM (SELECT n AS x FROM docs UNION ALL SELECT 3) v ORDER BY x"
        )
        .rows,
        vec![
            vec![Value::Integer(2)],
            vec![Value::Integer(3)],
            vec![Value::Integer(10)]
        ]
    );
    q(&c, "CREATE TABLE native(data BLOB)");
    q(&c, "INSERT INTO native VALUES (X'00FF')");
    assert_eq!(
        q(
            &c,
            "SELECT data AS x FROM native UNION ALL SELECT id FROM docs WHERE n=2"
        )
        .rows,
        vec![
            vec![Value::Binary(vec![0, 255])],
            vec![baseline.rows[0][0].clone()]
        ]
    );
    assert_eq!(
        q(
            &c,
            "SELECT id AS x FROM docs WHERE n=2 UNION ALL SELECT data FROM native"
        )
        .rows,
        vec![
            vec![baseline.rows[0][0].clone()],
            vec![Value::Binary(vec![0, 255])]
        ]
    );
    let params = Parameters::from([
        ("$flag".into(), Value::Boolean(true)),
        ("$data".into(), Value::Binary(vec![0, 255])),
    ]);
    assert_eq!(
        c.execute("SELECT $data AS x UNION ALL SELECT $flag", &params)
            .unwrap()
            .rows,
        vec![
            vec![Value::Binary(vec![0, 255])],
            vec![Value::Boolean(true)]
        ]
    );
    assert_eq!(
        q(&c, "VALUES (docs:a) UNION ALL VALUES (docs:b)").rows,
        vec![
            vec![baseline.rows[1][0].clone()],
            vec![baseline.rows[0][0].clone()]
        ]
    );
    assert!(!q(&c, "EXPLAIN SELECT n FROM docs UNION ALL SELECT 3")
        .rows
        .is_empty());
    assert!(!q(
        &c,
        "EXPLAIN QUERY PLAN SELECT n FROM docs UNION ALL SELECT 3"
    )
    .rows
    .is_empty());
    assert_eq!(
        q(
            &c,
            "SELECT '__fastdb_harmless' UNION SELECT '__fastdb_harmless'"
        )
        .rows
        .len(),
        1
    );
}
#[test]
fn union_all_insert_sources_are_atomic_and_native_conflicts_remain_native() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE source");
    q(&c, "INSERT INTO source {n:1}");
    q(&c, "CREATE TABLE target");
    q(&c, "CREATE UNIQUE INDEX target_n ON target(n)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO target {n:9}");
    assert!(c
        .execute(
            "INSERT INTO target(n) SELECT n FROM source UNION ALL SELECT n FROM source",
            &Parameters::new()
        )
        .is_err());
    assert_eq!(
        q(&c, "SELECT n FROM target").rows,
        vec![vec![Value::Integer(9)]]
    );
    assert_eq!(
        c.check_collection_integrity("target", Default::default())
            .unwrap()
            .documents,
        1
    );
    assert_eq!(
        q(
            &c,
            "INSERT INTO target(n) SELECT n FROM source UNION ALL SELECT 2 RETURNING n"
        )
        .affected,
        2
    );
    q(&c, "CREATE TABLE native(n INTEGER UNIQUE)");
    assert_eq!(
        q(
            &c,
            "INSERT OR IGNORE INTO native SELECT n FROM source UNION ALL SELECT n FROM source"
        )
        .affected,
        1
    );
    assert_eq!(
        q(&c, "SELECT n FROM native").rows,
        vec![vec![Value::Integer(1)]]
    );
    for sql in [
        "SELECT n FROM source UNION SELECT n FROM source",
        "SELECT n FROM source EXCEPT SELECT n FROM source",
        "SELECT n FROM source INTERSECT SELECT n FROM source",
        "INSERT INTO target(n) SELECT n FROM source UNION ALL SELECT 1,2",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(
        q(
            &c,
            "INSERT INTO target(n) VALUES (4) UNION ALL SELECT 5 RETURNING n"
        )
        .affected,
        2
    );
    q(&c, "ROLLBACK");
    assert_eq!(
        c.check_collection_integrity("target", Default::default())
            .unwrap()
            .documents,
        0
    );
}

#[test]
fn union_all_scalar_ordering_matches_native_sql_and_errors_preserve_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE TABLE baseline(n,t,data)");
    for values in ["(2,'z',X'00')", "(10,'A',X'FF')", "(NULL,NULL,NULL)"] {
        q(&c, &format!("INSERT INTO docs(n,t,data) VALUES {values}"));
        q(&c, &format!("INSERT INTO baseline VALUES {values}"));
    }
    for suffix in [
        "ORDER BY n",
        "ORDER BY n DESC NULLS FIRST",
        "ORDER BY t COLLATE NOCASE,n",
        "ORDER BY 3 DESC,1",
        "ORDER BY n LIMIT 3 OFFSET 1",
    ] {
        let query =
            format!("SELECT n,t,data FROM docs UNION ALL SELECT n,t,data FROM docs {suffix}");
        // The pinned engine rejects COLLATE directly in compound ORDER BY.
        let baseline = if suffix.contains("COLLATE") {
            format!("SELECT * FROM (SELECT n,t,data FROM baseline UNION ALL SELECT n,t,data FROM baseline) {suffix}")
        } else {
            query.replace("docs", "baseline")
        };
        assert_eq!(q(&c, &query).rows, q(&c, &baseline).rows, "{query}");
    }
    let empty=q(&c,"WITH v(x) AS (SELECT n FROM docs WHERE 0 UNION ALL SELECT n FROM docs WHERE 0) SELECT x FROM v");
    assert_eq!(empty.columns, vec!["x"]);
    assert!(empty.rows.is_empty());
    let profile = c
        .profile_select(
            "SELECT n FROM docs UNION ALL SELECT n FROM docs",
            &Parameters::new(),
        )
        .unwrap();
    assert_eq!(profile.result.rows.len(), 6);
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs(n) VALUES (99)");
    for (sql, params) in [
        (
            "INSERT INTO docs(n) SELECT n FROM docs UNION ALL SELECT $missing",
            Parameters::new(),
        ),
        (
            "INSERT INTO docs(n) SELECT n FROM docs UNION ALL SELECT 1",
            Parameters::from([("$unused".into(), Value::Integer(1))]),
        ),
        (
            "INSERT INTO docs(n) SELECT n FROM docs UNION ALL SELECT 1 ORDER BY 2",
            Parameters::new(),
        ),
    ] {
        assert!(c.execute(sql, &params).is_err(), "{sql}");
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        assert_eq!(
            c.check_collection_integrity("docs", Default::default())
                .unwrap()
                .documents,
            4
        );
    }
    q(&c, "ROLLBACK");
}
