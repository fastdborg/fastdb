use fastdb::{CancellationToken, Database, Parameters, TransactionState, Value};
use std::time::{Duration, Instant};

#[test]
fn expired_deadline_rejects_before_parsing_and_preserves_transaction() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let p = Parameters::new();
    c.execute("CREATE TABLE docs", &p).unwrap();
    c.execute("BEGIN", &p).unwrap();
    c.execute("INSERT INTO docs {n:1}", &p).unwrap();
    let token = CancellationToken::with_deadline(Instant::now());
    assert!(token.clone().is_cancelled());
    assert_eq!(
        c.execute_cancellable("invalid SQL", &p, &token)
            .unwrap_err()
            .code(),
        "FDB_CANCELLED"
    );
    assert_eq!(c.transaction_state(), TransactionState::Active);
    assert_eq!(
        c.execute("SELECT n FROM docs", &p).unwrap().rows,
        vec![vec![Value::Integer(1)]]
    );
    let future = CancellationToken::with_deadline(Instant::now() + Duration::from_secs(60));
    let clone = future.clone();
    clone.cancel();
    assert!(future.is_cancelled());
    c.execute("ROLLBACK", &p).unwrap();
}

#[test]
fn deadline_interrupts_engine_work_and_allows_fresh_retry() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let p = Parameters::new();
    c.execute("CREATE TABLE input(n)", &p).unwrap();
    for n in 0..100 {
        c.execute(&format!("INSERT INTO input VALUES({n})"), &p)
            .unwrap();
    }
    c.execute("CREATE TABLE docs", &p).unwrap();
    c.execute("CREATE UNIQUE INDEX docs_n ON docs(n)", &p)
        .unwrap();
    c.execute("BEGIN", &p).unwrap();
    c.execute("INSERT INTO docs {n:1}", &p).unwrap();
    for sql in [
        "SELECT count(*) FROM input a,input b,input c,input d,input e",
        "INSERT INTO docs(n) SELECT count(*) FROM input a,input b,input c,input d,input e",
    ] {
        let token = CancellationToken::with_deadline(Instant::now() + Duration::from_millis(20));
        assert_eq!(
            c.execute_cancellable(sql, &p, &token).unwrap_err().code(),
            "FDB_CANCELLED"
        );
        assert!(token.is_cancelled());
        assert_eq!(c.transaction_state(), TransactionState::Active);
        assert_eq!(
            c.check_collection_integrity("docs", Default::default())
                .unwrap()
                .documents,
            1
        );
    }
    let fresh = CancellationToken::with_deadline(Instant::now() + Duration::from_secs(60));
    c.execute_cancellable("INSERT INTO docs {n:2}", &p, &fresh)
        .unwrap();
    assert_eq!(
        c.execute("SELECT n FROM docs ORDER BY n", &p).unwrap().rows,
        vec![vec![Value::Integer(1)], vec![Value::Integer(2)]]
    );
    c.execute("ROLLBACK", &p).unwrap();
    assert!(c.execute("SELECT * FROM docs", &p).unwrap().rows.is_empty());
}
