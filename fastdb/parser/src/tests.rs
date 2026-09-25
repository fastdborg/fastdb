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

#[test]
fn spatial_index_declarations_are_additive_and_strict() {
    assert_eq!(parse("CREATE SEARCH INDEX IF NOT EXISTS locations ON places(profile.location) USING SPATIAL;").unwrap(),
        Statement::CreateSpatialIndex { if_not_exists:true, table:"places".into(), name:"locations".into(), path:vec!["profile".into(),"location".into()] });
    for sql in [
        "CREATE SEARCH INDEX i ON places(a,b) USING SPATIAL",
        "CREATE SEARCH INDEX i ON places(a) USING unknown",
        "CREATE SEARCH INDEX i ON places(a) USING SPATIAL extra",
    ] {
        assert!(parse(sql).is_err(), "{sql}");
    }
    assert!(matches!(
        parse("CREATE INDEX i ON places(a)").unwrap(),
        Statement::CreateIndex { .. }
    ));
}

#[test]
fn fulltext_index_declarations_preserve_ordered_paths() {
    assert_eq!(
        parse(
            "CREATE SEARCH INDEX IF NOT EXISTS titles ON posts(title,profile.body) USING FULLTEXT;"
        )
        .unwrap(),
        Statement::CreateFullTextIndex {
            if_not_exists: true,
            table: "posts".into(),
            name: "titles".into(),
            paths: vec![vec!["title".into()], vec!["profile".into(), "body".into()]]
        }
    );
    for sql in [
        "CREATE SEARCH INDEX i ON posts() USING FULLTEXT",
        "CREATE SEARCH INDEX i ON posts(title,) USING FULLTEXT",
        "CREATE SEARCH INDEX i ON posts(title) USING FULLTEXT extra",
        "CREATE SEARCH INDEX i ON posts(title + body) USING FULLTEXT",
    ] {
        assert!(parse(sql).is_err(), "{sql}");
    }
}

#[test]
fn record_brace_projection_grammar_preserves_plain_record_expressions() {
    let Statement::SelectRecordProjection { target, fields } =
        parse("SELECT posts:p1 {title,author.*,profile.city AS city,*,}").unwrap()
    else {
        panic!()
    };
    assert_eq!(target.table, "posts");
    assert_eq!(fields.len(), 4);
    assert_eq!(fields[1].path, vec!["author"]);
    assert!(fields[1].expand);
    assert_eq!(fields[2].alias.as_deref(), Some("city"));
    assert!(fields[3].path.is_empty());
    assert!(matches!(
        parse("SELECT posts:p1 AS ref").unwrap(),
        Statement::Sql(_)
    ));
    for sql in [
        "SELECT posts:p1 {}",
        "SELECT posts:p1 {author {name}}",
        "SELECT posts:p1 {author.*.name}",
        "SELECT posts:p1 {title} WHERE 1",
    ] {
        assert!(parse(sql).is_err(), "{sql}");
    }
}

#[test]
fn declared_inverse_relationship_grammar() {
    assert!(
        matches!(parse("DEFINE RELATION authored ON users FROM posts.meta.author").unwrap(),
        Statement::DefineRelation {path, ..} if path == ["meta", "author"])
    );
    assert!(matches!(
        parse("DROP RELATION IF EXISTS authored").unwrap(),
        Statement::DropRelation {
            if_exists: true,
            ..
        }
    ));
    assert!(
        matches!(parse("INFO FOR RELATION authored").unwrap(), Statement::Info {scope,..} if scope == "relation")
    );
    for sql in [
        "DEFINE RELATION r ON users FROM posts",
        "DEFINE RELATION r ON users FROM posts.a extra",
        "DROP RELATION IF r",
        "DROP RELATION r extra",
    ] {
        assert!(parse(sql).is_err(), "{sql}");
    }
    assert!(matches!(
        parse("CREATE TABLE relation(id TEXT)").unwrap(),
        Statement::Sql(_)
    ));
}

#[test]
fn vector_index_options_are_explicit_unique_and_order_independent() {
    for options in [
        "dimensions=3,metric='cosine'",
        "metric='cosine',dimensions=3",
    ] {
        assert_eq!(parse(&format!("CREATE SEARCH INDEX IF NOT EXISTS embeddings ON items(nested.v) USING VECTOR WITH ({options});")).unwrap(),Statement::CreateVectorIndex {if_not_exists:true,table:"items".into(),name:"embeddings".into(),path:vec!["nested".into(),"v".into()],dimensions:3,metric:"cosine".into()});
    }
    for options in [
        "dimensions=3",
        "metric='l2'",
        "dimensions=3,metric=l2",
        "dimensions=3,dimensions=3,metric='l2'",
        "dimensions=3.5,metric='l2'",
        "dimensions=-1,metric='l2'",
        "dimensions=3,metric='l2',other=1",
    ] {
        assert!(
            parse(&format!(
                "CREATE SEARCH INDEX i ON items(v) USING VECTOR WITH ({options})"
            ))
            .is_err(),
            "{options}"
        );
    }
    assert!(parse(
        "CREATE SEARCH INDEX i ON items(v,w) USING VECTOR WITH (dimensions=3,metric='l2')"
    )
    .is_err());
}

#[test]
fn javascript_function_ddl_keeps_source_and_typed_signatures() {
    assert_eq!(parse("CREATE OR REPLACE FUNCTION App::normalize(value string, fallback string?) RETURNS string? LANGUAGE JAVASCRIPT AS 'return value || fallback;';").unwrap(),Statement::CreateFunction {name:"app::normalize".into(),parameters:vec![("value".into(),"string".into()),("fallback".into(),"string?".into())],returns:"string?".into(),source:"return value || fallback;".into(),replace:true});
    assert_eq!(
        parse("DROP FUNCTION IF EXISTS app::normalize").unwrap(),
        Statement::DropFunction {
            name: "app::normalize".into(),
            if_exists: true
        }
    );
    assert_eq!(
        parse("INFO FOR FUNCTION app::normalize").unwrap(),
        Statement::Info {
            scope: "function".into(),
            name: Some("app::normalize".into())
        }
    );
    for sql in [
        "CREATE FUNCTION f() RETURNS string LANGUAGE JAVASCRIPT AS 'return 1'",
        "CREATE FUNCTION app::f(a) RETURNS string LANGUAGE JAVASCRIPT AS 'return 1'",
        "CREATE FUNCTION app::f() RETURNS string LANGUAGE PYTHON AS 'return 1'",
        "CREATE FUNCTION app::f() RETURNS string LANGUAGE JAVASCRIPT AS 'return 1' extra",
        "DROP FUNCTION IF app::f",
    ] {
        assert!(parse(sql).is_err(), "{sql}");
    }
}
