use fastdb::{Database, Document, Key, Parameters, Record, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
fn setup() -> (Database, fastdb::Connection) {
    let db = Database::open(":memory:").expect("open");
    let c = db.connect().expect("connect");
    q(&c, "CREATE TABLE users");
    q(&c, "DEFINE FIELD name ON users TYPE string REQUIRED");
    q(&c, "CREATE UNIQUE INDEX users_name ON users (name)");
    (db, c)
}
#[test]
fn column_list_insert_and_multirow_updates_use_pre_update_values() {
    let (_db, c) = setup();
    let rows=q(&c,"INSERT INTO users (id, name, a, b) VALUES (users:u1, 'Alice', 1, 2), (users:u2, 'Bob', 3, 4) RETURNING *");
    assert_eq!(rows.affected, 2);
    let params = Parameters::from([("$key".into(), Value::String("u3".into()))]);
    c.execute(
        "INSERT INTO users (id, name, a, b) VALUES (type::record('users', $key), 'Carol', 5, 6)",
        &params,
    )
    .expect("dynamic id");
    q(&c, "UPDATE users SET a=b, b=a WHERE a < 5 RETURNING *");
    assert_eq!(
        q(&c, "SELECT a, b FROM users WHERE id = users:u1").rows,
        vec![vec![Value::Integer(2), Value::Integer(1)]]
    );
    assert_eq!(
        q(&c, "SELECT a, b FROM users WHERE id = users:u2").rows,
        vec![vec![Value::Integer(4), Value::Integer(3)]]
    );
    assert_eq!(
        q(&c, "SELECT a, b FROM users WHERE id = users:u3").rows,
        vec![vec![Value::Integer(5), Value::Integer(6)]]
    );
    q(&c, "UPDATE users SET name=upper(name)");
    assert_eq!(
        c.lookup_index("users", "users_name", &Value::String("ALICE".into()))
            .expect("updated index")
            .len(),
        1
    );
    let deleted = q(&c, "DELETE FROM users WHERE a < 5 RETURNING *");
    assert_eq!(deleted.affected, 2);
    assert!(c
        .lookup_index("users", "users_name", &Value::String("ALICE".into()))
        .expect("deleted index")
        .is_empty());
}
#[test]
fn failed_statements_rollback_all_rows_and_keep_outer_transaction() {
    let (_db, c) = setup();
    assert!(c
        .execute(
            "INSERT INTO users (id,name) VALUES (users:u1,'same'),(users:u2,'same')",
            &Parameters::new()
        )
        .is_err());
    assert!(q(&c, "SELECT * FROM users").rows.is_empty());
    q(
        &c,
        "INSERT INTO users (id,name) VALUES (users:u1,'Alice'),(users:u2,'Bob')",
    );
    q(&c, "CREATE TABLE accounts (id INTEGER)");
    q(&c, "BEGIN");
    q(&c, "INSERT INTO accounts VALUES (42)");
    assert!(c
        .execute("UPDATE users SET name='duplicate'", &Parameters::new())
        .is_err());
    assert_eq!(
        q(&c, "SELECT name FROM users ORDER BY name").rows,
        vec![
            vec![Value::String("Alice".into())],
            vec![Value::String("Bob".into())]
        ]
    );
    assert_eq!(
        q(&c, "SELECT id FROM accounts").rows,
        vec![vec![Value::Integer(42)]]
    );
    q(&c, "DELETE FROM users");
    q(&c, "ROLLBACK");
    assert_eq!(q(&c, "SELECT * FROM users").rows.len(), 2);
    assert!(q(&c, "SELECT * FROM accounts").rows.is_empty());
}
#[test]
fn typed_parameters_and_validation_cannot_be_bypassed() {
    let (_db, c) = setup();
    q(&c, "DEFINE FIELD active ON users TYPE boolean");
    let params = Parameters::from([
        (
            "$id".into(),
            Value::Record(Record {
                table: "users".into(),
                key: Key::String("typed".into()),
            }),
        ),
        ("$active".into(), Value::Boolean(true)),
    ]);
    c.execute(
        "INSERT INTO users (id,name,active) VALUES ($id,'Typed',$active)",
        &params,
    )
    .expect("typed input");
    assert_eq!(
        q(&c, "SELECT active FROM users").rows,
        vec![vec![Value::Boolean(true)]]
    );
    let profile = Value::Object(Document::from([(
        "city".into(),
        Value::String("Bangkok".into()),
    )]));
    let params = Parameters::from([("$profile".into(), profile.clone())]);
    c.execute("UPDATE users SET profile=$profile", &params)
        .expect("typed object assignment");
    assert_eq!(q(&c, "SELECT profile FROM users").rows, vec![vec![profile]]);
    for sql in [
        "UPDATE users SET id = users:other",
        "UPDATE users SET name = 42",
        "UPDATE users SET active = 1",
        "INSERT INTO users (id,name) VALUES (1,'bad')",
        "INSERT INTO users (id,name) VALUES (users:u2,'x') ON CONFLICT DO NOTHING",
        "UPDATE users SET name='x', name='y'",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(
        q(&c, "SELECT name FROM users").rows,
        vec![vec![Value::String("Typed".into())]]
    );
}
#[test]
fn record_expression_projection_stays_typed_and_ordinary_sql_stays_relational() {
    let (_db, c) = setup();
    q(&c, "INSERT INTO users (id,name) VALUES (users:u1,'Alice')");
    let record = Value::Record(Record {
        table: "users".into(),
        key: Key::Integer(123),
    });
    assert_eq!(
        q(&c, "SELECT users:123 AS reference FROM users").rows,
        vec![vec![record]]
    );
    q(
        &c,
        "CREATE TABLE ordinary (id INTEGER PRIMARY KEY, name TEXT)",
    );
    q(&c, "INSERT INTO ordinary (id,name) VALUES (1,'x'),(2,'y')");
    q(&c, "UPDATE ordinary SET name=upper(name)");
    assert_eq!(
        q(&c, "DELETE FROM ordinary WHERE id=1 RETURNING id").rows,
        vec![vec![Value::Integer(1)]]
    );
}

#[test]
fn nested_set_unset_and_overlap_rules() {
    let (_db, c) = setup();
    q(&c,"INSERT INTO users {id: users:u1, name: 'Alice', profile: {city: 'Bangkok', zip: 10110}, a: 1, b: 2}");
    q(&c, "CREATE INDEX users_city ON users (profile.city)");
    q(
        &c,
        "UPDATE users:u1 SET profile.city='Berlin', a=b, b=a RETURNING *",
    );
    assert_eq!(
        q(
            &c,
            "SELECT u.profile.city AS city, u.profile.zip AS zip, a, b FROM users u"
        )
        .rows,
        vec![vec![
            Value::String("Berlin".into()),
            Value::Integer(10110),
            Value::Integer(2),
            Value::Integer(1)
        ]]
    );
    assert_eq!(
        c.lookup_index("users", "users_city", &Value::String("Berlin".into()))
            .expect("nested index")
            .len(),
        1
    );
    q(&c, "UPDATE users SET extra.deep.leaf=42 WHERE id=users:u1");
    q(
        &c,
        "UPDATE users SET \"profile.city\"='literal', profile.city='Paris'",
    );
    q(
        &c,
        "UPDATE users:u1 UNSET profile.city, missing.deep RETURNING *",
    );
    let row = q(&c, "SELECT * FROM users").exactly_one().expect("one");
    let Value::Object(doc) = &row[0] else {
        panic!("document");
    };
    let Value::Object(profile) = &doc["profile"] else {
        panic!("profile");
    };
    assert!(!profile.contains_key("city"));
    assert!(profile.contains_key("zip"));
    assert_eq!(doc["profile.city"], Value::String("literal".into()));
    assert!(c
        .lookup_index("users", "users_city", &Value::String("Paris".into()))
        .expect("unset index")
        .is_empty());
    for sql in [
        "UPDATE users SET profile=1, profile.city='x'",
        "UPDATE users SET name.first='x'",
        "UPDATE users UNSET name",
        "UPDATE users UNSET id",
        "UPDATE users SET a=1, a=2",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(
        q(&c, "SELECT name FROM users").rows,
        vec![vec![Value::String("Alice".into())]]
    );
}

#[test]
fn standalone_record_constructors_and_parameter_names() {
    let (_db, c) = setup();
    let params = Parameters::from([("$key".into(), Value::Integer(i64::MIN))]);
    let value = Value::Record(Record {
        table: "users".into(),
        key: Key::Integer(i64::MIN),
    });
    assert_eq!(
        c.execute("SELECT type::record('users', $key) AS id", &params)
            .expect("constructor")
            .rows,
        vec![vec![value]]
    );
    assert_eq!(
        q(&c, "SELECT users:123 AS id").rows,
        vec![vec![Value::Record(Record {
            table: "users".into(),
            key: Key::Integer(123)
        })]]
    );
    for sql in [
        "SELECT type::record('', 1)",
        "SELECT type::record('users', '')",
        "SELECT type::record('users', 1.5)",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    let params = Parameters::from([("$name::suffix".into(), Value::Integer(7))]);
    let baseline = turso_core::Database::open_file(
        turso_core::Database::io_for_path(":memory:").expect("io"),
        ":memory:",
    )
    .expect("baseline");
    let raw = baseline.connect().expect("baseline connection");
    let expected = match raw.prepare("SELECT $name::suffix") {
        Ok(_) => {
            panic!("update compatibility expectation: baseline now supports namespace parameters")
        }
        Err(e) => e.to_string(),
    };
    let error = c
        .execute("SELECT $name::suffix", &params)
        .expect_err("same baseline rejection");
    let fastdb::Error::Engine(actual) = error else {
        panic!("expected baseline error");
    };
    assert_eq!(actual.to_string(), expected);
    q(
        &c,
        "INSERT INTO users (id,name,n) VALUES (users:u1,'Alice',10),(users:u2,'Bob',2)",
    );
    let rows = q(
        &c,
        "SELECT type::record('users', n) AS r FROM users ORDER BY r",
    )
    .rows;
    assert_eq!(
        rows,
        vec![
            vec![Value::Record(Record {
                table: "users".into(),
                key: Key::Integer(2)
            })],
            vec![Value::Record(Record {
                table: "users".into(),
                key: Key::Integer(10)
            })]
        ]
    );
}

#[test]
fn indexed_and_anonymous_parameters_keep_statement_numbering() {
    let (_db, c) = setup();
    let params = Parameters::from([
        (
            "?1".into(),
            Value::Record(Record {
                table: "users".into(),
                key: Key::String("numbered".into()),
            }),
        ),
        ("?2".into(), Value::String("Alice".into())),
    ]);
    c.execute("INSERT INTO users (id,name) VALUES (?,?)", &params)
        .expect("anonymous slots");
    let params = Parameters::from([
        ("?1".into(), Value::String("Bob".into())),
        ("?2".into(), Value::String("Alice".into())),
    ]);
    c.execute("UPDATE users SET name=?1 WHERE name=?2", &params)
        .expect("numbered update");
    let params = Parameters::from([
        ("?1".into(), Value::Integer(1)),
        ("?2".into(), Value::Integer(2)),
    ]);
    assert_eq!(
        c.execute("SELECT ?1 + ?2", &params)
            .expect("ordinary positional SQL")
            .rows,
        vec![vec![Value::Integer(3)]]
    );
    assert_eq!(
        q(&c, "SELECT name FROM users").rows,
        vec![vec![Value::String("Bob".into())]]
    );
}

#[test]
fn explicit_tuple_updates_preserve_snapshots_and_atomic_validation() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,2),(3,4)",
        "CREATE TABLE docs",
        "INSERT INTO docs(a,b) SELECT a,b FROM native",
        "DEFINE FIELD a ON docs TYPE integer CHECK(a<10)",
        "CREATE UNIQUE INDEX docs_a ON docs(a)",
    ] {
        q(&c, sql);
    }
    for assignment in ["(a,b)=(b,a)", "(a,b)=(a+1,b+2)", "(a,b)=(SELECT b,a)"] {
        for source in ["native", "docs"] {
            q(&c, &format!("UPDATE {source} SET {assignment}"));
        }
        assert_eq!(
            q(&c, "SELECT a,b FROM docs ORDER BY a").rows,
            q(&c, "SELECT a,b FROM native ORDER BY a").rows
        );
    }
    q(&c, "BEGIN");
    q(&c, "INSERT INTO docs(a,b) VALUES(0,0)");
    let before = q(&c, "SELECT a,b FROM docs ORDER BY a").rows;
    let error = c
        .execute("UPDATE docs SET (a,b)=(SELECT a+6,b+1)", &Parameters::new())
        .unwrap_err();
    assert_eq!(error.code(), "FDB_VALIDATION");
    assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
    assert_eq!(q(&c, "SELECT a,b FROM docs ORDER BY a").rows, before);
    assert!(c
        .lookup_index("docs", "docs_a", &Value::Integer(6))
        .unwrap()
        .is_empty());
    let params = Parameters::from([
        ("$a".into(), Value::Integer(7)),
        ("$b".into(), Value::Integer(8)),
    ]);
    assert_eq!(
        c.execute(
            "UPDATE docs SET (a,b)=($a,$b) WHERE a=0 RETURNING a,b",
            &params
        )
        .unwrap()
        .rows,
        vec![vec![Value::Integer(7), Value::Integer(8)]]
    );
    for sql in [
        "UPDATE docs SET (a,b)=(1,2),a=3",
        "UPDATE docs SET (id,a)=(docs:other,1)",
        "UPDATE docs SET (a,b)=(1,2,3)",
        "UPDATE docs SET (a,b)=(SELECT b,a ORDER BY 3)",
    ] {
        assert!(c.execute(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(
        c.check_collection_integrity("docs", fastdb::IntegrityLimits::default())
            .unwrap()
            .index_entries,
        3
    );
    q(&c, "ROLLBACK");
    assert_eq!(
        q(&c, "SELECT a,b FROM docs ORDER BY a").rows,
        q(&c, "SELECT a,b FROM native ORDER BY a").rows
    );
}

#[test]
fn tuple_updates_preserve_typed_values_and_mixed_assignment_snapshots() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(a,b,c) VALUES(1,2,3)");
    let record = Value::Record(Record {
        table: "docs".into(),
        key: Key::String("target".into()),
    });
    let object = Value::Object(Document::from([(
        "nested".into(),
        Value::Array(vec![Value::Boolean(true), Value::Binary(vec![0, 255])]),
    )]));
    let params = Parameters::from([("?1".into(), record.clone()), ("?2".into(), object.clone())]);
    let result = c
        .execute("UPDATE docs SET (a,b)=(?1,?2),c=a RETURNING a,b,c", &params)
        .unwrap();
    assert_eq!(
        result.rows,
        vec![vec![record.clone(), object.clone(), Value::Integer(1)]]
    );
    assert_eq!(
        q(&c, "UPDATE docs SET (a,b)=(b,a),c=b RETURNING a,b,c").rows,
        vec![vec![object.clone(), record.clone(), object.clone()]]
    );
    let before = q(&c, "SELECT a,b,c FROM docs").rows;
    assert_eq!(
        c.execute(
            "UPDATE docs SET (a,b)=(?1,?2)",
            &Parameters::from([("?1".into(), Value::Null)])
        )
        .unwrap_err()
        .code(),
        "FDB_PARAMETER"
    );
    assert_eq!(q(&c, "SELECT a,b,c FROM docs").rows, before);
    let empty = c
        .execute(
            "UPDATE docs SET (a,b)=(?1,?2) WHERE 0 RETURNING a,b",
            &params,
        )
        .unwrap();
    assert_eq!(empty.columns, vec!["a", "b"]);
    assert!(empty.rows.is_empty());
    assert_eq!(empty.affected, 0);
}

#[test]
fn source_free_tuple_subqueries_match_native_snapshots_and_empty_rows() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE native(a INTEGER,b INTEGER)");
    q(&c, "INSERT INTO native VALUES(1,2),(3,4)");
    for (rhs, expected) in [
        (
            "(SELECT b,a)",
            vec![
                vec![Value::Integer(2), Value::Integer(1)],
                vec![Value::Integer(4), Value::Integer(3)],
            ],
        ),
        (
            "(SELECT 8,9 WHERE 0)",
            vec![
                vec![Value::Null, Value::Null],
                vec![Value::Null, Value::Null],
            ],
        ),
    ] {
        q(&c, "BEGIN");
        assert_eq!(
            q(&c, &format!("UPDATE native SET (a,b)={rhs} RETURNING a,b")).rows,
            expected
        );
        q(&c, "ROLLBACK");
    }
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(a,b) SELECT a,b FROM native");
    assert_eq!(
        q(&c, "UPDATE docs SET (a,b)=(SELECT b,a) RETURNING a,b").rows,
        vec![
            vec![Value::Integer(2), Value::Integer(1)],
            vec![Value::Integer(4), Value::Integer(3)]
        ]
    );
    assert_eq!(
        q(
            &c,
            "UPDATE docs SET (a,b)=(SELECT 8,9 WHERE 0) RETURNING a,b"
        )
        .rows,
        vec![
            vec![Value::Null, Value::Null],
            vec![Value::Null, Value::Null]
        ]
    );
}

#[test]
fn tuple_select_parameters_and_conditional_rows_keep_types() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs(n) VALUES(1),(2)");
    let record = Value::Record(Record {
        table: "docs".into(),
        key: Key::Integer(i64::MAX),
    });
    let object = Value::Object(Document::from([(
        "values".into(),
        Value::Array(vec![Value::Boolean(true), Value::Binary(vec![0, 255])]),
    )]));
    let params = Parameters::from([("$r".into(), record.clone()), ("$o".into(), object.clone())]);
    assert_eq!(
        c.execute(
            "UPDATE docs SET (a,b)=(SELECT $r,$o WHERE n=1) RETURNING n,a,b",
            &params
        )
        .unwrap()
        .rows,
        vec![
            vec![Value::Integer(1), record.clone(), object.clone()],
            vec![Value::Integer(2), Value::Null, Value::Null]
        ]
    );
    assert_eq!(
        q(
            &c,
            "UPDATE docs SET (a,b)=(SELECT b,a),n=n+1 WHERE n=1 RETURNING a,b,n"
        )
        .rows,
        vec![vec![object, record, Value::Integer(2)]]
    );
    let before = q(&c, "SELECT a,b,n FROM docs").rows;
    assert_eq!(
        c.execute("UPDATE docs SET (a,b)=(SELECT $r,$o)", &Parameters::new())
            .unwrap_err()
            .code(),
        "FDB_PARAMETER"
    );
    assert_eq!(q(&c, "SELECT a,b,n FROM docs").rows, before);
}

#[test]
fn tuple_predicate_binding_preserves_quoted_fields_and_nested_scope() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER, a INTEGER, b INTEGER, \"true\" INTEGER)",
        "INSERT INTO native VALUES(1,2,3,7),(2,4,5,8)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b,\"true\") SELECT n,a,b,\"true\" FROM native",
    ] {
        q(&c, sql);
    }
    for tuple in [
        "(SELECT b,a WHERE n=1)",
        "(SELECT \"true\",a WHERE \"true\"=7)",
        "(SELECT b,a WHERE n IN (SELECT 1))",
        "(SELECT b,a WHERE EXISTS(SELECT 1 FROM native x WHERE x.n=1))",
    ] {
        q(&c, "BEGIN");
        let expected = q(
            &c,
            &format!("UPDATE native SET (a,b)={tuple} RETURNING n,a,b"),
        )
        .rows;
        assert_eq!(
            q(
                &c,
                &format!("UPDATE docs SET (a,b)={tuple} RETURNING n,a,b")
            )
            .rows,
            expected,
            "{tuple}"
        );
        q(&c, "ROLLBACK");
    }
}

#[test]
fn tuple_select_bound_pagination_matches_native_empty_rows() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,2),(3,4)",
        "CREATE TABLE docs",
        "INSERT INTO docs(a,b) SELECT a,b FROM native",
    ] {
        q(&c, sql);
    }
    for (limit, offset) in [(0, 0), (1, 0), (1, 1), (-1, 0), (-1, 2)] {
        q(&c, "BEGIN");
        let params = Parameters::from([
            ("$limit".into(), Value::Integer(limit)),
            ("$offset".into(), Value::Integer(offset)),
        ]);
        let query = |source| {
            format!(
                "UPDATE {source} SET (a,b)=(SELECT b,a LIMIT $limit OFFSET $offset) RETURNING a,b"
            )
        };
        let native = query("native")
            .replace("$limit", &limit.to_string())
            .replace("$offset", &offset.to_string());
        let expected = c.execute(&native, &Parameters::new()).unwrap();
        let actual = c.execute(&query("docs"), &params).unwrap();
        assert_eq!(actual.rows, expected.rows, "{limit}/{offset}");
        assert_eq!(actual.affected, expected.affected);
        q(&c, "ROLLBACK");
    }
    let before = q(&c, "SELECT a,b FROM docs ORDER BY a").rows;
    assert_eq!(
        c.execute(
            "UPDATE docs SET (a,b)=(SELECT b,a LIMIT $limit)",
            &Parameters::new()
        )
        .unwrap_err()
        .code(),
        "FDB_PARAMETER"
    );
    assert_eq!(q(&c, "SELECT a,b FROM docs ORDER BY a").rows, before);
}

#[test]
fn tuple_select_keeps_each_candidates_typed_snapshot() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    let mut expected = Vec::new();
    for n in 1..=3 {
        let record = Value::Record(Record {
            table: "docs".into(),
            key: Key::Integer(n),
        });
        let bytes = Value::Binary(vec![n as u8, 0, 255]);
        let params = Parameters::from([
            ("$n".into(), Value::Integer(n)),
            ("$a".into(), record.clone()),
            ("$b".into(), bytes.clone()),
        ]);
        c.execute("INSERT INTO docs(n,a,b) VALUES($n,$a,$b)", &params)
            .unwrap();
        expected.push(vec![Value::Integer(n), bytes, record]);
    }
    q(&c, "BEGIN");
    assert_eq!(
        q(&c, "UPDATE docs SET (a,b)=(SELECT b,a) RETURNING n,a,b").rows,
        expected
    );
    assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, expected);
    q(&c, "ROLLBACK");
    for row in &mut expected {
        row.swap(1, 2);
    }
    assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, expected);
}

#[test]
fn sourceful_tuple_select_lookups_match_native_rows() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0),(3,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO lookup VALUES(1,2,3),(2,4,5)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
    ] {
        q(&c, sql);
    }
    q(&c, "CREATE TABLE lookup_docs");
    q(
        &c,
        "INSERT INTO lookup_docs(n,a,b) SELECT n,a,b FROM lookup",
    );
    for source in [
        "lookup",
        "lookup_docs",
        "(SELECT n,a,b FROM lookup)",
        "(SELECT n,a,b FROM lookup_docs)",
    ] {
        q(&c, "BEGIN");
        let expected=q(&c,"UPDATE native SET (a,b)=(SELECT x.a,x.b FROM lookup x WHERE x.n=native.n) RETURNING n,a,b");
        assert_eq!(q(&c,&format!("UPDATE docs SET (a,b)=(SELECT x.a,x.b FROM {source} x WHERE x.n=docs.n) RETURNING n,a,b")).rows,expected.rows);
        q(&c, "ROLLBACK");
    }
}

#[test]
fn tuple_lookup_validation_restores_indexes_and_allows_retry() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) VALUES(1,1,0),(2,2,0)",
        "DEFINE FIELD a ON docs TYPE integer CHECK(a<10)",
        "CREATE UNIQUE INDEX docs_a ON docs(a)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO lookup VALUES(1,7,8),(2,11,12)",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a,b) SELECT n,a,b FROM lookup",
    ] {
        q(&c, sql);
    }
    q(&c, "INSERT INTO lookup VALUES(2,6,9)");
    q(&c, "INSERT INTO lookup_docs(n,a,b) VALUES(2,6,9)");
    for source in ["lookup", "lookup_docs"] {
        for (local, aliases, distinct) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, false),
            (false, false, true),
            (true, true, true),
        ] {
            q(&c, "BEGIN");
            q(&c, "INSERT INTO docs(n,a,b) VALUES(0,0,0)");
            let before = q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows;
            let sql=format!("UPDATE docs SET (a,b)=(SELECT x.a,x.b FROM {source} x WHERE x.n=docs.n ORDER BY x.a DESC LIMIT 1) WHERE n>0 RETURNING a,b");
            let sql = if local {
                sql.replace(
                    &format!("SELECT x.a,x.b FROM {source} x"),
                    &format!(
                        "WITH chosen AS (SELECT n,a,b FROM {source}) SELECT x.a,x.b FROM chosen x"
                    ),
                )
            } else {
                sql
            };
            let sql = if aliases {
                sql.replace(
                    "SELECT x.a,x.b",
                    "SELECT x.a AS first_value,x.b AS second_value",
                )
            } else {
                sql
            };
            let sql = if distinct {
                sql.replace("SELECT x.a", "SELECT DISTINCT x.a")
            } else {
                sql
            };
            let error = c.execute(&sql, &Parameters::new()).unwrap_err();
            assert_eq!(error.code(), "FDB_VALIDATION", "{sql}: {error:?}");
            assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
            assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, before);
            assert!(c
                .lookup_index("docs", "docs_a", &Value::Integer(7))
                .unwrap()
                .is_empty());
            let ordered_retry = sql.replace("ORDER BY x.a DESC", "ORDER BY x.a ASC");
            assert_eq!(
                q(&c, &ordered_retry).rows,
                vec![
                    vec![Value::Integer(7), Value::Integer(8)],
                    vec![Value::Integer(6), Value::Integer(9)]
                ]
            );
            q(&c, "UPDATE docs SET (a,b)=(n,0) WHERE n>0");
            let retry = sql.replace("WHERE n>0", "WHERE n=1");
            assert_eq!(
                q(&c, &retry).rows,
                vec![vec![Value::Integer(7), Value::Integer(8)]]
            );
            assert_eq!(
                c.check_collection_integrity("docs", Default::default())
                    .unwrap()
                    .index_entries,
                3
            );
            q(&c, "ROLLBACK");
            assert_eq!(
                q(&c, "SELECT a,b FROM docs ORDER BY n").rows,
                vec![
                    vec![Value::Integer(1), Value::Integer(0)],
                    vec![Value::Integer(2), Value::Integer(0)]
                ]
            );
        }
    }
}

#[test]
fn tuple_self_lookups_materialize_before_any_update() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(
        &c,
        "INSERT INTO docs(n,a,b) VALUES(1,10,11),(2,20,21),(3,30,31)",
    );
    q(&c, "CREATE UNIQUE INDEX docs_a ON docs(a)");
    q(&c, "BEGIN");
    let result=q(&c,"UPDATE docs SET (a,b)=(SELECT x.a+100,x.b+100 FROM docs x WHERE x.n=CASE docs.n WHEN 1 THEN 3 ELSE docs.n-1 END) RETURNING n,a,b");
    let expected = vec![
        vec![Value::Integer(1), Value::Integer(130), Value::Integer(131)],
        vec![Value::Integer(2), Value::Integer(110), Value::Integer(111)],
        vec![Value::Integer(3), Value::Integer(120), Value::Integer(121)],
    ];
    assert_eq!(result.rows, expected);
    assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, expected);
    assert_eq!(
        c.check_collection_integrity("docs", Default::default())
            .unwrap()
            .index_entries,
        3
    );
    q(&c, "ROLLBACK");
    assert_eq!(
        q(&c, "SELECT a FROM docs ORDER BY n").rows,
        vec![
            vec![Value::Integer(10)],
            vec![Value::Integer(20)],
            vec![Value::Integer(30)]
        ]
    );
}

#[test]
fn tuple_lookup_multiple_matches_keep_columns_from_one_row() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO lookup VALUES(1,10,11),(1,20,21),(2,30,31)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
    ] {
        q(&c, sql);
    }
    for tuple in [
        "(SELECT docs.a,docs.b FROM lookup docs WHERE docs.n=2)",
        "(SELECT x.a,x.b FROM lookup x WHERE x.n=2)",
    ] {
        q(&c, "BEGIN");
        let native_tuple = tuple
            .replace("docs.", "x.")
            .replace("lookup docs", "lookup x");
        let expected = q(
            &c,
            &format!("UPDATE native SET (a,b)={native_tuple} RETURNING n,a,b"),
        );
        assert_eq!(
            q(
                &c,
                &format!("UPDATE docs SET (a,b)={tuple} RETURNING n,a,b")
            )
            .rows,
            expected.rows,
            "{tuple}"
        );
        q(&c, "ROLLBACK");
    }
    q(&c, "CREATE TABLE lookup_docs");
    q(
        &c,
        "INSERT INTO lookup_docs(n,a,b) SELECT n,a,b FROM lookup",
    );
    for ordering in ["x.a DESC", "x.a+docs.n DESC"] {
        q(&c, "BEGIN");
        let native_order = ordering.replace("docs.n", "native.n");
        let expected=q(&c,&format!("UPDATE native SET (a,b)=(SELECT x.a,x.b FROM lookup x WHERE x.n=native.n ORDER BY {native_order} LIMIT 1) RETURNING n,a,b"));
        assert_eq!(q(&c,&format!("UPDATE docs SET (a,b)=(SELECT x.a,x.b FROM lookup_docs x WHERE x.n=docs.n ORDER BY {ordering} LIMIT 1) RETURNING n,a,b")).rows,expected.rows);
        q(&c, "ROLLBACK");
    }
    for page in [
        "",
        " LIMIT 1",
        " LIMIT 1 OFFSET 1",
        " LIMIT 1 OFFSET 3",
        " ORDER BY x.a DESC LIMIT 1",
        " ORDER BY x.a+x.b DESC LIMIT 1 OFFSET 1",
    ] {
        q(&c, "BEGIN");
        let query = |target| {
            format!("UPDATE {target} SET (a,b)=(SELECT x.a,x.b FROM lookup x WHERE x.n={target}.n{page}) RETURNING n,a,b")
        };
        let expected = q(&c, &query("native"));
        let actual = q(&c, &query("docs"));
        assert_eq!(actual.rows, expected.rows, "{page}");
        assert_eq!(actual.affected, expected.affected);
        q(&c, "ROLLBACK");
    }
}

#[test]
fn tuple_iterator_sources_match_native_values_and_empty_rows() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,j TEXT,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,'[4,5]',0,0),(2,'[]',0,0)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,j,a,b) SELECT n,j,a,b FROM native",
    ] {
        q(&c, sql);
    }
    for iterator in ["json_each", "main.json_each", "json_tree"] {
        q(&c, "BEGIN");
        let query = |target| {
            format!("UPDATE {target} SET (a,b)=(SELECT x.key,x.value FROM {iterator}({target}.j) x WHERE x.type='integer') RETURNING n,a,b")
        };
        let expected = q(&c, &query("native"));
        assert_eq!(q(&c, &query("docs")).rows, expected.rows, "{iterator}");
        q(&c, "ROLLBACK");
    }
}

#[test]
fn ordered_tuple_lookup_collation_and_nulls_match_native() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a TEXT,b INTEGER)",
        "INSERT INTO native VALUES(1,NULL,0)",
        "CREATE TABLE lookup(a TEXT,b INTEGER)",
        "INSERT INTO lookup VALUES('a',1),('B',2),(NULL,3)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(a,b) SELECT a,b FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for order in [
            "x.a COLLATE BINARY",
            "x.a COLLATE NOCASE",
            "x.a COLLATE NOCASE DESC",
            "x.a NULLS LAST",
            "x.a DESC NULLS FIRST",
        ] {
            q(&c, "BEGIN");
            let expected=q(&c,&format!("UPDATE native SET (a,b)=(SELECT x.a,x.b FROM lookup x ORDER BY {order} LIMIT 1) RETURNING a,b"));
            assert_eq!(q(&c,&format!("UPDATE docs SET (a,b)=(SELECT x.a,x.b FROM {source} x ORDER BY {order} LIMIT 1) RETURNING a,b")).rows,expected.rows,"{source}: {order}");
            q(&c, "ROLLBACK");
        }
    }
}

#[test]
fn ordered_tuple_lookup_reuses_bound_projection_and_pagination_values() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs(n) VALUES(1)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER)",
        "INSERT INTO lookup VALUES(1,10),(1,20)",
    ] {
        q(&c, sql);
    }
    let sql="UPDATE docs SET (a,b)=(SELECT x.a+$limit,$offset FROM lookup x WHERE x.n=docs.n ORDER BY x.a DESC LIMIT $limit OFFSET $offset) RETURNING a,b";
    for (limit, offset, expected) in [
        (1, 0, vec![Value::Integer(21), Value::Integer(0)]),
        (1, 1, vec![Value::Integer(11), Value::Integer(1)]),
        (0, 0, vec![Value::Null, Value::Null]),
        (1, 2, vec![Value::Null, Value::Null]),
    ] {
        q(&c, "BEGIN");
        let params = Parameters::from([
            ("$limit".into(), Value::Integer(limit)),
            ("$offset".into(), Value::Integer(offset)),
        ]);
        assert_eq!(c.execute(sql, &params).unwrap().rows, vec![expected]);
        q(&c, "ROLLBACK");
    }
    assert_eq!(
        c.execute(sql, &Parameters::new()).unwrap_err().code(),
        "FDB_PARAMETER"
    );
    assert_eq!(
        q(&c, "SELECT a,b FROM docs").rows,
        vec![vec![Value::Null, Value::Null]]
    );
}

#[test]
fn tuple_join_sources_preserve_outer_join_nulls() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0),(3,0,0)",
        "CREATE TABLE lhs(n INTEGER,a INTEGER)",
        "INSERT INTO lhs VALUES(1,10),(2,20)",
        "CREATE TABLE rhs(n INTEGER,b INTEGER)",
        "INSERT INTO rhs VALUES(1,11)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lhs_docs",
        "INSERT INTO lhs_docs(n,a) SELECT n,a FROM lhs",
    ] {
        q(&c, sql);
    }
    for join in ["JOIN", "LEFT JOIN"] {
        for source in ["lhs", "lhs_docs"] {
            for projection in ["x.a,y.b", "x.a+1,coalesce(y.b,-1)"] {
                q(&c, "BEGIN");
                let expected=q(&c,&format!("UPDATE native SET (a,b)=(SELECT {projection} FROM lhs x {join} rhs y ON y.n=x.n WHERE x.n=native.n) RETURNING n,a,b"));
                assert_eq!(q(&c,&format!("UPDATE docs SET (a,b)=(SELECT {projection} FROM {source} x {join} rhs y ON y.n=x.n WHERE x.n=docs.n) RETURNING n,a,b")).rows,expected.rows,"{source}: {join}");
                q(&c, "ROLLBACK");
            }
        }
    }
}

#[test]
fn tuple_local_ctes_supply_correlated_lookup_rows() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO lookup VALUES(1,4,5)",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a,b) SELECT n,a,b FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for definition in [
            "chosen AS (SELECT n,a,b FROM SOURCE)",
            "base(k,v,w) AS (SELECT n,a,b FROM SOURCE), chosen AS (SELECT k AS n,v AS a,w AS b FROM base)",
        ] {
        q(&c, "BEGIN");
        let expected=q(&c,"UPDATE native SET (a,b)=(WITH chosen AS (SELECT n,a,b FROM lookup) SELECT x.a,x.b FROM chosen x WHERE x.n=native.n) RETURNING n,a,b");
        let definition=definition.replace("SOURCE",source);
        assert_eq!(q(&c,&format!("UPDATE docs SET (a,b)=(WITH {definition} SELECT x.a,x.b FROM chosen x WHERE x.n=docs.n) RETURNING n,a,b")).rows,expected.rows);
        q(&c, "ROLLBACK");
        }
    }
}

#[test]
fn tuple_lookup_aliases_match_native() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO lookup VALUES(1,6,9),(1,11,8),(2,4,7)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a,b) SELECT n,a,b FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for (projection, order, expected) in [
            ("x.a,x.b", "1 DESC", [[1, 11, 8], [2, 4, 7]]),
            ("x.a,x.b", "2 DESC", [[1, 6, 9], [2, 4, 7]]),
            (
                "x.a AS chosen,x.b AS other",
                "2 DESC",
                [[1, 6, 9], [2, 4, 7]],
            ),
            (
                "x.a AS chosen,x.b AS other",
                "chosen DESC",
                [[1, 11, 8], [2, 4, 7]],
            ),
            ("-x.a AS a,x.b other", "a DESC", [[1, -6, 9], [2, -4, 7]]),
            (
                "x.a AS chosen,x.b",
                "chosen + x.b DESC",
                [[1, 11, 8], [2, 4, 7]],
            ),
            (
                "x.a AS chosen,x.b AS chosen",
                "chosen DESC",
                [[1, 11, 8], [2, 4, 7]],
            ),
            (
                "x.a AS fastdb_tuple_values_0,x.b AS other",
                "fastdb_tuple_values_0 DESC",
                [[1, 11, 8], [2, 4, 7]],
            ),
        ] {
            q(&c, "BEGIN");
            let native = q(&c, &format!("UPDATE native SET (a,b)=(SELECT {projection} FROM lookup x WHERE x.n=native.n ORDER BY {order} LIMIT 1) RETURNING n,a,b"));
            assert_eq!(
                native.rows,
                expected
                    .into_iter()
                    .map(|row| row.into_iter().map(Value::Integer).collect::<Vec<_>>())
                    .collect::<Vec<_>>()
            );
            let sql = format!("UPDATE docs SET (a,b)=(SELECT {projection} FROM {source} x WHERE x.n=docs.n ORDER BY {order} LIMIT 1) RETURNING n,a,b");
            assert_eq!(q(&c, &sql).rows, native.rows, "{sql}");
            q(&c, "ROLLBACK");
        }
        for order in ["0", "3", "-1"] {
            q(&c, "BEGIN");
            let before = q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows;
            let native = c
                .execute(
                    &format!(
                        "UPDATE native SET (a,b)=(SELECT x.a,x.b FROM lookup x ORDER BY {order})"
                    ),
                    &Parameters::new(),
                )
                .unwrap_err();
            let error = c
                .execute(
                    &format!(
                        "UPDATE docs SET (a,b)=(SELECT x.a,x.b FROM {source} x ORDER BY {order})"
                    ),
                    &Parameters::new(),
                )
                .unwrap_err();
            assert_eq!(native.code(), "FDB_ENGINE");
            assert_eq!(error.code(), "FDB_VALIDATION", "{source}: {order}");
            assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, before);
            q(&c, "ROLLBACK");
        }
    }
}

#[test]
fn source_free_tuple_positions_preserve_candidates_and_empty_rows() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,6,11),(2,8,9)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
    ] {
        q(&c, sql);
    }
    for order in ["1", "2 DESC", "(2)"] {
        for tail in ["", " LIMIT 0", " LIMIT 1 OFFSET 1"] {
            q(&c, "BEGIN");
            let expected = q(&c, &format!("UPDATE native SET (a,b)=(SELECT b,a WHERE n=1 ORDER BY {order}{tail}) RETURNING n,a,b"));
            let sql = format!("UPDATE docs SET (a,b)=(SELECT b,a WHERE n=1 ORDER BY {order}{tail}) RETURNING n,a,b");
            assert_eq!(q(&c, &sql).rows, expected.rows, "{sql}");
            q(&c, "ROLLBACK");
        }
    }
}

#[test]
fn source_free_tuple_aliases_match_native_scope() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,6,11),(2,-8,9)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
    ] {
        q(&c, sql);
    }
    for (projection, predicate, order) in [
        ("b AS chosen,a AS other", "chosen>0 AND n=1", "chosen"),
        ("-a AS a,b AS b", "TARGET.a>0", "a DESC"),
        ("b AS chosen,a AS chosen", "chosen>0", "2"),
        ("b AS chosen,a", "chosen IN (11)", "chosen+a"),
    ] {
        q(&c, "BEGIN");
        let native_predicate = predicate.replace("TARGET", "native");
        let expected = q(&c, &format!("UPDATE native SET (a,b)=(SELECT {projection} WHERE {native_predicate} ORDER BY {order}) RETURNING n,a,b"));
        let predicate = predicate.replace("TARGET", "docs");
        let sql = format!("UPDATE docs SET (a,b)=(SELECT {projection} WHERE {predicate} ORDER BY {order}) RETURNING n,a,b");
        assert_eq!(q(&c, &sql).rows, expected.rows, "{sql}");
        q(&c, "ROLLBACK");
    }
    // Source-free logical SELECTs bind explicit aliases locally. Qualify a
    // document field when its name collides with an alias.
    q(&c, "BEGIN");
    assert_eq!(
        q(
            &c,
            "UPDATE docs SET (a,b)=(SELECT -a AS a,b AS b WHERE a>0 ORDER BY a) RETURNING n,a,b"
        )
        .rows,
        vec![
            vec![Value::Integer(1), Value::Null, Value::Null],
            vec![Value::Integer(2), Value::Integer(8), Value::Integer(9)]
        ]
    );
    q(&c, "ROLLBACK");
}

#[test]
fn distinct_tuple_lookups_deduplicate_before_pagination() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER,b INTEGER)",
        "INSERT INTO lookup VALUES(1,6,9),(1,6,9),(1,11,8),(1,11,8)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a,b) SELECT n,a,b FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for offset in [0, 1, 2] {
            q(&c, "BEGIN");
            let native = q(&c, &format!("UPDATE native SET (a,b)=(SELECT DISTINCT x.a,x.b FROM lookup x WHERE x.n=native.n ORDER BY 1 LIMIT 1 OFFSET {offset}) RETURNING n,a,b"));
            let sql = format!("UPDATE docs SET (a,b)=(SELECT DISTINCT x.a,x.b FROM {source} x WHERE x.n=docs.n ORDER BY 1 LIMIT 1 OFFSET {offset}) RETURNING n,a,b");
            assert_eq!(q(&c, &sql).rows, native.rows, "{sql}");
            q(&c, "ROLLBACK");
        }
    }
    q(&c, "BEGIN");
    let native = q(
        &c,
        "UPDATE native SET (a,b)=(SELECT DISTINCT b,a WHERE n=1) RETURNING n,a,b",
    );
    assert_eq!(
        q(
            &c,
            "UPDATE docs SET (a,b)=(SELECT DISTINCT b,a WHERE n=1) RETURNING n,a,b"
        )
        .rows,
        native.rows
    );
    q(&c, "ROLLBACK");
}

#[test]
fn aggregate_tuple_lookups_match_native_empty_and_grouped_rows() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a,b)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER)",
        "INSERT INTO lookup VALUES(1,6),(1,11),(1,6)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a) SELECT n,a FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for projection in [
            "sum(x.a),count(*)",
            "sum(x.a) FILTER(WHERE x.a>7),count(*) FILTER(WHERE x.a>7)",
            "sum(x.a) FILTER(WHERE x.a>TARGET.n*7),count(*) FILTER(WHERE x.a>TARGET.n*7)",
            "avg(x.a),total(x.a)",
            "min(x.a),max(x.a)",
            "sum(DISTINCT x.a),count(DISTINCT x.a)",
            "coalesce(sum(x.a),0)+1,count(*)*2",
        ] {
            for group in ["", " GROUP BY x.n", " GROUP BY x.n HAVING count(*)>3"] {
                q(&c, "BEGIN");
                let native_projection = projection.replace("TARGET", "native");
                let native = q(&c, &format!("UPDATE native SET (a,b)=(SELECT {native_projection} FROM lookup x WHERE x.n=native.n{group}) RETURNING n,a,b"));
                let projection = projection.replace("TARGET", "docs");
                let sql=format!("UPDATE docs SET (a,b)=(SELECT {projection} FROM {source} x WHERE x.n=docs.n{group}) RETURNING n,a,b");
                assert_eq!(q(&c, &sql).rows, native.rows, "{sql}");
                q(&c, "ROLLBACK");
            }
        }
        for offset in [0, 1, 2] {
            q(&c, "BEGIN");
            let native=q(&c,&format!("UPDATE native SET (a,b)=(SELECT sum(x.a) AS total_value,count(*) AS amount FROM lookup x WHERE x.n=native.n GROUP BY x.a HAVING amount>0 ORDER BY total_value DESC LIMIT 1 OFFSET {offset}) RETURNING n,a,b"));
            let sql=format!("UPDATE docs SET (a,b)=(SELECT sum(x.a) AS total_value,count(*) AS amount FROM {source} x WHERE x.n=docs.n GROUP BY x.a HAVING amount>0 ORDER BY total_value DESC LIMIT 1 OFFSET {offset}) RETURNING n,a,b");
            assert_eq!(q(&c, &sql).rows, native.rows, "{sql}");
            q(&c, "ROLLBACK");
        }
    }
}

#[test]
fn aggregate_tuple_validation_restores_indexes_and_allows_retry() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) VALUES(1,1,0),(2,2,0)",
        "DEFINE FIELD a ON docs TYPE integer CHECK(a<10)",
        "CREATE UNIQUE INDEX docs_a ON docs(a)",
        "CREATE TABLE lookup",
        "INSERT INTO lookup(n,a) VALUES(1,3),(1,4),(2,5),(2,6)",
    ] {
        q(&c, sql);
    }
    for (filtered, nested, windowed) in [
        (false, false, false),
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        q(&c, "BEGIN");
        q(&c, "INSERT INTO docs(n,a,b) VALUES(0,0,0)");
        let before = q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows;
        let sql="UPDATE docs SET (a,b)=(SELECT sum(x.a),count(*) FROM lookup x WHERE x.n=docs.n) WHERE n>0 RETURNING a,b";
        let sql = if filtered {
            sql.replace(
                "sum(x.a),count(*)",
                "sum(x.a) FILTER(WHERE x.a>docs.n),count(*) FILTER(WHERE x.a>docs.n)",
            )
        } else {
            sql.to_owned()
        };
        let sql = if nested {
            "UPDATE docs SET (a,b)=(SELECT (SELECT sum(x.a) FROM lookup x WHERE x.n=docs.n),(SELECT count(*) FROM lookup x WHERE x.n=docs.n)) WHERE n>0 RETURNING a,b".to_owned()
        } else {
            sql
        };
        let sql = if windowed {
            sql.replace("sum(x.a),count(*)", "sum(x.a) OVER(),count(*) OVER()")
        } else {
            sql
        };
        assert_eq!(
            c.execute(&sql, &Parameters::new()).unwrap_err().code(),
            "FDB_VALIDATION"
        );
        assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, before);
        assert!(c
            .lookup_index("docs", "docs_a", &Value::Integer(7))
            .unwrap()
            .is_empty());
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        let retry = sql.replace("x.n=docs.n", "x.n=docs.n AND x.a<6");
        assert_eq!(
            q(&c, &retry).rows,
            vec![
                vec![Value::Integer(7), Value::Integer(2)],
                vec![Value::Integer(5), Value::Integer(1)]
            ]
        );
        assert_eq!(
            c.check_collection_integrity("docs", Default::default())
                .unwrap()
                .index_entries,
            3
        );
        q(&c, "ROLLBACK");
        assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, before[1..]);
    }
}

#[test]
fn filtered_tuple_parameters_validate_before_mutation() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup",
        "INSERT INTO lookup(n,a) VALUES(1,6),(1,11)",
    ] {
        q(&c, sql);
    }
    let sql="UPDATE docs SET (a,b)=(SELECT sum(x.a) FILTER(WHERE x.a>$minimum),count(*) FILTER(WHERE x.a>$minimum) FROM lookup x WHERE x.n=docs.n) RETURNING n,a,b";
    q(&c, "BEGIN");
    let before = q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows;
    assert_eq!(
        c.execute(sql, &Parameters::new()).unwrap_err().code(),
        "FDB_PARAMETER"
    );
    assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, before);
    for (minimum, sum, count) in [
        (0, Value::Integer(17), 2),
        (7, Value::Integer(11), 1),
        (20, Value::Null, 0),
    ] {
        let params = Parameters::from([("$minimum".into(), Value::Integer(minimum))]);
        assert_eq!(
            c.execute(sql, &params).unwrap().rows,
            vec![
                vec![Value::Integer(1), sum, Value::Integer(count)],
                vec![Value::Integer(2), Value::Null, Value::Integer(0)]
            ]
        );
    }
    q(&c, "ROLLBACK");
    assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, before);
}

#[test]
fn tuple_aggregate_filter_subqueries_match_native() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a,b)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER)",
        "INSERT INTO lookup VALUES(1,6),(1,11)",
        "CREATE TABLE allowed(n INTEGER,a INTEGER)",
        "INSERT INTO allowed VALUES(1,11)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a) SELECT n,a FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for filter in [
            "x.a>(SELECT 7)",
            "x.a IN (SELECT a FROM allowed)",
            "EXISTS(SELECT 1 FROM allowed y WHERE y.a=x.a AND y.n=TARGET.n)",
        ] {
            q(&c, "BEGIN");
            let native_filter = filter.replace("TARGET", "native");
            let expected=q(&c,&format!("UPDATE native SET (a,b)=(SELECT sum(x.a) FILTER(WHERE {native_filter}),count(*) FILTER(WHERE {native_filter}) FROM lookup x WHERE x.n=native.n) RETURNING n,a,b"));
            let filter = filter.replace("TARGET", "docs");
            let sql=format!("UPDATE docs SET (a,b)=(SELECT sum(x.a) FILTER(WHERE {filter}),count(*) FILTER(WHERE {filter}) FROM {source} x WHERE x.n=docs.n) RETURNING n,a,b");
            assert_eq!(q(&c, &sql).rows, expected.rows, "{sql}");
            q(&c, "ROLLBACK");
        }
    }
}

#[test]
fn nested_tuple_projections_match_native() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a,b)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER)",
        "INSERT INTO lookup VALUES(1,6),(1,11)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a) SELECT n,a FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for projection in [
            "(SELECT max(x.a) FROM SOURCE x WHERE x.n=TARGET.n),TARGET.n+1",
            "EXISTS(SELECT 1 FROM SOURCE x WHERE x.n=TARGET.n),TARGET.n IN (SELECT n FROM SOURCE)",
        ] {
            q(&c, "BEGIN");
            let native_projection = projection
                .replace("SOURCE", "lookup")
                .replace("TARGET", "native");
            let expected = q(
                &c,
                &format!("UPDATE native SET (a,b)=(SELECT {native_projection}) RETURNING n,a,b"),
            );
            let projection = projection
                .replace("SOURCE", source)
                .replace("TARGET", "docs");
            let sql = format!("UPDATE docs SET (a,b)=(SELECT {projection}) RETURNING n,a,b");
            assert_eq!(q(&c, &sql).rows, expected.rows, "{sql}");
            q(&c, "ROLLBACK");
        }
    }
}

#[test]
fn window_tuple_projections_match_native() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a,b)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER)",
        "INSERT INTO lookup VALUES(1,6),(1,11)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a) SELECT n,a FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for (projection, window) in [
            ("row_number() OVER(ORDER BY x.a),sum(x.a) OVER()", ""),
            (
                "sum(x.a) FILTER(WHERE x.a>7) OVER(),count(*) FILTER(WHERE x.a>7) OVER()",
                "",
            ),
            (
                "row_number() OVER w,count(*) OVER w",
                " WINDOW w AS (ORDER BY x.a)",
            ),
        ] {
            for offset in [0, 1, 2] {
                q(&c, "BEGIN");
                let expected=q(&c,&format!("UPDATE native SET (a,b)=(SELECT {projection} FROM lookup x WHERE x.n=native.n{window} ORDER BY x.a DESC LIMIT 1 OFFSET {offset}) RETURNING n,a,b"));
                let sql=format!("UPDATE docs SET (a,b)=(SELECT {projection} FROM {source} x WHERE x.n=docs.n{window} ORDER BY x.a DESC LIMIT 1 OFFSET {offset}) RETURNING n,a,b");
                assert_eq!(q(&c, &sql).rows, expected.rows, "{sql}");
                q(&c, "ROLLBACK");
            }
        }
        for offset in [0, 1] {
            q(&c, "BEGIN");
            let expected=q(&c,&format!("UPDATE native SET (a,b)=(SELECT DISTINCT sum(x.a) OVER(),count(*) OVER() FROM lookup x WHERE x.n=native.n ORDER BY 1 LIMIT 1 OFFSET {offset}) RETURNING n,a,b"));
            let sql=format!("UPDATE docs SET (a,b)=(SELECT DISTINCT sum(x.a) OVER(),count(*) OVER() FROM {source} x WHERE x.n=docs.n ORDER BY 1 LIMIT 1 OFFSET {offset}) RETURNING n,a,b");
            assert_eq!(q(&c, &sql).rows, expected.rows, "{sql}");
            q(&c, "ROLLBACK");
        }
    }
}

#[test]
fn window_tuple_parameters_preserve_binding_and_rollback() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup",
        "INSERT INTO lookup(n,a) VALUES(1,6),(1,11)",
    ] {
        q(&c, sql);
    }
    let sql="UPDATE docs SET (a,b)=(SELECT row_number() OVER(ORDER BY x.a)+$delta,sum(x.a+$delta) OVER() FROM lookup x WHERE x.n=docs.n ORDER BY x.a DESC LIMIT 1) RETURNING n,a,b";
    q(&c, "BEGIN");
    let before = q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows;
    assert_eq!(
        c.execute(sql, &Parameters::new()).unwrap_err().code(),
        "FDB_PARAMETER"
    );
    assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, before);
    for delta in [0, 3, -2] {
        let params = Parameters::from([("$delta".into(), Value::Integer(delta))]);
        assert_eq!(
            c.execute(sql, &params).unwrap().rows,
            vec![
                vec![
                    Value::Integer(1),
                    Value::Integer(2 + delta),
                    Value::Integer(17 + 2 * delta)
                ],
                vec![Value::Integer(2), Value::Null, Value::Null]
            ]
        );
    }
    q(&c, "ROLLBACK");
    assert_eq!(q(&c, "SELECT n,a,b FROM docs ORDER BY n").rows, before);
}

#[test]
fn compound_tuple_selects_match_native() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for sql in [
        "CREATE TABLE native(n INTEGER,a,b)",
        "INSERT INTO native VALUES(1,0,0),(2,0,0)",
        "CREATE TABLE lookup(n INTEGER,a INTEGER)",
        "INSERT INTO lookup VALUES(1,6),(1,11)",
        "CREATE TABLE docs",
        "INSERT INTO docs(n,a,b) SELECT n,a,b FROM native",
        "CREATE TABLE lookup_docs",
        "INSERT INTO lookup_docs(n,a) SELECT n,a FROM lookup",
    ] {
        q(&c, sql);
    }
    for source in ["lookup", "lookup_docs"] {
        for operator in ["UNION ALL", "UNION", "INTERSECT", "EXCEPT"] {
            q(&c, "BEGIN");
            let body=format!("SELECT x.a,x.n FROM SOURCE x WHERE x.n=TARGET.n {operator} SELECT y.a,y.n FROM SOURCE y WHERE y.n=TARGET.n AND y.a=6 ORDER BY 1 DESC LIMIT 1");
            let native = body.replace("SOURCE", "lookup").replace("TARGET", "native");
            let expected = q(
                &c,
                &format!("UPDATE native SET (a,b)=(WITH selected(a,b) AS ({native}) SELECT a,b FROM selected) RETURNING n,a,b"),
            );
            let body = body.replace("SOURCE", source).replace("TARGET", "docs");
            let sql = format!("UPDATE docs SET (a,b)=({body}) RETURNING n,a,b");
            assert_eq!(q(&c, &sql).rows, expected.rows, "{sql}");
            q(&c, "ROLLBACK");
        }
    }
}
