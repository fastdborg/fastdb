use fastdb::{Database, Error, Parameters, ResultLimits};

#[test]
fn result_limits_cover_native_and_typed_rows_and_preserve_pending_writes() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let p = Parameters::new();
    for sql in [
        "CREATE TABLE native(n INTEGER)",
        "INSERT INTO native VALUES(1),(2)",
        "CREATE TABLE docs;",
        "INSERT INTO docs {n:1}",
        "INSERT INTO docs {n:2}",
        "BEGIN",
        "INSERT INTO native VALUES(3)",
    ] {
        c.execute(sql, &p).unwrap();
    }
    for sql in [
        "SELECT n AS n FROM native WHERE n<3 ORDER BY n",
        "SELECT n AS n FROM docs ORDER BY n",
    ] {
        let expected = c.execute(sql, &p).unwrap();
        let exact = ResultLimits {
            max_rows: 2,
            max_payload_bytes: 17,
        };
        assert_eq!(
            c.select_with_limits(sql, &p, exact).unwrap().rows,
            expected.rows
        );
        assert_eq!(
            c.profile_select_with_limits(sql, &p, exact)
                .unwrap()
                .result
                .rows,
            expected.rows
        );
        for limits in [
            ResultLimits {
                max_rows: 1,
                ..exact
            },
            ResultLimits {
                max_payload_bytes: 16,
                ..exact
            },
            ResultLimits {
                max_rows: 0,
                ..exact
            },
        ] {
            assert!(matches!(
                c.select_with_limits(sql, &p, limits),
                Err(Error::Limit(_))
            ));
        }
    }
    let zero = ResultLimits {
        max_rows: 0,
        max_payload_bytes: 1,
    };
    assert!(c
        .select_with_limits("SELECT n FROM native WHERE 0", &p, zero)
        .unwrap()
        .rows
        .is_empty());
    assert!(matches!(
        c.select_with_limits(
            "SELECT n FROM native WHERE 0",
            &p,
            ResultLimits {
                max_payload_bytes: 0,
                ..zero
            }
        ),
        Err(Error::Limit(_))
    ));
    assert!(matches!(
        c.select_with_limits("DELETE FROM native", &p, zero),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(c.execute("SELECT n FROM native", &p).unwrap().rows.len(), 3);
    c.execute("ROLLBACK", &p).unwrap();
    assert_eq!(c.execute("SELECT n FROM native", &p).unwrap().rows.len(), 2);
}

#[test]
fn fetch_limits_charge_expanded_duplicates_missing_targets_and_other_columns() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let p = Parameters::new();
    for sql in [
        "CREATE TABLE docs",
        "INSERT INTO docs {id:docs:a,n:7}",
        "CREATE TABLE native(id INTEGER PRIMARY KEY,n TEXT)",
        "INSERT INTO native VALUES(1,'猫')",
        "CREATE TABLE positions(n INTEGER)",
        "INSERT INTO positions VALUES(1),(2)",
        "BEGIN",
        "INSERT INTO docs {id:docs:pending,n:9}",
    ] {
        c.execute(sql, &p).unwrap();
    }
    for (sql, bytes) in [
        // v + {id: docs:a, n:7} = 1 + 2+5+1+8.
        ("SELECT record::fetch(docs:a) AS v", 17),
        ("SELECT record::fetch(docs:a) AS v FROM positions", 33),
        // Two fetched columns plus a native integer column.
        (
            "SELECT record::fetch(docs:a) AS v,record::fetch(docs:a) AS w,1 AS n",
            43,
        ),
        // Native target object: id key+integer, n key+UTF-8 value.
        ("SELECT record::fetch(native:1) AS v", 15),
        ("SELECT record::fetch(docs:absent) AS v", 2),
        ("SELECT record::fetch(unknown:absent) AS v", 2),
        ("SELECT record::fetch(NULL) AS v", 2),
        // Reference length must not count when its resolved value is null.
        (
            "SELECT record::fetch(docs:averylongmissingkey) AS v FROM positions",
            3,
        ),
    ] {
        let expected = c.profile_select(sql, &p).unwrap();
        let limits = ResultLimits {
            max_rows: expected.result.rows.len(),
            max_payload_bytes: bytes,
        };
        let actual = c.profile_select_with_limits(sql, &p, limits).unwrap();
        assert_eq!(actual.result.rows, expected.result.rows, "{sql}");
        assert_eq!(
            actual.metrics.fetch_batches, expected.metrics.fetch_batches,
            "{sql}"
        );
        assert!(
            matches!(
                c.select_with_limits(
                    sql,
                    &p,
                    ResultLimits {
                        max_payload_bytes: bytes - 1,
                        ..limits
                    }
                ),
                Err(Error::Limit(_))
            ),
            "{sql}"
        );
        assert!(
            matches!(
                c.select_with_limits(
                    sql,
                    &p,
                    ResultLimits {
                        max_rows: limits.max_rows - 1,
                        ..limits
                    }
                ),
                Err(Error::Limit(_))
            ),
            "{sql}"
        );
        assert_eq!(c.transaction_state(), fastdb::TransactionState::Active);
        assert_eq!(
            c.select_with_limits(sql, &p, limits).unwrap().rows,
            expected.result.rows
        );
    }
    assert_eq!(c.execute("SELECT n FROM docs", &p).unwrap().rows.len(), 2);
    c.execute("ROLLBACK", &p).unwrap();
    assert_eq!(c.execute("SELECT n FROM docs", &p).unwrap().rows.len(), 1);
}
