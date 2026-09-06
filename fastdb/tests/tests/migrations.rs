use fastdb::{Database, Migration, Parameters, TransactionState};
fn m(version: i64, sql: &str) -> Migration {
    Migration {
        version,
        name: format!("{version}_test.sql"),
        sql: sql.into(),
    }
}
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).expect(sql)
}
#[test]
fn migrations_apply_once_and_reject_changed_or_missing_history() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let plan = vec![
        m(1, "CREATE TABLE docs; CREATE TABLE audit(value INTEGER);"),
        m(
            2,
            "INSERT INTO docs {id:docs:p1,value:4}; INSERT INTO audit VALUES (1);",
        ),
    ];
    assert_eq!(c.migrate(&plan).unwrap().applied, vec![1, 2]);
    let report = c.migrate(&plan).unwrap();
    assert_eq!(report.already_applied, 2);
    assert!(report.applied.is_empty());
    assert_eq!(q(&c, "SELECT * FROM audit").rows.len(), 1);
    let mut changed = plan.clone();
    changed[0].sql.push(' ');
    assert!(c.migrate(&changed).is_err());
    assert!(c.migrate(&plan[..1]).is_err());
    assert_eq!(c.transaction_state(), TransactionState::Autocommit);
    assert!(c
        .execute("SELECT * FROM __fastdb_migrations", &Parameters::new())
        .is_err());
}
#[test]
fn failed_pending_run_rolls_back_ddl_data_and_history() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    let initial = m(
        1,
        "CREATE TABLE docs; DEFINE FIELD value ON docs TYPE integer;",
    );
    c.migrate(std::slice::from_ref(&initial)).unwrap();
    let mut plan = vec![
        initial,
        m(
            2,
            "CREATE TABLE audit(value INTEGER); INSERT INTO docs {id:docs:p1,value:1};",
        ),
        m(3, "INSERT INTO docs {id:docs:p2,value:'bad'};"),
    ];
    let error = c.migrate(&plan).unwrap_err();
    assert_eq!(error.code(), "FDB_MIGRATION");
    assert!(q(&c, "SELECT * FROM docs").rows.is_empty());
    assert!(c
        .execute("SELECT * FROM audit", &Parameters::new())
        .is_err());
    plan[2] = m(3, "INSERT INTO docs {id:docs:p2,value:2};");
    assert_eq!(c.migrate(&plan).unwrap().applied, vec![2, 3]);
    assert_eq!(q(&c, "SELECT * FROM docs").rows.len(), 2);
    plan.push(m(4,"CREATE TABLE runtime_work(value INTEGER); INSERT INTO docs {id:docs:p3,value:3}; SELECT array::append(1,2);"));
    assert_eq!(c.migrate(&plan).unwrap_err().code(), "FDB_MIGRATION");
    assert_eq!(c.transaction_state(), TransactionState::Autocommit);
    assert_eq!(q(&c, "SELECT * FROM docs").rows.len(), 2);
    assert!(c
        .execute("SELECT * FROM runtime_work", &Parameters::new())
        .is_err());
    plan[3] = m(4, "CREATE TABLE runtime_work(value INTEGER);");
    assert_eq!(c.migrate(&plan).unwrap().applied, vec![4]);
}
#[test]
fn migration_transaction_escape_and_invalid_order_fail_before_writes() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    for command in [
        "COMMIT",
        "END",
        "ROLLBACK",
        "BEGIN",
        "SAVEPOINT x",
        "RELEASE x",
        "PRAGMA user_version=1",
        "ATTACH ':memory:' AS other",
        "VACUUM",
        "CREATE TEMP TABLE transient(x)",
    ] {
        let plan = [m(1, &format!("CREATE TABLE docs; {command};"))];
        assert!(c.migrate(&plan).is_err(), "{command}");
        // No collection was created by the rejected plan.
        assert!(c
            .execute("SELECT docs:missing", &Parameters::new())
            .is_err());
    }
    assert!(c.migrate(&[m(2, ""), m(1, "")]).is_err());
    q(&c, "BEGIN");
    assert!(c.migrate(&[m(1, "CREATE TABLE docs;")]).is_err());
    assert_eq!(c.transaction_state(), TransactionState::Active);
    q(&c, "ROLLBACK");
}
