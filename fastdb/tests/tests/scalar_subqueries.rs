use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}

#[test]
fn collection_scalar_subqueries_preserve_values_and_empty_nulls() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(
        &c,
        "INSERT INTO docs {id:docs:a,n:1,link:docs:b,items:[1,true],meta:{ok:true}}",
    );
    q(&c, "INSERT INTO docs {id:docs:b,n:2}");
    for field in ["link", "items", "meta", "n"] {
        assert_eq!(
            q(
                &c,
                &format!("SELECT (SELECT {field} FROM docs WHERE id=docs:a) AS v")
            )
            .rows,
            q(
                &c,
                &format!("SELECT {field} AS v FROM docs WHERE id=docs:a")
            )
            .rows
        );
    }
    assert_eq!(
        q(&c, "SELECT (SELECT n FROM docs WHERE n=99) AS v").rows,
        vec![vec![Value::Null]]
    );
    assert_eq!(
        q(&c, "SELECT (SELECT n FROM docs ORDER BY n DESC) AS v").rows,
        vec![vec![Value::Integer(2)]]
    );
    assert_eq!(
        q(&c, "SELECT n FROM docs WHERE n=(SELECT max(n) FROM docs)").rows,
        vec![vec![Value::Integer(2)]]
    );
    assert_eq!(
        q(&c, "SELECT (SELECT n FROM docs ORDER BY n LIMIT 1)+10 AS v").rows,
        vec![vec![Value::Integer(11)]]
    );
    assert_eq!(
        q(
            &c,
            "SELECT (SELECT (SELECT n FROM docs ORDER BY n LIMIT 1) AS v FROM docs LIMIT 1) AS v"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    let binary = Value::Binary(vec![0x46, 0x44, 0x42, 0, 1, 2]);
    let projected = c
        .execute(
            "SELECT (SELECT $v FROM docs LIMIT 1) AS v",
            &Parameters::from([("$v".into(), binary.clone())]),
        )
        .unwrap();
    assert_eq!(projected.rows, vec![vec![binary]]);
    assert!(c
        .execute(
            "SELECT d.n,(SELECT x.n FROM docs x WHERE x.n=d.n) AS v FROM docs d",
            &Parameters::new()
        )
        .is_err());
    assert!(c
        .execute("SELECT (SELECT n,link FROM docs) AS v", &Parameters::new())
        .is_err());
}

#[test]
fn scalar_subqueries_share_parameters_ctes_and_atomic_insert_sources() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {n:1}");
    q(&c, "INSERT INTO docs {n:2}");
    let result = c
        .execute(
            "SELECT (SELECT n FROM docs WHERE n=$n) AS v",
            &Parameters::from([("$n".into(), Value::Integer(2))]),
        )
        .unwrap();
    assert_eq!(result.rows, vec![vec![Value::Integer(2)]]);
    assert_eq!(
        q(
            &c,
            "WITH chosen AS (SELECT n FROM docs WHERE n=2) SELECT (SELECT n FROM chosen) AS v"
        )
        .rows,
        result.rows
    );
    q(&c, "CREATE TABLE copies");
    q(&c, "DEFINE FIELD n ON copies TYPE integer REQUIRED");
    q(
        &c,
        "INSERT INTO copies(n) SELECT (SELECT max(n) FROM docs) AS v",
    );
    assert_eq!(q(&c, "SELECT n FROM copies").rows, result.rows);
    assert!(c
        .execute(
            "INSERT INTO copies(n) SELECT (SELECT n FROM docs WHERE n=99) AS v",
            &Parameters::new()
        )
        .is_err());
    assert_eq!(
        c.check_collection_integrity("copies", Default::default())
            .unwrap()
            .documents,
        1
    );
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(
        &c,
        "INSERT INTO native SELECT (SELECT max(n) FROM docs) AS v",
    );
    assert_eq!(q(&c, "SELECT n FROM native").rows, result.rows);
}

#[test]
fn collection_exists_matches_native_membership_and_insert_sources() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {n:1}");
    q(&c, "INSERT INTO docs {n:2}");
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "INSERT INTO native VALUES (1),(2)");
    for inner in [
        "SELECT n FROM SOURCE",
        "SELECT n,n FROM SOURCE WHERE n=99",
        "SELECT n,n FROM SOURCE WHERE n=2",
        "SELECT count(*) FROM SOURCE WHERE n=99",
        "SELECT n FROM SOURCE LIMIT 0",
        "SELECT n FROM SOURCE LIMIT 1 OFFSET 2",
        "SELECT n FROM SOURCE GROUP BY n HAVING n>1",
    ] {
        for prefix in ["", "NOT "] {
            let logical = format!(
                "SELECT {prefix}EXISTS ({}) AS v",
                inner.replace("SOURCE", "docs")
            );
            let native = format!(
                "SELECT {prefix}EXISTS ({}) AS v",
                inner.replace("SOURCE", "native")
            );
            assert_eq!(q(&c, &logical).rows, q(&c, &native).rows, "{logical}");
        }
    }
    assert_eq!(
        q(
            &c,
            "SELECT n FROM docs WHERE EXISTS (SELECT n FROM docs WHERE n=2) ORDER BY n"
        )
        .rows,
        q(&c, "SELECT n FROM native ORDER BY n").rows
    );
    assert!(q(
        &c,
        "SELECT n FROM docs WHERE NOT EXISTS (SELECT n FROM docs)"
    )
    .rows
    .is_empty());
    assert_eq!(
        q(&c, "SELECT EXISTS (SELECT * FROM docs) AS v").rows,
        vec![vec![Value::Integer(1)]]
    );
    assert_eq!(
        q(
            &c,
            "SELECT EXISTS (SELECT n FROM docs UNION SELECT n FROM docs) AS v"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    let result = c
        .execute(
            "WITH v AS (SELECT n FROM docs WHERE n=$n) SELECT EXISTS (SELECT n FROM v) AS v",
            &Parameters::from([("$n".into(), Value::Integer(2))]),
        )
        .unwrap();
    assert_eq!(result.rows, vec![vec![Value::Integer(1)]]);
    q(&c, "CREATE TABLE copies");
    q(
        &c,
        "INSERT INTO copies(n) SELECT EXISTS (SELECT n FROM docs) AS n",
    );
    assert_eq!(q(&c, "SELECT n FROM copies").rows, result.rows);
    q(
        &c,
        "INSERT INTO native SELECT EXISTS (SELECT n FROM docs WHERE n=99) AS n",
    );
    assert_eq!(
        q(&c, "SELECT n FROM native WHERE n=0").rows,
        vec![vec![Value::Integer(0)]]
    );
}

#[test]
fn collection_membership_subqueries_match_native_null_and_scalar_semantics() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {n:1}");
    q(&c, "INSERT INTO docs {n:2}");
    q(&c, "INSERT INTO docs {n:null}");
    q(&c, "CREATE TABLE native(n)");
    q(&c, "INSERT INTO native VALUES (1),(2),(NULL)");
    for lhs in ["1", "3", "NULL", "'1'", "1.0"] {
        for predicate in ["1", "n IS NOT NULL", "n=99"] {
            for negate in ["", "NOT "] {
                let sql =
                    format!("SELECT {lhs} {negate}IN (SELECT n FROM docs WHERE {predicate}) AS v");
                assert_eq!(
                    q(&c, &sql).rows,
                    q(&c, &sql.replace("FROM docs", "FROM native")).rows,
                    "{sql}"
                );
            }
        }
    }
    assert_eq!(
        q(
            &c,
            "SELECT n FROM docs WHERE n IN (SELECT n FROM docs WHERE n=2)"
        )
        .rows,
        vec![vec![Value::Integer(2)]]
    );
    assert_eq!(
        q(
            &c,
            "SELECT (SELECT max(n) FROM docs) IN (SELECT n FROM docs) AS v"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    q(&c, "CREATE TABLE copies");
    q(
        &c,
        "INSERT INTO copies(n) SELECT n FROM docs WHERE n NOT IN (SELECT n FROM docs WHERE n=1)",
    );
    assert_eq!(
        q(&c, "SELECT n FROM copies").rows,
        vec![vec![Value::Integer(2)]]
    );
}

#[test]
fn membership_subqueries_preserve_record_binary_keys_and_native_affinity() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {id:docs:a,link:docs:a,n:'1'}");
    q(&c, "INSERT INTO docs {id:docs:b,link:docs:1,n:'2'}");
    assert_eq!(q(&c, "SELECT docs:a IN (SELECT link FROM docs) AS a,type::record('docs','1') IN (SELECT link FROM docs) AS b").rows, vec![vec![Value::Integer(1), Value::Integer(0)]]);
    let binary = Value::Binary(
        b"FDB\x01{\"type\":\"Record\",\"value\":{\"table\":\"docs\",\"key\":{\"String\":\"a\"}}}"
            .to_vec(),
    );
    let result = c
        .execute(
            "SELECT $v IN (SELECT link FROM docs) AS v",
            &Parameters::from([("$v".into(), binary.clone())]),
        )
        .unwrap();
    assert_eq!(result.rows, vec![vec![Value::Integer(0)]]);
    c.execute(
        "INSERT INTO docs(link) VALUES ($v)",
        &Parameters::from([("$v".into(), binary.clone())]),
    )
    .unwrap();
    assert_eq!(
        c.execute(
            "SELECT $v IN (SELECT link FROM docs) AS v",
            &Parameters::from([("$v".into(), binary.clone())])
        )
        .unwrap()
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    q(&c, "CREATE TABLE binary_lhs(v BLOB)");
    c.execute(
        "INSERT INTO binary_lhs VALUES ($v)",
        &Parameters::from([("$v".into(), binary)]),
    )
    .unwrap();
    assert_eq!(
        q(
            &c,
            "SELECT v IN (SELECT link FROM docs) AS v FROM binary_lhs"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    q(&c, "CREATE TABLE lhs(n INTEGER)");
    q(&c, "INSERT INTO lhs VALUES (1),(3)");
    q(&c, "CREATE TABLE rhs(n)");
    q(&c, "INSERT INTO rhs VALUES ('1'),('2'),(NULL)");
    assert_eq!(
        q(
            &c,
            "SELECT n,n IN (SELECT n FROM docs) AS present FROM lhs ORDER BY n"
        )
        .rows,
        q(
            &c,
            "SELECT n,n IN (SELECT n FROM rhs) AS present FROM lhs ORDER BY n"
        )
        .rows
    );
    assert_eq!(
        q(
            &c,
            "WITH v AS (SELECT link FROM docs) SELECT docs:a IN (SELECT link FROM v) AS v"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
}

#[test]
fn membership_subquery_affinity_and_collation_match_native_sql() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(v) VALUES ('1'),('a'),('tail '),(2)");
    q(&c, "CREATE TABLE rhs(v)");
    q(&c, "INSERT INTO rhs VALUES ('1'),('a'),('tail '),(2)");
    let values = "(1),('1'),(2),('2'),('A'),('a'),('tail'),('tail '),(NULL),(X'31')";
    for (index, declaration) in [
        "",
        "INTEGER",
        "REAL",
        "NUMERIC",
        "TEXT",
        "BLOB",
        "TEXT COLLATE NOCASE",
    ]
    .iter()
    .enumerate()
    {
        let table = format!("left_{index}");
        q(&c, &format!("CREATE TABLE {table}(v {declaration})"));
        q(&c, &format!("INSERT INTO {table} VALUES {values}"));
        for left in [
            "l.v",
            "+l.v",
            "CAST(l.v AS TEXT)",
            "l.v COLLATE BINARY",
            "l.v COLLATE NOCASE",
            "l.v COLLATE RTRIM",
        ] {
            for right in [
                "v",
                "v COLLATE BINARY",
                "v COLLATE NOCASE",
                "v COLLATE RTRIM",
                "+v",
                "v || ''",
            ] {
                for negate in ["", "NOT "] {
                    let logical = format!("SELECT {left} {negate}IN (SELECT {right} FROM docs) AS matched FROM {table} l ORDER BY l.rowid");
                    let native = logical.replace("FROM docs", "FROM rhs");
                    assert_eq!(
                        q(&c, &logical).rows,
                        q(&c, &native).rows,
                        "{declaration}: {logical}"
                    );
                }
            }
        }
    }
}

#[test]
fn subquery_insert_failures_restore_documents_indexes_and_prior_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE source");
    q(&c, "INSERT INTO source(n) VALUES (1),(2),(9)");
    q(&c, "CREATE TABLE target");
    q(&c, "CREATE UNIQUE INDEX target_n ON target(n)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO target(n) VALUES (9)");
    let prior = q(&c, "SELECT id,n FROM target").rows;
    for predicate in [
        "n IN (SELECT n FROM source)",
        "n NOT IN (SELECT n FROM source WHERE n=99)",
        "EXISTS (SELECT n FROM source WHERE n=2)",
        "n <= (SELECT max(n) FROM source)",
    ] {
        let sql =
            format!("INSERT INTO target(n) SELECT n FROM source WHERE {predicate} ORDER BY n");
        assert!(c.execute(&sql, &Parameters::new()).is_err(), "{sql}");
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        assert_eq!(q(&c, "SELECT id,n FROM target").rows, prior);
        assert_eq!(
            c.check_collection_integrity("target", Default::default())
                .unwrap()
                .documents,
            1
        );
        assert!(q(&c, "SELECT n FROM target WHERE n=1").rows.is_empty());
        q(&c, "INSERT INTO target(n) SELECT n FROM source WHERE n IN (SELECT n FROM source WHERE n<9)");
        assert_eq!(
            q(&c, "SELECT n FROM target ORDER BY n").rows,
            vec![
                vec![Value::Integer(1)],
                vec![Value::Integer(2)],
                vec![Value::Integer(9)]
            ]
        );
        q(&c, "DELETE FROM target WHERE n<9");
    }
    q(&c, "ROLLBACK");
    assert!(q(&c, "SELECT n FROM target").rows.is_empty());
    assert_eq!(
        c.check_collection_integrity("target", Default::default())
            .unwrap()
            .documents,
        0
    );
    assert_eq!(
        q(&c, "SELECT n FROM source ORDER BY n").rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(9)]
        ]
    );
}

#[test]
fn source_free_subqueries_and_ctes_preserve_typed_parameters_and_helpers() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let record = q(&c, "SELECT type::record('docs','key') AS v").rows[0][0].clone();
    for value in [
        record,
        Value::Boolean(true),
        Value::Array(vec![Value::Integer(1)]),
        Value::Object(Default::default()),
        Value::Binary(b"FDB\x01payload".to_vec()),
        Value::vector32(&[1.0, 0.0]).unwrap(),
    ] {
        let params = Parameters::from([("$v".into(), value.clone())]);
        for sql in [
            "SELECT (SELECT $v AS v) AS v",
            "WITH chosen AS (SELECT $v AS v) SELECT v FROM chosen",
            "SELECT v FROM (SELECT $v AS v) chosen",
            "SELECT (SELECT (SELECT $v AS v) AS v) AS v",
        ] {
            assert_eq!(
                c.execute(sql, &params).unwrap().rows,
                vec![vec![value.clone()]],
                "{sql}"
            );
        }
        assert_eq!(
            c.execute("SELECT EXISTS (SELECT $v AS v) AS v", &params)
                .unwrap()
                .rows,
            vec![vec![Value::Integer(1)]]
        );
    }
    assert_eq!(
        q(&c, "SELECT (SELECT type::record('docs','key') AS v) AS v").rows,
        q(&c, "SELECT type::record('docs','key') AS v").rows
    );
    assert_eq!(
        q(
            &c,
            "SELECT docs:key IN (SELECT type::record('docs','key') AS v) AS v"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    q(&c, "CREATE TABLE docs");
    let numbered = Parameters::from([
        ("?1".into(), Value::Integer(3)),
        ("?2".into(), Value::Array(vec![Value::Boolean(true)])),
    ]);
    assert_eq!(
        c.execute("SELECT ?1 AS n,(SELECT ?2 AS v) AS v", &numbered)
            .unwrap()
            .rows,
        vec![vec![Value::Integer(3), numbered["?2"].clone()]]
    );
    q(&c, "CREATE TABLE native(v BLOB)");
    assert!(c
        .execute(
            "INSERT INTO native SELECT $v AS v",
            &Parameters::from([("$v".into(), Value::Array(vec![]))])
        )
        .is_err());
    assert!(q(&c, "SELECT * FROM native").rows.is_empty());
    let params = Parameters::from([("$v".into(), Value::Array(vec![Value::Integer(7)]))]);
    c.execute("INSERT INTO docs(v) SELECT (SELECT $v AS v) AS v", &params)
        .unwrap();
    assert_eq!(
        q(&c, "SELECT v FROM docs").rows,
        vec![vec![params["$v"].clone()]]
    );
}

#[test]
fn nested_anonymous_parameters_retain_statement_indices_and_fail_before_writes() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let a = Value::Array(vec![Value::Integer(7)]);
    let b = Value::Binary(b"FDB\x01not-json".to_vec());
    let params = Parameters::from([
        ("?1".into(), Value::Integer(4)),
        ("?2".into(), a.clone()),
        ("?3".into(), b.clone()),
    ]);
    assert_eq!(
        c.execute(
            "SELECT ? AS n,(SELECT ? AS v) AS a,(SELECT ? AS v) AS b",
            &params
        )
        .unwrap()
        .rows,
        vec![vec![Value::Integer(4), a.clone(), b.clone()]]
    );
    let cte_params = Parameters::from([("?1".into(), a.clone()), ("?2".into(), b.clone())]);
    assert_eq!(
        c.execute(
            "WITH chosen AS (SELECT ? AS a) SELECT a,(SELECT ? AS v) AS b FROM chosen",
            &cte_params
        )
        .unwrap()
        .rows,
        vec![vec![a.clone(), b]]
    );
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE UNIQUE INDEX docs_n ON docs(n)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs {n:9}");
    let prior = q(&c, "SELECT id,n FROM docs").rows;
    for invalid in [
        Parameters::from([("?1".into(), Value::Integer(1))]),
        Parameters::from([
            ("?1".into(), Value::Integer(1)),
            ("?2".into(), a.clone()),
            ("$unused".into(), Value::Boolean(true)),
        ]),
    ] {
        assert!(c
            .execute(
                "INSERT INTO docs(n,v) SELECT ? AS n,(SELECT ? AS v) AS v",
                &invalid
            )
            .is_err());
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        assert_eq!(q(&c, "SELECT id,n FROM docs").rows, prior);
        assert_eq!(
            c.check_collection_integrity("docs", Default::default())
                .unwrap()
                .documents,
            1
        );
    }
    c.execute(
        "INSERT INTO docs(n,v) SELECT ? AS n,(SELECT ? AS v) AS v",
        &Parameters::from([("?1".into(), Value::Integer(1)), ("?2".into(), a.clone())]),
    )
    .unwrap();
    assert_eq!(q(&c, "SELECT v FROM docs WHERE n=1").rows, vec![vec![a]]);
    q(&c, "ROLLBACK");
    assert_eq!(
        c.check_collection_integrity("docs", Default::default())
            .unwrap()
            .documents,
        0
    );
}

#[test]
fn native_source_expression_subqueries_preserve_explicit_logical_values() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE native(n INTEGER,b BLOB)");
    q(&c, "INSERT INTO native VALUES (1,X'31'),(2,X'32')");
    assert_eq!(
        c.execute(
            "SELECT '1' IN (SELECT n FROM native WHERE b=$b) AS v",
            &Parameters::from([("$b".into(), Value::Binary(vec![49]))])
        )
        .unwrap()
        .rows,
        q(
            &c,
            "SELECT '1' IN (SELECT n FROM native WHERE b=X'31') AS v"
        )
        .rows
    );
    let record = q(&c, "SELECT type::record('docs','key') AS v").rows[0][0].clone();
    for value in [
        record,
        Value::Boolean(false),
        Value::Array(vec![Value::Integer(8)]),
        Value::Object(Default::default()),
        Value::Binary(b"FDB\x01not-json".to_vec()),
        Value::vector32(&[0.0, 1.0]).unwrap(),
    ] {
        let params = Parameters::from([("$v".into(), value.clone())]);
        for sql in [
            "WITH chosen AS (SELECT $v AS v FROM native WHERE n=2) SELECT v FROM chosen",
            "SELECT v FROM (SELECT $v AS v FROM native WHERE n=2) chosen",
        ] {
            assert_eq!(
                c.execute(sql, &params).unwrap().rows,
                vec![vec![value.clone()]],
                "{sql}"
            );
        }
        assert_eq!(
            c.execute(
                "SELECT (SELECT $v AS v FROM native WHERE n=2) AS v",
                &params
            )
            .unwrap()
            .rows,
            vec![vec![value]]
        );
        assert_eq!(
            c.execute(
                "SELECT (SELECT $v AS v FROM native WHERE n=99) AS v",
                &params
            )
            .unwrap()
            .rows,
            vec![vec![Value::Null]]
        );
        assert_eq!(
            c.execute(
                "SELECT EXISTS (SELECT $v AS v FROM native WHERE n=2) AS v",
                &params
            )
            .unwrap()
            .rows,
            vec![vec![Value::Integer(1)]]
        );
    }
    assert_eq!(
        q(
            &c,
            "SELECT docs:key IN (SELECT type::record('docs','key') AS v FROM native) AS v"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    let params = Parameters::from([
        ("$v".into(), Value::Array(vec![Value::Boolean(true)])),
        ("$b".into(), Value::Binary(vec![50])),
    ]);
    assert_eq!(
        c.execute(
            "SELECT (SELECT $v AS v FROM native WHERE b=$b) AS v",
            &params
        )
        .unwrap()
        .rows,
        vec![vec![params["$v"].clone()]]
    );
    q(&c, "CREATE TABLE docs");
    c.execute(
        "INSERT INTO docs(v) SELECT (SELECT $v AS v FROM native WHERE b=$b) AS v",
        &params,
    )
    .unwrap();
    assert_eq!(
        q(&c, "SELECT v FROM docs").rows,
        vec![vec![params["$v"].clone()]]
    );
}

#[test]
fn scalar_subquery_pagination_matches_native_and_retains_scope() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n) VALUES (1),(2),(3),(3)");
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "INSERT INTO native VALUES (1),(2),(3),(3)");
    for distinct in ["", "DISTINCT "] {
        for pagination in [
            "LIMIT (SELECT min(n) FROM SOURCE)",
            "LIMIT (SELECT max(n) FROM SOURCE) OFFSET (SELECT min(n) FROM SOURCE)",
            "LIMIT (SELECT n-n FROM SOURCE LIMIT 1)",
            "LIMIT (SELECT -n FROM SOURCE LIMIT 1)",
        ] {
            let logical = format!(
                "SELECT {distinct}n FROM docs ORDER BY n {}",
                pagination.replace("SOURCE", "docs")
            );
            let native = logical.replace("FROM docs", "FROM native");
            assert_eq!(q(&c, &logical).rows, q(&c, &native).rows, "{logical}");
        }
    }
    assert_eq!(
        q(
            &c,
            "SELECT n FROM docs ORDER BY n LIMIT (SELECT count(*) FROM native)"
        )
        .rows
        .len(),
        4
    );
    for distinct in ["", "DISTINCT "] {
        let sql = format!("WITH chosen AS (SELECT n FROM docs) SELECT {distinct}n FROM docs ORDER BY n LIMIT (SELECT min(n) FROM chosen)");
        // The pinned engine cannot resolve an outer CTE from LIMIT.
        assert!(c
            .execute(&sql.replace("FROM docs", "FROM native"), &Parameters::new())
            .is_err());
        assert!(c.execute(&sql, &Parameters::new()).is_err());
    }
    let params = Parameters::from([
        ("$limit".into(), Value::Integer(2)),
        ("$offset".into(), Value::Integer(1)),
    ]);
    assert_eq!(c.execute("SELECT n FROM docs ORDER BY n LIMIT (SELECT $limit FROM docs LIMIT 1) OFFSET (SELECT $offset FROM docs LIMIT 1)", &params).unwrap().rows, vec![vec![Value::Integer(2)],vec![Value::Integer(3)]]);
    q(&c, "CREATE TABLE target");
    assert!(c
        .execute(
            "INSERT INTO target(n) SELECT n FROM docs LIMIT (SELECT n FROM docs WHERE n=99)",
            &Parameters::new()
        )
        .is_err());
    assert_eq!(
        c.check_collection_integrity("target", Default::default())
            .unwrap()
            .documents,
        0
    );
    q(
        &c,
        "INSERT INTO target(n) SELECT n FROM docs ORDER BY n LIMIT (SELECT min(n) FROM docs)",
    );
    assert_eq!(
        q(&c, "SELECT n FROM target").rows,
        vec![vec![Value::Integer(1)]]
    );
}

#[test]
fn compound_subquery_pagination_matches_native_and_is_atomic() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n) VALUES (1),(2),(3),(3)");
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "INSERT INTO native VALUES (1),(2),(3),(3)");
    for operator in ["UNION ALL", "UNION", "INTERSECT", "EXCEPT"] {
        for pagination in [
            "LIMIT (SELECT max(n) FROM docs) OFFSET (SELECT min(n) FROM docs)",
            "LIMIT (SELECT n-n FROM docs LIMIT 1)",
            "LIMIT (SELECT -n FROM docs LIMIT 1)",
        ] {
            let sql = format!("SELECT n FROM docs {operator} SELECT n FROM docs WHERE n=3 ORDER BY n {pagination}");
            let native = format!("SELECT n FROM (SELECT n FROM native {operator} SELECT n FROM native WHERE n=3) ORDER BY n {}", pagination.replace("FROM docs", "FROM native"));
            assert_eq!(q(&c, &sql).rows, q(&c, &native).rows, "{sql}");
        }
    }
    // The pinned direct native compound form rejects this pagination; wrapping
    // the compound in a native derived table supplies the successful oracle above.
    assert!(c.execute("SELECT n FROM native UNION ALL SELECT n FROM native WHERE n=3 ORDER BY n LIMIT (SELECT max(n) FROM native) OFFSET (SELECT min(n) FROM native)", &Parameters::new()).is_err());
    assert_eq!(q(&c, "SELECT n FROM native UNION SELECT n FROM native ORDER BY n LIMIT (SELECT min(n) FROM docs)").rows, vec![vec![Value::Integer(1)]]);
    let params = Parameters::from([
        ("$limit".into(), Value::Integer(2)),
        ("$offset".into(), Value::Integer(1)),
    ]);
    assert_eq!(c.execute("SELECT n FROM docs UNION SELECT n FROM docs ORDER BY n LIMIT (SELECT $limit FROM docs LIMIT 1) OFFSET (SELECT $offset FROM docs LIMIT 1)", &params).unwrap().rows, vec![vec![Value::Integer(2)], vec![Value::Integer(3)]]);
    q(&c, "CREATE TABLE target");
    assert!(c.execute("INSERT INTO target(n) SELECT n FROM docs UNION SELECT n FROM docs LIMIT (SELECT n FROM docs WHERE n=99)", &Parameters::new()).is_err());
    assert_eq!(
        c.check_collection_integrity("target", Default::default())
            .unwrap()
            .documents,
        0
    );
    q(&c, "INSERT INTO target(n) SELECT n FROM docs UNION SELECT n FROM docs ORDER BY n LIMIT (SELECT min(n) FROM docs)");
    assert_eq!(
        q(&c, "SELECT n FROM target").rows,
        vec![vec![Value::Integer(1)]]
    );
}

#[test]
fn pagination_errors_preserve_prior_transaction_work_and_scope() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n) VALUES (1),(2)");
    q(&c, "CREATE TABLE target");
    q(&c, "CREATE UNIQUE INDEX target_n ON target(n)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO target {id:target:prior,n:9}");
    let prior = q(&c, "SELECT id,n FROM target").rows;
    for source in [
        "SELECT n FROM docs",
        "SELECT DISTINCT n FROM docs",
        "SELECT n FROM docs UNION SELECT n FROM docs",
    ] {
        for limit in [
            "(SELECT n FROM docs WHERE n=99)",
            "(SELECT 1.5 FROM docs)",
            "(SELECT 'invalid' FROM docs)",
            "n+(SELECT min(n) FROM docs)",
        ] {
            let sql = format!("INSERT INTO target(n) {source} LIMIT {limit}");
            assert!(c.execute(&sql, &Parameters::new()).is_err(), "{sql}");
            assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
            assert_eq!(q(&c, "SELECT id,n FROM target").rows, prior);
            assert_eq!(
                c.check_collection_integrity("target", Default::default())
                    .unwrap()
                    .documents,
                1
            );
            assert!(q(&c, "SELECT n FROM target WHERE n=1").rows.is_empty());
        }
    }
    q(&c, "INSERT INTO target(n) SELECT n FROM docs UNION SELECT n FROM docs ORDER BY n LIMIT (SELECT min(n) FROM docs)");
    assert_eq!(
        q(&c, "SELECT n FROM target ORDER BY n").rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(9)]]
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
fn native_scalar_sources_project_into_collection_queries() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n) VALUES (1),(2)");
    q(&c, "CREATE TABLE native(n INTEGER,b BLOB)");
    q(
        &c,
        "INSERT INTO native VALUES (3,x'464442000102'),(4,x'ff')",
    );
    assert_eq!(q(&c, "SELECT n,(SELECT max(n) FROM native) AS maximum,(SELECT b FROM native ORDER BY n LIMIT 1) AS bytes,(SELECT n FROM native WHERE n=99) AS missing FROM docs ORDER BY n").rows,
        vec![vec![Value::Integer(1),Value::Integer(4),Value::Binary(vec![70,68,66,0,1,2]),Value::Null],vec![Value::Integer(2),Value::Integer(4),Value::Binary(vec![70,68,66,0,1,2]),Value::Null]]);
    assert_eq!(
        q(
            &c,
            "SELECT n FROM docs WHERE n<(SELECT min(n) FROM native) ORDER BY n"
        )
        .rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    assert_eq!(
        c.execute(
            "SELECT (SELECT b FROM native WHERE n=$n) AS b FROM docs LIMIT 1",
            &Parameters::from([("$n".into(), Value::Integer(4))])
        )
        .unwrap()
        .rows,
        vec![vec![Value::Binary(vec![255])]]
    );
    assert!(c
        .execute(
            "SELECT (SELECT n,b FROM native) AS v FROM docs",
            &Parameters::new()
        )
        .is_err());
    q(&c, "CREATE TABLE target");
    q(&c, "INSERT INTO target(n,b) SELECT (SELECT max(n) FROM native),(SELECT b FROM native ORDER BY n LIMIT 1) FROM docs LIMIT 1");
    assert_eq!(
        q(&c, "SELECT n,b FROM target").rows,
        vec![vec![
            Value::Integer(4),
            Value::Binary(vec![70, 68, 66, 0, 1, 2])
        ]]
    );
}

#[test]
fn native_exists_sources_filter_collections_and_validate_bindings() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n) VALUES (1),(2)");
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "INSERT INTO native VALUES (1),(2)");
    for inner in [
        "SELECT * FROM native",
        "SELECT n,n FROM native WHERE n=99",
        "SELECT count(*) FROM native WHERE n=99",
        "SELECT n FROM native LIMIT 0",
        "SELECT n FROM native LIMIT 1 OFFSET 2",
    ] {
        let sql = format!("SELECT n,EXISTS ({inner}) AS present,NOT EXISTS ({inner}) AS absent FROM docs ORDER BY n");
        assert_eq!(
            q(&c, &sql).rows,
            q(&c, &sql.replace("FROM docs", "FROM native")).rows,
            "{sql}"
        );
    }
    let sql = "SELECT n FROM docs WHERE EXISTS (SELECT n FROM native WHERE n=$n) ORDER BY n";
    assert_eq!(
        c.execute(sql, &Parameters::from([("$n".into(), Value::Integer(2))]))
            .unwrap()
            .rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    q(&c, "CREATE TABLE target");
    q(&c, "CREATE UNIQUE INDEX target_n ON target(n)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO target {id:target:prior,n:9}");
    let prior = q(&c, "SELECT id,n FROM target").rows;
    assert!(c
        .execute(&format!("INSERT INTO target(n) {sql}"), &Parameters::new())
        .is_err());
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(q(&c, "SELECT id,n FROM target").rows, prior);
    assert_eq!(
        c.check_collection_integrity("target", Default::default())
            .unwrap()
            .documents,
        1
    );
    c.execute(
        &format!("INSERT INTO target(n) {sql}"),
        &Parameters::from([("$n".into(), Value::Integer(2))]),
    )
    .unwrap();
    assert_eq!(
        q(&c, "SELECT n FROM target ORDER BY n").rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(9)]
        ]
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
fn native_scalar_affinity_matches_native_document_scalar_storage() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(
        &c,
        "INSERT INTO docs(v) VALUES ('2'),(2),('A'),('a '),('A '),('a  '),('a\t'),(''),(NULL)",
    );
    q(&c, "CREATE TABLE lhs(v BLOB)");
    q(
        &c,
        "INSERT INTO lhs VALUES ('2'),(2),('A'),('a '),('A '),('a  '),('a\t'),(''),(NULL)",
    );
    assert_eq!(q(&c, "SELECT a.v,(SELECT b.v FROM lhs b WHERE b.rowid=a.rowid) AS nested FROM lhs a ORDER BY a.rowid").rows, q(&c, "SELECT v,v FROM lhs ORDER BY rowid").rows);
    for (name, declaration, value) in [
        ("numbers", "INTEGER", "2"),
        ("strings", "TEXT", "'2'"),
        ("letters", "TEXT COLLATE NOCASE", "'a'"),
        ("trimmed", "TEXT COLLATE RTRIM", "'a'"),
    ] {
        q(&c, &format!("CREATE TABLE {name}(v {declaration})"));
        q(&c, &format!("INSERT INTO {name} VALUES ({value})"));
        for projection in [
            "v",
            "v COLLATE BINARY",
            "v COLLATE NOCASE",
            "v COLLATE RTRIM",
            "+v",
            "CAST(v AS TEXT)",
            "CAST(v AS NUMERIC)",
        ] {
            for op in ["=", "!=", "IS", "IS NOT", "<", "<=", ">", ">="] {
                for operands in [
                    format!("v {op} (SELECT {projection} FROM {name})"),
                    format!("(SELECT {projection} FROM {name}) {op} v"),
                    format!("v {op} ((SELECT {projection} FROM {name}) COLLATE NOCASE)"),
                    format!("((SELECT {projection} FROM {name}) COLLATE NOCASE) {op} v"),
                    format!("(v COLLATE BINARY) {op} ((SELECT {projection} FROM {name}) COLLATE NOCASE)"),
                    format!("((SELECT {projection} FROM {name}) COLLATE NOCASE) {op} (v COLLATE BINARY)"),
                    format!("(v COLLATE NOCASE) {op} ((SELECT {projection} FROM {name}) COLLATE RTRIM)"),
                    format!("((SELECT {projection} FROM {name}) COLLATE RTRIM) {op} (v COLLATE NOCASE)"),
                ] {
                    let sql = format!("SELECT {operands} AS matched FROM docs");
                    assert_eq!(
                        q(&c, &sql).rows,
                        q(&c, &sql.replace("FROM docs", "FROM lhs")).rows,
                        "{sql}"
                    );
                }
            }
        }
    }
}

#[test]
fn native_scalar_comparisons_preserve_binary_identity() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(
        &c,
        "INSERT INTO docs(v) VALUES (x'464442000102'),(docs:a),(NULL)",
    );
    q(&c, "CREATE TABLE native(v BLOB)");
    q(&c, "INSERT INTO native VALUES (x'464442000102')");
    for comparison in ["v=(SELECT v FROM native)", "(SELECT v FROM native)=v"] {
        assert_eq!(
            q(&c, &format!("SELECT {comparison} AS matched FROM docs")).rows,
            vec![
                vec![Value::Integer(1)],
                vec![Value::Integer(0)],
                vec![Value::Null]
            ]
        );
    }
}

#[test]
fn native_scalar_comparison_insert_failures_preserve_indexes_and_prior_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n,v) VALUES (1,'2'),(2,'2'),(9,'2')");
    q(&c, "CREATE TABLE native(v INTEGER)");
    q(&c, "INSERT INTO native VALUES (2)");
    q(&c, "CREATE TABLE target");
    q(&c, "CREATE UNIQUE INDEX target_n ON target(n)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO target {id:target:prior,n:9}");
    let prior = q(&c, "SELECT id,n FROM target").rows;
    for predicate in [
        "v=(SELECT v FROM native)",
        "(SELECT v FROM native)=v",
        "v=((SELECT v FROM native) COLLATE NOCASE)",
        "((SELECT v FROM native) COLLATE NOCASE)=v",
    ] {
        let sql = format!("INSERT INTO target(n) SELECT n FROM docs WHERE {predicate} ORDER BY n");
        assert!(c.execute(&sql, &Parameters::new()).is_err(), "{sql}");
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        assert_eq!(q(&c, "SELECT id,n FROM target").rows, prior);
        assert_eq!(
            c.check_collection_integrity("target", Default::default())
                .unwrap()
                .documents,
            1
        );
        assert!(q(&c, "SELECT n FROM target WHERE n=1").rows.is_empty());
        let retry = format!(
            "INSERT INTO target(n) SELECT n FROM docs WHERE ({predicate}) AND n<9 ORDER BY n"
        );
        q(&c, &retry);
        assert_eq!(
            q(&c, "SELECT n FROM target ORDER BY n").rows,
            vec![
                vec![Value::Integer(1)],
                vec![Value::Integer(2)],
                vec![Value::Integer(9)]
            ]
        );
        q(&c, "DELETE FROM target WHERE n<9");
    }
    q(&c, "ROLLBACK");
    assert_eq!(
        c.check_collection_integrity("target", Default::default())
            .unwrap()
            .documents,
        0
    );
}

#[test]
fn native_membership_sources_match_scalar_affinity_and_null_semantics() {
    for declaration in [
        "INTEGER",
        "TEXT",
        "TEXT COLLATE NOCASE",
        "TEXT COLLATE RTRIM",
        "BLOB",
    ] {
        let db = Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        q(&c, "CREATE TABLE docs");
        q(&c, "CREATE TABLE lhs(n BLOB)");
        q(
            &c,
            "INSERT INTO docs(n) VALUES (1),(2),('1'),('a'),('A'),(NULL)",
        );
        q(
            &c,
            "INSERT INTO lhs VALUES (1),(2),('1'),('a'),('A'),(NULL)",
        );
        q(&c, &format!("CREATE TABLE rhs(n {declaration})"));
        q(&c, "INSERT INTO rhs VALUES (1),('a'),(NULL)");
        for projection in ["n", "+n", "CAST(n AS TEXT)"] {
            for predicate in ["1", "n IS NOT NULL", "0"] {
                for op in ["IN", "NOT IN"] {
                    let suffix = format!("n {op} (SELECT {projection} FROM rhs WHERE {predicate})");
                    let expected =
                        q(&c, &format!("SELECT n,{suffix} FROM lhs ORDER BY rowid")).rows;
                    let actual = q(&c, &format!("SELECT n,{suffix} FROM docs ORDER BY rowid")).rows;
                    assert_eq!(actual, expected, "{declaration}: {suffix}");
                }
            }
        }
    }
}

#[test]
fn native_membership_insert_parameters_and_binary_identity() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE TABLE rhs(n BLOB)");
    q(&c, "CREATE TABLE target");
    q(&c, "CREATE UNIQUE INDEX target_n ON target(n)");
    let record = Value::Record(fastdb::Record {
        table: "docs".into(),
        key: fastdb::Key::String("a".into()),
    });
    let bytes = Value::Binary(
        b"FDB\x01{\"type\":\"Record\",\"value\":{\"table\":\"docs\",\"key\":{\"String\":\"a\"}}}"
            .to_vec(),
    );
    let params = Parameters::from([("$record".into(), record), ("$bytes".into(), bytes)]);
    c.execute("INSERT INTO docs(n) VALUES ($record),($bytes)", &params)
        .unwrap();
    c.execute(
        "INSERT INTO rhs VALUES ($bytes)",
        &Parameters::from([("$bytes".into(), params["$bytes"].clone())]),
    )
    .unwrap();
    assert_eq!(
        q(
            &c,
            "SELECT n IN (SELECT n FROM rhs) FROM docs ORDER BY rowid"
        )
        .rows,
        vec![vec![Value::Integer(0)], vec![Value::Integer(1)]]
    );
    q(&c, "DELETE FROM docs");
    q(&c, "DELETE FROM rhs");
    q(&c, "INSERT INTO docs(n) VALUES (1),(2)");
    q(&c, "INSERT INTO rhs VALUES (1),(2)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO target {id:target:prior,n:9}");
    let sql = "INSERT INTO target(n) SELECT n IN (SELECT n FROM rhs WHERE n>$min) FROM docs";
    assert_eq!(
        c.execute(sql, &Parameters::new()).unwrap_err().code(),
        "FDB_PARAMETER"
    );
    let result = c.execute(sql, &Parameters::from([("$min".into(), Value::Integer(0))]));
    assert!(result.is_err());
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
        c.execute(sql, &Parameters::from([("$min".into(), Value::Integer(1))]))
            .unwrap()
            .affected,
        2
    );
    assert_eq!(
        c.check_collection_integrity("target", Default::default())
            .unwrap()
            .documents,
        3
    );
    q(&c, "ROLLBACK");
}
