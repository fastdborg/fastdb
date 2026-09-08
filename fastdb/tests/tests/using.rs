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

#[test]
fn using_window_keys_and_windowed_writes_preserve_native_results() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| c.execute(sql, &Parameters::new()).unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {k:1}",
        "INSERT INTO docs {k:2}",
        "CREATE TABLE baseline(k INTEGER)",
        "INSERT INTO baseline VALUES(1),(2)",
        "CREATE TABLE b(k INTEGER,n INTEGER)",
        "INSERT INTO b VALUES(1,10),(1,20),(2,30)",
    ] {
        query(sql);
    }
    for (window, clause) in [
        ("(PARTITION BY k ORDER BY b.n)", ""),
        ("w", " WINDOW w AS (PARTITION BY k ORDER BY b.n)"),
    ] {
        let sql = |source: &str| {
            format!("SELECT k,row_number() OVER {window} AS r,sum(b.n) OVER {window} AS total FROM {source} a JOIN b USING(k){clause} ORDER BY k,b.n")
        };
        let expected = query(&sql("baseline"));
        for source in ["docs", "(SELECT k FROM docs)"] {
            let logical = sql(source);
            let actual = query(&logical);
            assert_eq!(actual.columns, expected.columns);
            assert_eq!(actual.rows, expected.rows, "{logical}");
            assert_eq!(
                c.profile_select(&logical, &Parameters::new())
                    .unwrap()
                    .result
                    .rows,
                expected.rows
            );
        }
    }
    query("CREATE TABLE copied(k INTEGER,r INTEGER CHECK(r=1))");
    query("INSERT INTO copied VALUES(99,1)");
    query("BEGIN");
    let insert = "INSERT INTO copied SELECT k,row_number() OVER(PARTITION BY k ORDER BY b.n) FROM docs a JOIN b USING(k)";
    assert!(c.execute(insert, &Parameters::new()).is_err());
    assert_eq!(
        query("SELECT * FROM copied").rows,
        vec![vec![Value::Integer(99), Value::Integer(1)]]
    );
    query(&format!("{insert} WHERE b.n<>20"));
    assert_eq!(
        query("SELECT count(*) FROM copied").rows,
        vec![vec![Value::Integer(3)]]
    );
    query("ROLLBACK");
    assert_eq!(
        query("SELECT * FROM copied").rows,
        vec![vec![Value::Integer(99), Value::Integer(1)]]
    );
}

#[test]
fn source_free_subqueries_resolve_outer_using_keys_without_local_capture() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| {
        c.execute(sql, &Parameters::new())
            .unwrap_or_else(|error| panic!("{sql}: {error}"))
    };
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {k:1}",
        "INSERT INTO docs {k:2}",
        "CREATE TABLE baseline(k INTEGER)",
        "INSERT INTO baseline VALUES(1),(2)",
        "CREATE TABLE b(k INTEGER)",
        "INSERT INTO b VALUES(1),(2),(3)",
        "CREATE TABLE local_values(k INTEGER)",
        "INSERT INTO local_values VALUES(99)",
    ] {
        query(sql);
    }
    for join in ["JOIN", "LEFT JOIN", "RIGHT JOIN"] {
        for expression in [
            "(SELECT k)",
            "(SELECT k+10 WHERE k=2)",
            "EXISTS(SELECT k WHERE k=2)",
            "(SELECT k FROM local_values)",
            "(SELECT 99 AS k WHERE k=1)",
            "(SELECT 99 AS k WHERE k=99)",
            "(SELECT k AS v ORDER BY v)",
            "(SELECT k ORDER BY k)",
            "(SELECT 99 AS k ORDER BY k)",
            "(SELECT k WHERE EXISTS(SELECT 1))",
            "(SELECT k LIMIT 0)",
        ] {
            let sql = |source: &str| {
                format!(
                    "SELECT k,{expression} AS value FROM {source} a {join} b USING(k) ORDER BY k"
                )
            };
            let expected = query(&sql("baseline"));
            for source in ["docs", "(SELECT k FROM docs)"] {
                let logical = sql(source);
                let actual = query(&logical);
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
fn correlated_using_preserves_typed_keys_and_atomic_writes() {
    for (first, second) in [
        ("docs:a", "docs:b"),
        ("true", "false"),
        ("x'00ff'", "x'0100'"),
    ] {
        let db = Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        let query = |sql: &str| {
            c.execute(sql, &Parameters::new())
                .unwrap_or_else(|error| panic!("{sql}: {error}"))
        };
        query("CREATE TABLE docs");
        query("CREATE TABLE other");
        for (table, value) in [("docs", first), ("other", first), ("other", second)] {
            if value.starts_with("x'") {
                query(&format!("INSERT INTO {table}(k) VALUES({value})"));
            } else {
                query(&format!("INSERT INTO {table} {{k:{value}}}"));
            }
        }
        let stored = query("SELECT k FROM docs");
        assert!(match &stored.rows[0][0] {
            Value::Record(record) => first == "docs:a" && record.table == "docs",
            Value::Boolean(value) => first == "true" && *value,
            Value::Binary(value) => first == "x'00ff'" && value == &[0, 255],
            _ => false,
        });
        for join in ["JOIN", "LEFT JOIN", "RIGHT JOIN"] {
            for source in ["docs", "(SELECT k FROM docs)"] {
                for (suffix, empty) in [
                    ("", false),
                    (" ORDER BY k", false),
                    (" AS v ORDER BY v LIMIT 1", false),
                    (" ORDER BY k LIMIT 0", true),
                    (" AS v ORDER BY v LIMIT 1 OFFSET 1", true),
                ] {
                    let sql = format!("SELECT k,(SELECT k{suffix}) AS correlated FROM {source} a {join} other b USING(k) ORDER BY k");
                    let result = query(&sql);
                    let expected = query(if join == "RIGHT JOIN" {
                        "SELECT k FROM other ORDER BY k"
                    } else {
                        "SELECT k FROM docs ORDER BY k"
                    });
                    assert_eq!(result.rows.len(), expected.rows.len(), "{sql}");
                    for (row, expected) in result.rows.iter().zip(&expected.rows) {
                        assert_eq!(
                            row,
                            &vec![
                                expected[0].clone(),
                                if empty {
                                    Value::Null
                                } else {
                                    expected[0].clone()
                                }
                            ],
                            "{sql}"
                        );
                    }
                    assert_eq!(
                        c.profile_select(&sql, &Parameters::new())
                            .unwrap()
                            .result
                            .rows,
                        result.rows
                    );
                }
            }
        }
        query("CREATE TABLE copied");
        query("CREATE UNIQUE INDEX copied_k ON copied(k)");
        let insert =
            "INSERT INTO copied(k) SELECT (SELECT k ORDER BY k) FROM docs a RIGHT JOIN other b USING(k)";
        query("BEGIN");
        query(insert);
        let expected = query("SELECT k FROM other ORDER BY k");
        assert_eq!(query("SELECT k FROM copied ORDER BY k").rows, expected.rows);
        assert!(c.execute(insert, &Parameters::new()).is_err());
        assert_eq!(query("SELECT k FROM copied ORDER BY k").rows, expected.rows);
        query("ROLLBACK");
        assert!(query("SELECT * FROM copied").rows.is_empty());
        query(insert);
        assert_eq!(query("SELECT k FROM copied ORDER BY k").rows, expected.rows);
    }
}

#[test]
fn using_duplicate_key_columns_preserve_first_lookup_and_star_positions() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| {
        c.execute(sql, &Parameters::new())
            .unwrap_or_else(|error| panic!("{sql}: {error}"))
    };
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {k:1}",
        "INSERT INTO docs {k:2}",
        "CREATE TABLE baseline(k INTEGER)",
        "INSERT INTO baseline VALUES(1),(2)",
        "CREATE TABLE b(k INTEGER)",
        "INSERT INTO b VALUES(1),(3)",
    ] {
        query(sql);
    }
    for (left, right) in [
        ("k,k+10 AS k", "k"),
        ("k", "k,k+20 AS k"),
        ("k,k+10 AS k", "k,k+20 AS k"),
    ] {
        for join in ["JOIN", "LEFT JOIN", "RIGHT JOIN"] {
            for projection in ["*", "a.*,b.*", "k,a.k,b.k"] {
                let sql = |source: &str| {
                    format!(
                    "SELECT {projection} FROM (SELECT {left} FROM {source}) a {join} (SELECT {right} FROM b) b USING(k) ORDER BY a.k,b.k"
                )
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
fn using_optional_keys_match_null_baseline_and_missing_columns_do_not_poison_connection() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| {
        c.execute(sql, &Parameters::new())
            .unwrap_or_else(|error| panic!("{sql}: {error}"))
    };
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {n:1,k:1}",
        "INSERT INTO docs {n:2,k:null}",
        "INSERT INTO docs {n:3}",
        "CREATE TABLE baseline(n INTEGER,k INTEGER)",
        "INSERT INTO baseline VALUES(1,1),(2,NULL),(3,NULL)",
        "CREATE TABLE b(m INTEGER,k INTEGER)",
        "INSERT INTO b VALUES(10,1),(20,NULL),(30,2)",
    ] {
        query(sql);
    }
    for join in ["JOIN", "LEFT JOIN", "RIGHT JOIN"] {
        for source in ["docs", "(SELECT n,k FROM docs)"] {
            for filter in ["", " WHERE k IS NULL", " WHERE k IS NOT NULL"] {
                let sql = |source: &str| {
                    format!(
                    "SELECT a.n,b.m,k,a.k,b.k FROM {source} a {join} b USING(k){filter} ORDER BY a.n,b.m"
                )
                };
                let expected = query(&sql("baseline"));
                let logical = sql(source);
                assert_eq!(query(&logical).rows, expected.rows, "{logical}");
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
    query("BEGIN");
    for invalid in [
        "SELECT * FROM (SELECT n FROM docs) a JOIN b USING(k)",
        "SELECT * FROM docs a JOIN (SELECT m FROM b) b USING(k)",
    ] {
        assert!(c.execute(invalid, &Parameters::new()).is_err(), "{invalid}");
        query("INSERT INTO docs {n:4,k:4}");
        query("DELETE FROM docs WHERE n=4");
    }
    query("ROLLBACK");
    assert_eq!(
        query("SELECT count(*) FROM docs").rows,
        vec![vec![Value::Integer(3)]]
    );
}

#[test]
fn using_managed_indexes_preserve_join_results_through_updates_and_rollback() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let query = |sql: &str| {
        c.execute(sql, &Parameters::new())
            .unwrap_or_else(|error| panic!("{sql}: {error}"))
    };
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {n:1,k:1}",
        "INSERT INTO docs {n:2,k:2}",
        "INSERT INTO docs {n:3,k:null}",
        "CREATE TABLE other",
        "INSERT INTO other {m:10,k:1}",
        "INSERT INTO other {m:20,k:3}",
        "INSERT INTO other {m:30,k:null}",
    ] {
        query(sql);
    }
    let queries: Vec<_> = ["JOIN", "LEFT JOIN", "RIGHT JOIN"]
        .into_iter()
        .flat_map(|join| {
            ["", " WHERE k=1", " WHERE k IS NULL"].into_iter().map(move |filter| {
            format!("SELECT a.n,b.m,k FROM docs a {join} other b USING(k){filter} ORDER BY a.n,b.m")
        })
        })
        .collect();
    let before: Vec<_> = queries.iter().map(|sql| query(sql).rows).collect();
    query("CREATE INDEX docs_k ON docs(k)");
    query("CREATE INDEX other_k ON other(k)");
    let plan = query(&format!("EXPLAIN QUERY PLAN {}", queries[1]));
    assert!(
        plan.rows.iter().flatten().any(|value| {
            matches!(value, Value::String(detail) if detail.contains("USING INDEX docs_k"))
        }),
        "{:?}",
        plan.rows
    );
    for (sql, expected) in queries.iter().zip(&before) {
        assert_eq!(query(sql).rows, *expected, "{sql}");
        assert_eq!(
            c.profile_select(sql, &Parameters::new())
                .unwrap()
                .result
                .rows,
            *expected,
            "{sql}"
        );
    }
    query("BEGIN");
    query("UPDATE docs SET k=3 WHERE n=1");
    query("DELETE FROM other WHERE m=30");
    let indexed: Vec<_> = queries.iter().map(|sql| query(sql).rows).collect();
    assert_ne!(indexed[0], before[0]);
    query("DROP INDEX docs_k");
    query("DROP INDEX other_k");
    for (sql, expected) in queries.iter().zip(&indexed) {
        assert_eq!(query(sql).rows, *expected, "{sql}");
    }
    query("ROLLBACK");
    for (sql, expected) in queries.iter().zip(&before) {
        assert_eq!(query(sql).rows, *expected, "{sql}");
    }
    // Transactional index DDL must have restored both catalog entries too.
    query("DROP INDEX docs_k");
    query("DROP INDEX other_k");
}
