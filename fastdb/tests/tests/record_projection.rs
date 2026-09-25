use fastdb::{Database, Parameters, ResultLimits, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
fn setup(c: &fastdb::Connection) {
    q(
        c,
        "INSERT INTO users {id:users:u1,name:'Alice',manager:users:u2}",
    );
    q(c, "INSERT INTO users {id:users:u2,name:'Bob'}");
    q(c,"INSERT INTO posts {id:posts:p1,title:'Hello',author:users:u1,profile:{city:'Bangkok'},missing_author:users:none,tags:[1,2]}");
}
#[test]
fn brace_fields_preserve_typed_positions_and_one_hop_objects() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    setup(&c);
    let result=q(&c,"SELECT posts:p1 {title,author.*,profile.*,profile.city AS city,missing_author.*,unknown,author AS author}");
    assert_eq!(
        result.columns,
        vec![
            "title",
            "author",
            "profile",
            "city",
            "missing_author",
            "unknown",
            "author"
        ]
    );
    let author = q(&c, "SELECT users:u1").rows[0][0].clone();
    let fields = q(
        &c,
        "SELECT p.title,p.profile,p.profile.city,p.author FROM posts p WHERE p.id=posts:p1",
    )
    .rows
    .remove(0);
    assert_eq!(
        result.rows,
        vec![vec![
            fields[0].clone(),
            author,
            fields[1].clone(),
            fields[2].clone(),
            Value::Null,
            Value::Null,
            fields[3].clone()
        ]]
    );
    // Expansion does not follow the manager link a second time.
    let Value::Object(author) = &result.rows[0][1] else {
        panic!()
    };
    assert!(matches!(author.get("manager"), Some(Value::Record(_))));
    let all = q(&c, "SELECT posts:p1 {*}");
    assert_eq!(all.columns, vec!["document"]);
    assert_eq!(all.rows, q(&c, "SELECT posts:p1").rows);
    let missing = q(&c, "SELECT posts:no_such_record {title,author.* AS writer}");
    assert_eq!(missing.columns, vec!["title", "writer"]);
    assert!(missing.rows.is_empty());
    assert_eq!(
        q(&c, "SELECT posts:p1 { title, }").rows,
        vec![vec![Value::String("Hello".into())]]
    );
    assert_eq!(
        q(&c, "SELECT posts:p1 { title AS same,author AS same }").columns,
        vec!["same", "same"]
    );
}
#[test]
fn brace_fetches_share_snapshots_limits_and_indexed_lookup() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    setup(&c);
    let sql = "SELECT posts:p1 {title,author.* AS first,author.* AS again}";
    let profile = c.profile_select(sql, &Parameters::new()).unwrap();
    assert_eq!(profile.result.rows[0][1], profile.result.rows[0][2]);
    assert_eq!(profile.metrics.fetch_batches, 1);
    let single = c
        .profile_select("SELECT posts:p1 {author.*}", &Parameters::new())
        .unwrap();
    assert_eq!(
        profile.metrics.fetch_rows_read,
        single.metrics.fetch_rows_read
    );
    assert_eq!(profile.metrics.fullscan_steps, 0);
    assert!(profile.metrics.rows_read <= 2);
    for limits in [
        ResultLimits {
            max_rows: 0,
            max_payload_bytes: usize::MAX,
        },
        ResultLimits {
            max_rows: 1,
            max_payload_bytes: 30,
        },
    ] {
        assert_eq!(
            c.select_with_limits(sql, &Parameters::new(), limits)
                .unwrap_err()
                .code(),
            "FDB_LIMIT"
        );
    }
    // Exact final payload limit includes each fetched occurrence and column label.
    let single = "SELECT posts:p1 {author.* AS a}";
    let Value::Object(author) = &profile.result.rows[0][1] else {
        panic!()
    };
    // id (users/u1), name Alice, manager(users/u2) plus object keys and label a.
    let bytes = 1
        + "id".len()
        + "users".len()
        + "u1".len()
        + "name".len()
        + "Alice".len()
        + "manager".len()
        + "users".len()
        + "u2".len();
    assert!(author.contains_key("id"));
    assert!(c
        .select_with_limits(
            single,
            &Parameters::new(),
            ResultLimits {
                max_rows: 1,
                max_payload_bytes: bytes
            }
        )
        .is_ok());
    assert_eq!(
        c.select_with_limits(
            single,
            &Parameters::new(),
            ResultLimits {
                max_rows: 1,
                max_payload_bytes: bytes - 1
            }
        )
        .unwrap_err()
        .code(),
        "FDB_LIMIT"
    );
    assert_eq!(
        c.select_with_limits(
            "SELECT posts:p1 {missing_author.* AS a}",
            &Parameters::new(),
            ResultLimits {
                max_rows: 1,
                max_payload_bytes: 2
            }
        )
        .unwrap()
        .rows,
        vec![vec![Value::Null]]
    );
    q(&c, "BEGIN");
    q(&c, "UPDATE users:u1 {name:'Pending'}");
    let pending = q(&c, "SELECT posts:p1 {author.*}");
    assert!(format!("{:?}", pending.rows).contains("Pending"));
    assert!(c
        .execute("SELECT posts:p1 {title.*}", &Parameters::new())
        .is_err());
    q(&c, "ROLLBACK");
    assert!(format!("{:?}", q(&c, "SELECT posts:p1 {author.*}").rows).contains("Alice"));
}
#[test]
fn brace_quoted_paths_native_links_and_invalid_forms() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("braces.db");
    {
        let db = Database::open(path.to_str().unwrap()).unwrap();
        let c = db.connect().unwrap();
        q(&c, "CREATE TABLE native(id TEXT PRIMARY KEY,name TEXT)");
        q(&c, "INSERT INTO native VALUES('n','Native')");
        q(
            &c,
            r#"INSERT INTO `odd table` {id:`odd table`:1, "odd.field":{link:native:n}, count:7}"#,
        );
    }
    let db = Database::open(path.to_str().unwrap()).unwrap();
    let c = db.connect().unwrap();
    let result = q(
        &c,
        "SELECT `odd table`:1 { `odd.field`.link.* AS related, count AS `odd label` }",
    );
    assert_eq!(result.columns, vec!["related", "odd label"]);
    let Value::Object(native) = &result.rows[0][0] else {
        panic!()
    };
    assert_eq!(native.get("name"), Some(&Value::String("Native".into())));
    assert_eq!(result.rows[0][1], Value::Integer(7));
    for sql in [
        "SELECT `odd table`:1 {}",
        "SELECT `odd table`:1 {count.*.name}",
        "SELECT `odd table`:1 {count {name}}",
        "SELECT `odd table`:1 {count+1}",
        "SELECT `odd table`:1 {count} LIMIT 1",
        "SELECT `odd table`:1 {count,count.*}",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert!(c
        .execute(
            "SELECT `odd table`:1 {count}",
            &Parameters::from([("$unused".into(), Value::Integer(1))])
        )
        .is_err());
    // Ordinary record expressions and SQL meanings remain intact.
    assert!(matches!(
        q(&c, "SELECT `odd table`:1 AS ref").rows[0][0],
        Value::Record(_)
    ));
}
