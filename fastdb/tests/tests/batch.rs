use fastdb::{Database, Parameters, TransactionState, Value};
#[test]
fn multiline_scripts_preserve_literals_comments_offsets_and_final_statement() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let script="-- Unicode ไทย\nCREATE TABLE posts;\n/* ; */ INSERT INTO posts {\n id:posts:p1,\n text:'semi;colon', tags:[';', [1,2]]\n};\nSELECT text FROM posts";
    let reports = c.execute_batch(script).unwrap();
    assert_eq!(reports.len(), 3);
    assert_eq!(reports[0].offset, script.find("CREATE").unwrap());
    assert!(reports.iter().all(|r| r.execution.result.is_ok()));
    assert_eq!(
        reports[2].execution.result.as_ref().unwrap().rows,
        vec![vec![Value::String("semi;colon".into())]]
    );
    assert!(c
        .execute_batch("; -- nothing\n /* ; */ ;")
        .unwrap()
        .is_empty());
}
#[test]
fn batches_stop_on_error_and_leave_explicit_transaction_control_to_caller() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    c.execute("CREATE TABLE posts", &Parameters::new()).unwrap();
    let reports = c
        .execute_batch(
            "BEGIN; INSERT INTO posts {id:posts:p1}; INSERT INTO posts {id:posts:p1}; COMMIT;",
        )
        .unwrap();
    assert_eq!(reports.len(), 3);
    assert!(reports[2].execution.result.is_err());
    assert_eq!(
        reports[2].execution.transaction_after,
        TransactionState::Active
    );
    c.execute("ROLLBACK", &Parameters::new()).unwrap();
    assert!(c
        .execute("SELECT * FROM posts", &Parameters::new())
        .unwrap()
        .rows
        .is_empty());
    assert!(c
        .execute_batch("INSERT INTO posts {id:posts:p2}; SELECT 'unterminated")
        .is_err());
    assert!(c
        .execute("SELECT * FROM posts", &Parameters::new())
        .unwrap()
        .rows
        .is_empty());
}
#[test]
fn sql_trigger_bodies_and_case_end_are_single_statements() {
    let script="CREATE TABLE source(n); CREATE TABLE events(n); CREATE TRIGGER log_insert AFTER INSERT ON source BEGIN INSERT INTO events VALUES(CASE WHEN NEW.n=1 THEN 10 ELSE 20 END); INSERT INTO events VALUES(30); END; INSERT INTO source VALUES(1); SELECT n FROM events ORDER BY n;";
    let pieces = fastql_parser::split_script(script).unwrap();
    assert_eq!(pieces.len(), 5);
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let reports = c.execute_batch(script).unwrap();
    for report in &reports {
        assert!(
            report.execution.result.is_ok(),
            "{:?}",
            report.execution.result
        );
    }
    assert_eq!(reports.len(), 5);
    assert_eq!(
        reports[4].execution.result.as_ref().unwrap().rows,
        vec![vec![Value::Integer(10)], vec![Value::Integer(30)]]
    );
}
