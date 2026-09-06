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
        "INSERT INTO docs {id:docs:a,n:1,flag:true,profile:{city:'A'},tags:[1,2],ref:docs:b}",
    );
    q(
        &c,
        "INSERT INTO docs {id:docs:b,n:2,flag:false,profile:{city:'B'},tags:[],ref:docs:a}",
    );
    q(&c, "UPDATE docs SET data=X'31'");
    (db, c)
}
#[test]
fn derived_columns_preserve_types_paths_parameters_and_nested_stars() {
    let (_db, c) = setup();
    let expected = q(
        &c,
        "SELECT id,flag,docs.profile.city,tags,data,ref FROM docs ORDER BY id",
    );
    let result=q(&c,"SELECT q.id,q.flag,q.profile.city,q.tags,q.data,q.ref FROM (SELECT id,flag,profile,tags,data,ref FROM docs) q ORDER BY q.id");
    assert_eq!(result.rows, expected.rows);
    assert_eq!(q(&c,"SELECT r.* FROM (SELECT q.id,q.flag,q.profile.city AS city,q.tags,q.data,q.ref FROM (SELECT id,flag,profile,tags,data,ref FROM docs) q) r ORDER BY r.id").rows,expected.rows);
    assert_eq!(
        q(
            &c,
            "SELECT q.document.profile.city FROM (SELECT * FROM docs) q ORDER BY q.document.id"
        )
        .rows,
        vec![
            vec![Value::String("A".into())],
            vec![Value::String("B".into())]
        ]
    );
    let params = Parameters::from([
        ("$min".into(), Value::Integer(2)),
        ("$offset".into(), Value::Integer(10)),
    ]);
    assert_eq!(
        c.execute(
            "SELECT q.n+$offset FROM (SELECT n FROM docs WHERE n >= $min) q",
            &params
        )
        .unwrap()
        .rows,
        vec![vec![Value::Integer(12)]]
    );
    assert_eq!(q(&c,"SELECT q.* FROM (SELECT n+1 AS next,doc::get(profile,'$.city') AS city FROM docs) q ORDER BY q.next").rows,vec![vec![Value::Integer(2),Value::String("A".into())],vec![Value::Integer(3),Value::String("B".into())]]);
    assert_eq!(
        q(&c, "SELECT * FROM (SELECT 42 AS answer) q").rows,
        vec![vec![Value::Integer(42)]]
    );
    let positional = Parameters::from([
        ("?1".into(), Value::Integer(10)),
        ("?2".into(), Value::Integer(2)),
    ]);
    assert_eq!(
        c.execute(
            "SELECT q.n+? FROM (SELECT n FROM docs WHERE n>=?) q",
            &positional
        )
        .unwrap()
        .rows,
        vec![vec![Value::Integer(12)]]
    );
    assert_eq!(
        q(
            &c,
            "SELECT record::fetch(q.ref) FROM (SELECT id,ref FROM docs) q ORDER BY q.id"
        )
        .rows,
        q(&c, "SELECT record::fetch(ref) FROM docs ORDER BY id").rows
    );
    q(&c, "UPDATE docs SET embedding=vector32('[1,0]')");
    assert_eq!(
        q(
            &c,
            "SELECT q.embedding FROM (SELECT id,embedding FROM docs) q ORDER BY q.id"
        )
        .rows,
        q(&c, "SELECT embedding FROM docs ORDER BY id").rows
    );
    let empty = q(&c, "SELECT q.* FROM (SELECT id,flag FROM docs WHERE n=0) q");
    assert_eq!(empty.columns, vec!["id", "flag"]);
    assert!(empty.rows.is_empty());
}
#[test]
fn derived_sources_support_grouping_distinct_and_outer_joins() {
    let (_db, c) = setup();
    q(&c, "CREATE TABLE labels(n INTEGER,label TEXT)");
    q(&c, "INSERT INTO labels VALUES (1,'one'),(3,'three')");
    assert_eq!(q(&c,"SELECT q.n,l.label FROM (SELECT n FROM docs) q LEFT JOIN labels l ON q.n=l.n ORDER BY q.n").rows,q(&c,"SELECT d.n,l.label FROM docs d LEFT JOIN labels l ON d.n=l.n ORDER BY d.n").rows);
    assert_eq!(q(&c,"SELECT l.n,q.flag,q.data FROM labels l LEFT JOIN (SELECT n,flag,data FROM docs) q ON l.n=q.n ORDER BY l.n").rows,vec![vec![Value::Integer(1),Value::Boolean(true),Value::Binary(vec![49])],vec![Value::Integer(3),Value::Null,Value::Null]]);
    assert_eq!(
        q(
            &c,
            "SELECT q.total FROM (SELECT sum(n) AS total FROM docs) q WHERE q.total>2"
        )
        .rows,
        vec![vec![Value::Integer(3)]]
    );
    assert_eq!(
        q(
            &c,
            "SELECT q.data,count(*) FROM (SELECT DISTINCT data FROM docs) q GROUP BY q.data"
        )
        .rows,
        vec![vec![Value::Binary(vec![49]), Value::Integer(1)]]
    );
    assert_eq!(
        q(
            &c,
            "SELECT q.n FROM (SELECT n FROM docs ORDER BY n DESC LIMIT 1) q"
        )
        .rows,
        vec![vec![Value::Integer(2)]]
    );
}
#[test]
fn derived_write_sources_validate_and_keep_unsupported_forms_guarded() {
    let (_db, c) = setup();
    q(&c, "CREATE TABLE copied");
    q(
        &c,
        "DEFINE FIELD n ON copied TYPE integer REQUIRED CHECK (n<2)",
    );
    assert!(c
        .execute(
            "INSERT INTO copied (n) SELECT q.n FROM (SELECT n FROM docs ORDER BY n) q",
            &Parameters::new()
        )
        .is_err());
    assert!(q(&c, "SELECT * FROM copied").rows.is_empty());
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "BEGIN");
    let params = Parameters::from([("$min".into(), Value::Integer(2))]);
    assert_eq!(
        c.execute(
            "INSERT INTO native SELECT q.n FROM (SELECT n FROM docs WHERE n >= $min) q RETURNING n",
            &params
        )
        .unwrap()
        .rows,
        vec![vec![Value::Integer(2)]]
    );
    q(&c, "ROLLBACK");
    assert!(q(&c, "SELECT * FROM native").rows.is_empty());
    for sql in [
        "SELECT q.* FROM (SELECT n,n FROM docs) q",
        "SELECT q.* FROM (SELECT record::fetch(ref) AS target FROM docs) q",
        "SELECT q.* FROM (SELECT '__fastdb_pack'(n) FROM docs) q",
        "SELECT q.* FROM (SELECT * FROM '__fastdb_catalog') q",
        "SELECT q.missing FROM (SELECT n FROM docs) q",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
}
