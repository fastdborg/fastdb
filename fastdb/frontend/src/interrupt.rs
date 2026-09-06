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
}
