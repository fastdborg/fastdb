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
