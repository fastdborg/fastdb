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
        "WITH docs AS (SELECT 2 AS n) SELECT sum(docs.n) OVER w FROM docs WINDOW w AS (PARTITION BY docs.n ORDER BY docs.n)",
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

#[test]
fn cte_named_window_qualifiers_match_native_window_results() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    let sql = "WITH docs(n) AS (VALUES(1),(2),(2)) SELECT docs.n,sum(docs.n) OVER w AS total FROM docs WINDOW w AS (PARTITION BY docs.n ORDER BY docs.n) ORDER BY docs.n";
    let expected = q(&c, &sql.replace("docs", "safe"));
    assert_eq!(q(&c, sql).rows, expected.rows);
    assert_eq!(
        c.profile_select(sql, &Parameters::new())
            .unwrap()
            .result
            .rows,
        expected.rows
    );
}

#[test]
fn aliased_with_writes_can_use_a_cte_named_after_the_collection() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE UNIQUE INDEX docs_n ON docs(n)");
    q(&c, "CREATE TABLE native(n INTEGER UNIQUE)");
    q(&c, "INSERT INTO docs(n) VALUES(1),(2),(3)");
    q(&c, "INSERT INTO native VALUES(1),(2),(3)");
    for delete in [false, true] {
        q(&c, "BEGIN");
        let query = |table| {
            let prefix =
                format!("WITH {table} AS (SELECT $n AS n), chosen AS (SELECT n FROM {table})");
            if delete {
                format!("{prefix} DELETE FROM {table} AS target WHERE target.n IN (SELECT n FROM chosen) RETURNING n")
            } else {
                format!("{prefix} UPDATE {table} AS target SET n=target.n+10 WHERE target.n IN (SELECT n FROM chosen) RETURNING n")
            }
        };
        let params = Parameters::from([("$n".into(), Value::Integer(2))]);
        let expected = c.execute(&query("native"), &params).unwrap();
        let actual = c.execute(&query("docs"), &params).unwrap();
        assert_eq!(actual.rows, expected.rows);
        assert_eq!(actual.affected, 1);
        assert_eq!(
            q(&c, "SELECT n FROM docs ORDER BY n").rows,
            q(&c, "SELECT n FROM native ORDER BY n").rows
        );
        c.check_collection_integrity("docs", Default::default())
            .unwrap();
        q(&c, "ROLLBACK");
        assert_eq!(
            q(&c, "SELECT n FROM docs ORDER BY n").rows,
            vec![
                vec![Value::Integer(1)],
                vec![Value::Integer(2)],
                vec![Value::Integer(3)]
            ]
        );
    }
}

#[test]
fn update_assignments_use_typed_subquery_candidates_before_mutation() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE UNIQUE INDEX docs_n ON docs(n)");
    q(&c, "INSERT INTO docs(n) VALUES(1),(2),(3)");
    q(&c, "CREATE TABLE native(n INTEGER UNIQUE)");
    q(&c, "INSERT INTO native VALUES(1),(2),(3)");
    for expression in [
        "(SELECT max(n) FROM native)+10",
        "(SELECT max(n) FROM docs)+10",
        "(SELECT n FROM chosen)+10",
        "n IN (SELECT n FROM chosen)",
        "EXISTS(SELECT n FROM chosen)",
    ] {
        q(&c, "BEGIN");
        let params = if expression.contains("chosen") {
            Parameters::from([("$value".into(), Value::Integer(2))])
        } else {
            Parameters::new()
        };
        let make = |table: &str, expression: &str| {
            let prefix = if expression.contains("chosen") {
                "WITH chosen AS (SELECT $value AS n) "
            } else {
                ""
            };
            format!("{prefix}UPDATE {table} SET n={expression} WHERE n=2 RETURNING n")
        };
        // Membership/EXISTS yield 1, which conflicts with the retained unique row.
        let actual = c.execute(&make("docs", expression), &params);
        let expected = c.execute(
            &make("native", &expression.replace("docs", "native")),
            &params,
        );
        match (expected, actual) {
            (Ok(a), Ok(b)) => {
                assert_eq!(a.rows, b.rows);
                assert_eq!(a.affected, b.affected);
            }
            (Err(a), Err(b)) => assert_eq!(a.code(), b.code()),
            (a, b) => panic!("{expression}: {a:?} / {b:?}"),
        }
        c.check_collection_integrity("docs", Default::default())
            .unwrap();
        q(&c, "ROLLBACK");
    }
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs(n) VALUES(9)");
    let sql="WITH chosen AS (SELECT n FROM docs WHERE n<$max) UPDATE docs SET n=(SELECT max(n) FROM chosen)+n WHERE n<3 RETURNING n";
    assert!(c.execute(sql, &Parameters::new()).is_err());
    assert!(c
        .execute(sql, &Parameters::from([("$max".into(), Value::Integer(3))]))
        .is_err());
    assert_eq!(
        c.check_collection_integrity("docs", Default::default())
            .unwrap()
            .documents,
        4
    );
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    let retry = sql.replace(")+n", ")+n+10");
    assert_eq!(
        c.execute(
            &retry,
            &Parameters::from([("$max".into(), Value::Integer(3))])
        )
        .unwrap()
        .rows,
        vec![vec![Value::Integer(13)], vec![Value::Integer(14)]]
    );
    q(&c, "ROLLBACK");
    q(&c, "CREATE TABLE flags");
    q(&c, "INSERT INTO flags {id:flags:a,flag:true}");
    q(&c, "DEFINE FIELD flag ON docs TYPE boolean");
    q(&c, "BEGIN");
    assert_eq!(
        q(
            &c,
            "UPDATE docs SET flag=(SELECT flag FROM flags) WHERE n=1 RETURNING flag"
        )
        .rows,
        vec![vec![Value::Boolean(true)]]
    );
    assert!(c
        .execute("UPDATE docs SET n=sum(n)", &Parameters::new())
        .is_err());
    q(&c, "ROLLBACK");
}

#[test]
fn nested_membership_assignment_operands_reach_select_lowering() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n) VALUES(2)");
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "INSERT INTO native VALUES(2)");
    for expr in [
        "(SELECT 1) IN (SELECT 1)",
        "(SELECT 1) NOT IN (SELECT 2)",
        "((SELECT 1) IN (SELECT 1)) IN (SELECT 1)",
        "(SELECT NULL) IN (SELECT 1)",
        "(SELECT NULL) NOT IN (SELECT 1 WHERE 0)",
    ] {
        q(&c, "BEGIN");
        let expected = q(&c, &format!("UPDATE native SET n={expr} RETURNING n"));
        let actual = q(&c, &format!("UPDATE docs SET n={expr} RETURNING n"));
        assert_eq!(actual.rows, expected.rows, "{expr}");
        assert_eq!(actual.affected, 1);
        q(&c, "ROLLBACK");
    }
    assert!(c
        .execute("UPDATE docs SET n=sum(n) IN (SELECT 1)", &Parameters::new())
        .is_err());
    assert_eq!(
        q(&c, "SELECT n FROM docs").rows,
        vec![vec![Value::Integer(2)]]
    );
}

#[test]
fn self_read_assignments_and_late_validation_preserve_atomic_candidates() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "DEFINE FIELD n ON docs TYPE integer");
    q(&c, "CREATE UNIQUE INDEX docs_n ON docs(n)");
    q(
        &c,
        "INSERT INTO docs(id,n) VALUES(docs:a,1),(docs:b,2),(docs:c,3)",
    );
    q(&c, "CREATE TABLE native(n INTEGER UNIQUE)");
    q(&c, "INSERT INTO native VALUES(1),(2),(3)");
    q(&c, "BEGIN");
    let actual = q(
        &c,
        "UPDATE docs SET n=n+(SELECT max(n) FROM docs) RETURNING n",
    );
    let expected = q(
        &c,
        "UPDATE native SET n=n+(SELECT max(n) FROM native) RETURNING n",
    );
    assert_eq!(actual.rows, expected.rows);
    assert_eq!(
        q(&c, "SELECT n FROM docs ORDER BY n").rows,
        vec![
            vec![Value::Integer(4)],
            vec![Value::Integer(5)],
            vec![Value::Integer(6)]
        ]
    );
    c.check_collection_integrity("docs", Default::default())
        .unwrap();
    q(&c, "ROLLBACK");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs(id,n) VALUES(docs:z,9)");
    let changes_before = q(&c, "SELECT total_changes()").rows;
    let error=c.execute("UPDATE docs SET n=CASE WHEN n=1 THEN n+10 ELSE (SELECT n FROM native WHERE 0) END RETURNING n",&Parameters::new()).unwrap_err();
    assert_eq!(error.code(), "FDB_VALIDATION");
    let changes_after = q(&c, "SELECT total_changes()").rows;
    assert!(
        matches!((&changes_before[0][0], &changes_after[0][0]), (Value::Integer(before), Value::Integer(after)) if after > before)
    );
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
    let result = q(
        &c,
        "UPDATE docs SET n=CASE WHEN n=1 THEN n+10 ELSE (SELECT 100)+n END RETURNING n",
    );
    assert_eq!(result.affected, 4);
    assert_eq!(
        q(&c, "SELECT n FROM docs ORDER BY n").rows,
        vec![
            vec![Value::Integer(11)],
            vec![Value::Integer(102)],
            vec![Value::Integer(103)],
            vec![Value::Integer(109)]
        ]
    );
    c.check_collection_integrity("docs", Default::default())
        .unwrap();
    q(&c, "ROLLBACK");
    assert_eq!(
        q(&c, "SELECT n FROM docs ORDER BY n").rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(3)]
        ]
    );
}

#[test]
fn target_named_cte_writes_preserve_native_table_binding() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n) VALUES(1),(2),(3)");
    q(&c, "CREATE UNIQUE INDEX docs_n ON docs(n)");
    q(&c, "CREATE TABLE native(n INTEGER UNIQUE)");
    q(&c, "INSERT INTO native VALUES(1),(2),(3)");
    for shape in [
        "direct",
        "derived",
        "compound",
        "union",
        "intersect",
        "except",
        "exists",
        "membership",
        "nested_with",
        "shadowed_with",
    ] {
        for hint in ["", "MATERIALIZED", "NOT MATERIALIZED"] {
            for (aliased, alias_cte) in [(false, false), (true, false), (true, true)] {
                for projection in ["n", "sum(n) AS n", "n+1 AS n", "count(*) AS n"] {
                    for delete in [false, true] {
                        let sql = |table: &str| {
                            let name = if alias_cte { "target" } else { table };
                            let alias = if aliased { " AS target" } else { "" };
                            let write = if delete {
                                format!("DELETE FROM {table}{alias}")
                            } else {
                                format!("UPDATE {table}{alias} SET n=n+10")
                            };
                            let body = format!("SELECT {projection} FROM {name}");
                            let body = match shape {
                                "derived" => format!("SELECT n FROM ({body}) q"),
                                "compound" => format!("{body} UNION ALL SELECT 2"),
                                "nested_with" => format!("WITH inner_q AS ({body}) SELECT n FROM inner_q"),
                                "shadowed_with" => format!("WITH {name} AS (SELECT 3 AS n), inner_q AS ({body}) SELECT n FROM inner_q"),
                                "union" => format!("{body} UNION SELECT 2"),
                                "intersect" => format!("{body} INTERSECT SELECT 2"),
                                "except" => format!("{body} EXCEPT SELECT 2"),
                                "exists" => format!("SELECT n FROM ({body}) q WHERE EXISTS(SELECT 1 FROM {name} x WHERE x.n=2)"),
                                "membership" => format!("SELECT n FROM ({body}) q WHERE n IN (SELECT x.n FROM {name} x)"),
                                _ => body,
                            };
                            format!("WITH {name} AS (SELECT 2 AS n), chosen AS {hint} ({body}) {write} WHERE n IN (SELECT n FROM chosen) RETURNING n")
                        };
                        q(&c, "BEGIN");
                        let expected = match c.execute(&sql("native"), &Parameters::new()) {
                            Ok(result) => result,
                            Err(error) => {
                                assert!(
                                    shape == "nested_with"
                                        && alias_cte
                                        && error.to_string().contains("no such table: target"),
                                    "{}: {error}",
                                    sql("native")
                                );
                                // This shape is rejected by the pinned native write
                                // planner and supplies no collection result oracle.
                                q(&c, "ROLLBACK");
                                continue;
                            }
                        };
                        let actual = q(&c, &sql("docs"));
                        assert_eq!(actual.rows, expected.rows, "{}", sql("docs"));
                        assert_eq!(actual.affected, expected.affected);
                        assert_eq!(
                            q(&c, "SELECT n FROM docs ORDER BY n").rows,
                            q(&c, "SELECT n FROM native ORDER BY n").rows
                        );
                        q(&c, "ROLLBACK");
                        assert_eq!(
                            q(&c, "SELECT n FROM docs ORDER BY n").rows,
                            vec![
                                vec![Value::Integer(1)],
                                vec![Value::Integer(2)],
                                vec![Value::Integer(3)]
                            ]
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn target_named_cte_constraint_failure_restores_rows_indexes_and_prior_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "CREATE UNIQUE INDEX docs_n ON docs(n)");
    q(&c, "INSERT INTO docs(n) VALUES(1),(2),(3)");
    let original = q(&c, "SELECT n FROM docs ORDER BY n").rows;
    for alias in ["docs", "target"] {
        q(&c, "BEGIN");
        q(&c, "INSERT INTO docs(n) VALUES(9)");
        let pending = q(&c, "SELECT n FROM docs ORDER BY n").rows;
        let sql = format!("WITH {alias} AS (SELECT 2 AS n), chosen AS (SELECT n FROM {alias} WHERE n<$max) UPDATE docs AS {alias} SET n=CASE WHEN n=1 THEN 10 ELSE 20 END WHERE n IN (SELECT n FROM chosen) RETURNING n");
        let params = Parameters::from([("$max".into(), Value::Integer(4))]);
        assert!(c.execute(&sql, &Parameters::new()).is_err());
        assert!(c.execute(&sql, &params).is_err());
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        assert_eq!(q(&c, "SELECT n FROM docs ORDER BY n").rows, pending);
        let audit = c
            .check_collection_integrity("docs", Default::default())
            .unwrap();
        assert_eq!(audit.documents, 4);
        assert_eq!(audit.index_entries, 4);
        let retry = sql.replace("CASE WHEN n=1 THEN 10 ELSE 20 END", "n+10");
        let result = c.execute(&retry, &params).unwrap();
        assert_eq!(result.affected, 3);
        assert_eq!(
            q(&c, "SELECT n FROM docs ORDER BY n").rows,
            vec![
                vec![Value::Integer(9)],
                vec![Value::Integer(11)],
                vec![Value::Integer(12)],
                vec![Value::Integer(13)]
            ]
        );
        c.check_collection_integrity("docs", Default::default())
            .unwrap();
        q(&c, "ROLLBACK");
        assert_eq!(q(&c, "SELECT n FROM docs ORDER BY n").rows, original);
        c.check_collection_integrity("docs", Default::default())
            .unwrap();
    }
}

#[test]
fn pinned_nested_write_cte_flattening_preserves_closed_source_results() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "INSERT INTO native VALUES(1),(2),(3)");
    for alias in ["", " AS target"] {
        for hint in ["", "MATERIALIZED", "NOT MATERIALIZED"] {
            for projection in ["n", "n+1 AS n", "sum(n) AS n", "count(*) AS n"] {
                for shadow in [false, true] {
                    for delete in [false, true] {
                        let local = if shadow {
                            "native AS (SELECT 3 AS n), "
                        } else {
                            ""
                        };
                        let nested = format!("WITH native AS (SELECT 2 AS n), chosen AS ({hint_placeholder}{local}inner_q AS {hint} (SELECT {projection} FROM native) SELECT n FROM inner_q)",hint_placeholder="WITH ");
                        let (local, source) = if shadow {
                            ("local_native AS (SELECT 3 AS n), ", "local_native")
                        } else {
                            ("", "native")
                        };
                        let flat = format!("WITH native AS (SELECT 2 AS n), {local}inner_q AS {hint} (SELECT {projection} FROM {source}), chosen AS (SELECT n FROM inner_q)");
                        let write = if delete {
                            format!("DELETE FROM native{alias}")
                        } else {
                            format!("UPDATE native{alias} SET n=n+10")
                        };
                        let mut expected = None;
                        for prefix in [nested, flat] {
                            q(&c, "BEGIN");
                            let sql = format!(
                                "{prefix} {write} WHERE n IN (SELECT n FROM chosen) RETURNING n"
                            );
                            let result = q(&c, &sql);
                            let after = q(&c, "SELECT n FROM main.native ORDER BY n").rows;
                            if let Some((rows, affected, stored)) = &expected {
                                assert_eq!(&result.rows, rows, "{sql}");
                                assert_eq!(result.affected, *affected, "{sql}");
                                assert_eq!(&after, stored, "{sql}");
                            } else {
                                expected = Some((result.rows, result.affected, after));
                            }
                            q(&c, "ROLLBACK");
                        }
                    }
                }
            }
        }
    }
}
