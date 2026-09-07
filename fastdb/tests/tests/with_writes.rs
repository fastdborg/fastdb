use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn with_updates_and_deletes_preserve_candidates_and_atomic_indexes() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE UNIQUE INDEX docs_n ON docs(n)");
    q(&c, "INSERT INTO docs(n) VALUES(1),(2),(3)");
    q(&c, "CREATE TABLE native(n INTEGER UNIQUE)");
    q(&c, "INSERT INTO native VALUES(1),(2),(3)");
    for prefix in [
        "WITH chosen AS (SELECT $n AS n)",
        "WITH chosen AS (SELECT n FROM docs WHERE n=$n)",
    ] {
        for verb in ["UPDATE", "DELETE"] {
            q(&c, "BEGIN");
            let params = Parameters::from([("$n".into(), Value::Integer(2))]);
            let make = |target| {
                if verb == "UPDATE" {
                    format!("{prefix} UPDATE {target} SET n=n+10 WHERE n IN (SELECT n FROM chosen) RETURNING n")
                } else {
                    format!("{prefix} DELETE FROM {target} WHERE n IN (SELECT n FROM chosen) RETURNING n")
                }
            };
            let expected = c
                .execute(
                    &make("native").replace(prefix, "WITH chosen AS (SELECT $n AS n)"),
                    &params,
                )
                .unwrap();
            let actual = c
                .execute(&make("docs"), &params)
                .unwrap_or_else(|e| panic!("{}: {e}", make("docs")));
            assert_eq!(actual.rows, expected.rows);
            assert_eq!(actual.affected, expected.affected);
            assert_eq!(
                q(&c, "SELECT n FROM docs ORDER BY n").rows,
                q(&c, "SELECT n FROM native ORDER BY n").rows
            );
            c.check_collection_integrity("docs", Default::default())
                .unwrap();
            q(&c, "ROLLBACK");
        }
    }
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs(n) VALUES(9)");
    let sql="WITH chosen AS (SELECT n FROM docs WHERE n<$max) UPDATE docs SET n=n+1 WHERE n IN (SELECT n FROM chosen) RETURNING n";
    assert!(c.execute(sql, &Parameters::new()).is_err());
    assert!(c
        .execute(sql, &Parameters::from([("$max".into(), Value::Integer(3))]))
        .is_err());
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(
        q(&c, "SELECT n FROM docs ORDER BY n").rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(3)],
            vec![Value::Integer(9)]
        ]
    );
    assert_eq!(
        c.check_collection_integrity("docs", Default::default())
            .unwrap()
            .documents,
        4
    );
    let retry = sql.replace("n=n+1", "n=n+10");
    assert_eq!(
        c.execute(
            &retry,
            &Parameters::from([("$max".into(), Value::Integer(3))])
        )
        .unwrap()
        .affected,
        2
    );
    c.check_collection_integrity("docs", Default::default())
        .unwrap();
    q(&c, "ROLLBACK");
}

#[test]
fn pinned_same_name_cte_write_resolution_differs_from_candidate_select() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "INSERT INTO native VALUES(1),(2),(3)");
    let prefix = "WITH native AS (SELECT 2 AS n), chosen AS (SELECT n FROM native)";
    assert_eq!(
        q(
            &c,
            &format!(
                "{prefix} SELECT n FROM main.native WHERE n IN (SELECT n FROM chosen) ORDER BY n"
            )
        )
        .rows,
        vec![vec![Value::Integer(2)]]
    );
    for (cte_name, expected) in [("native", vec![12]), ("target", vec![11, 12, 13])] {
        q(&c, "BEGIN");
        let sql = format!("WITH {cte_name} AS (SELECT 2 AS n), chosen AS (SELECT n FROM {cte_name}) UPDATE native AS target SET n=n+10 WHERE n IN (SELECT n FROM chosen) RETURNING n");
        let result = q(&c, &sql);
        assert_eq!(
            result.rows,
            expected
                .into_iter()
                .map(|n| vec![Value::Integer(n)])
                .collect::<Vec<_>>(),
            "{sql}"
        );
        q(&c, "ROLLBACK");
    }
    for suffix in [
        "UPDATE native SET n=n+10 WHERE n IN (SELECT n FROM chosen) RETURNING n",
        "DELETE FROM native WHERE n IN (SELECT n FROM chosen) RETURNING n",
    ] {
        q(&c, "BEGIN");
        let result = q(&c, &format!("{prefix} {suffix}"));
        let expected = if suffix.starts_with("UPDATE") {
            [11, 12, 13]
        } else {
            [1, 2, 3]
        };
        assert_eq!(
            result.rows,
            expected
                .into_iter()
                .map(|n| vec![Value::Integer(n)])
                .collect::<Vec<_>>()
        );
        assert_eq!(result.affected, 3);
        q(&c, "ROLLBACK");
        assert_eq!(
            q(&c, "SELECT n FROM native ORDER BY n").rows,
            vec![
                vec![Value::Integer(1)],
                vec![Value::Integer(2)],
                vec![Value::Integer(3)]
            ]
        );
    }
}

#[test]
fn native_cte_names_can_shadow_collections_without_hiding_table_access() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {id:docs:a,n:9}");
    for sql in [
        "WITH docs AS (SELECT 2 AS n) SELECT n FROM docs",
        "WITH safe AS (SELECT 2 AS n) SELECT docs.n FROM safe AS docs WHERE docs.n>1 ORDER BY docs.n",
        "WITH safe AS (SELECT 2 AS n) SELECT docs.* FROM safe docs",
        "WITH docs AS (SELECT 2 AS n) SELECT d.n FROM docs AS d",
        "WITH docs AS (SELECT 2 AS n) SELECT docs.n FROM docs WHERE docs.n>1 ORDER BY docs.n",
        "WITH docs AS (SELECT 2 AS n) SELECT docs.* FROM docs",
        "WITH docs AS (SELECT 2 AS n) SELECT docs.n FROM docs GROUP BY docs.n HAVING docs.n>1",
        "WITH docs AS (SELECT 2 AS n), chosen AS (SELECT n FROM docs) SELECT n FROM chosen",
        "WITH docs AS (SELECT 2 AS n) SELECT n FROM (SELECT n FROM docs) AS chosen",
    ] {
        assert_eq!(q(&c, sql).rows, vec![vec![Value::Integer(2)]]);
        assert_eq!(
            c.profile_select(sql, &Parameters::new())
                .unwrap()
                .result
                .rows,
            vec![vec![Value::Integer(2)]]
        );
    }
    assert_eq!(
        q(&c, "SELECT n FROM docs").rows,
        vec![vec![Value::Integer(9)]]
    );
}
