use fastdb::{Database, Parameters};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn collection_windows_match_pinned_relational_results() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE samples");
    q(&c, "CREATE TABLE baseline (category TEXT, value INTEGER)");
    for (category, value) in [("a", 2), ("a", 4), ("a", 8), ("b", 3)] {
        q(
            &c,
            &format!("INSERT INTO samples {{category:'{category}',value:{value}}}"),
        );
        q(
            &c,
            &format!("INSERT INTO baseline VALUES ('{category}',{value})"),
        );
    }
    for (projection, tail) in [
        (
            "category,value,row_number() OVER (PARTITION BY category ORDER BY value) AS position",
            "",
        ),
        (
            "category,value,sum(value) OVER (PARTITION BY category ORDER BY value ) AS rolling",
            "",
        ),
        (
            "category,value,count(*) OVER (PARTITION BY category) AS n",
            "",
        ),
        (
            "category,value,sum(value) OVER w AS total",
            "WINDOW w AS (PARTITION BY category ORDER BY value)",
        ),
    ] {
        let actual = q(
            &c,
            &format!("SELECT {projection} FROM samples {tail} ORDER BY category,value"),
        );
        let expected = q(
            &c,
            &format!("SELECT {projection} FROM baseline {tail} ORDER BY category,value"),
        );
        assert_eq!(actual.rows, expected.rows, "{projection}");
    }
}
#[test]
fn window_results_flow_through_validated_insert_select() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE samples");
    q(&c, "INSERT INTO samples {value:2}");
    q(&c, "INSERT INTO samples {value:5}");
    q(&c, "CREATE TABLE ranked");
    q(
        &c,
        "DEFINE FIELD position ON ranked TYPE integer CHECK (position < 2)",
    );
    assert!(c
        .execute(
            "INSERT INTO ranked (position) SELECT row_number() OVER (ORDER BY value) FROM samples",
            &Parameters::new()
        )
        .is_err());
    assert!(q(&c, "SELECT * FROM ranked").rows.is_empty());
    // Window functions remain invalid in RETURNING and predicates.
    assert!(c
        .execute(
            "UPDATE samples SET value=3 RETURNING row_number() OVER ()",
            &Parameters::new()
        )
        .is_err());
    assert!(c
        .execute(
            "SELECT value FROM samples WHERE row_number() OVER ()=1",
            &Parameters::new()
        )
        .is_err());
}

#[test]
fn unsupported_frames_match_native_errors_and_local_order_is_rejected() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE samples");
    q(&c, "CREATE TABLE baseline(value INTEGER)");
    let projection = "sum(value) OVER (ORDER BY value ROWS BETWEEN 1 PRECEDING AND CURRENT ROW)";
    let actual = c
        .execute(
            &format!("SELECT {projection} FROM samples"),
            &Parameters::new(),
        )
        .unwrap_err();
    let native = c
        .execute(
            &format!("SELECT {projection} FROM baseline"),
            &Parameters::new(),
        )
        .unwrap_err();
    assert_eq!(actual.to_string(), native.to_string());
    let actual = c
        .execute(
            "SELECT lag(value) OVER (ORDER BY value) FROM samples",
            &Parameters::new(),
        )
        .unwrap_err();
    let native = c
        .execute(
            "SELECT lag(value) OVER (ORDER BY value) FROM baseline",
            &Parameters::new(),
        )
        .unwrap_err();
    assert_eq!(actual.to_string(), native.to_string());

    assert!(c
        .execute(
            "SELECT sum(value ORDER BY value) OVER () FROM samples",
            &Parameters::new()
        )
        .is_err());
}
