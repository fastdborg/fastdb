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

#[test]
fn unnamed_derived_collections_preserve_values_and_join_scope() {
    let (_db, c) = setup();
    for (anonymous, named) in [
        ("SELECT n,flag,data FROM (SELECT n,flag,data FROM docs) ORDER BY n", "SELECT n,flag,data FROM (SELECT n,flag,data FROM docs) d ORDER BY n"),
        ("SELECT n,m FROM (SELECT n FROM docs) JOIN (SELECT n AS m FROM docs) ON n=m ORDER BY n", "SELECT n,m FROM (SELECT n FROM docs) a JOIN (SELECT n AS m FROM docs) b ON n=m ORDER BY n"),
        ("SELECT * FROM (SELECT flag,data FROM docs WHERE n=1)", "SELECT * FROM (SELECT flag,data FROM docs WHERE n=1) d"),
        ("SELECT flag,data FROM (SELECT flag,data FROM (SELECT flag,data FROM docs))", "SELECT flag,data FROM docs"),
        ("SELECT n,m FROM (SELECT n+0 AS n FROM docs) JOIN (SELECT n+0 AS m FROM docs) ON n=m ORDER BY n", "SELECT a.n AS n,b.m AS m FROM (SELECT n+0 AS n FROM docs) a JOIN (SELECT n+0 AS m FROM docs) b ON a.n=b.m ORDER BY a.n"),
    ] {
        let expected = q(&c, named);
        let actual = q(&c, anonymous);
        assert_eq!(actual.columns, expected.columns, "{anonymous}");
        assert_eq!(actual.rows, expected.rows, "{anonymous}");
        assert_eq!(c.profile_select(anonymous, &Parameters::new()).unwrap().result.rows, expected.rows, "{anonymous}");
    }
    assert!(c
        .execute(
            "SELECT n FROM (SELECT n FROM docs) __fastdb_anonymous_0",
            &Parameters::new()
        )
        .is_err());
    assert!(c
        .execute(
            "SELECT n FROM (SELECT n FROM docs) JOIN (SELECT n FROM docs) ON 1=1",
            &Parameters::new()
        )
        .is_err());
    q(&c, "CREATE TABLE copied");
    q(
        &c,
        "INSERT INTO copied (n,flag,data) SELECT n,flag,data FROM (SELECT n,flag,data FROM docs)",
    );
    assert_eq!(
        q(&c, "SELECT n,flag,data FROM copied ORDER BY n").rows,
        q(&c, "SELECT n,flag,data FROM docs ORDER BY n").rows
    );
}

#[test]
fn unnamed_derived_sources_preserve_parameters_and_correlation() {
    let (_db, c) = setup();
    let mut params = Parameters::new();
    params.insert("$min".into(), Value::Integer(1));
    params.insert("$extra".into(), Value::Integer(5));
    for source in ["docs d", "(SELECT n FROM docs) d"] {
        for (anonymous, named) in [
            ("SELECT n+$extra FROM (SELECT n FROM docs WHERE n>=$min) WHERE n>=d.n ORDER BY n LIMIT 1", "SELECT n+$extra FROM (SELECT n FROM docs WHERE n>=$min) i WHERE n>=d.n ORDER BY n LIMIT 1"),
            ("SELECT flag FROM (SELECT n,flag FROM docs WHERE n>=$min) WHERE n=d.n AND $extra=5", "SELECT flag FROM (SELECT n,flag FROM docs WHERE n>=$min) i WHERE n=d.n AND $extra=5"),
        ] {
            let expected_sql = format!("SELECT n,({named}) FROM {source} ORDER BY n");
            let sql = format!("SELECT n,({anonymous}) FROM {source} ORDER BY n");
            let expected = c.execute(&expected_sql, &params).unwrap().rows;
            let values = if anonymous.starts_with("SELECT flag") {
                [Value::Boolean(true), Value::Boolean(false)]
            } else {
                [Value::Integer(6), Value::Integer(7)]
            };
            assert_eq!(expected, vec![vec![Value::Integer(1), values[0].clone()], vec![Value::Integer(2), values[1].clone()]]);
            let mut missing = params.clone();
            missing.remove("$min");
            assert!(matches!(c.execute(&sql, &missing), Err(fastdb::Error::Parameter(_))));
            assert!(matches!(c.profile_select(&sql, &missing), Err(fastdb::Error::Parameter(_))));

            assert_eq!(c.execute(&sql, &params).expect(&sql).rows, expected, "{sql}");
            assert_eq!(c.profile_select(&sql, &params).expect(&sql).result.rows, expected, "{sql}");
        }
    }
}

#[test]
fn mixed_unnamed_derived_sources_preserve_native_columns() {
    let (_db, c) = setup();
    q(&c, "CREATE TABLE labels(m INTEGER,label TEXT)");
    q(&c, "INSERT INTO labels VALUES(1,'A'),(3,'C')");
    for sql in [
        "SELECT n,flag,label FROM (SELECT n,flag FROM docs) LEFT JOIN (SELECT m,label FROM labels) ON n=m ORDER BY n",
        "SELECT n,flag,label FROM (SELECT m,label FROM labels) RIGHT JOIN (SELECT n,flag FROM docs) ON n=m ORDER BY n",
    ] {
        let expected = vec![vec![Value::Integer(1),Value::Boolean(true),Value::String("A".into())],vec![Value::Integer(2),Value::Boolean(false),Value::Null]];
        assert_eq!(q(&c,sql).rows, expected, "{sql}");
        assert_eq!(c.profile_select(sql,&Parameters::new()).unwrap().result.rows,expected,"{sql}");
    }
    let sql = "SELECT * FROM (SELECT flag FROM docs WHERE n=1) CROSS JOIN (SELECT label FROM labels WHERE m=1)";
    let result = q(&c, sql);
    assert_eq!(result.columns, vec!["flag", "label"]);
    assert_eq!(
        result.rows,
        vec![vec![Value::Boolean(true), Value::String("A".into())]]
    );
}

#[test]
fn mixed_derived_comparisons_preserve_native_affinity_and_collation() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {n:1,v:'a'}",
        "INSERT INTO docs {n:2,v:'01'}",
        "INSERT INTO docs {n:3,v:null}",
        "CREATE TABLE labels(m INTEGER,label TEXT COLLATE NOCASE)",
        "INSERT INTO labels VALUES(1,'A'),(2,'1'),(3,NULL)",
    ] {
        q(&c, sql);
    }
    for native_source in [
        "(SELECT m,label FROM labels)",
        "(SELECT m,label FROM (SELECT m,label FROM labels))",
        "(SELECT m,label COLLATE BINARY AS label FROM labels)",
        "(WITH l AS MATERIALIZED (SELECT m,label FROM labels) SELECT m,label FROM l)",
        "(SELECT m,label FROM labels ORDER BY m LIMIT 3)",
    ] {
        for predicate in [
            "v=label",
            "label=v",
            "v IS label",
            "v<label",
            "label IN (v)",
            "label NOT IN (v)",
            "label COLLATE BINARY IN (v)",
            "label COLLATE NOCASE IN (v,NULL)",
            "label NOT IN (v,NULL)",
            "label IN (v,NULL)",
            "label IN (v,'z')",
            "v=+label",
        ] {
            let sql = format!("SELECT n,m FROM (SELECT n,v FROM docs) CROSS JOIN {native_source} WHERE {predicate} ORDER BY n,m");
            // Document values have no declared SQL column affinity/collation.
            // Literal operands isolate the native right-hand column semantics.
            let mut expected = Vec::new();
            for (n, value) in [(1, "'a'"), (2, "'01'"), (3, "NULL")] {
                let condition = predicate.replace('v', value);
                expected.extend(
                    q(
                        &c,
                        &format!("SELECT {n},m FROM {native_source} WHERE {condition} ORDER BY m"),
                    )
                    .rows,
                );
            }
            assert_eq!(q(&c, &sql).rows, expected, "{sql}");
            assert_eq!(
                c.profile_select(&sql, &Parameters::new())
                    .unwrap()
                    .result
                    .rows,
                expected,
                "{sql}"
            );
        }
    }
}
