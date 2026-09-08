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
            "json_each(json_array(1+0,CAST('2' AS INTEGER),NULL)) AS j",
            empty.clone(),
        ),
        (
            "json_tree(json_object('a',json_array(1,2))) AS j",
            empty.clone(),
        ),
        (
            "json_each(CASE WHEN 1 THEN '[1,2]' ELSE 'invalid' END) AS j",
            empty.clone(),
        ),
        (
            "json_each('[' || $values || ']') AS j",
            Parameters::from([("$values".into(), Value::String("1,2,null".into()))]),
        ),
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
    let query="SELECT d.n+json_each.value AS n FROM json_each(coalesce($json,'[0]')) CROSS JOIN docs d ORDER BY json_each.key";
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

#[test]
fn correlated_json_iterators_match_native_rows() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let params = Parameters::new();
    for sql in [
        "CREATE TABLE native(n INTEGER,j TEXT)",
        "INSERT INTO native VALUES(1,'[1,2]'),(2,'[3]'),(3,'[]'),(4,NULL)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,j) SELECT n,j FROM native",
    ] {
        c.execute(sql, &params).unwrap();
    }
    for arg in ["d.j", "coalesce(d.j,'[]')"] {
        for join in ["CROSS JOIN", "LEFT JOIN"] {
            let query = |source| {
                format!("SELECT d.n,x.key,x.value FROM {source} d {join} json_each({arg}) x ORDER BY d.n,x.key")
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
    let query = |source| {
        format!("SELECT d.n,x.value,y.value FROM {source} d CROSS JOIN json_each(d.j) x CROSS JOIN json_each(json_array(x.value+1)) y ORDER BY d.n,x.key")
    };
    let expected = c.execute(&query("native"), &params).unwrap();
    assert_eq!(
        c.execute(&query("docs"), &params).unwrap().rows,
        expected.rows
    );
    for sql in [
        "CREATE TABLE output",
        "DEFINE FIELD n ON output TYPE integer CHECK(n<3)",
        "CREATE UNIQUE INDEX output_n ON output(n)",
        "BEGIN",
        "INSERT INTO output(n) VALUES(0)",
    ] {
        c.execute(sql, &params).unwrap();
    }
    let sql = "INSERT INTO output(n) SELECT x.value FROM docs d CROSS JOIN json_each(d.j) x WHERE x.value IS NOT NULL ORDER BY d.n,x.key RETURNING n";
    assert_eq!(
        c.execute(sql, &params).unwrap_err().code(),
        "FDB_VALIDATION"
    );
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(
        c.execute("SELECT n FROM output", &params).unwrap().rows,
        vec![vec![Value::Integer(0)]]
    );
    assert!(c
        .lookup_index("output", "output_n", &Value::Integer(1))
        .unwrap()
        .is_empty());
    c.execute("UPDATE docs SET j='[]' WHERE n=2", &params)
        .unwrap();
    assert_eq!(
        c.execute(sql, &params).unwrap().rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    c.execute("ROLLBACK", &params).unwrap();
    assert_eq!(
        c.check_collection_integrity("output", fastdb::IntegrityLimits::default())
            .unwrap()
            .documents,
        0
    );
}
