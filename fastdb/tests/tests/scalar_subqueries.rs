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
