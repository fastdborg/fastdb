use fastdb::{Database, Document, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
fn setup() -> (Database, fastdb::Connection) {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE posts");
    q(
        &c,
        "INSERT INTO posts {id:posts:p1,tags:[true,posts:p2],nested:{x:null}}",
    );
    (db, c)
}
#[test]
fn nested_helpers_preserve_types_in_select_set_and_values() {
    let (_db, c) = setup();
    let rows=q(&c,"SELECT doc::get(doc::row(p),'$.tags') AS tags, doc::has(p.nested,'$.x') AS present, record::id(p.id) AS key FROM posts p").rows;
    assert!(
        matches!(&rows[0][0], Value::Array(a) if a[0]==Value::Boolean(true) && matches!(a[1],Value::Record(_)))
    );
    assert_eq!(
        rows[0][1..],
        [Value::Boolean(true), Value::String("p1".into())]
    );
    q(&c, "UPDATE posts SET tags=array::append(tags,posts:p3)");
    assert_eq!(
        q(
            &c,
            "SELECT record::id(doc::get(tags,'$[2]')) AS key FROM posts"
        )
        .rows,
        vec![vec![Value::String("p3".into())]]
    );
    q(
        &c,
        "INSERT INTO posts (id,tags) VALUES (posts:p2,array::new(posts:p1,5))",
    );
    assert_eq!(
        q(
            &c,
            "SELECT record::id(doc::get(tags,'$[0]')) AS key FROM posts WHERE id=posts:p2"
        )
        .rows,
        vec![vec![Value::String("p1".into())]]
    );
}
#[test]
fn typed_parameters_and_binary_payloads_are_lossless() {
    let (_db, c) = setup();
    let object = Value::Object(Document::from([("yes".into(), Value::Boolean(true))]));
    let params = Parameters::from([
        ("$obj".into(), object.clone()),
        ("$n".into(), Value::Integer(3)),
    ]);
    let rows = c
        .execute(
            "SELECT array::append(tags,$obj) AS a, $n+1 AS n FROM posts WHERE $n=3",
            &params,
        )
        .unwrap()
        .rows;
    assert!(matches!(&rows[0][0],Value::Array(a) if a[2]==object));
    assert_eq!(rows[0][1], Value::Integer(4));
    let bytes = b"FDB\x01{\"type\":\"String\",\"value\":\"encoded-looking\"}".to_vec();
    let params = Parameters::from([
        ("?1".into(), Value::Binary(bytes.clone())),
        ("?2".into(), Value::Integer(8)),
    ]);
    let rows = c
        .execute("SELECT array::new(?1) AS a, ?2 AS n", &params)
        .unwrap()
        .rows;
    assert_eq!(
        rows,
        vec![vec![
            Value::Array(vec![Value::Binary(bytes)]),
            Value::Integer(8)
        ]]
    );
}
#[test]
fn helper_predicates_sorting_and_outer_join_nulls() {
    let (_db, c) = setup();
    assert_eq!(q(&c,"SELECT record::id(id) AS key FROM posts WHERE doc::has(nested,'$.x') AND record::id(id)='p1' ORDER BY key").rows.len(),1);
    let rows = q(
        &c,
        "SELECT doc::row(b) AS missing FROM posts a LEFT JOIN posts b ON b.id=posts:absent",
    )
    .rows;
    assert_eq!(rows, vec![vec![Value::Null]]);
    assert!(c
        .execute("SELECT doc::row(unknown) FROM posts", &Parameters::new())
        .is_err());
    assert!(c
        .execute("SELECT array::append(1,2) FROM posts", &Parameters::new())
        .is_err());
}
#[test]
fn parentheses_and_lazy_null_helpers_preserve_arrays() {
    let (_db, c) = setup();
    let rows=q(&c,"SELECT (tags) AS a, coalesce(missing,tags) AS b, ifnull(tags,array::append(1,2)) AS c FROM posts").rows;
    assert_eq!(rows[0][0], rows[0][1]);
    assert_eq!(rows[0][1], rows[0][2]);
    assert!(matches!(&rows[0][0], Value::Array(_)));
    q(
        &c,
        "UPDATE posts SET tags=array::append(coalesce(missing,(tags)),5)",
    );
    assert_eq!(
        q(&c, "SELECT doc::get(tags,'$[2]') AS n FROM posts").rows,
        vec![vec![Value::Integer(5)]]
    );
    assert_eq!(
        q(&c, "SELECT coalesce(NULL,NULL) AS n FROM posts").rows,
        vec![vec![Value::Null]]
    );
}
#[test]
fn scalar_aliases_and_helper_ordering_use_logical_results() {
    let (_db, c) = setup();
    let result = q(
        &c,
        "SELECT 1, 'hello', record::id(id), doc::has(nested,'$.x') AS \"a.b\" FROM posts",
    );
    assert_eq!(result.rows[0][0], Value::Integer(1));
    assert_eq!(result.columns[3], "a.b");
    assert!(result.columns.iter().all(|n| !n.contains("__fastdb_")));
    q(&c, "UPDATE posts:p1 {ref:type::record('posts',10)}");
    q(
        &c,
        "INSERT INTO posts {id:posts:p2,ref:type::record('posts',2)}",
    );
    assert_eq!(
        q(
            &c,
            "SELECT record::id(id) AS k FROM posts p ORDER BY doc::get(doc::row(p),'$.ref')"
        )
        .rows,
        vec![
            vec![Value::String("p2".into())],
            vec![Value::String("p1".into())]
        ]
    );
    assert!(c
        .execute(
            "SELECT array::new(DISTINCT id) FROM posts",
            &Parameters::new()
        )
        .is_err());
}

#[test]
fn native_binary_arguments_and_casts_use_payload_bytes() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let q = |sql: &str| c.execute(sql, &Parameters::new()).expect(sql);
    q("CREATE TABLE docs");
    q("CREATE TABLE baseline(data BLOB)");
    for value in ["X'313233'", "X'414243'", "X''", "NULL"] {
        q(&format!("INSERT INTO docs (data) VALUES ({value})"));
        q(&format!("INSERT INTO baseline VALUES ({value})"));
    }
    for projection in [
        "length(data)",
        "hex(data)",
        "typeof(data)",
        "substr(data,1,2)",
        "CAST(data AS TEXT)",
        "CAST(data AS INTEGER)",
        "length(coalesce(data,X''))",
        "hex(substr(data,1,2))",
    ] {
        let sql = |table: &str| format!("SELECT {projection} AS value FROM {table} ORDER BY value");
        assert_eq!(
            q(&sql("docs")).rows,
            q(&sql("baseline")).rows,
            "{projection}"
        );
    }
    let p = Parameters::from([("$data".into(), Value::Binary(vec![1, 2]))]);
    assert_eq!(
        c.execute("SELECT length($data) AS n FROM docs LIMIT 1", &p)
            .unwrap()
            .rows,
        vec![vec![Value::Integer(2)]]
    );
    q("CREATE INDEX docs_data ON docs(data)");
    assert_eq!(
        q("SELECT length(data) FROM docs WHERE data=X'313233'").rows,
        vec![vec![Value::Integer(3)]]
    );
    q("BEGIN");
    assert_eq!(q("UPDATE docs SET data=substr(data,1,2) WHERE data=X'313233' RETURNING hex(data) AS bytes").rows,vec![vec![Value::String("3132".into())]]);
    assert_eq!(
        c.lookup_index("docs", "docs_data", &Value::Binary(vec![0x31, 0x32]))
            .unwrap()
            .len(),
        1
    );
    q("ROLLBACK");
    assert_eq!(
        c.lookup_index("docs", "docs_data", &Value::Binary(vec![0x31, 0x32, 0x33]))
            .unwrap()
            .len(),
        1
    );
}
