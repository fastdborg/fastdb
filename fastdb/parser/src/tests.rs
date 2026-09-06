use super::*;
#[test]
fn sql_is_preserved() {
    for sql in [
        "CREATE TABLE posts (id INTEGER PRIMARY KEY);",
        "CREATE TABLE posts AS SELECT 1;",
        "CREATE TABLE posts ();",
        "SELECT r'posts:p1'",
        "SELECT :name, @name, $name::suffix, ?1, ?",
        "SELECT [posts:p1], 'users:u1'",
        "SELECT 1 AS document",
        "SELECT 'a; b' /* ; */;",
    ] {
        assert_eq!(
            parse(sql).expect("SQL dispatch"),
            Statement::Sql(sql.into())
        );
    }
}
#[test]
fn collection_and_records() {
    assert_eq!(
        parse("CREATE TABLE IF NOT EXISTS posts; -- hi").expect("collection"),
        Statement::CreateCollection {
            name: "posts".into(),
            if_not_exists: true
        }
    );
    assert_eq!(
        parse("SELECT posts:`123`;").expect("record"),
        Statement::SelectRecord(Record {
            table: "posts".into(),
            key: Key::String("123".into())
        })
    );
    assert_eq!(
        parse("SELECT posts:-9223372036854775808").expect("record"),
        Statement::SelectRecord(Record {
            table: "posts".into(),
            key: Key::Integer(i64::MIN)
        })
    );
    assert!(parse("SELECT posts:9223372036854775808").is_err());
}
#[test]
fn document_nested_arrays_and_duplicate_keys() {
    let Statement::Insert { value: Expr::Object(fields), .. } = parse("INSERT INTO posts {id: posts:p1, tags: [1, [2, 3], {name: 'a]b'}], profile: {city: 'Bangkok'},} RETURNING *;").expect("nested literal") else { panic!("expected insert"); };
    assert!(matches!(fields["tags"], Expr::Array(_)));
    assert!(parse("INSERT INTO posts {a: 1, a: 2}").is_err());
    assert!(parse("INSERT INTO posts {id: posts:``}").is_err());
}

#[test]
fn interactive_script_completeness_tracks_lexical_and_trigger_boundaries() {
    for sql in [
        "",
        "-- comment\n",
        "SELECT ';';",
        "SELECT 1; /* done */",
        "CREATE TRIGGER t AFTER INSERT ON s BEGIN SELECT CASE WHEN 1 THEN 2 END; END;",
    ] {
        assert!(crate::script_complete(sql).unwrap(), "{sql}");
    }
    for sql in [
        "SELECT 1",
        "SELECT 'unfinished",
        "SELECT 1; /* open",
        "INSERT INTO docs {v:1",
        "CREATE TRIGGER t AFTER INSERT ON s BEGIN SELECT 1;",
    ] {
        assert!(!crate::script_complete(sql).unwrap(), "{sql}");
    }
    assert!(crate::script_complete("SELECT );").is_err());
}

#[test]
fn delegated_sql_has_a_delimiter_depth_limit_before_engine_parsing() {
    let sql = |depth: usize| format!("SELECT {}1{}", "(".repeat(depth), ")".repeat(depth));
    assert!(parse(&sql(64)).is_ok());
    let error = parse(&sql(65)).unwrap_err();
    assert!(error.message.contains("delimiter nesting limit"));
    assert_eq!(error.offset, "SELECT ".len() + 64);
    assert!(parse(&sql(10_000)).is_err());
    for quoted in [
        format!("'{}'", "(".repeat(100)),
        format!("\"{}\"", "(".repeat(100)),
        format!("[{}]", "(".repeat(100)),
    ] {
        assert!(parse(&format!("SELECT {quoted} /* {} */", "(".repeat(100))).is_ok());
    }
}

#[test]
fn tokenizer_bounds_bytes_before_scanning_and_tokens_before_copying() {
    let exact = " ".repeat(MAX_INPUT_BYTES);
    assert!(tokenize(&exact).unwrap().is_empty());
    let error = tokenize(&(exact + "é")).unwrap_err();
    assert_eq!(error.offset, 0);
    assert!(error.message.contains("input byte limit"));
    let exact = "x ".repeat(MAX_TOKENS);
    assert_eq!(
        tokenize(&(exact.clone() + "/* ignored */")).unwrap().len(),
        MAX_TOKENS
    );
    let error = tokenize(&(exact.clone() + "'next'")).unwrap_err();
    assert_eq!(error.offset, exact.len());
    assert!(error.message.contains("input token limit"));
    assert!(split_script(&(exact + "x"))
        .unwrap_err()
        .message
        .contains("input token limit"));
    assert!(tokenize("SELECT 'x x x', /* x x */ 1").is_ok());
}
