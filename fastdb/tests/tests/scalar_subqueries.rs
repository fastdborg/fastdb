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
