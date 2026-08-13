#![forbid(unsafe_code)]
#![deny(warnings)]

use std::sync::mpsc;
use std::time::Duration;
use tempfile::tempdir;
use turso_fastdb::{Database, ErrorCategory, Failpoint, StatementResult, Value};

fn row_count(connection: &turso_fastdb::Connection, source: &str) -> usize {
    let response = connection.execute(source).unwrap();
    let StatementResult::Rows(rows) = &response.statements[0] else {
        panic!("expected rows")
    };
    rows.len()
}

#[test]
fn p3_script_001_ordered_results_and_later_failures_keep_standalone_commits() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    let response = conn
        .execute(
            "DEFINE TABLE person SCHEMALESS; \
             CREATE person:a SET n=1; \
             SELECT * FROM person:a",
        )
        .unwrap();
    assert_eq!(response.statements.len(), 3);
    assert!(matches!(response.statements[0], StatementResult::None));
    assert!(matches!(response.statements[1], StatementResult::Rows(_)));
    assert!(matches!(response.statements[2], StatementResult::Rows(_)));

    assert_eq!(
        conn.execute("CREATE person:b SET n=2; @")
            .unwrap_err()
            .category(),
        ErrorCategory::Parse
    );
    assert_eq!(row_count(&conn, "SELECT * FROM person:b"), 1);

    assert_eq!(
        conn.execute("CREATE person:c SET n=3; CREATE person:a SET n=9")
            .unwrap_err()
            .category(),
        ErrorCategory::Constraint
    );
    assert_eq!(row_count(&conn, "SELECT * FROM person:c"), 1);

    assert_eq!(
        conn.execute("CREATE person:d SET n=4; USE NS unsupported")
            .unwrap_err()
            .category(),
        ErrorCategory::UnsupportedSyntax
    );
    assert_eq!(row_count(&conn, "SELECT * FROM person:d"), 1);
}

#[test]
fn p3_txn_001_commit_cancel_visibility_and_schema_publication() {
    let db = Database::open_memory().unwrap();
    let writer = db.connect().unwrap();
    let reader = db.connect().unwrap();
    let response = writer
        .execute(
            "BEGIN; DEFINE TABLE person SCHEMALESS; \
             CREATE person:a SET n=1; SELECT * FROM person:a",
        )
        .unwrap();
    assert_eq!(response.statements.len(), 4);
    assert_eq!(row_count(&writer, "SELECT * FROM person:a"), 1);

    let (send, receive) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        send.send(row_count(&reader, "SELECT * FROM person:a"))
            .unwrap();
    });
    assert!(receive.recv_timeout(Duration::from_millis(100)).is_err());
    assert!(matches!(
        writer.execute("COMMIT").unwrap().statements[0],
        StatementResult::None
    ));
    assert_eq!(receive.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
    handle.join().unwrap();

    writer.execute("BEGIN; CREATE person:b SET n=2").unwrap();
    writer.execute("CANCEL").unwrap();
    assert_eq!(row_count(&writer, "SELECT * FROM person:b"), 0);
}

#[test]
fn p3_txn_002_any_active_error_rolls_back_and_requires_cancel() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    let error = conn
        .execute("BEGIN; CREATE person:a SET n=1; CREATE person:a SET n=2")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Constraint);
    for source in ["SELECT * FROM person", "COMMIT", "BEGIN"] {
        assert_eq!(
            conn.execute(source).unwrap_err().category(),
            ErrorCategory::Transaction,
            "{source}"
        );
    }
    conn.execute("CANCEL").unwrap();
    assert_eq!(row_count(&conn, "SELECT * FROM person"), 0);

    conn.execute("BEGIN; CREATE person:b SET n=1").unwrap();
    assert_eq!(
        conn.execute("SELECT FROM").unwrap_err().category(),
        ErrorCategory::Parse
    );
    conn.execute("CANCEL").unwrap();
    assert_eq!(row_count(&conn, "SELECT * FROM person:b"), 0);

    conn.execute("BEGIN; CREATE person:c SET n=1").unwrap();
    assert_eq!(
        conn.execute("USE NS unsupported").unwrap_err().category(),
        ErrorCategory::UnsupportedSyntax
    );
    conn.execute("CANCEL").unwrap();
    assert_eq!(row_count(&conn, "SELECT * FROM person:c"), 0);

    conn.execute("BEGIN; CREATE person:param SET n=1").unwrap();
    assert_eq!(
        conn.execute("SELECT * FROM person WHERE n=$missing")
            .unwrap_err()
            .category(),
        ErrorCategory::Schema
    );
    conn.execute("CANCEL").unwrap();
    assert_eq!(row_count(&conn, "SELECT * FROM person:param"), 0);
}

#[test]
fn p3_txn_003_busy_nested_missing_and_connection_reuse() {
    let db = Database::open_memory().unwrap();
    let first = db.connect().unwrap();
    let second = db.connect().unwrap();
    first.execute("BEGIN").unwrap();
    assert_eq!(
        first.execute("BEGIN").unwrap_err().category(),
        ErrorCategory::Transaction
    );
    first.execute("CANCEL").unwrap();

    first.execute("BEGIN").unwrap();
    let busy = second.execute("BEGIN").unwrap_err();
    assert_eq!(busy.category(), ErrorCategory::Transaction);
    assert_eq!(
        busy.to_string(),
        "transaction error: database is busy or locked by another transaction"
    );
    first.execute("CANCEL").unwrap();
    second
        .execute("BEGIN; CREATE note:a SET n=1; COMMIT")
        .unwrap();
    assert_eq!(row_count(&second, "SELECT * FROM note:a"), 1);

    for source in ["COMMIT", "CANCEL"] {
        assert_eq!(
            second.execute(source).unwrap_err().category(),
            ErrorCategory::Transaction
        );
    }
}

#[test]
fn p3_txn_004_commit_and_rollback_cleanup_failures_poison_or_break() {
    let db = Database::open_memory().unwrap();
    let conn = db.connect().unwrap();
    conn.execute("BEGIN; CREATE person:a SET n=1").unwrap();
    conn.arm_failpoint(Failpoint::CommitFailure);
    assert_eq!(
        conn.execute("COMMIT").unwrap_err().category(),
        ErrorCategory::Transaction
    );
    conn.disarm_all_failpoints();
    assert_eq!(
        conn.execute("SELECT * FROM person").unwrap_err().category(),
        ErrorCategory::Transaction
    );
    conn.execute("CANCEL").unwrap();
    assert_eq!(row_count(&conn, "SELECT * FROM person"), 0);

    conn.execute("BEGIN; CREATE person:b SET n=1").unwrap();
    conn.arm_failpoint(Failpoint::RollbackFailure);
    let error = conn.execute("CREATE person:b SET n=2").unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Transaction);
    conn.disarm_all_failpoints();
    for source in ["CANCEL", "SELECT * FROM person"] {
        assert_eq!(
            conn.execute(source).unwrap_err().category(),
            ErrorCategory::Transaction
        );
    }
}

#[test]
fn p3_txn_005_drop_rolls_back_uncommitted_file_state() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("drop.fastdb");
    let path = path.to_str().unwrap();
    {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        conn.execute("BEGIN; CREATE person:a SET n=1").unwrap();
    }
    {
        let db = Database::open(path).unwrap();
        let conn = db.connect().unwrap();
        assert_eq!(row_count(&conn, "SELECT * FROM person:a"), 0);
        conn.execute("CREATE person:b SET n=2").unwrap();
        let rows = conn.execute("SELECT n FROM person:b").unwrap();
        assert_eq!(
            rows.statements[0],
            StatementResult::Rows(vec![Value::Object(
                [("n".into(), Value::Integer(2))].into_iter().collect()
            )])
        );
    }
}
