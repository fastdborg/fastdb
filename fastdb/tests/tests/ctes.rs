use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
fn setup() -> (Database, fastdb::Connection) {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(
        &c,
        "INSERT INTO docs {id:docs:a,n:1,flag:true,profile:{city:'A'},ref:docs:b}",
    );
    q(
        &c,
        "INSERT INTO docs {id:docs:b,n:2,flag:false,profile:{city:'B'},ref:docs:a}",
    );
    q(&c, "UPDATE docs SET data=X'31'");
    (db, c)
}
#[test]
fn collection_ctes_preserve_typed_columns_chains_and_parameters() {
    let (_db, c) = setup();
    assert_eq!(q(&c,"WITH a AS (SELECT id,n,flag,profile,data,ref FROM docs), b AS (SELECT * FROM a) SELECT b.id,b.flag,b.profile.city,b.data,b.ref FROM b ORDER BY b.id").rows,
        q(&c,"SELECT id,flag,docs.profile.city,data,ref FROM docs ORDER BY id").rows);
    let params = Parameters::from([("$min".into(), Value::Integer(2))]);
    let result = c
        .execute(
            "WITH a(value,enabled) AS (SELECT n,flag FROM docs WHERE n >= $min) SELECT * FROM a",
            &params,
        )
        .unwrap();
    assert_eq!(result.columns, vec!["value", "enabled"]);
    assert_eq!(
        result.rows,
        vec![vec![Value::Integer(2), Value::Boolean(false)]]
    );
    assert_eq!(
        q(
            &c,
            "WITH a AS (SELECT DISTINCT data FROM docs) SELECT * FROM a"
        )
        .rows,
        vec![vec![Value::Binary(vec![49])]]
    );
    assert_eq!(
        q(
            &c,
            "WITH a AS (SELECT sum(n) AS total FROM docs) SELECT total FROM a"
        )
        .rows,
        vec![vec![Value::Integer(3)]]
    );
    assert_eq!(
        q(
            &c,
            "WITH a AS (SELECT id,ref FROM docs) SELECT record::fetch(ref) FROM a ORDER BY id"
        )
        .rows,
        q(&c, "SELECT record::fetch(ref) FROM docs ORDER BY id").rows
    );
    let empty = q(
        &c,
        "WITH a AS (SELECT id,flag FROM docs WHERE n=0) SELECT * FROM a",
    );
    assert_eq!(empty.columns, vec!["id", "flag"]);
    assert!(empty.rows.is_empty());
}
#[test]
fn cte_scope_native_sources_and_materialized_references_are_preserved() {
    let (_db, c) = setup();
    q(&c, "CREATE TABLE labels(n INTEGER,label TEXT)");
    q(&c, "INSERT INTO labels VALUES (1,'one')");
    assert_eq!(q(&c,"WITH l AS (SELECT * FROM labels), a AS (SELECT n,flag FROM docs) SELECT a.n,l.label FROM a LEFT JOIN l ON a.n=l.n ORDER BY a.n").rows,
        vec![vec![Value::Integer(1),Value::String("one".into())],vec![Value::Integer(2),Value::Null]]);
    assert_eq!(
        q(
            &c,
            "WITH docs AS (SELECT n FROM main.docs) SELECT n FROM docs ORDER BY n"
        )
        .rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    assert_eq!(q(&c,"WITH a AS (SELECT n FROM docs) SELECT a.n,r.n FROM a JOIN (WITH a AS (SELECT n+10 AS n FROM docs) SELECT n FROM a) r ON r.n=a.n+10 ORDER BY a.n").rows,
        vec![vec![Value::Integer(1),Value::Integer(11)],vec![Value::Integer(2),Value::Integer(12)]]);
    q(&c, "CREATE INDEX docs_data ON docs(data)");
    let binary = Parameters::from([("$blob".into(), Value::Binary(vec![49]))]);
    assert_eq!(c.execute("WITH l AS (SELECT $blob AS data), a AS (SELECT data FROM docs WHERE data=$blob) SELECT l.data,a.data FROM l JOIN a ON l.data=a.data",&binary).unwrap().rows,
        vec![vec![Value::Binary(vec![49]),Value::Binary(vec![49])];2]);
    let materialized=q(&c,"WITH a AS MATERIALIZED (SELECT random() AS value FROM docs) SELECT x.value,y.value FROM a x JOIN a y ON x.value=y.value");
    assert_eq!(materialized.rows.len(), 2);
    for row in materialized.rows {
        assert_eq!(row[0], row[1]);
    }
    assert_eq!(
        q(
            &c,
            "WITH a AS NOT MATERIALIZED (SELECT n FROM docs) SELECT n FROM a ORDER BY n"
        )
        .rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    let recursive=c.execute("WITH RECURSIVE a(n) AS (VALUES (1) UNION ALL SELECT n+1 FROM a WHERE n<2) SELECT sum(n) FROM a",&Parameters::new()).unwrap_err();
    assert_eq!(recursive.code(), "FDB_ENGINE");
    assert!(recursive
        .to_string()
        .contains("Recursive CTEs are not yet supported"));
}
#[test]
fn cte_insert_sources_validate_atomically_and_reject_unsupported_definitions() {
    let (_db, c) = setup();
    q(&c, "CREATE TABLE copied");
    q(
        &c,
        "DEFINE FIELD n ON copied TYPE integer REQUIRED CHECK (n<2)",
    );
    assert!(c
        .execute(
            "INSERT INTO copied (n) WITH a AS (SELECT n FROM docs) SELECT n FROM a",
            &Parameters::new()
        )
        .is_err());
    assert!(q(&c, "SELECT * FROM copied").rows.is_empty());
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "BEGIN");
    let params = Parameters::from([("$min".into(), Value::Integer(2))]);
    assert_eq!(c.execute("INSERT INTO native WITH a AS (SELECT n FROM docs WHERE n >= $min) SELECT n FROM a RETURNING n",&params).unwrap().rows,vec![vec![Value::Integer(2)]]);
    q(&c, "ROLLBACK");
    assert!(q(&c, "SELECT * FROM native").rows.is_empty());
    for sql in [
        "WITH a(x,y) AS (SELECT n FROM docs) SELECT * FROM a",
        "WITH a(x,x) AS (SELECT n,flag FROM docs) SELECT * FROM a",
        "WITH a AS (SELECT record::fetch(ref) AS r FROM docs) SELECT * FROM a",
        "WITH RECURSIVE a AS (SELECT n FROM docs) SELECT * FROM a",
        "WITH a AS (SELECT * FROM b), b AS (SELECT n FROM docs) SELECT * FROM a",
        "WITH a AS (SELECT '__fastdb_pack'(n) FROM docs) SELECT * FROM a",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
}

#[test]
fn leading_with_insert_preserves_validation_conflicts_and_scope_boundaries() {
    let (_db, c) = setup();
    q(&c, "CREATE TABLE copied");
    q(&c, "DEFINE FIELD flag ON copied TYPE boolean REQUIRED");
    q(&c, "BEGIN");
    let params = Parameters::from([("$min".into(), Value::Integer(2))]);
    let rows=c.execute("WITH a AS (SELECT n,flag FROM docs WHERE n >= $min) INSERT INTO copied (n,flag) SELECT n,flag FROM a RETURNING n,flag",&params).unwrap().rows;
    assert_eq!(rows, vec![vec![Value::Integer(2), Value::Boolean(false)]]);
    q(&c, "ROLLBACK");
    assert!(q(&c, "SELECT * FROM copied").rows.is_empty());
    q(
        &c,
        "DEFINE FIELD n ON copied TYPE integer REQUIRED CHECK (n<2)",
    );
    assert!(c
        .execute(
            "WITH a AS (SELECT n,flag FROM docs) INSERT INTO copied (n,flag) SELECT n,flag FROM a",
            &Parameters::new()
        )
        .is_err());
    assert!(q(&c, "SELECT * FROM copied").rows.is_empty());
    q(&c, "CREATE TABLE native(n INTEGER PRIMARY KEY)");
    assert_eq!(
        q(
            &c,
            "WITH a AS (SELECT n FROM docs) INSERT INTO native SELECT n FROM a RETURNING n"
        )
        .rows
        .len(),
        2
    );
    assert_eq!(
        q(
            &c,
            "WITH a AS (SELECT n FROM docs) INSERT OR IGNORE INTO native SELECT n FROM a"
        )
        .affected,
        0
    );
    assert_eq!(
        q(
            &c,
            "WITH a AS (SELECT 3 AS n) INSERT INTO native SELECT n FROM a RETURNING n"
        )
        .rows,
        vec![vec![Value::Integer(3)]]
    );
    q(&c, "CREATE TABLE a(n INTEGER)");
    q(&c, "INSERT INTO a VALUES (99)");
    let error=c.execute("WITH a AS (SELECT n FROM docs) INSERT OR REPLACE INTO native SELECT n FROM a RETURNING (SELECT n FROM a LIMIT 1)",&Parameters::new()).unwrap_err();
    assert_eq!(error.code(), "FDB_UNSUPPORTED");
    let error=c.execute("WITH a AS (SELECT n FROM docs) INSERT OR REPLACE INTO native SELECT n FROM a RETURNING n IN a",&Parameters::new()).unwrap_err();
    assert_eq!(error.code(), "FDB_UNSUPPORTED");

    assert_eq!(
        q(&c, "SELECT n FROM native ORDER BY n").rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(3)]
        ]
    );
}
