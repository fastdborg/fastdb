use fastdb::{Database, Parameters, Value};
fn q(c: &fastdb::Connection, sql: &str) -> fastdb::QueryResult {
    c.execute(sql, &Parameters::new()).unwrap()
}
#[test]
fn profiles_measure_scan_reduction_without_changing_typed_results() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE docs");
    q(&c, "BEGIN");
    for n in 0..200 {
        q(&c,&format!("INSERT INTO docs (id,group_no,payload,flag) VALUES (type::record('docs',{n}),{},X'02ff',true)",n%20));
    }
    q(&c, "COMMIT");
    let sql = "SELECT id,payload,flag FROM docs WHERE group_no=$group ORDER BY id";
    let params = Parameters::from([("$group".into(), Value::Integer(7))]);
    let scan = c.profile_select(sql, &params).unwrap();
    assert_eq!(scan.result.rows, c.execute(sql, &params).unwrap().rows);
    assert_eq!(scan.result.rows.len(), 10);
    assert!(scan.metrics.fullscan_steps >= 199, "{:?}", scan.metrics);
    assert!(scan.metrics.vm_steps > 0);
    assert_eq!(scan.metrics.rows_written, 0);
    q(&c, "CREATE INDEX docs_group ON docs(group_no)");
    let indexed = c.profile_select(sql, &params).unwrap();
    assert_eq!(indexed.result.rows, scan.result.rows);
    assert_eq!(indexed.result.columns, scan.result.columns);
    assert!(
        indexed.metrics.fullscan_steps < scan.metrics.fullscan_steps,
        "{:?}",
        indexed.metrics
    );
    assert!(
        indexed.metrics.rows_read < scan.metrics.rows_read,
        "{:?} vs {:?}",
        indexed.metrics,
        scan.metrics
    );
    assert_eq!(
        c.profile_select(sql, &params).unwrap().metrics,
        indexed.metrics
    );
    let nested=c.profile_select("WITH a AS (SELECT id,payload,flag FROM docs WHERE group_no=$group) SELECT * FROM a ORDER BY id",&params).unwrap();
    assert_eq!(nested.result.rows, indexed.result.rows);
    assert!(nested.metrics.vm_steps > 0);
}
#[test]
fn native_profiles_bind_values_and_reject_writes_before_execution() {
    let db = Database::open(":memory:").unwrap();
    let c = db.connect().unwrap();
    q(&c, "CREATE TABLE native(n INTEGER)");
    q(&c, "INSERT INTO native VALUES (1),(2),(3)");
    let sql = "SELECT n FROM native WHERE n>$n ORDER BY n";
    let params = Parameters::from([("$n".into(), Value::Integer(1))]);
    let profile = c.profile_select(sql, &params).unwrap();
    assert_eq!(profile.result.rows, c.execute(sql, &params).unwrap().rows);
    assert!(profile.metrics.rows_read >= 3, "{:?}", profile.metrics);
    assert!(profile.metrics.fullscan_steps >= 2);
    for sql in [
        "DELETE FROM native",
        "CREATE TABLE docs",
        "SELECT 1; DELETE FROM native",
        "SELECT __fastdb_pack(n) FROM native",
    ] {
        assert!(c.profile_select(sql, &Parameters::new()).is_err(), "{sql}");
    }
    assert_eq!(
        q(&c, "SELECT count(*) FROM native").rows,
        vec![vec![Value::Integer(3)]]
    );
    q(&c, "CREATE TABLE docs");
    q(&c, "INSERT INTO docs {id:docs:a,ref:docs:a}");
    assert!(c
        .profile_select("SELECT record::fetch(ref) FROM docs", &Parameters::new())
        .is_err());
}
