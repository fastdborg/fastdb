//! A cancellation-only handle that does not keep a database connection alive.
use crate::Connection;
use std::sync::{Arc, Weak};
#[derive(Clone)]
pub struct InterruptHandle {
    connection: Weak<turso_core::Connection>,
}
impl InterruptHandle {
    /// Request interruption of active engine statements. Returns false if the
    /// connection has been dropped. True means live, not confirmed cancellation.
    /// Idle requests do not poison subsequent statements.
    pub fn interrupt(&self) -> bool {
        let Some(connection) = self.connection.upgrade() else {
            return false;
        };
        connection.interrupt();
        true
    }
}
impl Connection {
    pub fn interrupt_handle(&self) -> InterruptHandle {
        InterruptHandle {
            connection: Arc::downgrade(&self.engine),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, Parameters, Value};
    use std::sync::atomic::{AtomicBool, Ordering};
    fn arm_after_write(connection: &Connection) -> Arc<AtomicBool> {
        let baseline = connection.engine.total_changes();
        let engine = Arc::downgrade(&connection.engine);
        let fired = Arc::new(AtomicBool::new(false));
        let flag = fired.clone();
        connection.engine.set_progress_handler(
            1,
            Some(Box::new(move || {
                engine
                    .upgrade()
                    .is_some_and(|c| c.total_changes() > baseline)
                    && !flag.swap(true, Ordering::SeqCst)
            })),
        );
        fired
    }
    fn q(connection: &Connection, sql: &str) -> crate::QueryResult {
        connection.execute(sql, &Parameters::new()).expect(sql)
    }
    #[test]
    fn interrupted_collection_mutations_restore_documents_and_indexes() {
        for (statement, outer) in [
            "UPDATE docs SET value=value+100",
            "DELETE FROM docs WHERE value>0",
            "INSERT INTO docs (value) SELECT value+100 FROM docs",
            "WITH a AS (SELECT value+100 AS value FROM docs) INSERT INTO docs (value) SELECT value FROM a",
            "INSERT INTO docs (value) SELECT a.value FROM (SELECT value+100 AS value FROM docs) a",
            "INSERT INTO docs (value) SELECT value+100 FROM docs UNION ALL SELECT value+200 FROM docs",
            "WITH a AS MATERIALIZED (SELECT value FROM docs) INSERT INTO docs (value) SELECT value+100 FROM a UNION ALL SELECT value+200 FROM a",
            "INSERT INTO docs (value) SELECT value+100 FROM docs UNION SELECT value+100 FROM docs",
            "INSERT INTO docs (value) SELECT value+100 FROM docs INTERSECT SELECT value+100 FROM docs",
            "INSERT INTO docs (value) SELECT value+100 FROM docs EXCEPT SELECT value+200 FROM docs",
            "INSERT INTO docs (value) VALUES (101),(102) UNION VALUES (102),(103)",
        ]
        .into_iter()
        .flat_map(|statement| [false, true].map(|outer| (statement, outer)))
        {
            let db = Database::open(":memory:").unwrap();
            let c = db.connect().unwrap();
            q(&c, "CREATE TABLE docs");
            q(&c, "CREATE UNIQUE INDEX values_idx ON docs(value)");
            for i in 1..=3 {
                q(&c, &format!("INSERT INTO docs {{id:docs:{i},value:{i}}}"));
            }
            let before = q(&c, "SELECT * FROM docs ORDER BY id").rows;
            q(&c, "CREATE TABLE prior(value INTEGER)");
            if outer {
                q(&c, "BEGIN");
                q(&c, "INSERT INTO prior VALUES (1)");
            }
            let fired = arm_after_write(&c);
            let report = c.execute_report(statement, &Parameters::new());
            c.engine.set_progress_handler(0, None);
            assert_eq!(
                report.transaction_after,
                if outer {
                    crate::TransactionState::Active
                } else {
                    crate::TransactionState::Autocommit
                },
                "{statement}, outer={outer}"
            );
            assert_eq!(
                q(&c, "SELECT * FROM prior").rows.len(),
                usize::from(outer),
                "prior work: {statement}"
            );
            assert!(
                fired.load(Ordering::SeqCst),
                "no partial write reached: {statement}"
            );
            assert_eq!(
                report.result.unwrap_err().code(),
                "FDB_CANCELLED",
                "{statement}"
            );
            assert_eq!(
                q(&c, "SELECT * FROM docs ORDER BY id").rows,
                before,
                "{statement}"
            );
            for i in 1..=3 {
                assert_eq!(
                    c.lookup_index("docs", "values_idx", &Value::Integer(i))
                        .unwrap()
                        .len(),
                    1
                );
                assert!(c
                    .lookup_index("docs", "values_idx", &Value::Integer(i + 100))
                    .unwrap()
                    .is_empty());
            }
            assert_eq!(c.check_collection_integrity("docs", Default::default()).unwrap().documents, 3);
            q(&c, "UPDATE docs SET value=value+10");
            assert_eq!(
                c.lookup_index("docs", "values_idx", &Value::Integer(11))
                    .unwrap()
                    .len(),
                1
            );
            if outer {
                q(&c, "ROLLBACK");
                assert!(q(&c, "SELECT * FROM prior").rows.is_empty());
                assert_eq!(q(&c, "SELECT * FROM docs ORDER BY id").rows, before);
            }
        }
    }
    #[test]
    fn interrupted_index_build_removes_metadata_and_physical_storage() {
        let db = Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        q(&c, "CREATE TABLE docs");
        for i in 1..=3 {
            q(&c, &format!("INSERT INTO docs {{value:{i}}}"));
        }
        let fired = arm_after_write(&c);
        let result = c.create_index("docs", "build_idx", vec!["value".into()], true);
        c.engine.set_progress_handler(0, None);
        assert!(fired.load(Ordering::SeqCst));
        assert_eq!(result.unwrap_err().code(), "FDB_CANCELLED");
        assert!(c.catalog("docs").unwrap().indexes.is_empty());
        let names = c
            .run(
                "SELECT name FROM sqlite_schema WHERE name='build_idx' OR name LIKE '__fastdb_i_%'",
                &[],
            )
            .unwrap();
        assert!(names.is_empty(), "orphaned index storage: {names:?}");
        q(&c, "CREATE UNIQUE INDEX build_idx ON docs(value)");
        for i in 1..=3 {
            assert_eq!(
                c.lookup_index("docs", "build_idx", &Value::Integer(i))
                    .unwrap()
                    .len(),
                1
            );
        }
    }
    #[test]
    fn interrupted_catalog_mutations_restore_schema_and_prior_transaction_work() {
        for statement in [
            "DROP INDEX values_idx",
            "DROP TABLE docs",
            "DEFINE FIELD OVERWRITE value ON docs TYPE number REQUIRED",
        ] {
            for outer in [false, true] {
                let db = Database::open(":memory:").unwrap();
                let c = db.connect().unwrap();
                q(&c, "CREATE TABLE docs");
                q(&c, "DEFINE FIELD value ON docs TYPE integer REQUIRED");
                q(&c, "CREATE UNIQUE INDEX values_idx ON docs(value)");
                q(&c, "INSERT INTO docs {id:docs:a,value:1}");
                q(&c, "CREATE TABLE prior(value INTEGER)");
                let info = q(&c, "INFO FOR TABLE docs").rows;
                let schema = c
                    .run("SELECT name,sql FROM sqlite_schema ORDER BY name", &[])
                    .unwrap();
                if outer {
                    q(&c, "BEGIN");
                    q(&c, "INSERT INTO prior VALUES (1)");
                }
                let fired = arm_after_write(&c);
                let report = c.execute_report(statement, &Parameters::new());
                c.engine.set_progress_handler(0, None);
                assert!(fired.load(Ordering::SeqCst), "{statement}");
                assert_eq!(
                    report.result.unwrap_err().code(),
                    "FDB_CANCELLED",
                    "{statement}"
                );
                assert_eq!(
                    report.transaction_after,
                    if outer {
                        crate::TransactionState::Active
                    } else {
                        crate::TransactionState::Autocommit
                    }
                );
                assert_eq!(q(&c, "INFO FOR TABLE docs").rows, info, "{statement}");
                assert_eq!(
                    c.run("SELECT name,sql FROM sqlite_schema ORDER BY name", &[])
                        .unwrap(),
                    schema,
                    "{statement}"
                );
                assert_eq!(
                    c.lookup_index("docs", "values_idx", &Value::Integer(1))
                        .unwrap()
                        .len(),
                    1
                );
                assert_eq!(q(&c, "SELECT * FROM prior").rows.len(), usize::from(outer));
                assert!(c
                    .execute("INSERT INTO docs {value:1.5}", &Parameters::new())
                    .is_err());
                q(&c, statement);
                if outer {
                    q(&c, "ROLLBACK");
                    assert_eq!(q(&c, "INFO FOR TABLE docs").rows, info);
                    assert_eq!(
                        c.lookup_index("docs", "values_idx", &Value::Integer(1))
                            .unwrap()
                            .len(),
                        1
                    );
                    assert!(q(&c, "SELECT * FROM prior").rows.is_empty());
                }
            }
        }
    }
    #[test]
    fn cancelled_index_replacement_restores_old_index_on_outer_rollback() {
        let db = Database::open(":memory:").unwrap();
        let c = db.connect().unwrap();
        q(&c, "CREATE TABLE docs");
        q(&c, "INSERT INTO docs {value:1,other:10}");
        q(&c, "INSERT INTO docs {value:2,other:20}");
        q(&c, "CREATE UNIQUE INDEX values_idx ON docs(value)");
        let before = q(&c, "INFO FOR TABLE docs").rows;
        q(&c, "BEGIN");
        q(&c, "DROP INDEX values_idx");
        let fired = arm_after_write(&c);
        let report = c.execute_report(
            "CREATE UNIQUE INDEX values_idx ON docs(other)",
            &Parameters::new(),
        );
        c.engine.set_progress_handler(0, None);
        assert!(fired.load(Ordering::SeqCst));
        assert_eq!(report.result.unwrap_err().code(), "FDB_CANCELLED");
        assert_eq!(report.transaction_after, crate::TransactionState::Active);
        assert!(c.catalog("docs").unwrap().indexes.is_empty());
        q(&c, "ROLLBACK");
        assert_eq!(q(&c, "INFO FOR TABLE docs").rows, before);
        assert_eq!(
            c.lookup_index("docs", "values_idx", &Value::Integer(1))
                .unwrap()
                .len(),
            1
        );
        assert!(c
            .lookup_index("docs", "values_idx", &Value::Integer(10))
            .unwrap()
            .is_empty());
        q(&c, "BEGIN");
        q(&c, "DROP INDEX values_idx");
        q(&c, "CREATE UNIQUE INDEX values_idx ON docs(other)");
        q(&c, "COMMIT");
        assert_eq!(
            c.lookup_index("docs", "values_idx", &Value::Integer(10))
                .unwrap()
                .len(),
            1
        );
        assert!(c
            .lookup_index("docs", "values_idx", &Value::Integer(1))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn interrupted_cte_lowering_and_reads_do_not_fall_back_or_run_writes() {
        use std::sync::atomic::AtomicUsize;
        let statements = [
            "WITH l AS (SELECT * FROM labels), a AS (SELECT value FROM docs) SELECT a.value,l.value FROM a JOIN l ON a.value=l.value",
            "WITH l AS (SELECT * FROM labels), a AS (SELECT value FROM docs) INSERT INTO copied (value) SELECT a.value FROM a JOIN l ON a.value=l.value",
        ];
        for statement in statements {
            for after in [1, 5, 20] {
                let db = Database::open(":memory:").unwrap();
                let c = db.connect().unwrap();
                q(&c, "CREATE TABLE docs");
                q(&c, "CREATE TABLE copied");
                q(&c, "CREATE UNIQUE INDEX copied_value ON copied(value)");
                q(&c, "CREATE TABLE labels(value INTEGER)");
                q(&c, "CREATE TABLE prior(value INTEGER)");
                for value in 1..=3 {
                    q(&c, &format!("INSERT INTO docs {{value:{value}}}"));
                    q(&c, &format!("INSERT INTO labels VALUES ({value})"));
                }
                q(&c, "BEGIN");
                q(&c, "INSERT INTO prior VALUES (99)");
                let steps = Arc::new(AtomicUsize::new(0));
                let count = steps.clone();
                c.engine.set_progress_handler(
                    1,
                    Some(Box::new(move || {
                        count.fetch_add(1, Ordering::SeqCst) + 1 == after
                    })),
                );
                let report = c.execute_report(statement, &Parameters::new());
                c.engine.set_progress_handler(0, None);
                assert!(
                    steps.load(Ordering::SeqCst) >= after,
                    "interruption point not reached"
                );
                assert_eq!(
                    report.result.unwrap_err().code(),
                    "FDB_CANCELLED",
                    "step {after}: {statement}"
                );
                assert_eq!(report.transaction_after, crate::TransactionState::Active);
                assert_eq!(
                    q(&c, "SELECT * FROM prior").rows,
                    vec![vec![Value::Integer(99)]]
                );
                assert!(q(&c, "SELECT * FROM copied").rows.is_empty());
                for value in 1..=3 {
                    assert!(c
                        .lookup_index("copied", "copied_value", &Value::Integer(value))
                        .unwrap()
                        .is_empty());
                }
                assert_eq!(
                    q(&c, "SELECT count(*) FROM docs").rows,
                    vec![vec![Value::Integer(3)]]
                );
                q(&c, statement);
                q(&c, "ROLLBACK");
                assert!(q(&c, "SELECT * FROM copied").rows.is_empty());
                assert!(q(&c, "SELECT * FROM prior").rows.is_empty());
            }
        }
    }

    #[test]
    fn native_target_subquery_cancellation_matches_native_transaction_disposition() {
        use std::sync::atomic::AtomicUsize;
        use turso_ext::{scalar, ResultCode, Value as ExtValue};
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        #[scalar(name = "native_subquery_tick")]
        fn native_subquery_tick(args: &[ExtValue]) -> ExtValue {
            CALLS.fetch_add(1, Ordering::SeqCst);
            ExtValue::from_integer(args[0].to_integer().expect("integer source"))
        }
        for predicate in [
            "value IN (SELECT native_subquery_tick(value)+native_subquery_tick(0) FROM docs)",
            "value NOT IN (SELECT native_subquery_tick(value)+native_subquery_tick(10) FROM docs)",
            "EXISTS (SELECT value FROM docs WHERE native_subquery_tick(value)+native_subquery_tick(0)=3)",
            "value <= (SELECT max(native_subquery_tick(value)+native_subquery_tick(0)) FROM docs)",
        ] {
            for after in [2, 4] {
                for outer in [false, true] {
                    let mut observed = Vec::new();
                    for logical in [false, true] {
                        let db = Database::open(":memory:").unwrap();
                        let c = db.connect().unwrap();
                        unsafe {
                            let api = c.engine._build_turso_ext();
                            let code = (api.register_scalar_function)(api.ctx, c"native_subquery_tick".as_ptr(), 1, false, 0, native_subquery_tick, None, None);
                            c.engine._free_extension_ctx(api);
                            assert_eq!(code, ResultCode::OK);
                        }
                        q(&c, "CREATE TABLE docs");
                        q(&c, "INSERT INTO docs(value) VALUES (1),(2),(3)");
                        q(&c, "CREATE TABLE source_native(value INTEGER)");
                        q(&c, "INSERT INTO source_native VALUES (1),(2),(3)");
                        q(&c, "CREATE TABLE target(value INTEGER UNIQUE)");
                        q(&c, "CREATE TABLE prior(value INTEGER)");
                        if outer { q(&c, "BEGIN"); q(&c, "INSERT INTO prior VALUES (99)"); }
                        let sql = format!("INSERT INTO target SELECT value FROM docs WHERE {predicate}");
                        let sql = if logical { sql } else { sql.replace("FROM docs", "FROM source_native") };
                        CALLS.store(0, Ordering::SeqCst);
                        let fired = Arc::new(AtomicBool::new(false));
                        let flag = fired.clone();
                        c.engine.set_progress_handler(1, Some(Box::new(move || CALLS.load(Ordering::SeqCst) >= after && !flag.swap(true, Ordering::SeqCst))));
                        let report = c.execute_report(&sql, &Parameters::new());
                        c.engine.set_progress_handler(0, None);
                        assert!(fired.load(Ordering::SeqCst), "{sql}");
                        assert_eq!(CALLS.load(Ordering::SeqCst), after);
                        assert_eq!(report.result.unwrap_err().code(), "FDB_CANCELLED", "{sql}");
                        assert!(q(&c, "SELECT * FROM target").rows.is_empty());
                        observed.push((report.transaction_after, q(&c, "SELECT * FROM prior").rows));
                        assert_eq!(c.check_collection_integrity("docs", Default::default()).unwrap().documents, 3);
                        assert_eq!(q(&c, &sql).affected, 3);
                        assert_eq!(q(&c, "SELECT value FROM target ORDER BY value").rows, vec![vec![Value::Integer(1)], vec![Value::Integer(2)], vec![Value::Integer(3)]]);
                        if c.transaction_state() == crate::TransactionState::Active {
                            q(&c, "ROLLBACK");
                            assert!(q(&c, "SELECT * FROM target").rows.is_empty());
                        }
                    }
                    assert_eq!(observed[0], observed[1], "{predicate}, after={after}, outer={outer}");
                }
            }
        }
    }

    #[test]
    fn interrupted_compound_and_subquery_sources_discard_rows_and_allow_exact_retry() {
        use std::sync::atomic::AtomicUsize;
        use turso_ext::{scalar, ResultCode, Value as ExtValue};
        static ROWS: AtomicUsize = AtomicUsize::new(0);
        #[scalar(name = "union_source_tick")]
        fn union_source_tick(args: &[ExtValue]) -> ExtValue {
            ROWS.fetch_add(1, Ordering::SeqCst);
            ExtValue::from_integer(args[0].to_integer().expect("integer source"))
        }
        for (operator, insert) in [
            "UNION ALL",
            "UNION",
            "INTERSECT",
            "EXCEPT",
            "IN",
            "NOT IN",
            "EXISTS",
            "SCALAR",
        ]
        .into_iter()
        .flat_map(|operator| [false, true].map(|insert| (operator, insert)))
        {
            let expected = match operator {
                "INTERSECT" => vec![],
                "EXCEPT" | "IN" | "NOT IN" | "EXISTS" | "SCALAR" => vec![1, 2, 3],
                _ => vec![1, 2, 3, 11, 12, 13],
            };
            for after in [2, 4] {
                for outer in [false, true] {
                    let db = Database::open(":memory:").unwrap();
                    let c = db.connect().unwrap();
                    unsafe {
                        let api = c.engine._build_turso_ext();
                        let code = (api.register_scalar_function)(
                            api.ctx,
                            c"union_source_tick".as_ptr(),
                            1,
                            false,
                            0,
                            union_source_tick,
                            None,
                            None,
                        );
                        c.engine._free_extension_ctx(api);
                        assert_eq!(code, ResultCode::OK);
                    }
                    q(&c, "CREATE TABLE docs");
                    q(&c, "CREATE TABLE copied");
                    q(&c, "CREATE UNIQUE INDEX copied_value ON copied(value)");
                    q(&c, "CREATE TABLE prior(value INTEGER)");
                    for value in 1..=3 {
                        q(&c, &format!("INSERT INTO docs {{value:{value}}}"));
                    }
                    if outer {
                        q(&c, "BEGIN");
                        q(&c, "INSERT INTO prior VALUES (99)");
                    }
                    let prefix = if insert {
                        "INSERT INTO copied(value) "
                    } else {
                        ""
                    };
                    let source = match operator {
                        "IN" => "SELECT value FROM docs WHERE value IN (SELECT union_source_tick(value)+union_source_tick(0) FROM docs)".to_owned(),
                        "NOT IN" => "SELECT value FROM docs WHERE value NOT IN (SELECT union_source_tick(value)+union_source_tick(10) FROM docs)".to_owned(),
                        "EXISTS" => "SELECT value FROM docs WHERE EXISTS (SELECT value FROM docs WHERE union_source_tick(value)+union_source_tick(0)=3)".to_owned(),
                        "SCALAR" => "SELECT value FROM docs WHERE value <= (SELECT max(union_source_tick(value)+union_source_tick(0)) FROM docs)".to_owned(),
                        _ => format!("SELECT union_source_tick(value) AS value FROM docs {operator} SELECT union_source_tick(value+10) FROM docs"),
                    };
                    let statement = format!("{prefix}{source}");
                    ROWS.store(0, Ordering::SeqCst);
                    let fired = Arc::new(AtomicBool::new(false));
                    let flag = fired.clone();
                    c.engine.set_progress_handler(
                        1,
                        Some(Box::new(move || {
                            ROWS.load(Ordering::SeqCst) >= after
                                && !flag.swap(true, Ordering::SeqCst)
                        })),
                    );
                    let report = c.execute_report(&statement, &Parameters::new());
                    c.engine.set_progress_handler(0, None);
                    assert!(
                        fired.load(Ordering::SeqCst),
                        "{operator}, source point {after}, insert={insert}, outer={outer}"
                    );
                    assert_eq!(ROWS.load(Ordering::SeqCst), after);
                    assert_eq!(
                        report.result.unwrap_err().code(),
                        "FDB_CANCELLED",
                        "{statement}"
                    );
                    assert_eq!(
                        report.transaction_after,
                        if outer {
                            crate::TransactionState::Active
                        } else {
                            crate::TransactionState::Autocommit
                        }
                    );
                    assert_eq!(q(&c, "SELECT * FROM prior").rows.len(), usize::from(outer));
                    assert_eq!(
                        c.check_collection_integrity("copied", Default::default())
                            .unwrap()
                            .documents,
                        0
                    );
                    assert_eq!(
                        c.check_collection_integrity("docs", Default::default())
                            .unwrap()
                            .documents,
                        3
                    );
                    let retry = q(&c, &statement);
                    if insert {
                        assert_eq!(retry.affected, expected.len() as i64);
                        assert_eq!(
                            c.check_collection_integrity("copied", Default::default())
                                .unwrap()
                                .documents,
                            expected.len() as u64
                        );
                    } else {
                        assert_eq!(retry.rows.len(), expected.len());
                    }
                    let rows = if insert {
                        q(&c, "SELECT value FROM copied").rows
                    } else {
                        retry.rows
                    };
                    let mut values = rows
                        .into_iter()
                        .map(|row| match row.as_slice() {
                            [Value::Integer(value)] => *value,
                            _ => panic!("unexpected retry row: {row:?}"),
                        })
                        .collect::<Vec<_>>();
                    values.sort_unstable();
                    assert_eq!(values, expected, "{statement}");
                    if outer {
                        q(&c, "ROLLBACK");
                        assert_eq!(
                            c.check_collection_integrity("copied", Default::default())
                                .unwrap()
                                .documents,
                            0
                        );
                        assert!(q(&c, "SELECT * FROM prior").rows.is_empty());
                    }
                }
            }
        }
    }
}
