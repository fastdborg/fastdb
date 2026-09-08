use fastdb::{Database, Parameters, Value};

#[test]
fn closed_json_table_functions_match_native_mixed_joins() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let empty = Parameters::new();
    for sql in [
        "CREATE TABLE docs",
        "CREATE TABLE native(n INTEGER)",
        "INSERT INTO native VALUES(1),(2)",
        "INSERT INTO docs(n) SELECT n FROM native",
    ] {
        c.execute(sql, &empty).unwrap();
    }
    for (iterator, params) in [
        ("json_each('[1,2,null]') AS j", empty.clone()),
        (
            "json_each($json) AS j",
            Parameters::from([("$json".into(), Value::String("[1,2,null]".into()))]),
        ),
        ("json_tree('{\"a\":[1,2]}') AS j", empty.clone()),
        (
            "json_each(?1) AS j",
            Parameters::from([("?1".into(), Value::String("[1,2,null]".into()))]),
        ),
        (
            "json_each(?) AS j",
            Parameters::from([("?1".into(), Value::String("[1,2,null]".into()))]),
        ),
        ("json_each('{\"a\":[1,2]}','$.a') AS j", empty.clone()),
    ] {
        for (projection, join) in [
            ("d.n,j.key,j.value,j.type,j.atom", "CROSS JOIN"),
            ("d.n,j.*", "CROSS JOIN"),
            ("d.n,j.value", "LEFT JOIN"),
        ] {
            let on = if join == "LEFT JOIN" {
                " ON d.n=j.value"
            } else {
                ""
            };
            let query = |source| {
                format!(
                    "SELECT {projection} FROM {source} d {join} {iterator}{on} ORDER BY d.n,j.id"
                )
            };
            let expected = c.execute(&query("native"), &params).unwrap();
            let sql = query("docs");
            for actual in [
                c.execute(&sql, &params).unwrap(),
                c.profile_select(&sql, &params).unwrap().result,
            ] {
                assert_eq!(actual.columns, expected.columns, "{sql}");
                assert_eq!(actual.rows, expected.rows, "{sql}");
            }
        }
    }
}

#[test]
fn json_table_function_sources_preserve_write_failure_and_retry() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let empty = Parameters::new();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs(n) VALUES(1)",
        "CREATE TABLE output",
        "DEFINE FIELD n ON output TYPE integer CHECK(n<3)",
        "CREATE UNIQUE INDEX output_n ON output(n)",
        "BEGIN",
        "INSERT INTO output {id:output:prior,n:0}",
    ] {
        c.execute(sql, &empty).unwrap();
    }
    let query="SELECT d.n+json_each.value AS n FROM json_each($json) CROSS JOIN docs d ORDER BY json_each.key";
    assert_eq!(
        c.execute(query, &empty).unwrap_err().code(),
        "FDB_PARAMETER"
    );
    let sql = format!("INSERT INTO output(n) {query} RETURNING n");
    let bad = Parameters::from([("$json".into(), Value::String("[1,2]".into()))]);
    assert_eq!(c.execute(&sql, &bad).unwrap_err().code(), "FDB_VALIDATION");
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(
        c.execute("SELECT n FROM output", &empty).unwrap().rows,
        vec![vec![Value::Integer(0)]]
    );
    assert!(c
        .lookup_index("output", "output_n", &Value::Integer(2))
        .unwrap()
        .is_empty());
    let good = Parameters::from([("$json".into(), Value::String("[1]".into()))]);
    assert_eq!(
        c.execute(&sql, &good).unwrap().rows,
        vec![vec![Value::Integer(2)]]
    );
    assert_eq!(
        c.check_collection_integrity("output", fastdb::IntegrityLimits::default())
            .unwrap()
            .documents,
        2
    );
    c.execute("ROLLBACK", &empty).unwrap();
    assert_eq!(
        c.check_collection_integrity("output", fastdb::IntegrityLimits::default())
            .unwrap()
            .documents,
        0
    );
}
