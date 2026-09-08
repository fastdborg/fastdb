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

#[test]
fn pinned_using_collation_follows_normalized_join_operand_order() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE a(k TEXT COLLATE NOCASE,a INTEGER)",
        "CREATE TABLE b(k TEXT COLLATE BINARY,b INTEGER)",
        "INSERT INTO a VALUES('A',1)",
        "INSERT INTO b VALUES('a',2)",
    ] {
        query(sql);
    }
    let matched = vec![vec![Value::Integer(1), Value::Integer(2)]];
    for join in ["JOIN", "LEFT JOIN"] {
        assert_eq!(
            query(&format!("SELECT a.a,b.b FROM a {join} b USING(k)")).rows,
            matched
        );
    }
    assert_eq!(
        query("SELECT a.a,b.b FROM a RIGHT JOIN b USING(k)").rows,
        vec![vec![Value::Null, Value::Integer(2)]]
    );
    assert_eq!(
        query("SELECT a.a,b.b FROM a RIGHT JOIN b ON a.k=b.k").rows,
        matched
    );
    assert!(query("SELECT a.a,b.b FROM b JOIN a USING(k)")
        .rows
        .is_empty());
    assert_eq!(
        query("SELECT a.a,b.b FROM b RIGHT JOIN a USING(k)").rows,
        matched
    );
}

#[test]
fn collection_using_keys_and_closed_stars_match_native_joins() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {k:1,left_value:10}",
        "INSERT INTO docs {k:2,left_value:20}",
        "CREATE TABLE baseline(k INTEGER,left_value INTEGER)",
        "INSERT INTO baseline VALUES(1,10),(2,20)",
        "CREATE TABLE b(k INTEGER,right_value INTEGER)",
        "INSERT INTO b VALUES(1,100),(3,300)",
        "CREATE TABLE c(k INTEGER,last_value INTEGER)",
        "INSERT INTO c VALUES(1,1000),(2,2000),(3,3000)",
    ] {
        query(sql);
    }
    for join in ["JOIN", "LEFT JOIN", "RIGHT JOIN"] {
        for chain in ["", " JOIN c USING(k)"] {
            for projection in ["*", "a.*,b.*", "k,a.k,b.k", "k+1 AS next"] {
                let sql = |source: &str| {
                    format!("SELECT {projection} FROM {source} a {join} b USING(k){chain} ORDER BY a.k,b.k")
                };
                let expected = query(&sql("baseline"));
                let logical = sql("(SELECT k,left_value FROM docs)");
                let actual = query(&logical);
                assert_eq!(actual.columns, expected.columns, "{logical}");
                assert_eq!(actual.rows, expected.rows, "{logical}");
                assert_eq!(
                    c.profile_select(&logical, &Parameters::new())
                        .unwrap()
                        .result
                        .rows,
                    expected.rows,
                    "{logical}"
                );
            }
        }
        let sql = |source: &str| {
            format!("SELECT k,a.k,b.k FROM {source} a {join} b USING(k) ORDER BY a.k,b.k")
        };
        assert_eq!(query(&sql("docs")).rows, query(&sql("baseline")).rows);
    }
}

#[test]
fn collection_using_preserves_collation_and_atomic_typed_writes() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {k:'A',left_value:1}",
        "CREATE TABLE baseline(k TEXT,left_value INTEGER)",
        "INSERT INTO baseline VALUES('A',1)",
        "CREATE TABLE b(k TEXT COLLATE BINARY,right_value INTEGER)",
        "INSERT INTO b VALUES('a',2)",
    ] {
        query(sql);
    }
    for join in ["JOIN", "LEFT JOIN", "RIGHT JOIN"] {
        let sql = |source: &str| {
            format!("SELECT a.left_value,b.right_value FROM (SELECT k COLLATE NOCASE AS k,left_value FROM {source}) a {join} b USING(k)")
        };
        assert_eq!(
            query(&sql("docs")).rows,
            query(&sql("baseline")).rows,
            "{join}"
        );
    }
    query("CREATE TABLE refs");
    query("INSERT INTO refs {k:docs:a}");
    query("INSERT INTO refs {k:docs:b}");
    query("CREATE TABLE copied");
    query("CREATE UNIQUE INDEX copied_k ON copied(k)");
    query("BEGIN");
    let insert = "INSERT INTO copied(k) SELECT k FROM refs a JOIN refs b USING(k)";
    query(insert);
    let expected = query("SELECT k FROM refs ORDER BY k");
    assert_eq!(query("SELECT k FROM copied ORDER BY k").rows, expected.rows);
    assert!(c.execute(insert, &Parameters::new()).is_err());
    assert_eq!(query("SELECT k FROM copied ORDER BY k").rows, expected.rows);
    query("ROLLBACK");
    assert!(query("SELECT * FROM copied WHERE k=docs:a").rows.is_empty());
    query(insert);
    assert_eq!(query("SELECT k FROM copied ORDER BY k").rows, expected.rows);
}

#[test]
fn collection_multi_key_using_preserves_outer_filters_and_key_order() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {k:1,t:'x',v:10}",
        "INSERT INTO docs {k:2,t:'y',v:20}",
        "CREATE TABLE baseline(k INTEGER,t TEXT,v INTEGER)",
        "INSERT INTO baseline VALUES(1,'x',10),(2,'y',20)",
        "CREATE TABLE b(k INTEGER,t TEXT,w INTEGER)",
        "INSERT INTO b VALUES(1,'x',100),(2,'z',200)",
    ] {
        query(sql);
    }
    for keys in ["k,t", "t,k", "K,\"t\""] {
        for join in ["JOIN", "LEFT JOIN", "RIGHT JOIN"] {
            for filter in ["", " WHERE b.w IS NULL", " WHERE k=2"] {
                let sql = |source: &str| {
                    format!("SELECT * FROM (SELECT k,t,v FROM {source}) a {join} b USING({keys}){filter} ORDER BY a.k,b.k")
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
                    expected.rows,
                    "{logical}"
                );
            }
        }
    }
}

#[test]
fn using_group_aliases_and_ordering_match_native_closed_sources() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {k:1}",
        "INSERT INTO docs {k:2}",
        "CREATE TABLE baseline(k INTEGER)",
        "INSERT INTO baseline VALUES(1),(2)",
        "CREATE TABLE b(k INTEGER)",
        "INSERT INTO b VALUES(1),(1),(2)",
    ] {
        query(sql);
    }
    for (projection, group, having) in [
        ("k,count(*) AS total", "k", "total>1"),
        ("k+10 AS k,count(*) AS total", "k", "k>10"),
        ("k-k AS k,count(*) AS total", "k", "total>0"),
        ("k%2 AS parity,count(*) AS total", "1", "total>0"),
    ] {
        for ordering in ["1,total", "total DESC,1"] {
            let sql = |source: &str| {
                format!("SELECT {projection} FROM {source} a JOIN b USING(k) GROUP BY {group} HAVING {having} ORDER BY {ordering}")
            };
            let expected = query(&sql("baseline"));
            let logical = sql("(SELECT k FROM docs)");
            let actual = query(&logical);
            assert_eq!(actual.columns, expected.columns, "{logical}");
            assert_eq!(actual.rows, expected.rows, "{logical}");
            assert_eq!(
                c.profile_select(&logical, &Parameters::new())
                    .unwrap()
                    .result
                    .rows,
                expected.rows,
                "{logical}"
            );
        }
    }
}
