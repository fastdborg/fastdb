#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::{Database, StatementResult};

#[test]
fn p13_stop_001_ambient_and_context_calls_fail_before_mutation() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    let stopped_calls = [
        "http::get('http://127.0.0.1/private')",
        "file::get('bucket','secret')",
        "api::req::body()",
        "session::token()",
        "record::exists(person:secret)",
        "eval::surql('RETURN 1')",
        "sequence::next('orders')",
        "sleep(1s)",
    ];

    for (index, call) in stopped_calls.iter().enumerate() {
        let source = format!("CREATE stopped:{index} SET result = {call}");
        let error = connection.execute(&source).unwrap_err();
        let rendered = error.to_string();
        assert!(!rendered.contains("private"));
        assert!(!rendered.contains("secret"));
    }

    let selected = connection.execute("SELECT * FROM stopped").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}

#[test]
fn p13_stop_002_out_of_contract_constants_are_not_values() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    for source in [
        "CREATE stopped:time SET result = time::minimum",
        "CREATE stopped:inf SET result = math::inf",
        "CREATE stopped:neg_inf SET result = math::neg_inf",
    ] {
        assert!(connection.execute(source).is_err());
    }
    let selected = connection.execute("SELECT * FROM stopped").unwrap();
    let StatementResult::Rows(rows) = &selected.statements[0] else {
        panic!("expected rows")
    };
    assert!(rows.is_empty());
}
