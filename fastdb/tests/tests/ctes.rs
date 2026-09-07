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

#[test]
fn values_ctes_preserve_records_binary_composites_and_vectors() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let id = Value::Record(fastdb::Record {
        table: "docs".into(),
        key: fastdb::Key::String("a".into()),
    });
    let data = Value::Binary(vec![0, 255]);
    let payload = Value::Object(std::collections::BTreeMap::from([(
        "n".into(),
        Value::Integer(7),
    )]));
    let vector = Value::vector64(&[1.0, 2.0, 3.0]).unwrap();
    let params = Parameters::from([
        ("$id".into(), id.clone()),
        ("$flag".into(), Value::Boolean(true)),
        ("$data".into(), data.clone()),
        ("$payload".into(), payload.clone()),
        ("$vector".into(), vector.clone()),
    ]);
    let sql = "WITH input(id,flag,data,payload,embedding) AS (VALUES ($id,$flag,$data,$payload,$vector)) SELECT * FROM input";
    assert_eq!(
        c.execute(sql, &params).unwrap().rows,
        vec![vec![
            id.clone(),
            Value::Boolean(true),
            data.clone(),
            payload,
            vector
        ]]
    );
    assert_eq!(c.execute("WITH input(id,flag,data,payload,embedding) AS (VALUES ($id,$flag,$data,$payload,$vector)) SELECT record::id(id),input.payload.n FROM input", &params).unwrap().rows, vec![vec![Value::String("a".into()),Value::Integer(7)]]);
    assert_eq!(q(&c, "WITH input AS (VALUES (docs:a,1),(docs:b,2)) SELECT column1,column2 FROM input ORDER BY column2").rows[0], vec![id,Value::Integer(1)]);
    assert_eq!(
        q(&c, "SELECT v.column1 FROM (VALUES (docs:a)) v").rows,
        vec![vec![Value::Record(fastdb::Record {
            table: "docs".into(),
            key: fastdb::Key::String("a".into())
        })]]
    );
    let direct = c
        .execute(
            "VALUES ($flag,$data)",
            &Parameters::from([
                ("$flag".into(), Value::Boolean(true)),
                ("$data".into(), data.clone()),
            ]),
        )
        .unwrap();
    assert_eq!(direct.columns, vec!["column1", "column2"]);
    assert_eq!(direct.rows, vec![vec![Value::Boolean(true), data]]);
    assert_eq!(q(&c,"WITH input AS (VALUES (X'ff',1),(X'00',2)) SELECT hex(column1),column2 FROM input ORDER BY column2").rows,vec![vec![Value::String("FF".into()),Value::Integer(1)],vec![Value::String("00".into()),Value::Integer(2)]]);
}

#[test]
fn values_ctes_and_leading_with_values_insert_atomically() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(
        &c,
        "DEFINE FIELD n ON docs TYPE integer REQUIRED CHECK(n>0)",
    );
    q(&c, "CREATE UNIQUE INDEX docs_n ON docs(n)");
    let inserted=q(&c,"WITH input(id,n) AS (VALUES (docs:a,1),(docs:b,2)) INSERT INTO docs(id,n) SELECT id,n FROM input RETURNING id,n");
    assert_eq!(inserted.affected, 2);
    assert!(matches!(inserted.rows[0][0], Value::Record(_)));
    q(&c, "BEGIN");
    q(
        &c,
        "WITH unused AS (SELECT 1) INSERT INTO docs(id,n) VALUES (docs:c,3) RETURNING id,n",
    );
    let error=c.execute("WITH input(id,n) AS (VALUES (docs:d,4),(docs:e,2)) INSERT INTO docs(id,n) SELECT id,n FROM input",&Parameters::new()).unwrap_err();
    assert_eq!(error.code(), "FDB_CONSTRAINT");
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(
        q(&c, "SELECT n FROM docs ORDER BY n").rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(3)]
        ]
    );
    let error = c
        .execute(
            "WITH unused AS (SELECT 1) INSERT INTO docs(id,n) VALUES (docs:d,4),(docs:e,0)",
            &Parameters::new(),
        )
        .unwrap_err();
    assert_eq!(error.code(), "FDB_VALIDATION");
    assert!(c
        .lookup_index("docs", "docs_n", &Value::Integer(4))
        .unwrap()
        .is_empty());
    q(&c, "ROLLBACK");
    q(&c, "CREATE TABLE copied(n INTEGER,data BLOB)");
    q(&c,"WITH input(id,n,data) AS (VALUES (docs:a,1,X'00ff')) INSERT INTO copied SELECT n,data FROM input");
    assert_eq!(
        q(&c, "SELECT n,hex(data) FROM copied").rows,
        vec![vec![Value::Integer(1), Value::String("00FF".into())]]
    );
    q(
        &c,
        "WITH unused AS (SELECT 1) INSERT INTO copied VALUES (2,X'01')",
    );
    assert_eq!(
        q(&c, "SELECT count(*) FROM copied").rows,
        vec![vec![Value::Integer(2)]]
    );
}

#[test]
fn values_rows_preserve_mixed_types_and_parameter_identity() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let record = Value::Record(fastdb::Record {
        table: "docs".into(),
        key: fastdb::Key::Integer(7),
    });
    let bytes = Value::Binary(
        b"FDB\x01{\"type\":\"Record\",\"value\":{\"table\":\"docs\",\"key\":{\"Integer\":7}}}"
            .to_vec(),
    );
    let array = Value::Array(vec![Value::Integer(1), Value::Null]);
    let params = Parameters::from([
        ("$record".into(), record.clone()),
        ("$bytes".into(), bytes.clone()),
        ("$array".into(), array.clone()),
        ("$flag".into(), Value::Boolean(true)),
    ]);
    let rows=c.execute("WITH v(n,x) AS (VALUES (1,$record),(2,$bytes),(3,NULL),(4,7),(5,'7'),(6,$array),(7,$flag)) SELECT x FROM v ORDER BY n", &params).unwrap().rows;
    assert_eq!(
        rows,
        vec![
            vec![record.clone()],
            vec![bytes.clone()],
            vec![Value::Null],
            vec![Value::Integer(7)],
            vec![Value::String("7".into())],
            vec![array],
            vec![Value::Boolean(true)]
        ]
    );
    assert_eq!(c.execute("WITH v(n,x) AS (VALUES (1,$record),(2,$bytes),(3,NULL)) SELECT x=$record FROM v ORDER BY n", &Parameters::from([("$record".into(),record.clone()),("$bytes".into(),bytes)])).unwrap().rows,vec![vec![Value::Integer(1)],vec![Value::Integer(0)],vec![Value::Null]]);
    assert_eq!(
        c.execute(
            "WITH v(x) AS (VALUES (?)) SELECT x FROM v",
            &Parameters::from([("?1".into(), record.clone())])
        )
        .unwrap()
        .rows,
        vec![vec![record.clone()]]
    );
    q(&c, "CREATE TABLE docs");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs {n:1}");
    for (i, sql) in [
        "WITH v(id,n) AS (VALUES ($record,2),($record)) INSERT INTO docs(id,n) SELECT id,n FROM v",
        "WITH v(id,n) AS (VALUES ($record,$missing)) INSERT INTO docs(id,n) SELECT id,n FROM v",
        "WITH v(id,n) AS (VALUES ($record,2)) INSERT INTO docs(id,n) SELECT id,n FROM v",
    ]
    .into_iter()
    .enumerate()
    {
        // The final case has an extra parameter; none may insert a prefix.
        let mut params = Parameters::from([("$record".into(), record.clone())]);
        if i == 2 {
            params.insert("$unused".into(), Value::Integer(9));
        }
        assert!(c.execute(sql, &params).is_err(), "{sql}");
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        assert_eq!(
            q(&c, "SELECT n FROM docs").rows,
            vec![vec![Value::Integer(1)]]
        );
    }
    q(&c, "ROLLBACK");
}

#[test]
fn duplicate_collection_cte_names_preserve_positions_types_and_chains() {
    let (_db, c) = setup();
    for definition in [
        "q AS (SELECT flag AS x,data AS X FROM docs ORDER BY n)",
        "q(x,X) AS (SELECT flag,data FROM docs ORDER BY n)",
        "q(x,X) AS MATERIALIZED (SELECT flag,data FROM docs ORDER BY n)",
    ] {
        let expected = q(&c, "SELECT flag AS x,data AS X FROM docs ORDER BY n");
        for tail in [
            "SELECT q.* FROM q",
            ", r AS (SELECT q.* FROM q) SELECT r.* FROM r",
        ] {
            let sql = format!("WITH {definition} {tail}");
            let actual = q(&c, &sql);
            assert_eq!(actual.columns, expected.columns, "{sql}");
            assert_eq!(actual.rows, expected.rows, "{sql}");
            assert_eq!(
                c.profile_select(&sql, &Parameters::new())
                    .unwrap()
                    .result
                    .rows,
                expected.rows
            );
        }
        assert_eq!(
            q(&c, &format!("WITH {definition} SELECT q.X FROM q")).rows,
            vec![vec![Value::Boolean(true)], vec![Value::Boolean(false)]]
        );
    }
    q(&c, "CREATE TABLE copied");
    q(&c, "DEFINE FIELD flag ON copied TYPE boolean");
    q(&c, "BEGIN");
    q(&c, "WITH q(x,x) AS (SELECT flag,data FROM docs) INSERT INTO copied(flag,data) SELECT q.* FROM q");
    assert_eq!(
        q(&c, "SELECT flag,data FROM copied ORDER BY flag").rows,
        q(&c, "SELECT flag,data FROM docs ORDER BY flag").rows
    );
    q(&c, "ROLLBACK");
    assert!(q(&c, "SELECT * FROM copied").rows.is_empty());
}

#[test]
fn duplicate_native_cte_stars_keep_positions_in_mixed_queries() {
    let (_db, c) = setup();
    q(&c, "CREATE TABLE baseline(n INTEGER)");
    q(&c, "INSERT INTO baseline VALUES(1),(2)");
    for definition in [
        "q AS (SELECT 10 AS x,20 AS X)",
        "q(x,X) AS (SELECT 10,20)",
        "q(x,X) AS MATERIALIZED (SELECT 10,20)",
        "seed AS (SELECT 10 AS x,20 AS X), q AS (SELECT seed.* FROM seed)",
    ] {
        for projection in ["v.*", "v.X AS first_value"] {
            let query = |source: &str| {
                format!(
                "WITH {definition} SELECT {projection} FROM {source} d JOIN q v ON 1 ORDER BY d.n"
            )
            };
            let expected = q(&c, &query("baseline"));
            let sql = query("docs");
            let actual = q(&c, &sql);
            assert_eq!(actual.columns, expected.columns, "{sql}");
            assert_eq!(actual.rows, expected.rows, "{sql}");
            assert_eq!(
                c.profile_select(&sql, &Parameters::new())
                    .unwrap()
                    .result
                    .rows,
                expected.rows
            );
        }
    }
    q(&c, "CREATE TABLE copied(a INTEGER,b INTEGER CHECK(b<>a))");
    q(&c, "BEGIN");
    q(
        &c,
        "WITH q(x,x) AS (SELECT 10,20) INSERT INTO copied SELECT v.* FROM docs d JOIN q v ON 1",
    );
    assert_eq!(
        q(&c, "SELECT * FROM copied").rows,
        vec![vec![Value::Integer(10), Value::Integer(20)]; 2]
    );
    q(&c, "ROLLBACK");
    assert!(q(&c, "SELECT * FROM copied").rows.is_empty());
}

#[test]
fn mixed_qualified_native_column_labels_match_source_metadata() {
    let (_db, c) = setup();
    q(&c, "CREATE TABLE baseline(n INTEGER)");
    q(&c, "INSERT INTO baseline VALUES(1),(2)");
    q(&c, "CREATE TABLE labels(Original INTEGER)");
    q(&c, "INSERT INTO labels VALUES(10)");
    for (prefix, source) in [
        ("", "labels v"),
        ("", "(SELECT Original FROM labels) v"),
        ("WITH q AS (SELECT Original FROM labels) ", "q v"),
        (
            "WITH q AS (SELECT Original,20 AS ORIGINAL FROM labels) ",
            "q v",
        ),
    ] {
        for expression in [
            "v.original",
            "v.ORIGINAL",
            "(v.original)",
            "v.original AS explicit",
        ] {
            let query = |outer: &str| {
                format!(
                    "{prefix}SELECT {expression} FROM {outer} d JOIN {source} ON 1 ORDER BY d.n"
                )
            };
            let expected = q(&c, &query("baseline"));
            let sql = query("docs");
            let actual = q(&c, &sql);
            assert_eq!(actual.columns, expected.columns, "{sql}");
            assert_eq!(actual.rows, expected.rows, "{sql}");
            assert_eq!(
                c.profile_select(&sql, &Parameters::new())
                    .unwrap()
                    .result
                    .columns,
                expected.columns
            );
        }
    }
}
