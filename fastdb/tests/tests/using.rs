use fastdb::{Database, Parameters, Value};

#[test]
fn pinned_using_join_merge_rules_and_full_join_rejection() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE a(k INTEGER,left_value INTEGER)",
        "CREATE TABLE b(k INTEGER,right_value INTEGER)",
        "INSERT INTO a VALUES(1,10),(2,20)",
        "INSERT INTO b VALUES(1,100),(3,300)",
    ] {
        query(sql);
    }
    let rows = |values: Vec<Vec<Option<i64>>>| {
        values
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(|value| value.map_or(Value::Null, Value::Integer))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    for (join, columns, expected) in [
        (
            "JOIN",
            vec!["k", "left_value", "right_value"],
            vec![vec![Some(1), Some(10), Some(100)]],
        ),
        (
            "LEFT JOIN",
            vec!["k", "left_value", "right_value"],
            vec![
                vec![Some(1), Some(10), Some(100)],
                vec![Some(2), Some(20), None],
            ],
        ),
        (
            "RIGHT JOIN",
            vec!["left_value", "k", "right_value"],
            vec![
                vec![None, Some(3), Some(300)],
                vec![Some(10), Some(1), Some(100)],
            ],
        ),
    ] {
        let result = query(&format!(
            "SELECT * FROM a {join} b USING(k) ORDER BY a.k,b.k"
        ));
        assert_eq!(result.columns, columns, "{join}");
        assert_eq!(result.rows, rows(expected), "{join}");
    }
    let qualified = query("SELECT a.*,b.* FROM a LEFT JOIN b USING(k) ORDER BY a.k,b.k");
    assert_eq!(
        qualified.columns,
        vec!["k", "left_value", "k", "right_value"]
    );
    assert_eq!(
        qualified.rows,
        rows(vec![
            vec![Some(1), Some(10), Some(1), Some(100)],
            vec![Some(2), Some(20), None, None]
        ])
    );
    let right = query("SELECT k,a.k,b.k FROM a RIGHT JOIN b USING(k) ORDER BY a.k,b.k");
    assert_eq!(
        right.rows,
        rows(vec![
            vec![Some(3), None, Some(3)],
            vec![Some(1), Some(1), Some(1)]
        ])
    );
    let error = c
        .execute("SELECT * FROM a FULL JOIN b USING(k)", &Parameters::new())
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("FULL OUTER JOIN requires an equality condition"));
}

#[test]
fn pinned_using_multiple_keys_and_chained_right_join_star_order() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE a(k INTEGER,t TEXT,a INTEGER)",
        "CREATE TABLE b(k INTEGER,t TEXT,b INTEGER)",
        "CREATE TABLE c(k INTEGER,c INTEGER)",
        "INSERT INTO a VALUES(1,'x',10),(2,'y',20)",
        "INSERT INTO b VALUES(1,'x',100),(2,'z',200)",
        "INSERT INTO c VALUES(1,1000),(2,2000)",
    ] {
        query(sql);
    }
    let multiple = query("SELECT * FROM a LEFT JOIN b USING(t,k) ORDER BY a.k");
    assert_eq!(multiple.columns, vec!["k", "t", "a", "b"]);
    assert_eq!(
        multiple.rows,
        vec![
            vec![
                Value::Integer(1),
                Value::String("x".into()),
                Value::Integer(10),
                Value::Integer(100)
            ],
            vec![
                Value::Integer(2),
                Value::String("y".into()),
                Value::Integer(20),
                Value::Null
            ],
        ]
    );
    assert_eq!(
        query("SELECT * FROM a LEFT JOIN b USING(k,t) ORDER BY a.k").rows,
        multiple.rows
    );
    let chain =
        query("SELECT k,a.k,b.k,c.k FROM a LEFT JOIN b USING(k,t) JOIN c USING(k) ORDER BY a.k");
    assert_eq!(
        chain.rows,
        vec![
            vec![Value::Integer(1); 4],
            vec![
                Value::Integer(2),
                Value::Integer(2),
                Value::Null,
                Value::Integer(2)
            ],
        ]
    );
    let right = query("SELECT * FROM a RIGHT JOIN b USING(k,t) JOIN c USING(k) ORDER BY b.k");
    assert_eq!(right.columns, vec!["c", "a", "k", "t", "b"]);
    assert_eq!(
        right.rows,
        vec![
            vec![
                Value::Integer(1000),
                Value::Integer(10),
                Value::Integer(1),
                Value::String("x".into()),
                Value::Integer(100)
            ],
            vec![
                Value::Integer(2000),
                Value::Null,
                Value::Integer(2),
                Value::String("z".into()),
                Value::Integer(200)
            ],
        ]
    );
}

#[test]
fn mixed_right_join_stars_follow_pinned_source_order() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {n:1}",
        "CREATE TABLE baseline(n INTEGER)",
        "INSERT INTO baseline VALUES(1)",
        "CREATE TABLE b(k INTEGER)",
        "INSERT INTO b VALUES(1),(2)",
        "CREATE TABLE c(v INTEGER)",
        "INSERT INTO c VALUES(10)",
    ] {
        query(sql);
    }
    for projection in ["*", "d.*,b.*,c.*", "b.k AS first,*"] {
        let sql = |source: &str| {
            format!("SELECT {projection} FROM (SELECT n AS a FROM {source}) d RIGHT JOIN b ON d.a=b.k JOIN c ON 1 ORDER BY b.k")
        };
        let expected = query(&sql("baseline"));
        let logical = sql("docs");
        let actual = query(&logical);
        assert_eq!(actual.columns, expected.columns, "{logical}");
        assert_eq!(actual.rows, expected.rows, "{logical}");
        assert_eq!(
            c.profile_select(&logical, &Parameters::new())
                .unwrap()
                .result
                .rows,
            expected.rows
        );
    }
    query("CREATE TABLE expected(x,y,z)");
    query("CREATE TABLE actual(x,y,z)");
    query("INSERT INTO expected SELECT * FROM (SELECT n AS a FROM baseline) d RIGHT JOIN b ON d.a=b.k JOIN c ON 1 ORDER BY b.k");
    query("BEGIN");
    query("INSERT INTO actual SELECT * FROM (SELECT n AS a FROM docs) d RIGHT JOIN b ON d.a=b.k JOIN c ON 1 ORDER BY b.k");
    assert_eq!(
        query("SELECT * FROM actual").rows,
        query("SELECT * FROM expected").rows
    );
    query("ROLLBACK");
    assert!(query("SELECT * FROM actual").rows.is_empty());
}
