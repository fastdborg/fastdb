use fastdb::{Database, Key, Parameters, Record, Value};
fn query(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
fn setup() -> (Database, fastdb::Connection) {
    let db = Database::open(":memory:").expect("open");
    let c = db.connect().expect("connect");
    query(&c, "CREATE TABLE users");
    query(&c, "CREATE INDEX users_city ON users (profile.city)");
    query(&c,"INSERT INTO users {id: users:2, name: 'Bob', active: true, profile: {city: 'Bangkok'}, tags: ['rust'], nickname: null}");
    query(
        &c,
        "INSERT INTO users {id: users:10, name: 'Alice', active: false, profile: {city: 'Berlin'}}",
    );
    query(
        &c,
        "INSERT INTO users {id: users:-1, name: 'Carol', profile: {city: 'Bangkok'}}",
    );
    (db, c)
}
#[test]
fn projections_filters_sort_and_pagination() {
    let (_db, c) = setup();
    let result=query(&c,"SELECT u.id, u.name, u.active, u.profile.city AS city, u.tags, u.unknown FROM users AS u WHERE u.profile.city = 'Bangkok' ORDER BY u.name LIMIT 1 OFFSET 0");
    assert_eq!(
        result.columns,
        vec!["id", "name", "active", "city", "tags", "unknown"]
    );
    assert_eq!(
        result.rows[0],
        vec![
            Value::Record(Record {
                table: "users".into(),
                key: Key::Integer(2)
            }),
            Value::String("Bob".into()),
            Value::Boolean(true),
            Value::String("Bangkok".into()),
            Value::Array(vec![Value::String("rust".into())]),
            Value::Null
        ]
    );
    let result = query(
        &c,
        "SELECT name, length(name) AS size FROM users WHERE active = true ORDER BY name",
    );
    assert_eq!(
        result.rows,
        vec![vec![Value::String("Bob".into()), Value::Integer(3)]]
    );
    assert_eq!(
        query(&c, "SELECT name FROM users WHERE id = users:10").rows,
        vec![vec![Value::String("Alice".into())]]
    );
    let values = query(&c, "SELECT id FROM users ORDER BY id").rows;
    assert_eq!(
        values.into_iter().map(|r| r[0].clone()).collect::<Vec<_>>(),
        [-1, 2, 10].map(|key| Value::Record(Record {
            table: "users".into(),
            key: Key::Integer(key)
        }))
    );
}
#[test]
fn full_documents_preserve_absence_and_literal_keys() {
    let (_db, c) = setup();
    query(&c,"INSERT INTO users {id: users:literal, \"profile.city\": 'literal', profile: {city: 'nested'}}");
    assert_eq!(query(&c,"SELECT u.\"profile.city\" AS literal, u.profile.city AS nested FROM users u WHERE id = users:literal").rows,vec![vec![Value::String("literal".into()),Value::String("nested".into())]]);
    let row = query(&c, "SELECT u.* FROM users u WHERE id = users:2")
        .exactly_one()
        .expect("one");
    let Value::Object(doc) = &row[0] else {
        panic!("document");
    };
    assert_eq!(doc["nickname"], Value::Null);
    assert!(!doc.contains_key("unknown"));
    assert!(c
        .execute("SELECT u.profile = u.tags FROM users u", &Parameters::new())
        .is_err());
    assert!(c
        .execute("SELECT name, name FROM users", &Parameters::new())
        .is_err());
}
#[test]
fn joins_and_typed_parameter_filters() {
    let (_db, c) = setup();
    query(&c, "CREATE TABLE posts");
    query(
        &c,
        "INSERT INTO posts {id: posts:p1, title: 'Hello', author: users:2}",
    );
    query(
        &c,
        "INSERT INTO posts {id: posts:p2, title: 'Orphan', author: users:404}",
    );
    let result = query(
        &c,
        "SELECT p.title, u.name FROM posts p LEFT JOIN users u ON p.author = u.id ORDER BY p.title",
    );
    assert_eq!(
        result.rows,
        vec![
            vec![Value::String("Hello".into()), Value::String("Bob".into())],
            vec![Value::String("Orphan".into()), Value::Null]
        ]
    );
    query(
        &c,
        "CREATE TABLE cities(name TEXT PRIMARY KEY, country TEXT)",
    );
    query(&c, "INSERT INTO cities VALUES ('Bangkok', 'Thailand')");
    assert_eq!(query(&c,"SELECT u.name, c.country FROM users u JOIN cities c ON u.profile.city = c.name ORDER BY u.name").rows.len(),2);
    let params = Parameters::from([
        ("$city".into(), Value::String("Bangkok".into())),
        (
            "$id".into(),
            Value::Record(Record {
                table: "users".into(),
                key: Key::Integer(2),
            }),
        ),
    ]);
    assert_eq!(
        c.execute(
            "SELECT name FROM users WHERE users.profile.city = $city AND id = $id",
            &params
        )
        .expect("bound filters")
        .rows
        .len(),
        1
    );
    assert_eq!(
        query(&c, "SELECT name FROM users WHERE id = 'users:2'")
            .rows
            .len(),
        0
    );
}
#[test]
fn planner_uses_managed_index_and_sees_transaction_changes() {
    let (_db, c) = setup();
    let plan = query(
        &c,
        "EXPLAIN QUERY PLAN SELECT u.name FROM users u WHERE u.profile.city = 'Bangkok'",
    );
    let details = format!("{:?}", plan.rows);
    assert!(details.contains("users_city"), "{details}");
    assert!(details.to_ascii_uppercase().contains("SEARCH"), "{details}");
    query(&c, "BEGIN");
    query(&c, "UPDATE users:2 {profile: {city: 'Paris'}}");
    assert_eq!(
        query(&c, "SELECT * FROM users u WHERE u.profile.city = 'Bangkok'")
            .rows
            .len(),
        1
    );
    query(&c, "ROLLBACK");
    assert_eq!(
        query(&c, "SELECT * FROM users u WHERE u.profile.city = 'Bangkok'")
            .rows
            .len(),
        2
    );
    assert!(c
        .execute("SELECT u.doc FROM __fastdb_catalog u", &Parameters::new())
        .is_err());
    assert!(c
        .execute(
            "SELECT __fastdb_value(u.doc, '[]') FROM users u",
            &Parameters::new()
        )
        .is_err());
}

#[test]
fn positional_ordering_and_indexed_null_semantics() {
    let (_db, c) = setup();
    assert_eq!(
        query(&c, "SELECT id FROM users ORDER BY 1").rows,
        query(&c, "SELECT id FROM users ORDER BY id").rows
    );
    query(
        &c,
        "INSERT INTO users {id: users:nullcity, profile: {city: null}}",
    );
    assert_eq!(
        query(&c, "SELECT id FROM users u WHERE u.profile.city IS NULL")
            .rows
            .len(),
        1
    );
    assert_eq!(
        query(&c, "SELECT id FROM users u WHERE u.profile.city = NULL")
            .rows
            .len(),
        0
    );
    let plan = query(
        &c,
        "EXPLAIN QUERY PLAN SELECT name FROM users WHERE id = users:2",
    );
    assert!(format!("{:?}", plan.rows).contains("SEARCH"));
    assert!(c
        .execute("SELECT DISTINCT profile FROM users", &Parameters::new())
        .is_err());
}

#[test]
fn deeply_nested_qualified_paths_preserve_values_and_index_predicates() {
    let (_db, c) = setup();
    query(&c, "INSERT INTO users {id:users:deep, profile:{address:{city:'Paris',details:{active:true,ref:users:2}},\"address.city\":{label:'literal'}}}");
    let sql = "SELECT u.profile.address.city AS city, u.profile.address.details.active AS active, u.profile.address.details.ref AS reference, u.profile.\"address.city\".label AS literal, u.profile.missing.value AS missing FROM users u WHERE u.profile.address.city='Paris'";
    let expected = vec![vec![
        Value::String("Paris".into()),
        Value::Boolean(true),
        Value::Record(Record {
            table: "users".into(),
            key: Key::Integer(2),
        }),
        Value::String("literal".into()),
        Value::Null,
    ]];
    assert_eq!(query(&c, sql).rows, expected);
    query(
        &c,
        "CREATE INDEX users_deep_city ON users (profile.address.city)",
    );
    assert_eq!(query(&c, sql).rows, expected);
    assert_eq!(query(&c,"SELECT u.profile.address.city FROM users u WHERE u.profile.address.city='Paris' ORDER BY u.profile.address.city").columns,vec!["city"]);
    assert_eq!(query(&c,"SELECT a.profile.address.city AS city FROM users a JOIN users b ON a.profile.address.details.ref=b.id WHERE b.id=users:2").rows,vec![vec![Value::String("Paris".into())]]);
    assert!(c
        .execute(
            "SELECT __fastdb_path(u,profile,address,city) FROM users u",
            &Parameters::new()
        )
        .is_err());
    assert!(c
        .execute(
            "SELECT missing.profile.address.city FROM users u",
            &Parameters::new()
        )
        .is_err());
    assert_eq!(
        query(
            &c,
            "SELECT 'u.profile.address.city' AS literal FROM users WHERE id=users:deep"
        )
        .rows,
        vec![vec![Value::String("u.profile.address.city".into())]]
    );
}

#[test]
fn deep_paths_handle_quoting_comments_and_depth_limits() {
    let (_db, c) = setup();
    query(
        &c,
        r#"INSERT INTO users {id:users:quoted, profile:{"a.b":{"quo""te":7}}}"#,
    );
    assert_eq!(query(&c,r#"SELECT [u].profile /* gap */ .[a.b]."quo""te" AS value FROM users u WHERE id=users:quoted"#).rows,vec![vec![Value::Integer(7)]]);
    let path = std::iter::repeat_n("missing", 64)
        .collect::<Vec<_>>()
        .join(".");
    assert_eq!(
        query(
            &c,
            &format!("SELECT u.{path} AS absent FROM users u WHERE id=users:2")
        )
        .rows,
        vec![vec![Value::Null]]
    );
    let error = c
        .execute(
            &format!("SELECT u.{path}.extra FROM users u"),
            &Parameters::new(),
        )
        .unwrap_err();
    assert_eq!(error.code(), "FDB_LIMIT");
    query(&c, "CREATE TABLE ordinary(value INTEGER)");
    query(&c, "INSERT INTO ordinary VALUES (9)");
    assert_eq!(
        query(&c, "SELECT main.ordinary.value FROM ordinary").rows,
        vec![vec![Value::Integer(9)]]
    );
}

#[test]
fn expression_column_names_preserve_literals_and_render_public_paths() {
    let (_db, c) = setup();
    query(
        &c,
        "INSERT INTO users {id:users:labels,profile:{address:{city:'Paris'}}}",
    );
    let result=query(&c,"SELECT upper(users.profile.address.city), '__fastdb_fetch', coalesce(NULL,'__fastdb_h_doc_get'), record::id(id), lower('__fastdb_path(x,y,z,w)') FROM users WHERE id=users:labels");
    assert_eq!(
        result.columns[0],
        r#"upper ("users"."profile"."address"."city")"#
    );
    assert_eq!(result.columns[1], "'__fastdb_fetch'");
    assert!(result.columns[2].contains("'__fastdb_h_doc_get'"));
    assert!(result.columns[3].starts_with("record::id"));
    assert!(result.columns[4].contains("'__fastdb_path(x,y,z,w)'"));
    assert_eq!(result.rows[0][0], Value::String("PARIS".into()));
    let returned=query(&c,"UPDATE users SET profile.address.city='Rome' WHERE id=users:labels RETURNING upper(users.profile.address.city)");
    assert_eq!(returned.columns[0], result.columns[0]);
    assert_eq!(returned.rows[0][0], Value::String("ROME".into()));
}

#[test]
fn distinct_uses_scalar_equality_and_paginates_after_deduplication() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    for (i, value) in [
        Value::Integer(1),
        Value::Number(1.0),
        Value::Boolean(true),
        Value::Integer(2),
        Value::Null,
        Value::Null,
        Value::String("1".into()),
    ]
    .into_iter()
    .enumerate()
    {
        c.execute(
            "INSERT INTO docs (id,v) VALUES (type::record('docs',$id),$v)",
            &Parameters::from([
                ("$id".into(), Value::Integer(i as i64)),
                ("$v".into(), value),
            ]),
        )
        .unwrap();
    }
    let all = query(&c, "SELECT DISTINCT v FROM docs ORDER BY v");
    query(&c, "CREATE TABLE baseline(v)");
    query(
        &c,
        "INSERT INTO baseline VALUES (1),(1.0),(1),(2),(NULL),(NULL),('1')",
    );
    assert_eq!(
        query(
            &c,
            "SELECT DISTINCT v+0 AS n FROM docs ORDER BY n DESC LIMIT 2 OFFSET 1"
        )
        .rows,
        query(
            &c,
            "SELECT DISTINCT v+0 AS n FROM baseline ORDER BY n DESC LIMIT 2 OFFSET 1"
        )
        .rows
    );
    assert_eq!(
        query(
            &c,
            "SELECT DISTINCT count(*) AS n FROM docs GROUP BY v ORDER BY n"
        )
        .rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(3)]
        ]
    );
    assert_eq!(all.rows.len(), 4);
    assert_eq!(all.rows[0], vec![Value::Null]);
    assert_eq!(all.rows[2], vec![Value::Integer(2)]);
    assert_eq!(all.rows[3], vec![Value::String("1".into())]);
    let page = c
        .execute(
            "SELECT DISTINCT v FROM docs ORDER BY v LIMIT $n OFFSET $skip",
            &Parameters::from([
                ("$n".into(), Value::Integer(1)),
                ("$skip".into(), Value::Integer(2)),
            ]),
        )
        .unwrap();
    assert_eq!(page.rows, vec![vec![Value::Integer(2)]]);
    assert_eq!(
        query(&c, "SELECT DISTINCT length('x') AS n FROM docs ORDER BY n").rows,
        vec![vec![Value::Integer(1)]]
    );
    assert_eq!(
        query(&c, "SELECT DISTINCT count(*) AS n FROM docs").rows,
        vec![vec![Value::Integer(7)]]
    );
    assert_eq!(query(&c,"SELECT DISTINCT row_number() OVER (ORDER BY id) AS n FROM docs ORDER BY n DESC LIMIT 2").rows,vec![vec![Value::Integer(7)],vec![Value::Integer(6)]]);
}

#[test]
fn distinct_preserves_record_identity_collation_and_binary_values() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    for value in [
        Value::Record(Record {
            table: "Docs".into(),
            key: Key::Integer(1),
        }),
        Value::Record(Record {
            table: "docs".into(),
            key: Key::Integer(1),
        }),
        Value::Record(Record {
            table: "docs".into(),
            key: Key::String("1".into()),
        }),
        Value::Binary(vec![1]),
        Value::Binary(vec![1]),
        Value::String("A".into()),
        Value::String("a".into()),
    ] {
        c.execute(
            "INSERT INTO docs (v) VALUES ($v)",
            &Parameters::from([("$v".into(), value)]),
        )
        .unwrap();
    }
    let values = query(&c, "SELECT DISTINCT v FROM docs");
    assert_eq!(values.rows.len(), 5);
    assert_eq!(
        values
            .rows
            .iter()
            .filter(|r| matches!(&r[0], Value::Record(_)))
            .count(),
        2
    );
    assert_eq!(
        query(
            &c,
            "SELECT DISTINCT v COLLATE NOCASE AS text FROM docs WHERE typeof(v)='text'"
        )
        .rows
        .len(),
        1
    );
    assert_eq!(
        query(&c, "SELECT DISTINCT v FROM docs ORDER BY (1) LIMIT 2")
            .rows
            .len(),
        2
    );
    for position in ["0", "99", "-1", "(99)"] {
        assert!(c
            .execute(
                &format!("SELECT DISTINCT v FROM docs ORDER BY {position}"),
                &Parameters::new()
            )
            .is_err());
    }
    query(&c, "CREATE TABLE copied");
    assert_eq!(
        query(&c, "INSERT INTO copied (v) SELECT DISTINCT v FROM docs").affected,
        5
    );
    query(&c, "CREATE TABLE arrays");
    query(&c, "INSERT INTO arrays {v:[1]}");
    assert!(c
        .execute("SELECT DISTINCT v FROM arrays", &Parameters::new())
        .is_err());
}

#[test]
fn distinct_orders_the_projected_volatile_value() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    for _ in 0..64 {
        query(&c, "INSERT INTO docs {}");
    }
    for order in ["n", "1", "random()"] {
        let result = query(
            &c,
            &format!("SELECT DISTINCT random() AS n FROM docs ORDER BY {order}"),
        );
        let numbers = result
            .rows
            .iter()
            .map(|r| match r[0] {
                Value::Integer(i) => i,
                _ => panic!("integer random value"),
            })
            .collect::<Vec<_>>();
        assert!(
            numbers.windows(2).all(|pair| pair[0] < pair[1]),
            "{order}: {numbers:?}"
        );
    }
}

#[test]
fn collated_order_aliases_match_relational_precedence_and_distinct() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "CREATE TABLE baseline(v,n)");
    for (v, n) in [("a", "z"), ("B", "y"), ("c", "x"), ("a", "w")] {
        query(&c, &format!("INSERT INTO docs {{v:'{v}',n:'{n}'}}"));
        query(&c, &format!("INSERT INTO baseline VALUES ('{v}','{n}')"));
    }
    for distinct in ["", "DISTINCT "] {
        for order in [
            "n COLLATE NOCASE",
            "(n) COLLATE NOCASE DESC",
            "(1) COLLATE NOCASE",
            "v COLLATE NOCASE",
        ] {
            let collection = query(
                &c,
                &format!("SELECT {distinct}v AS n FROM docs ORDER BY {order}"),
            );
            let ordinary = query(
                &c,
                &format!("SELECT {distinct}v AS n FROM baseline ORDER BY {order}"),
            );
            assert_eq!(collection.rows, ordinary.rows, "{distinct}{order}");
        }
    }
    query(&c, "CREATE TABLE rel(x)");
    query(&c, "INSERT INTO rel VALUES ('A'),('a')");
    assert_eq!(
        query(
            &c,
            "SELECT DISTINCT d.v,r.x COLLATE NOCASE AS x FROM docs d CROSS JOIN rel r ORDER BY d.v"
        )
        .rows
        .len(),
        3
    );
}

#[test]
fn nested_order_aliases_match_relational_results_and_reuse_distinct_outputs() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "CREATE TABLE baseline(v,n)");
    for (v, n) in [(1, 9), (2, 8), (3, 7), (1, 9)] {
        query(&c, &format!("INSERT INTO docs {{v:{v},n:{n}}}"));
        query(&c, &format!("INSERT INTO baseline VALUES ({v},{n})"));
    }
    for distinct in ["", "DISTINCT "] {
        for order in ["n+0", "abs(n) DESC", "coalesce(n,0)", "n+docs.n DESC"] {
            let collection = query(
                &c,
                &format!("SELECT {distinct}v AS n FROM docs ORDER BY {order}"),
            );
            let ordinary = query(
                &c,
                &format!(
                    "SELECT {distinct}v AS n FROM baseline ORDER BY {}",
                    order.replace("docs.", "baseline.")
                ),
            );
            assert_eq!(collection.rows, ordinary.rows, "{distinct}{order}");
        }
    }
    for _ in 0..60 {
        query(&c, "INSERT INTO docs {v:1}");
    }
    let result = query(&c, "SELECT DISTINCT random() AS n FROM docs ORDER BY n+0");
    let values = result
        .rows
        .iter()
        .map(|r| match r[0] {
            Value::Integer(i) => i,
            _ => panic!("integer random"),
        })
        .collect::<Vec<_>>();
    assert!(values.windows(2).all(|p| p[0] < p[1]));
    assert!(c
        .execute(
            "SELECT DISTINCT sum(v) AS n FROM docs ORDER BY sum(n)",
            &Parameters::new()
        )
        .is_err());
}

#[test]
fn order_alias_helpers_and_mixed_sources_retain_types() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "INSERT INTO docs {id:docs:2,v:1}");
    query(&c, "INSERT INTO docs {id:docs:10,v:2}");
    query(&c, "CREATE TABLE rel(v,bias)");
    query(&c, "INSERT INTO rel VALUES (1,5),(2,1)");
    for distinct in ["", "DISTINCT "] {
        let records = query(
            &c,
            &format!("SELECT {distinct}id AS ref FROM docs ORDER BY record::id(ref) DESC"),
        );
        assert!(matches!(
            &records.rows[0][0],
            Value::Record(Record {
                key: Key::Integer(10),
                ..
            })
        ));
        assert_eq!(query(&c,&format!("SELECT {distinct}d.v AS score FROM docs d JOIN rel r ON d.v=r.v ORDER BY score+r.bias")).rows,vec![vec![Value::Integer(2)],vec![Value::Integer(1)]]);
        assert_eq!(query(&c,&format!("SELECT {distinct}v AS bucket,sum(v) AS total FROM docs GROUP BY v ORDER BY abs(total) DESC")).rows,vec![vec![Value::Integer(2),Value::Integer(2)],vec![Value::Integer(1),Value::Integer(1)]]);
    }
}

#[test]
fn ordering_ordinal_recognition_matches_pinned_constant_expression_rules() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "CREATE TABLE baseline(v INTEGER)");
    for value in [2, 1, 3] {
        query(&c, &format!("INSERT INTO docs {{v:{value}}}"));
        query(&c, &format!("INSERT INTO baseline VALUES ({value})"));
    }
    for distinct in ["", "DISTINCT "] {
        for order in [
            "1",
            "+1",
            "(1)",
            "((+1))",
            "1 COLLATE BINARY",
            "+(1)",
            "+(+1)",
            "-(-1)",
            "-(+1)",
            "1.0",
            "1+0",
        ] {
            let sql =
                |table: &str| format!("SELECT {distinct}v FROM {table} ORDER BY {order},v DESC");
            assert_eq!(
                query(&c, &sql("docs")).rows,
                query(&c, &sql("baseline")).rows,
                "{distinct}{order}"
            );
        }
        for order in [
            "0",
            "-1",
            "+0",
            "(2)",
            "9223372036854775808",
            "18446744073709551615",
            "-9223372036854775808",
        ] {
            let sql = |table: &str| format!("SELECT {distinct}v FROM {table} ORDER BY {order}");
            assert!(
                c.execute(&sql("baseline"), &Parameters::new()).is_err(),
                "native {order}"
            );
            assert!(
                c.execute(&sql("docs"), &Parameters::new()).is_err(),
                "{distinct}{order}"
            );
        }
    }
}

#[test]
fn typed_record_ranges_use_numeric_keys_and_preserve_write_rollback() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    for key in [-10, -2, 2, 10] {
        c.execute("INSERT INTO docs (id,ref) VALUES (type::record('docs',$key),type::record('Docs',$key))", &Parameters::from([("$key".into(),Value::Integer(key))])).unwrap();
    }
    assert_eq!(
        query(
            &c,
            "SELECT record::id(id) AS key FROM docs WHERE ref < docs:10 ORDER BY id"
        )
        .rows,
        vec![
            vec![Value::Integer(-10)],
            vec![Value::Integer(-2)],
            vec![Value::Integer(2)]
        ]
    );
    assert_eq!(query(&c,"SELECT record::id(id) AS key FROM docs WHERE (ref) >= type::record('DOCS',2) ORDER BY id").rows, vec![vec![Value::Integer(2)],vec![Value::Integer(10)]]);
    query(&c, "UPDATE docs SET flag = ref <= docs:2");
    assert_eq!(
        query(&c, "SELECT flag FROM docs ORDER BY id").rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(1)],
            vec![Value::Integer(1)],
            vec![Value::Integer(0)]
        ]
    );
    query(&c, "BEGIN");
    assert_eq!(
        query(
            &c,
            "DELETE FROM docs WHERE ref > docs:2 RETURNING ref > docs:2 AS matched"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    query(&c, "ROLLBACK");
    assert_eq!(
        query(&c, "SELECT count(*) FROM docs").rows,
        vec![vec![Value::Integer(4)]]
    );
    let p = Parameters::from([
        (
            "$a".into(),
            Value::Record(Record {
                table: "docs".into(),
                key: Key::Integer(10),
            }),
        ),
        (
            "$b".into(),
            Value::Record(Record {
                table: "DOCS".into(),
                key: Key::String("2".into()),
            }),
        ),
    ]);
    assert_eq!(
        c.execute("SELECT $a < $b AS value", &p).unwrap().rows,
        vec![vec![Value::Integer(1)]]
    );
    let p = Parameters::from([
        ("$a".into(), Value::Null),
        (
            "$b".into(),
            Value::Record(Record {
                table: "docs".into(),
                key: Key::Integer(1),
            }),
        ),
    ]);
    assert_eq!(
        c.execute("SELECT $a < $b AS value", &p).unwrap().rows,
        vec![vec![Value::Null]]
    );
    let p = Parameters::from([
        (
            "$a".into(),
            Value::Record(Record {
                table: "docs".into(),
                key: Key::Integer(1),
            }),
        ),
        ("$b".into(), Value::Integer(1)),
    ]);
    assert!(c.execute("SELECT $a < $b AS value", &p).is_err());
}

#[test]
fn typed_scalar_ranges_keep_native_numeric_null_and_text_rules() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "CREATE TABLE baseline(a,b)");
    for (a, b) in [
        ("1", "1.0"),
        ("9223372036854775807", "9.223372036854776e18"),
        ("null", "2"),
        ("true", "0"),
        ("'a'", "'B'"),
        ("'2'", "2"),
    ] {
        query(&c, &format!("INSERT INTO docs {{a:{a},b:{b}}}"));
        query(&c, &format!("INSERT INTO baseline VALUES ({a},{b})"));
    }
    for op in ["<", "<=", ">", ">="] {
        let sql = |table: &str| format!("SELECT a {op} b AS value FROM {table} ORDER BY value");
        assert_eq!(
            query(&c, &sql("docs")).rows,
            query(&c, &sql("baseline")).rows,
            "{op}"
        );
    }
}

#[test]
fn typed_between_matches_record_ranges_and_native_null_logic() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    for key in [1, 2, 10, 20] {
        c.execute(
            "INSERT INTO docs (id) VALUES (type::record('docs',$key))",
            &Parameters::from([("$key".into(), Value::Integer(key))]),
        )
        .unwrap();
    }
    assert_eq!(query(&c,"SELECT record::id(id) AS key FROM docs WHERE id BETWEEN docs:2 AND docs:10 ORDER BY id").rows,vec![vec![Value::Integer(2)],vec![Value::Integer(10)]]);
    assert_eq!(query(&c,"SELECT record::id(id) AS key FROM docs WHERE id NOT BETWEEN docs:2 AND docs:10 ORDER BY id").rows,vec![vec![Value::Integer(1)],vec![Value::Integer(20)]]);
    query(&c, "BEGIN");
    assert_eq!(query(&c,"DELETE FROM docs WHERE id BETWEEN docs:2 AND docs:10 RETURNING id BETWEEN docs:2 AND docs:10 AS matched").rows,vec![vec![Value::Integer(1)],vec![Value::Integer(1)]]);
    query(&c, "ROLLBACK");
    assert_eq!(
        query(&c, "SELECT count(*) FROM docs").rows,
        vec![vec![Value::Integer(4)]]
    );
    query(&c, "CREATE TABLE values_doc");
    query(&c, "CREATE TABLE baseline(a,b,c)");
    for (a, b, c_) in [
        ("null", "1", "2"),
        ("0", "1", "null"),
        ("3", "null", "2"),
        ("1", "null", "2"),
        ("1", "0", "null"),
        ("1", "0", "2"),
        ("1.0", "true", "2"),
        ("'a'", "'A'", "'z'"),
    ] {
        query(
            &c,
            &format!("INSERT INTO values_doc {{a:{a},b:{b},c:{c_}}}"),
        );
        query(&c, &format!("INSERT INTO baseline VALUES ({a},{b},{c_})"));
    }
    for not in ["", "NOT "] {
        let sql = |table: &str| {
            format!("SELECT a {not}BETWEEN b AND c AS value FROM {table} ORDER BY value")
        };
        assert_eq!(
            query(&c, &sql("values_doc")).rows,
            query(&c, &sql("baseline")).rows,
            "{not}"
        );
    }
}

#[test]
fn managed_membership_plans_preserve_results_and_residual_predicates() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    for (key, score) in [("1", 1), ("1.0", 2), ("2", 3), ("null", 4), ("'1'", 5)] {
        query(&c, &format!("INSERT INTO docs {{key:{key},score:{score}}}"));
    }
    let tails = [
        "key IN (1,2,1,NULL)",
        "(key IN ((1),2)) AND score>1",
        "((key) = (1)) AND score>1",
        "key NOT IN (1,NULL)",
        "key IN ()",
        "key IN (1) OR score=5",
    ];
    let sql = |tail: &str| format!("SELECT score FROM docs WHERE {tail} ORDER BY score");
    let before = tails
        .iter()
        .map(|tail| query(&c, &sql(tail)).rows)
        .collect::<Vec<_>>();
    query(&c, "CREATE INDEX docs_key ON docs(key)");
    for (i, tail) in tails.iter().enumerate() {
        assert_eq!(query(&c, &sql(tail)).rows, before[i], "{tail}");
        let plan = query(&c, &format!("EXPLAIN QUERY PLAN {}", sql(tail)));
        let plan = format!("{:?}", plan.rows);
        if i < 3 {
            assert!(plan.contains("SEARCH i"), "{tail}: {plan}");
        }
    }
    let params = Parameters::from([
        ("$a".into(), Value::Integer(1)),
        ("$b".into(), Value::Integer(2)),
    ]);
    assert_eq!(
        c.execute(
            "SELECT score FROM docs WHERE key IN ($a,$b) ORDER BY score",
            &params
        )
        .unwrap()
        .rows,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(3)]
        ]
    );
    query(&c, "BEGIN");
    assert_eq!(
        query(
            &c,
            "DELETE FROM docs WHERE (key IN (1,2)) AND score>1 RETURNING score"
        )
        .rows
        .len(),
        2
    );
    query(&c, "ROLLBACK");
    assert_eq!(query(&c, &sql(tails[0])).rows, before[0]);
}

#[test]
fn managed_membership_preserves_typed_record_identity() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "INSERT INTO docs {id:docs:a,ref:docs:1}");
    query(&c, "INSERT INTO docs {id:docs:b,ref:docs:`1`}");
    query(&c, "INSERT INTO docs {id:docs:c,ref:docs:2}");
    let sql = "SELECT id FROM docs WHERE ref IN (type::record('DOCS',1),docs:2,docs:2) ORDER BY id";
    let before = query(&c, sql).rows;
    assert_eq!(before.len(), 2);
    query(&c, "CREATE INDEX docs_ref ON docs(ref)");
    assert_eq!(query(&c, sql).rows, before);
    assert!(
        format!("{:?}", query(&c, &format!("EXPLAIN QUERY PLAN {sql}")).rows).contains("SEARCH i")
    );
}

#[test]
fn managed_null_filters_preserve_missing_values_and_outer_join_results() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "INSERT INTO docs {id:docs:a,marker:1}");
    query(&c, "INSERT INTO docs {id:docs:b,marker:2,key:null}");
    query(&c, "INSERT INTO docs {id:docs:c,marker:3,key:1}");
    query(&c, "INSERT INTO docs {id:docs:d,marker:4,key:'value'}");
    query(&c, "CREATE TABLE ordinary(marker INTEGER)");
    query(&c, "INSERT INTO ordinary VALUES (1),(2),(3),(4),(5)");
    let tails = [
        "key IS NULL",
        "((key) IS NULL) AND marker>1",
        "key IS NOT NULL",
        "key NOTNULL",
        "key=NULL",
        "key ISNULL",
        "NULL IS key",
        "NULL IS NOT key",
    ];
    let sql = |tail: &str| format!("SELECT marker FROM docs WHERE {tail} ORDER BY marker");
    let before = tails
        .iter()
        .map(|tail| query(&c, &sql(tail)).rows)
        .collect::<Vec<_>>();
    let join="SELECT n.marker FROM docs d RIGHT JOIN ordinary n ON d.marker=n.marker WHERE d.key IS NULL ORDER BY n.marker";
    let joined = query(&c, join).rows;
    assert_eq!(
        joined,
        vec![
            vec![Value::Integer(1)],
            vec![Value::Integer(2)],
            vec![Value::Integer(5)]
        ]
    );
    query(&c, "CREATE INDEX docs_key ON docs(key)");
    for (i, tail) in tails.iter().enumerate() {
        assert_eq!(query(&c, &sql(tail)).rows, before[i], "{tail}");
        if i < 2 {
            let plan = format!(
                "{:?}",
                query(&c, &format!("EXPLAIN QUERY PLAN {}", sql(tail))).rows
            );
            // Pinned Turso scans native IS NULL indexes too. The useful
            // restriction is applied to compact keys before fetching documents.
            assert!(
                plan.contains(" AS i") && plan.contains("SEARCH c"),
                "{plan}"
            );
        }
    }
    assert_eq!(query(&c, join).rows, joined);
    query(&c, "BEGIN");
    assert_eq!(
        query(
            &c,
            "DELETE FROM docs WHERE key IS NULL AND marker>1 RETURNING marker"
        )
        .rows,
        vec![vec![Value::Integer(2)]]
    );
    query(&c, "ROLLBACK");
    assert_eq!(query(&c, &sql(tails[0])).rows, before[0]);
    query(&c, "UPDATE docs SET key=9 WHERE key IS NULL");
    assert!(query(&c, &sql(tails[0])).rows.is_empty());
    assert_eq!(query(&c, &sql(tails[2])).rows.len(), 4);
}

#[test]
fn binary_literals_match_typed_fields_and_managed_index_keys() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(
        &c,
        "INSERT INTO docs (id,data) VALUES (docs:a,X'0102'),(docs:b,X'03'),(docs:c,NULL)",
    );
    let tails = [
        "data=X'0102'",
        "(X'0102')=data",
        "data IS X'0102'",
        "data!=X'0102'",
        "data IS NOT X'0102'",
        "data IN (X'0102',X'03',X'0102',NULL)",
        "data NOT IN (X'0102',X'03')",
    ];
    let sql = |tail: &str| format!("SELECT id FROM docs WHERE {tail} ORDER BY id");
    let expected = [
        vec!["a"],
        vec!["a"],
        vec!["a"],
        vec!["b"],
        vec!["b", "c"],
        vec!["a", "b"],
        vec![],
    ];
    for indexed in [false, true] {
        if indexed {
            query(&c, "CREATE INDEX docs_data ON docs(data)");
        }
        for (tail, keys) in tails.iter().zip(&expected) {
            let rows = keys
                .iter()
                .map(|key| {
                    vec![Value::Record(Record {
                        table: "docs".into(),
                        key: Key::String((*key).into()),
                    })]
                })
                .collect::<Vec<_>>();
            assert_eq!(query(&c, &sql(tail)).rows, rows, "{indexed}: {tail}");
        }
    }
    for tail in [tails[0], tails[5]] {
        let plan = query(&c, &format!("EXPLAIN QUERY PLAN {}", sql(tail)));
        assert!(format!("{:?}", plan.rows).contains("SEARCH i"));
    }
    let p = Parameters::from([("$data".into(), Value::Binary(vec![1, 2]))]);
    assert_eq!(
        c.execute("SELECT $data=X'0102' AS same FROM docs LIMIT 1", &p)
            .unwrap()
            .rows,
        vec![vec![Value::Integer(1)]]
    );
    query(&c, "BEGIN");
    assert_eq!(
        query(
            &c,
            "DELETE FROM docs WHERE data=X'0102' RETURNING data=X'0102' AS matched"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    query(&c, "ROLLBACK");
    assert_eq!(
        c.lookup_index("docs", "docs_data", &Value::Binary(vec![1, 2]))
            .unwrap()
            .len(),
        1
    );
    // Raw bytes that imitate a record encoding remain a binary value.
    let bytes =
        b"FDB\x01{\"type\":\"Record\",\"value\":{\"table\":\"docs\",\"key\":{\"String\":\"a\"}}}";
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert!(query(&c, &format!("SELECT id FROM docs WHERE id=X'{hex}'"))
        .rows
        .is_empty());
}

#[test]
fn binary_payload_order_and_ranges_match_native_blobs() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "CREATE TABLE baseline(data BLOB)");
    for hex in ["", "00", "02", "0200", "0A", "FF"] {
        query(&c, &format!("INSERT INTO docs (data) VALUES (X'{hex}')"));
        query(&c, &format!("INSERT INTO baseline VALUES (X'{hex}')"));
    }
    for distinct in ["", "DISTINCT "] {
        for tail in [
            "ORDER BY data",
            "ORDER BY data DESC",
            "WHERE data < X'0A' ORDER BY data",
            "WHERE data >= (X'02') ORDER BY data",
            "WHERE data BETWEEN X'02' AND X'0A' ORDER BY data",
            "WHERE data NOT BETWEEN X'02' AND X'0A' ORDER BY data",
        ] {
            let sql = |table: &str| format!("SELECT {distinct}data FROM {table} {tail}");
            assert_eq!(
                query(&c, &sql("docs")).rows,
                query(&c, &sql("baseline")).rows,
                "{distinct}{tail}"
            );
        }
    }
    assert_eq!(
        query(
            &c,
            "SELECT (X'02')=X'02' AS same,X'02'<X'0A' AS smaller FROM docs LIMIT 1"
        )
        .rows,
        vec![vec![Value::Integer(1), Value::Integer(1)]]
    );
    assert_eq!(
        query(&c, "SELECT X'02' AS bytes FROM docs LIMIT 1").rows,
        vec![vec![Value::Binary(vec![2])]]
    );
    assert_eq!(
        query(
            &c,
            "SELECT X'02' IN (X'02',X'03') AS member FROM docs LIMIT 1"
        )
        .rows,
        vec![vec![Value::Integer(1)]]
    );
    query(&c, "CREATE INDEX docs_data ON docs(data)");
    assert_eq!(
        query(
            &c,
            "SELECT data FROM docs WHERE data IN (X'02',X'0A') ORDER BY data"
        )
        .rows,
        vec![vec![Value::Binary(vec![2])], vec![Value::Binary(vec![10])]]
    );
    query(&c, "BEGIN");
    assert_eq!(
        query(
            &c,
            "DELETE FROM docs WHERE data BETWEEN X'02' AND X'0A' RETURNING data"
        )
        .rows
        .len(),
        3
    );
    query(&c, "ROLLBACK");
    assert_eq!(
        query(&c, "SELECT data FROM docs ORDER BY data").rows,
        query(&c, "SELECT data FROM baseline ORDER BY data").rows
    );
}

#[test]
fn binary_membership_normalizes_literals_fields_and_function_results() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "CREATE TABLE baseline(data BLOB)");
    for hex in ["", "02", "0A"] {
        query(&c, &format!("INSERT INTO docs (data) VALUES (X'{hex}')"));
        query(&c, &format!("INSERT INTO baseline VALUES (X'{hex}')"));
    }
    query(&c, "INSERT INTO docs (data) VALUES (NULL)");
    query(&c, "INSERT INTO baseline VALUES (NULL)");
    let p = Parameters::from([("$bytes".into(), Value::Binary(vec![2]))]);
    for predicate in [
        "X'02' IN (data)",
        "(X'02') NOT IN (data,NULL)",
        "X'02' IN (data,X'02',substr(X'0203',1,1))",
        "data IN (substr(X'0203',1,1),X'0A')",
        "data NOT IN (substr(X'0203',1,1),NULL)",
        "X'02' IN ($bytes)",
        "$bytes IN (data,substr(X'0203',1,1))",
        "X'02' IN ()",
    ] {
        let p = if predicate.contains("$bytes") {
            p.clone()
        } else {
            Parameters::new()
        };
        let sql = |table| format!("SELECT {predicate} AS matched FROM {table} ORDER BY data");
        assert_eq!(
            c.execute(&sql("docs"), &p).unwrap().rows,
            c.execute(&sql("baseline"), &p).unwrap().rows,
            "{predicate}"
        );
    }
    assert_eq!(
        query(&c, "SELECT X'02' IN (docs:a) AS matched FROM docs LIMIT 1").rows,
        vec![vec![Value::Integer(0)]]
    );
    let bytes =
        b"FDB\x01{\"type\":\"Record\",\"value\":{\"table\":\"docs\",\"key\":{\"String\":\"a\"}}}";
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        query(
            &c,
            &format!("SELECT X'{hex}' IN (docs:a) AS matched FROM docs LIMIT 1")
        )
        .rows,
        vec![vec![Value::Integer(0)]]
    );
    query(&c, "CREATE INDEX docs_data ON docs(data)");
    query(&c, "BEGIN");
    assert_eq!(
        query(&c, "DELETE FROM docs WHERE X'02' IN (data) RETURNING data").rows,
        vec![vec![Value::Binary(vec![2])]]
    );
    query(&c, "ROLLBACK");
    assert_eq!(
        c.lookup_index("docs", "docs_data", &Value::Binary(vec![2]))
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn native_function_membership_preserves_binary_and_collation() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(
        &c,
        "CREATE TABLE baseline(data BLOB, label TEXT, number INTEGER)",
    );
    for values in ["X'02','A',2", "X'0A','b',3", "NULL,NULL,NULL"] {
        query(
            &c,
            &format!("INSERT INTO docs (data,label,number) VALUES ({values})"),
        );
        query(&c, &format!("INSERT INTO baseline VALUES ({values})"));
    }
    for predicate in [
        "substr(X'0203',1,1) IN (data)",
        "(substr(X'0203',1,1)) NOT IN (data,NULL)",
        "substr(X'0203',1,1) IN (data,X'02')",
        "lower('A') COLLATE NOCASE IN (label)",
        "(lower('A') COLLATE NOCASE) NOT IN (label,NULL)",
        "abs(-2) IN (number,3)",
        "abs(-2) IN ('2')",
        "CAST('2' AS INTEGER) IN ('2')",
        "CAST('a' AS TEXT) COLLATE NOCASE IN ('A')",
    ] {
        let sql = |table| format!("SELECT {predicate} AS matched FROM {table} ORDER BY data");
        assert_eq!(
            query(&c, &sql("docs")).rows,
            query(&c, &sql("baseline")).rows,
            "{predicate}"
        );
    }
    let bytes =
        b"FDB\x01{\"type\":\"Record\",\"value\":{\"table\":\"docs\",\"key\":{\"String\":\"a\"}}}";
    let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    assert_eq!(
        query(
            &c,
            &format!("SELECT substr(X'{hex}',1) IN (docs:a) AS matched FROM docs LIMIT 1")
        )
        .rows,
        vec![vec![Value::Integer(0)]]
    );
}

#[test]
fn cast_and_scalar_membership_keep_native_affinity() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "CREATE TABLE baseline(data BLOB, number, label)");
    for values in ["X'32',2,'A'", "X'33','2','b'", "NULL,NULL,NULL"] {
        query(
            &c,
            &format!("INSERT INTO docs (data,number,label) VALUES ({values})"),
        );
        query(&c, &format!("INSERT INTO baseline VALUES ({values})"));
    }
    for predicate in [
        "CAST('2' AS BLOB) IN (data)",
        "CAST('2' AS BLOB) NOT IN (data,NULL)",
        "+X'32' IN (data)",
        "+data IN (X'32')",
        "data IN (+data)",
        "X'32' IN (+data)",
        "'a' IN (label COLLATE NOCASE)",
        "CAST('2' AS REAL) IN (number)",
        "CAST('2' AS NUMERIC) IN (number)",
        "+CAST('2' AS INTEGER) IN ('2')",
        "CAST('2' AS INTEGER) IN (number)",
        "CAST('2' AS INTEGER) IN ('2')",
        "CAST(2 AS TEXT) IN (number)",
        "CAST('a' COLLATE NOCASE AS TEXT) IN (label)",
        "CAST('a' AS TEXT) COLLATE NOCASE IN (label)",
        "(1+1) IN (number)",
        "2 IN (number)",
        "NULL IN (data)",
        "CAST(data AS TEXT) IN ('2')",
    ] {
        let sql = |table| format!("SELECT {predicate} AS matched FROM {table} ORDER BY data");
        assert_eq!(
            query(&c, &sql("docs")).rows,
            query(&c, &sql("baseline")).rows,
            "{predicate}"
        );
    }
}

#[test]
fn binary_predicate_contexts_use_native_truth_conversion() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    query(&c, "CREATE TABLE docs");
    query(&c, "CREATE TABLE baseline(data BLOB)");
    for hex in ["31", "30", "2D32", "", "6162"] {
        query(&c, &format!("INSERT INTO docs (data) VALUES (X'{hex}')"));
        query(&c, &format!("INSERT INTO baseline VALUES (X'{hex}')"));
    }
    query(&c, "INSERT INTO docs (data) VALUES (NULL)");
    query(&c, "INSERT INTO baseline VALUES (NULL)");
    for template in [
        "SELECT data FROM SOURCE WHERE data ORDER BY data",
        "SELECT CASE WHEN data THEN 1 ELSE 0 END AS truth FROM SOURCE ORDER BY data",
        "SELECT data FROM SOURCE GROUP BY data HAVING data ORDER BY data",
        "SELECT count(*) FILTER (WHERE data) AS total FROM SOURCE",
        "SELECT sum(1) FILTER (WHERE data) AS total FROM SOURCE",
        "SELECT count(*) AS total FROM SOURCE a JOIN SOURCE b ON a.data",
        "SELECT count(*) AS total FROM SOURCE a LEFT JOIN SOURCE b ON a.data",
    ] {
        assert_eq!(
            query(&c, &template.replace("SOURCE", "docs")).rows,
            query(&c, &template.replace("SOURCE", "baseline")).rows,
            "{template}"
        );
    }
    query(&c, "CREATE INDEX docs_data ON docs(data)");
    query(&c, "BEGIN");
    assert_eq!(
        query(&c, "DELETE FROM docs WHERE data RETURNING data")
            .rows
            .len(),
        2
    );
    query(&c, "ROLLBACK");
    assert_eq!(
        c.lookup_index("docs", "docs_data", &Value::Binary(b"1".to_vec()))
            .unwrap()
            .len(),
        1
    );
}
