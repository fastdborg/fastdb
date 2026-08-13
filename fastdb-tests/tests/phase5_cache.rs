#![forbid(unsafe_code)]
#![deny(warnings)]

use turso_fastdb::{Database, Params, Value};

#[test]
fn p5_cache_001_parse_and_prepared_select_caches_are_bounded_and_value_free() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE item:one SET n=1; CREATE item:two SET n=2")
        .unwrap();

    let source = "SELECT * FROM item WHERE n >= $minimum ORDER BY id";
    for minimum in [1, 2, 1] {
        let mut params = Params::new();
        params.insert("minimum".into(), Value::Integer(minimum));
        connection.execute_with_params(source, &params).unwrap();
    }
    let stats = connection.cache_stats().unwrap();
    assert!(stats.parse_hits >= 2, "{stats:?}");
    assert!(stats.prepared_hits >= 2, "{stats:?}");
    assert_eq!(stats.prepared_entries, 1, "values must not enter keys");

    connection
        .execute("CREATE item:three SET n=3 RETURN NONE")
        .unwrap();
    let mut after_write_params = Params::new();
    after_write_params.insert("minimum".into(), Value::Integer(1));
    connection
        .execute_with_params(source, &after_write_params)
        .unwrap();
    assert!(
        connection.cache_stats().unwrap().prepared_hits > stats.prepared_hits,
        "data-only publication must keep prepared SELECT candidates warm"
    );

    for index in 0..140 {
        connection
            .execute(&format!(
                "SELECT * FROM item WHERE n >= {index} ORDER BY id"
            ))
            .unwrap();
    }
    let bounded = connection.cache_stats().unwrap();
    assert_eq!(bounded.parse_entries, 128);
    assert!(bounded.parse_source_bytes <= 4 * 1024 * 1024);
    assert!(bounded.prepared_entries <= 64);

    let oversized = format!("SELECT * FROM item; /*{}*/", "x".repeat(65 * 1024));
    let entries_before = bounded.parse_entries;
    connection.execute(&oversized).unwrap();
    connection.execute(&oversized).unwrap();
    assert_eq!(
        connection.cache_stats().unwrap().parse_entries,
        entries_before,
        "sources over 64 KiB must not enter the parse cache"
    );
}

#[test]
fn p5_cache_002_transactions_schema_publication_and_errors_invalidate_safely() {
    let database = Database::open_memory().unwrap();
    let connection = database.connect().unwrap();
    connection.execute("CREATE item:one SET n=1").unwrap();
    connection.execute("SELECT * FROM item WHERE n=1").unwrap();
    let primed = connection.cache_stats().unwrap();
    assert_eq!(primed.prepared_entries, 1);

    connection.execute("BEGIN").unwrap();
    connection.execute("SELECT * FROM item WHERE n=1").unwrap();
    connection.execute("CANCEL").unwrap();
    let after_transaction = connection.cache_stats().unwrap();
    assert_eq!(after_transaction.prepared_hits, primed.prepared_hits);

    connection
        .execute("DEFINE INDEX by_n ON item FIELDS n")
        .unwrap();
    connection.execute("SELECT * FROM item WHERE n=1").unwrap();
    let after_schema = connection.cache_stats().unwrap();
    assert!(after_schema.prepared_misses > primed.prepared_misses);
    assert_eq!(after_schema.prepared_entries, 1);

    connection.execute("SELECT FROM item").unwrap_err();
    let invalidated = connection.cache_stats().unwrap();
    assert_eq!(invalidated.parse_entries, 0);
    assert_eq!(invalidated.prepared_entries, 0);
}
