#![forbid(unsafe_code)]
#![deny(warnings)]

use tempfile::tempdir;
use turso_fastdb::decode::RegexValue;
use turso_fastdb::{Database, Params, StatementResult, Value};

fn row(result: &StatementResult) -> &std::collections::BTreeMap<String, Value> {
    let StatementResult::Rows(rows) = result else {
        panic!("expected rows")
    };
    let Value::Object(row) = &rows[0] else {
        panic!("expected object row")
    };
    row
}

fn create_fixture(database: &Database) {
    let connection = database.connect().unwrap();
    connection
        .execute(
            "CREATE string_fn:one SET \
             capital = string::capitalize('héLLO world'), \
             concat = string::concat('a',1,NONE,NULL), \
             contains_value = string::contains('abc','b'), \
             ends = string::ends_with('abc','bc'), starts = string::starts_with('abc','ab'), \
             length = string::len('hé😊'), lower = string::lowercase('HÉ'), \
             repeated = string::repeat('ab',3), replaced = string::replace('a-b-a','a','x'), \
             reversed = string::reverse('hé😊'), sliced = string::slice('abcdef',2,3), \
             slug = string::slug('Héllo, World!'), trimmed = string::trim(' x '), \
             upper = string::uppercase('hé'), words = string::words(' hello  world '), \
             split_value = string::split('a,b,c',','), joined = string::join('-','a','b',1), \
             html = string::html::encode('<a href=\"x\">&</a>'), \
             email_user = parse::email::user('a.b+tag@example.com'), \
             email_host = parse::email::host('a.b+tag@example.com'), \
             url_domain = parse::url::domain('https://sub.example.com:8443/a?x=1#f'), \
             url_path = parse::url::path('https://sub.example.com:8443/a?x=1#f'), \
             url_port = parse::url::port('https://sub.example.com:8443/a?x=1#f'), \
             url_query = parse::url::query('https://sub.example.com:8443/a?x=1#f'), \
             url_scheme = parse::url::scheme('https://sub.example.com/a'), \
             distance = string::distance::levenshtein('kitten','sitting'), \
             osa = string::distance::osa('ca','abc'), \
             jaro = string::similarity::jaro('martha','marhta'), \
             semver_cmp = string::semver::compare('1.2.3','1.10.0'), \
             semver_inc = string::semver::inc::major('1.2.3'), \
             semver_set = string::semver::set::minor('1.2.3',9), \
             semver_major = string::semver::major('1.2.3'), \
             valid_alpha = string::is_alpha('abcXYZ'), \
             valid_datetime = string::is_datetime('2024-01-01T00:00:00Z'), \
             valid_domain = string::is_domain('example.com'), \
             valid_email = string::is_email('a@example.com'), \
             valid_ip = string::is_ip('127.0.0.1'), \
             valid_record = string::is_record('person:one'), \
             valid_semver = string::is_semver('1.2.3'), \
             valid_url = string::is_url('https://example.com/a'), \
             valid_uuid = string::is_uuid('0198f166-bd73-7d55-b930-69a43bba69f0')",
        )
        .unwrap();
    connection
        .execute_with_params(
            "UPDATE string_fn:one SET regex_match = string::matches('abc123',$pattern)",
            &Params::from([(
                "pattern".into(),
                Value::Regex(RegexValue::new("[a-z]+").unwrap()),
            )]),
        )
        .unwrap();
}

fn assert_fixture(database: &Database) {
    let selected = database
        .connect()
        .unwrap()
        .execute("SELECT * FROM string_fn:one")
        .unwrap();
    let row = row(&selected.statements[0]);
    for (key, expected) in [
        ("capital", "HéLLO World"),
        ("concat", "a1NONENULL"),
        ("lower", "hé"),
        ("repeated", "ababab"),
        ("replaced", "x-b-x"),
        ("reversed", "😊éh"),
        ("sliced", "c"),
        ("slug", "hello-world"),
        ("trimmed", "x"),
        ("upper", "HÉ"),
        ("joined", "a-b-1"),
        ("email_user", "a.b+tag"),
        ("email_host", "example.com"),
        ("url_domain", "sub.example.com"),
        ("url_path", "/a"),
        ("url_query", "x=1"),
        ("url_scheme", "https"),
        ("semver_inc", "2.0.0"),
        ("semver_set", "1.9.3"),
    ] {
        assert_eq!(row.get(key), Some(&Value::Str(expected.into())), "{key}");
    }
    assert_eq!(row.get("length"), Some(&Value::Integer(3)));
    assert_eq!(row.get("url_port"), Some(&Value::Integer(8443)));
    assert_eq!(row.get("distance"), Some(&Value::Integer(3)));
    assert_eq!(row.get("osa"), Some(&Value::Integer(3)));
    assert_eq!(row.get("semver_cmp"), Some(&Value::Integer(-1)));
    assert_eq!(row.get("semver_major"), Some(&Value::Integer(1)));
    for key in [
        "contains_value",
        "ends",
        "starts",
        "regex_match",
        "valid_alpha",
        "valid_datetime",
        "valid_domain",
        "valid_email",
        "valid_ip",
        "valid_record",
        "valid_semver",
        "valid_url",
        "valid_uuid",
    ] {
        assert_eq!(row.get(key), Some(&Value::Bool(true)), "{key}");
    }
    assert_eq!(
        row.get("words"),
        Some(&Value::Array(vec!["hello".into(), "world".into()]))
    );
    assert_eq!(
        row.get("split_value"),
        Some(&Value::Array(vec!["a".into(), "b".into(), "c".into()]))
    );
    assert_eq!(
        row.get("html"),
        Some(&Value::Str(
            "&lt;a&#32;href&#61;&quot;x&quot;&gt;&amp;&lt;&#47;a&gt;".into()
        ))
    );
    let Value::Float(jaro) = row.get("jaro").unwrap() else {
        panic!("expected float")
    };
    assert!((*jaro - 0.944_444_444_444_444_5).abs() < f64::EPSILON);
}

#[test]
fn p13_fn_015_string_parsing_and_semver_match_reference_in_memory() {
    let database = Database::open_memory().unwrap();
    create_fixture(&database);
    assert_fixture(&database);
}

#[test]
fn p13_fn_016_string_functions_survive_disk_reopen_and_fail_atomically() {
    let directory = tempdir().unwrap();
    let path = directory.path().join("strings.fastdb");
    {
        let database = Database::open(path.to_str().unwrap()).unwrap();
        create_fixture(&database);
    }
    let database = Database::open(path.to_str().unwrap()).unwrap();
    assert_fixture(&database);
    let connection = database.connect().unwrap();
    assert!(connection
        .execute("CREATE bad:one SET value = string::repeat('x',-1)")
        .is_err());
    let selected = connection.execute("SELECT * FROM bad").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
