use fastdb::{Database, Parameters, Value};
use turso_core::{Database as Baseline, Numeric, Value as EngineValue};
fn convert(v: EngineValue) -> Value {
    match v {
        EngineValue::Null => Value::Null,
        EngineValue::Numeric(Numeric::Integer(i)) => Value::Integer(i),
        EngineValue::Numeric(Numeric::Float(n)) => Value::Number(n.into()),
        EngineValue::Text(t) => Value::String(t.as_str().into()),
        EngineValue::Blob(b) => Value::Binary(b),
    }
}
#[test]
fn ordinary_sql_matches_the_pinned_engine() {
    let baseline = Baseline::open_file(Baseline::io_for_path(":memory:").expect("io"), ":memory:")
        .expect("baseline");
    let raw = baseline.connect().expect("baseline connection");
    let db = Database::open(":memory:").expect("fastdb");
    let c = db.connect().expect("fastdb connection");
    for sql in [
        "CREATE TABLE accounts(id INTEGER PRIMARY KEY, name TEXT UNIQUE)",
        "INSERT INTO accounts VALUES (1, 'Alice'), (2, 'Bob') RETURNING *",
        "SELECT id, upper(name), id / 2.0 FROM accounts ORDER BY id",
        "SELECT r'posts:p1' FROM (SELECT 1 AS r)",
        "SELECT 9223372036854775807, NULL, x'00ff', 'users:u1'",
        "SELECT 1 AS [document], 2 AS `field`, 3 AS 'alias'",
        "BEGIN",
        "UPDATE accounts SET name = 'Charlie' WHERE id = 1 RETURNING *",
        "ROLLBACK",
        "SELECT * FROM accounts ORDER BY id",
        "CREATE TABLE copied AS SELECT * FROM accounts",
        "SELECT * FROM copied ORDER BY id",
        "INSERT INTO accounts VALUES (3, 'Bob') ON CONFLICT(name) DO UPDATE SET id = 3 RETURNING *",
        "SELECT * FROM accounts ORDER BY id",
    ] {
        let mut stmt = raw.prepare(sql).expect(sql);
        let names: Vec<String> = (0..stmt.num_columns())
            .map(|i| stmt.get_column_name(i).into_owned())
            .collect();
        let expected: Vec<Vec<Value>> = stmt
            .run_collect_rows()
            .expect(sql)
            .into_iter()
            .map(|row| row.into_iter().map(convert).collect())
            .collect();
        let actual = c.execute(sql, &Parameters::new()).expect(sql);
        assert_eq!(actual.columns, names, "{sql}");
        assert_eq!(actual.rows, expected, "{sql}");
    }
}

#[test]
fn malformed_collection_sql_preserves_native_parse_errors_and_active_work() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let baseline =
        Baseline::open_file(Baseline::io_for_path(":memory:").unwrap(), ":memory:").unwrap();
    let raw = baseline.connect().unwrap();
    for sql in [
        "CREATE TABLE docs",
        "CREATE TABLE native(v INTEGER)",
        "BEGIN",
        "INSERT INTO docs {v:1}",
    ] {
        c.execute(sql, &Parameters::new()).unwrap();
    }
    for sql in [
        "SELECT 1 + FROM docs",
        "SELECT * FROM docs WHERE",
        "WITH x AS (SELECT * FROM docs WHERE) SELECT * FROM x",
        "INSERT INTO native SELECT FROM docs",
        "SELECT 'docs' + FROM native",
        "SELECT 1 + FROM native",
    ] {
        let expected = raw
            .prepare(sql)
            .expect_err("baseline parse failure")
            .to_string();
        let report = c.execute_report(sql, &Parameters::new());
        let fastdb::Error::Engine(actual) = report.result.unwrap_err() else {
            panic!("expected native parse error: {sql}");
        };
        assert_eq!(actual.to_string(), expected, "{sql}");
        assert_eq!(report.transaction_after, fastdb::TransactionState::Active);
    }
    assert_eq!(
        c.execute("SELECT v FROM docs", &Parameters::new())
            .unwrap()
            .rows,
        vec![vec![Value::Integer(1)]]
    );
    assert!(c
        .execute("SELECT * FROM native", &Parameters::new())
        .unwrap()
        .rows
        .is_empty());
    assert_eq!(
        c.execute("SELECT 1; SELECT 2", &Parameters::new())
            .unwrap_err()
            .code(),
        "FDB_UNSUPPORTED"
    );
    c.execute("ROLLBACK", &Parameters::new()).unwrap();
}
